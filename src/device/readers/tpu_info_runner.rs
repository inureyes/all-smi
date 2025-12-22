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
            let mut current_device_idx: u32 = 0;
            
            loop {
                let child_res = Command::new("tpu-info")
                    .arg("--metrics")
                    .arg("duty_cycle_percent,hbm_usage,tensorcore_utilization,memory_total,power_usage")
                    .arg("--rate")
                    .arg("1")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn();

                match child_res {
                    Ok(mut child) => {
                        {
                            let mut s = status.lock().unwrap();
                            *s = "tpu-info running".to_string();
                        }

                        if let Some(stdout) = child.stdout.take() {
                            let reader = BufReader::new(stdout);
                            for line_res in reader.lines() {
                                if let Ok(line) = line_res {
                                    let line = line.trim();
                                    if line.is_empty() { continue; }

                                    // 1. Detect device section header
                                    // Common formats: "Device 0:", "Chip 0:", "[0]", etc.
                                    if let Some(idx) = Self::extract_device_index(line) {
                                        current_device_idx = idx;
                                        continue;
                                    }

                                    // 2. Parse metric line for the current device
                                    Self::parse_and_update(line, current_device_idx, &metrics_store);
                                    
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

    fn extract_device_index(line: &str) -> Option<u32> {
        let line_lower = line.to_lowercase();
        // Look for patterns like "device 0", "chip 1", "tpu 2"
        for prefix in &["device", "chip", "tpu"] {
            if line_lower.starts_with(prefix) {
                return line_lower
                    .trim_start_matches(prefix)
                    .trim_matches(|c: char| !c.is_numeric())
                    .parse().ok();
            }
        }
        // Handle bracket format: "[0]"
        if line.starts_with('[') && line.ends_with(']') {
            return line.trim_matches(|c: char| !c.is_numeric()).parse().ok();
        }
        None
    }

    fn parse_and_update(line: &str, device_idx: u32, store: &Arc<RwLock<HashMap<u32, HashMap<String, f64>>>>) {
        let line = line.trim();
        if line.is_empty() || line.starts_with("---") || line.starts_with("===") { return; }

        let parts: Vec<&str> = if line.contains(':') {
            line.splitn(2, ':').collect()
        } else if line.contains('=') {
            line.splitn(2, '=').collect()
        } else {
            line.split_whitespace().collect()
        };

        if parts.len() >= 2 {
            let key = parts[0].trim().to_lowercase();
            let raw_value = parts[1].trim();
            let value_str = raw_value.split_whitespace().next().unwrap_or("0");
            
            if let Ok(mut value) = value_str.parse::<f64>() {
                let suffix = raw_value.to_lowercase();
                if suffix.contains("mb") || suffix.contains("mib") { value *= 1024.0 * 1024.0; }
                else if suffix.contains("gb") || suffix.contains("gib") { value *= 1024.0 * 1024.0 * 1024.0; }
                else if suffix.contains("kb") || suffix.contains("kib") { value *= 1024.0; }
                else if suffix.contains("mw") { value /= 1000.0; }
                
                if let Ok(mut store_guard) = store.write() {
                    let device_map = store_guard.entry(device_idx).or_insert_with(HashMap::new);
                    device_map.insert(key.clone(), value);
                }
                
                #[cfg(debug_assertions)]
                eprintln!("[DEBUG] Parsed metric [Dev {}]: {} = {}", device_idx, key, value);
            }
        }
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
