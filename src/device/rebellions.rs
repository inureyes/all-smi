use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

// Global status for error messages
static REBELLIONS_STATUS: Mutex<Option<String>> = Mutex::new(None);

/// PCI information for Rebellions device
#[derive(Debug, Deserialize)]
struct RblnPciInfo {
    #[allow(dead_code)]
    dev: String,
    bus_id: String,
    numa_node: String,
    link_speed: String,
    link_width: String,
}

/// Memory information for Rebellions device
#[derive(Debug, Deserialize)]
struct RblnMemoryInfo {
    used: String,
    total: String,
}

/// JSON structure for single device from rbln-smi
#[derive(Debug, Deserialize)]
struct RblnDevice {
    #[allow(dead_code)]
    npu: String,
    name: String,
    sid: String,
    uuid: String,
    device: String,
    status: String,
    fw_ver: String,
    pci: RblnPciInfo,
    temperature: String,
    card_power: String,
    pstate: String,
    memory: RblnMemoryInfo,
    util: String,
    board_info: String,
    #[allow(dead_code)]
    location: u32,
}

/// JSON response structure from rbln-smi
#[derive(Debug, Deserialize)]
struct RblnResponse {
    #[serde(rename = "KMD_version")]
    kmd_version: String,
    devices: Vec<RblnDevice>,
    #[allow(dead_code)]
    contexts: Vec<serde_json::Value>, // Empty array in the example
}

pub struct RebellionsReader {
    command_path: String,
}

impl RebellionsReader {
    pub fn new() -> Self {
        // Try to find rbln-smi in common locations
        let command_path = if std::path::Path::new("/usr/local/bin/rbln-smi").exists() {
            "/usr/local/bin/rbln-smi".to_string()
        } else if std::path::Path::new("/usr/bin/rbln-smi").exists() {
            "/usr/bin/rbln-smi".to_string()
        } else {
            // Fallback to PATH lookup
            "rbln-smi".to_string()
        };

        RebellionsReader { command_path }
    }

    /// Store an error message in the global status
    fn set_status(message: String) {
        if let Ok(mut status) = REBELLIONS_STATUS.lock() {
            *status = Some(message);
        }
    }

    /// Create reader with custom command path
    #[allow(dead_code)]
    pub fn with_command_path(path: String) -> Self {
        RebellionsReader { command_path: path }
    }

    /// Parse float value from string, returning 0.0 on error
    fn parse_float(s: &str) -> f64 {
        s.parse::<f64>().unwrap_or(0.0)
    }

    /// Parse integer value from string, returning 0 on error
    #[allow(dead_code)]
    fn parse_u32(s: &str) -> u32 {
        s.parse::<u32>().unwrap_or(0)
    }

    /// Parse temperature from string (removes 'C' suffix)
    fn parse_temperature(temp_str: &str) -> u32 {
        temp_str.trim_end_matches('C').parse::<u32>().unwrap_or(0)
    }

    /// Parse power from string (removes 'mW' suffix)
    fn parse_power_mw(power_str: &str) -> f64 {
        power_str
            .trim_end_matches("mW")
            .parse::<f64>()
            .unwrap_or(0.0)
    }

    /// Parse memory value from string to u64
    fn parse_memory(mem_str: &str) -> u64 {
        mem_str.parse::<u64>().unwrap_or(0)
    }

    /// Execute rbln-smi command and parse the output
    fn get_rbln_info(&self) -> Result<RblnResponse, String> {
        let output = Command::new(&self.command_path)
            .arg("-j")
            .output()
            .map_err(|e| format!("Failed to execute rbln-smi: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("rbln-smi failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse JSON response
        let response: RblnResponse = serde_json::from_str(&stdout)
            .map_err(|e| format!("Failed to parse rbln-smi JSON: {e}"))?;

        Ok(response)
    }

    /// Determine the device model based on device name and memory
    fn get_device_model(name: &str, total_memory_bytes: u64) -> String {
        // Convert bytes to GB for classification
        let total_memory_gb = total_memory_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

        // The device name already provides the model (e.g., RBLN-CA22)
        // But we can enhance it with ATOM variant based on memory
        let variant = if total_memory_gb <= 16.0 {
            "ATOM"
        } else if total_memory_gb <= 32.0 {
            "ATOM+"
        } else {
            "ATOM Max"
        };

        format!("{name} ({variant})")
    }
}

impl GpuReader for RebellionsReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        match self.get_rbln_info() {
            Ok(response) => {
                // Clear any previous error status
                if let Ok(mut status) = REBELLIONS_STATUS.lock() {
                    *status = None;
                }

                let time_str = Local::now().format("%a %b %d %H:%M:%S %Y").to_string();
                let hostname = get_hostname();
                let kmd_version = response.kmd_version.clone();

                response
                    .devices
                    .into_iter()
                    .enumerate()
                    .map(|(idx, device)| {
                        let total_memory = Self::parse_memory(&device.memory.total);
                        let model = Self::get_device_model(&device.name, total_memory);

                        let mut detail = HashMap::new();
                        detail.insert("KMD Version".to_string(), kmd_version.clone());
                        detail.insert("Firmware Version".to_string(), device.fw_ver.clone());
                        detail.insert("Device Name".to_string(), device.device.clone());
                        detail.insert("Serial ID".to_string(), device.sid.clone());
                        detail.insert("Status".to_string(), device.status.clone());
                        detail.insert("PCIe Bus".to_string(), device.pci.bus_id.clone());
                        detail.insert("PCIe Link Speed".to_string(), device.pci.link_speed.clone());
                        detail.insert(
                            "PCIe Link Width".to_string(),
                            format!("x{}", device.pci.link_width),
                        );
                        detail.insert("NUMA Node".to_string(), device.pci.numa_node.clone());
                        detail.insert("Performance State".to_string(), device.pstate.clone());
                        detail.insert("Board Info".to_string(), device.board_info.clone());

                        GpuInfo {
                            uuid: device.uuid,
                            time: time_str.clone(),
                            name: format!("Rebellions {model}"),
                            device_type: "NPU".to_string(),
                            host_id: "local".to_string(),
                            hostname: hostname.clone(),
                            instance: format!("local:{idx}"),
                            utilization: Self::parse_float(&device.util),
                            ane_utilization: 0.0,
                            dla_utilization: None,
                            temperature: Self::parse_temperature(&device.temperature),
                            used_memory: Self::parse_memory(&device.memory.used),
                            total_memory,
                            frequency: 0, // Not provided by rbln-smi
                            power_consumption: Self::parse_power_mw(&device.card_power) / 1000.0, // Convert mW to W
                            detail,
                        }
                    })
                    .collect()
            }
            Err(e) => {
                let error_msg = format!("Error reading Rebellions devices: {e}");
                eprintln!("{error_msg}");
                Self::set_status(error_msg);
                vec![]
            }
        }
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // Since rbln-smi doesn't provide process information,
        // we'll return an empty list
        // TODO: Implement process detection for Rebellions NPUs if available
        vec![]
    }
}

impl Default for RebellionsReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Get a user-friendly message about Rebellions status
#[allow(dead_code)]
pub fn get_rebellions_status_message() -> Option<String> {
    if let Ok(status) = REBELLIONS_STATUS.lock() {
        status.clone()
    } else {
        None
    }
}
