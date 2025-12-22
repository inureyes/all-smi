use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock, OnceLock};
use std::thread;
use std::time::Duration;

static RUNNER: OnceLock<TpuInfoRunner> = OnceLock::new();

pub fn get_runner() -> &'static TpuInfoRunner {
    RUNNER.get_or_init(TpuInfoRunner::new)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TableType {
    None,
    RuntimeUtilization,
    TensorCoreUtilization,
}

#[derive(Clone)]
pub struct TpuInfoRunner {
    /// Latest captured metrics per device index
    /// HashMap<DeviceIndex, HashMap<MetricName, Value>>
    pub device_metrics: Arc<RwLock<HashMap<u32, HashMap<String, f64>>>>,
    /// Status message for notification
    pub status: Arc<Mutex<String>>,
}

impl TpuInfoRunner {
    pub fn new() -> Self {
        let runner = Self {
            device_metrics: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(Mutex::new("Initializing tpu-info...".to_string())),
        };
        runner.start_background_thread();
        runner
    }

    fn start_background_thread(&self) {
        let metrics_store = self.device_metrics.clone();
        let status = self.status.clone();

        thread::spawn(move || {
            let mut current_table = TableType::None;
            
            loop {
                // Attempt to run tpu-info in streaming mode
                // Setting TERM=dumb and NO_COLOR=1 to get plain text output
                // Using multiple --metric flags as required by tpu-info
                // Removed invalid metrics: memory_total, power_usage
                let child_res = Command::new("tpu-info")
                    .arg("--streaming")
                    .arg("--rate")
                    .arg("2")
                    .arg("--metric").arg("duty_cycle_percent")
                    .arg("--metric").arg("hbm_usage")
                    .arg("--metric").arg("tensorcore_utilization")
                    .env("TERM", "dumb")
                    .env("NO_COLOR", "1")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn();

                match child_res {
                    Ok(mut child) => {
                        // Keep "Initializing..." status until we actually get data
                        // This prevents the notification from flashing too quickly if the process starts fast but data takes time
                        // {
                        //    let mut s = status.lock().unwrap();
                        //    *s = "tpu-info running".to_string();
                        // }

                        if let Some(stdout) = child.stdout.take() {
                            let reader = BufReader::new(stdout);
                            for line_res in reader.lines() {
                                if let Ok(line) = line_res {
                                    #[cfg(debug_assertions)]
                                    eprintln!("[DEBUG] tpu-info raw: {}", line);

                                    Self::parse_line(&line, &mut current_table, &metrics_store);
                                    
                                    let mut s = status.lock().unwrap();
                                    if s.contains("Initializing") {
                                        *s = "Ready".to_string();
                                    }
                                }
                            }
                        }
                        let _ = child.wait();
                        let mut s = status.lock().unwrap();
                        *s = "tpu-info exited, restarting...".to_string();
                    }
                    Err(e) => {
                        let mut s = status.lock().unwrap();
                        *s = format!("Failed to start tpu-info: {}", e);
                        thread::sleep(Duration::from_secs(10));
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    fn parse_line(line: &str, current_table: &mut TableType, store: &Arc<RwLock<HashMap<u32, HashMap<String, f64>>>>) {
        let line = line.trim();
        if line.is_empty() { return; }

        // 1. Detect table headers
        if line.contains("TPU Runtime Utilization") {
            *current_table = TableType::RuntimeUtilization;
            return;
        } else if line.contains("TensorCore Utilization") {
            *current_table = TableType::TensorCoreUtilization;
            return;
        } else if line.contains("TPU Chips") || line.contains("TPU Process Info") {
            *current_table = TableType::None; // Skip these tables
            return;
        }

        // 2. Parse table rows
        // Rich tables use box characters like │, ┌, └, ├
        if line.contains('│') {
            let parts: Vec<&str> = line.split('│')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            match *current_table {
                TableType::RuntimeUtilization => {
                    // Header: ["Chip", "HBM Usage (GiB)", "Duty cycle"]
                    // Row: "0", "1.23 GiB / 16.00 GiB", "45.67%"
                    if parts.len() >= 3 {
                        if let Ok(idx) = parts[0].parse::<u32>() {
                            let hbm_str = parts[1];
                            let duty_str = parts[2];

                            let (used_hbm, total_hbm) = Self::parse_hbm_usage(hbm_str);
                            let duty = Self::parse_percent(duty_str);

                            if let Ok(mut map_guard) = store.write() {
                                let dev_map = map_guard.entry(idx).or_insert_with(HashMap::new);
                                dev_map.insert("hbm_usage".to_string(), used_hbm);
                                dev_map.insert("memory_total".to_string(), total_hbm);
                                dev_map.insert("duty_cycle_percent".to_string(), duty);
                            }
                        }
                    }
                }
                TableType::TensorCoreUtilization => {
                    // Header: ["Core ID", "TensorCore Utilization"]
                    // Row: "0", "45.67%"
                    if parts.len() >= 2 {
                        if let Ok(idx) = parts[0].parse::<u32>() {
                            let util = Self::parse_percent(parts[1]);
                            if let Ok(mut map_guard) = store.write() {
                                let dev_map = map_guard.entry(idx).or_insert_with(HashMap::new);
                                dev_map.insert("tensorcore_utilization".to_string(), util);
                            }
                        }
                    }
                }
                TableType::None => {}
            }
        }
    }

    fn parse_hbm_usage(s: &str) -> (f64, f64) {
        // "1.23 GiB / 16.00 GiB"
        let parts: Vec<&str> = s.split('/').map(|p| p.trim()).collect();
        if parts.len() >= 2 {
            (Self::parse_bytes(parts[0]), Self::parse_bytes(parts[1]))
        } else {
            (0.0, 0.0)
        }
    }

    fn parse_bytes(s: &str) -> f64 {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.is_empty() { return 0.0; }
        if let Ok(mut val) = parts[0].parse::<f64>() {
            if parts.len() >= 2 {
                let unit = parts[1].to_lowercase();
                if unit.contains("gi") || unit == "gb" { val *= 1024.0 * 1024.0 * 1024.0; }
                else if unit.contains("mi") || unit == "mb" { val *= 1024.0 * 1024.0; }
                else if unit.contains("ki") || unit == "kb" { val *= 1024.0; }
            }
            val
        } else {
            0.0
        }
    }

    fn parse_percent(s: &str) -> f64 {
        // "45.67%" or "N/A"
        s.trim_end_matches('%').parse::<f64>().unwrap_or(0.0)
    }

    pub fn get_status(&self) -> Option<String> {
        let s = self.status.lock().unwrap().clone();
        if s == "Ready" { None } else { Some(s) }
    }
    
    pub fn get_metric(&self, device_idx: u32, key: &str) -> Option<f64> {
        self.device_metrics.read().unwrap()
            .get(&device_idx)
            .and_then(|m| m.get(key).copied())
    }
}
