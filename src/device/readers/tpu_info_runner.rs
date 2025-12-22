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
    /// Latest captured metrics (key-value map)
    pub metrics: Arc<RwLock<HashMap<String, f64>>>,
    /// Status message for notification (e.g. "Running", "Failed")
    pub status: Arc<Mutex<String>>,
}

impl TpuInfoRunner {
    pub fn new() -> Self {
        let runner = Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(Mutex::new("Initializing tpu-info...".to_string())),
        };
        runner.start_background_thread();
        runner
    }

    fn start_background_thread(&self) {
        let metrics_store = self.metrics.clone();
        let status = self.status.clone();

        thread::spawn(move || {
            loop {
                // Attempt to run tpu-info in streaming mode
                // We request specific metrics based on user feedback
                let child_res = Command::new("tpu-info")
                    .arg("--metrics")
                    .arg("duty_cycle_percent,hbm_usage,tensorcore_utilization,memory_total,power_usage") // Added likely metrics
                    .arg("--rate")
                    .arg("1") // 1Hz update rate
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
                                    if !line.trim().is_empty() {
                                        // Parse line and update metrics
                                        // Format assumption: "metric_name: value" or "metric_name=value" or "value" if only one asked?
                                        // We will try to extract key-value pairs
                                        Self::parse_and_update(&line, &metrics_store);
                                        
                                        // Once we get data, update status to empty (Ready)
                                        let mut s = status.lock().unwrap();
                                        if s.contains("Initializing") {
                                            *s = "Ready".to_string();
                                        }
                                    }
                                }
                            }
                        }

                        // Process exited
                        let _ = child.wait();
                        {
                            let mut s = status.lock().unwrap();
                            *s = "tpu-info exited, restarting...".to_string();
                        }
                    }
                    Err(e) => {
                        {
                            let mut s = status.lock().unwrap();
                            *s = format!("Failed to start tpu-info: {}", e); // Show error in notification
                        }
                        // Sleep longer if failed to start (e.g. not installed)
                        thread::sleep(Duration::from_secs(10));
                    }
                }

                // Sleep before restart
                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    fn parse_and_update(line: &str, store: &Arc<RwLock<HashMap<String, f64>>>) {
        // Simple parser for "key: value" or "key=value"
        let line = line.trim();
        
        // Skip separators or empty lines
        if line.is_empty() || line.starts_with("---") || line.starts_with("===") {
            return;
        }

        let parts: Vec<&str> = if line.contains(':') {
            line.splitn(2, ':').collect()
        } else if line.contains('=') {
            line.splitn(2, '=').collect()
        } else {
            // Space separated is risky without known keys, but let's try
            line.split_whitespace().collect()
        };

        if parts.len() >= 2 {
            let key = parts[0].trim().to_lowercase();
            let raw_value = parts[1].trim();
            
            // Extract numeric part and handle suffixes
            // e.g. "12.5 %", "100 MB", "50 W"
            let value_str = raw_value.split_whitespace().next().unwrap_or("0");
            
            if let Ok(mut value) = value_str.parse::<f64>() {
                // Handle unit suffixes if present in the *raw_value* (not just the first part)
                let suffix = raw_value.to_lowercase();
                
                if suffix.contains("mb") || suffix.contains("mib") {
                    value *= 1024.0 * 1024.0;
                } else if suffix.contains("gb") || suffix.contains("gib") {
                    value *= 1024.0 * 1024.0 * 1024.0;
                } else if suffix.contains("kb") || suffix.contains("kib") {
                    value *= 1024.0;
                } else if suffix.contains("mw") {
                    value /= 1000.0; // Convert to W
                }
                
                if let Ok(mut map) = store.write() {
                    map.insert(key.clone(), value);
                }
                
                #[cfg(debug_assertions)]
                eprintln!("[DEBUG] Parsed metric: {} = {}", key, value);
            } else {
                #[cfg(debug_assertions)]
                eprintln!("[DEBUG] Failed to parse value '{}' from line: {}", raw_value, line);
            }
        } else {
             #[cfg(debug_assertions)]
             eprintln!("[DEBUG] Unparsed line (unknown format): {}", line);
        }
    }

    pub fn get_status(&self) -> Option<String> {
        let s = self.status.lock().unwrap().clone();
        if s == "Ready" {
            None
        } else {
            Some(s)
        }
    }
    
    pub fn get_metric(&self, key: &str) -> Option<f64> {
        self.metrics.read().unwrap().get(key).copied()
    }
}
