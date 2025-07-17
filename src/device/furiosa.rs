use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

/// Collection method for Furiosa NPU metrics
#[derive(Debug, Clone, Copy)]
pub enum CollectionMethod {
    /// Use furiosactl command-line tool
    Furiosactl,
    /// Read directly from device files in /dev
    DeviceFile,
}

/// JSON structure for furiosactl info output
#[derive(Debug, Deserialize)]
struct FuriosaInfoJson {
    dev_name: String,
    product_name: String,
    device_uuid: String,
    device_sn: String,
    firmware: String,
    temperature: String,
    power: String,
    clock: String,
    pci_bdf: String,
    pci_dev: String,
}

/// JSON structure for furiosactl list output
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FuriosaListJson {
    npu: String,
    cores: Vec<FuriosaCoreInfo>,
    devfiles: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FuriosaCoreInfo {
    idx: u32,
    status: String,
}

/// JSON structure for furiosactl ps output
#[derive(Debug, Deserialize)]
struct FuriosaProcessJson {
    npu: String,
    pid: u32,
    cmd: String,
}

/// Configuration for Furiosa reader
pub struct FuriosaConfig {
    /// Primary method to use for collecting metrics
    pub primary_method: CollectionMethod,
    /// Fallback method if primary fails
    pub fallback_method: Option<CollectionMethod>,
}

impl Default for FuriosaConfig {
    fn default() -> Self {
        Self {
            primary_method: CollectionMethod::Furiosactl,
            fallback_method: Some(CollectionMethod::DeviceFile),
        }
    }
}

pub struct FuriosaReader {
    config: FuriosaConfig,
}

impl FuriosaReader {
    pub fn new() -> Self {
        Self::with_config(FuriosaConfig::default())
    }

    pub fn with_config(config: FuriosaConfig) -> Self {
        FuriosaReader { config }
    }

    /// Extract base NPU name from device string
    /// e.g., "npu0pe0-1" -> "npu0"
    fn get_base_npu_name(device: &str) -> String {
        if let Some(idx) = device.find("pe") {
            device[..idx].to_string()
        } else {
            device.to_string()
        }
    }

    /// Get NPU utilization from furiosactl top (single sample)
    fn get_npu_utilization(&self) -> Option<HashMap<String, f64>> {
        use std::io::{BufRead, BufReader};
        use std::process::Stdio;

        // Run furiosactl top with a short interval to get one sample
        let mut child = Command::new("furiosactl")
            .args(["top", "--interval", "100"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let stdout = child.stdout.take()?;
        let reader = BufReader::new(stdout);
        let mut utilization_map = HashMap::new();
        let mut lines_read = 0;

        // Read a few lines to get utilization data
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("Device") && line.contains("NPU(%)") {
                // Skip header line
                continue;
            }

            // Parse data line: "2023-03-21T09:45:56.699483936Z  152616    npu1pe0-1      19.06    100.00     0.00   ./npu_runtime_test"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                if let (Some(device), Some(npu_util)) = (parts.get(2), parts.get(3)) {
                    if let Ok(util) = npu_util.parse::<f64>() {
                        let base_npu = Self::get_base_npu_name(device);
                        // Store the maximum utilization for each NPU
                        utilization_map
                            .entry(base_npu)
                            .and_modify(|e: &mut f64| *e = e.max(util))
                            .or_insert(util);
                    }
                }
            }

            lines_read += 1;
            if lines_read > 10 {
                break;
            }
        }

        // Kill the process after getting some data
        let _ = child.kill();
        let _ = child.wait();

        if utilization_map.is_empty() {
            None
        } else {
            Some(utilization_map)
        }
    }

    /// Collect NPU info using the configured method with fallback
    fn collect_npu_info(&self) -> Vec<GpuInfo> {
        // Try primary method first
        let mut result = match self.config.primary_method {
            CollectionMethod::Furiosactl => self.collect_via_furiosactl(),
            CollectionMethod::DeviceFile => self.collect_via_device_files(),
        };

        // If primary method failed and we have a fallback, try it
        if result.is_empty() {
            if let Some(fallback) = self.config.fallback_method {
                eprintln!(
                    "Primary method {:?} failed, trying fallback {:?}",
                    self.config.primary_method, fallback
                );
                result = match fallback {
                    CollectionMethod::Furiosactl => self.collect_via_furiosactl(),
                    CollectionMethod::DeviceFile => self.collect_via_device_files(),
                };
            }
        }

        result
    }

    /// Collect NPU information using furiosactl
    fn collect_via_furiosactl(&self) -> Vec<GpuInfo> {
        match Command::new("furiosactl")
            .args(["info", "--format", "json"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    let mut devices = self.parse_furiosactl_info_json(&output_str);

                    // Try to get utilization data
                    if let Some(utilization_map) = self.get_npu_utilization() {
                        for device in &mut devices {
                            if let Some(util) = utilization_map.get(&device.instance) {
                                device.utilization = *util;
                            }
                        }
                    }

                    devices
                } else {
                    eprintln!(
                        "furiosactl command failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    vec![]
                }
            }
            Err(e) => {
                eprintln!("Failed to execute furiosactl: {e}");
                vec![]
            }
        }
    }

    /// Collect NPU information by reading device files
    fn collect_via_device_files(&self) -> Vec<GpuInfo> {
        // TODO: Implement device file reading
        // This will read from /dev/furiosa* or similar device files
        eprintln!("Device file collection not yet implemented");
        vec![]
    }

    /// Parse furiosactl info JSON output
    fn parse_furiosactl_info_json(&self, output: &str) -> Vec<GpuInfo> {
        match serde_json::from_str::<Vec<FuriosaInfoJson>>(output) {
            Ok(devices) => {
                let hostname = get_hostname();
                let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

                devices
                    .into_iter()
                    .map(|device| {
                        let mut detail = HashMap::new();
                        detail.insert("serial_number".to_string(), device.device_sn);
                        detail.insert("firmware".to_string(), device.firmware);
                        detail.insert("pci_address".to_string(), device.pci_bdf);
                        detail.insert("pci_device".to_string(), device.pci_dev);

                        // Parse temperature (remove °C suffix)
                        let temperature = device
                            .temperature
                            .trim_end_matches("°C")
                            .parse::<u32>()
                            .unwrap_or(0);

                        // Parse power (remove W suffix)
                        let power = device
                            .power
                            .trim_end_matches(" W")
                            .parse::<f64>()
                            .unwrap_or(0.0);

                        // Parse frequency (remove MHz suffix)
                        let frequency = device
                            .clock
                            .trim_end_matches(" MHz")
                            .parse::<u32>()
                            .unwrap_or(0);

                        GpuInfo {
                            uuid: device.device_uuid,
                            time: time.clone(),
                            name: format!("Furiosa {}", device.product_name),
                            device_type: "NPU".to_string(),
                            hostname: hostname.clone(),
                            instance: device.dev_name.clone(),
                            utilization: 0.0, // TODO: Get from furiosactl top or other source
                            ane_utilization: 0.0,
                            dla_utilization: None,
                            temperature,
                            used_memory: 0,  // TODO: Get memory info when available
                            total_memory: 0, // TODO: Get memory info when available
                            frequency,
                            power_consumption: power,
                            detail,
                        }
                    })
                    .collect()
            }
            Err(e) => {
                eprintln!("Failed to parse furiosactl JSON output: {e}");
                vec![]
            }
        }
    }

    /// Get processes using Furiosa NPUs via furiosactl
    fn get_furiosa_processes_via_furiosactl(&self) -> Vec<ProcessInfo> {
        match Command::new("furiosactl")
            .args(["ps", "--format", "json"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    self.parse_furiosactl_ps_json(&output_str)
                } else {
                    eprintln!(
                        "furiosactl ps failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    vec![]
                }
            }
            Err(e) => {
                eprintln!("Failed to execute furiosactl ps: {e}");
                vec![]
            }
        }
    }

    /// Parse furiosactl ps JSON output
    fn parse_furiosactl_ps_json(&self, output: &str) -> Vec<ProcessInfo> {
        match serde_json::from_str::<Vec<FuriosaProcessJson>>(output) {
            Ok(processes) => {
                processes
                    .into_iter()
                    .map(|proc| {
                        let base_npu = Self::get_base_npu_name(&proc.npu);

                        // Extract process name from command
                        let process_name = proc
                            .cmd
                            .split_whitespace()
                            .next()
                            .and_then(|cmd| cmd.split('/').next_back())
                            .unwrap_or("unknown")
                            .to_string();

                        // Get system process info if available
                        let sys_info =
                            crate::device::process_utils::get_system_process_info(proc.pid);

                        ProcessInfo {
                            device_id: 0, // TODO: Map NPU name to device index
                            device_uuid: base_npu,
                            pid: proc.pid,
                            process_name,
                            used_memory: 0, // TODO: Get from actual data when available
                            cpu_percent: sys_info.as_ref().map(|s| s.0).unwrap_or(0.0),
                            memory_percent: sys_info.as_ref().map(|s| s.1).unwrap_or(0.0),
                            memory_rss: sys_info.as_ref().map(|s| s.2).unwrap_or(0),
                            memory_vms: sys_info.as_ref().map(|s| s.3).unwrap_or(0),
                            user: sys_info.as_ref().map(|s| s.4.clone()).unwrap_or_default(),
                            state: sys_info.as_ref().map(|s| s.5.clone()).unwrap_or_default(),
                            start_time: sys_info.as_ref().map(|s| s.6.clone()).unwrap_or_default(),
                            cpu_time: sys_info.as_ref().map(|s| s.7).unwrap_or(0),
                            command: proc.cmd,
                            ppid: sys_info.as_ref().map(|s| s.9).unwrap_or(0),
                            threads: sys_info.as_ref().map(|s| s.10).unwrap_or(0),
                            uses_gpu: true, // Using NPU
                            priority: 0,
                            nice_value: 0,
                            gpu_utilization: 0.0, // TODO: Get from furiosactl top
                        }
                    })
                    .collect()
            }
            Err(e) => {
                // Empty array is valid JSON, no need to log error
                if output.trim() != "[]" {
                    eprintln!("Failed to parse furiosactl ps JSON output: {e}");
                }
                vec![]
            }
        }
    }

    /// Get processes using Furiosa NPUs via device files
    fn get_furiosa_processes_via_device_files(&self) -> Vec<ProcessInfo> {
        // TODO: Get processes using Furiosa NPUs via /dev
        vec![]
    }

    /// Collect process info using the configured method with fallback
    fn collect_process_info(&self) -> Vec<ProcessInfo> {
        // Try primary method first
        let mut result = match self.config.primary_method {
            CollectionMethod::Furiosactl => self.get_furiosa_processes_via_furiosactl(),
            CollectionMethod::DeviceFile => self.get_furiosa_processes_via_device_files(),
        };

        // If primary method failed and we have a fallback, try it
        if result.is_empty() {
            if let Some(fallback) = self.config.fallback_method {
                result = match fallback {
                    CollectionMethod::Furiosactl => self.get_furiosa_processes_via_furiosactl(),
                    CollectionMethod::DeviceFile => self.get_furiosa_processes_via_device_files(),
                };
            }
        }

        result
    }
}

impl GpuReader for FuriosaReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        self.collect_npu_info()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.collect_process_info()
    }
}
