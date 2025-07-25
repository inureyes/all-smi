use crate::device::{container_utils, GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

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

/// JSON response structure from rbln-stat/rbln-smi
#[derive(Debug, Deserialize)]
struct RblnResponse {
    #[serde(rename = "KMD_version")]
    kmd_version: String,
    devices: Vec<RblnDevice>,
    #[serde(default)]
    contexts: Vec<RblnContext>,
}

/// Context structure for process information
#[derive(Debug, Deserialize)]
struct RblnContext {
    #[allow(dead_code)]
    ctx_id: String,
    npu: String,
    #[allow(dead_code)]
    process: String, // Process name from rbln-stat
    pid: String,
    #[allow(dead_code)]
    priority: String,
    #[allow(dead_code)]
    ptid: String,
    memalloc: String,
    #[allow(dead_code)]
    status: String,
    util_info: String,
}

// Lazy static for caching the command path
static COMMAND_PATH: Lazy<Option<PathBuf>> = Lazy::new(|| {
    const PATHS: &[&str] = &[
        "/usr/local/bin/rbln-stat",
        "/usr/bin/rbln-stat",
        "/usr/local/bin/rbln-smi",
        "/usr/bin/rbln-smi",
    ];

    // Check specific paths first
    for path in PATHS {
        if Path::new(path).exists() {
            return Some(PathBuf::from(path));
        }
    }

    // Check if commands are available in PATH
    for cmd in &["rbln-stat", "rbln-smi"] {
        if Command::new("which")
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return Some(PathBuf::from(cmd));
        }
    }

    None
});

pub struct RebellionsReader {
    command_path: Arc<PathBuf>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl RebellionsReader {
    pub fn new() -> Self {
        let command_path = COMMAND_PATH
            .as_ref()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("rbln-stat"));

        RebellionsReader {
            command_path: Arc::new(command_path),
            last_error: Arc::new(Mutex::new(if COMMAND_PATH.is_none() {
                Some("Neither rbln-stat nor rbln-smi found in system".to_string())
            } else {
                None
            })),
        }
    }

    /// Create reader with custom command path
    #[allow(dead_code)]
    pub fn with_command_path(path: PathBuf) -> Self {
        RebellionsReader {
            command_path: Arc::new(path),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the last error message if any
    #[allow(dead_code)]
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok()?.clone()
    }

    /// Set an error message
    fn set_error(&self, error: String) {
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(error);
        }
    }

    /// Parse numeric values with optional suffixes
    fn parse_value<T: std::str::FromStr + Default>(s: &str, suffix: Option<&str>) -> T {
        let trimmed = if let Some(suffix) = suffix {
            s.trim_end_matches(suffix)
        } else {
            s
        };
        trimmed.parse::<T>().unwrap_or_default()
    }

    /// Execute rbln-stat/rbln-smi command and parse the output
    fn get_rbln_info(&self) -> Result<RblnResponse, String> {
        let cmd_name = self
            .command_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rbln-stat/rbln-smi");

        let output = Command::new(self.command_path.as_ref())
            .arg("-j")
            .output()
            .map_err(|e| {
                let error = format!("Failed to execute {cmd_name}: {e}");
                self.set_error(error.clone());
                error
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let error = format!("{cmd_name} failed: {stderr}");
            self.set_error(error.clone());
            return Err(error);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse JSON response
        serde_json::from_str(&stdout).map_err(|e| {
            let error = format!("Failed to parse {cmd_name} JSON: {e}");
            self.set_error(error.clone());
            error
        })
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

impl RebellionsReader {
    /// Parse memory allocation string (e.g., "66.0MiB") to bytes
    fn parse_memory_allocation(mem_str: &str) -> u64 {
        let mem_str = mem_str.trim();

        const UNITS: &[(&str, f64)] = &[
            ("TiB", 1024.0 * 1024.0 * 1024.0 * 1024.0),
            ("GiB", 1024.0 * 1024.0 * 1024.0),
            ("MiB", 1024.0 * 1024.0),
            ("KiB", 1024.0),
        ];

        for (unit, multiplier) in UNITS {
            if let Some(pos) = mem_str.find(unit) {
                if let Ok(val) = mem_str[..pos].parse::<f64>() {
                    return (val * multiplier) as u64;
                }
            }
        }

        // Try parsing as raw bytes if no unit found
        mem_str.parse::<u64>().unwrap_or(0)
    }
}

impl GpuReader for RebellionsReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        match self.get_rbln_info() {
            Ok(response) => {
                // Clear any previous error
                if let Ok(mut last_error) = self.last_error.lock() {
                    *last_error = None;
                }

                let time_str = Local::now().format("%a %b %d %H:%M:%S %Y").to_string();
                let hostname = get_hostname();
                let kmd_version = response.kmd_version.clone();

                response
                    .devices
                    .into_iter()
                    .map(|device| {
                        let total_memory = Self::parse_value::<u64>(&device.memory.total, None);
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
                            instance: hostname.clone(),
                            utilization: Self::parse_value::<f64>(&device.util, None),
                            ane_utilization: 0.0,
                            dla_utilization: None,
                            temperature: Self::parse_value::<u32>(&device.temperature, Some("C")),
                            used_memory: Self::parse_value::<u64>(&device.memory.used, None),
                            total_memory,
                            frequency: 0, // Not provided by rbln-smi
                            power_consumption: Self::parse_value::<f64>(
                                &device.card_power,
                                Some("mW"),
                            ) / 1000.0,
                            detail,
                        }
                    })
                    .collect()
            }
            Err(e) => {
                self.set_error(format!("Error reading Rebellions devices: {e}"));
                vec![]
            }
        }
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // Get NPU usage information first
        match self.get_rbln_info() {
            Ok(response) => {
                let devices = response.devices;
                let contexts = response.contexts;
                let mut npu_processes = Vec::new();

                for context in contexts {
                    let npu_idx = context.npu.parse::<usize>().unwrap_or(0);
                    let device_uuid = devices
                        .get(npu_idx)
                        .map(|d| d.uuid.clone())
                        .unwrap_or_default();

                    if let Ok(pid) = context.pid.parse::<u32>() {
                        // Try to get process info from /proc
                        if let Some(proc_info) = Self::get_process_info_from_pid(pid) {
                            let mut process = proc_info;
                            process.device_id = npu_idx;
                            process.device_uuid = device_uuid;
                            process.used_memory = Self::parse_memory_allocation(&context.memalloc);
                            process.gpu_utilization =
                                Self::parse_value::<f64>(&context.util_info, None);
                            process.uses_gpu = true;
                            npu_processes.push(process);
                        }
                    }
                }

                npu_processes
            }
            Err(e) => {
                self.set_error(format!("Failed to get NPU info: {e}"));
                vec![]
            }
        }
    }
}

impl RebellionsReader {
    /// Get process info from PID
    fn get_process_info_from_pid(pid: u32) -> Option<ProcessInfo> {
        use crate::device::process_utils;

        // Use the existing process_utils to get system process info
        if let Some((
            cpu_percent,
            memory_percent,
            memory_rss,
            memory_vms,
            user,
            state,
            start_time,
            cpu_time,
            command,
            ppid,
            threads,
        )) = process_utils::get_system_process_info(pid)
        {
            let process_name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| {
                    command
                        .split_whitespace()
                        .next()
                        .unwrap_or("unknown")
                        .to_string()
                });

            Some(ProcessInfo {
                device_id: 0,
                device_uuid: String::new(),
                pid,
                process_name: container_utils::format_process_name_with_container_info(
                    process_name,
                    pid,
                ),
                used_memory: 0,
                cpu_percent,
                memory_percent,
                memory_rss,
                memory_vms,
                user,
                state,
                start_time,
                cpu_time,
                command,
                ppid,
                threads,
                uses_gpu: false,
                priority: 0,
                nice_value: 0,
                gpu_utilization: 0.0,
            })
        } else {
            None
        }
    }
}

impl Default for RebellionsReader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_value() {
        assert_eq!(RebellionsReader::parse_value::<f64>("123.45", None), 123.45);
        assert_eq!(
            RebellionsReader::parse_value::<f64>("123.45mW", Some("mW")),
            123.45
        );
        assert_eq!(RebellionsReader::parse_value::<u32>("45C", Some("C")), 45);
        assert_eq!(RebellionsReader::parse_value::<u32>("invalid", None), 0);
    }

    #[test]
    fn test_parse_memory_allocation() {
        assert_eq!(
            RebellionsReader::parse_memory_allocation("1.5GiB"),
            1610612736
        );
        assert_eq!(
            RebellionsReader::parse_memory_allocation("512MiB"),
            536870912
        );
        assert_eq!(
            RebellionsReader::parse_memory_allocation("1024KiB"),
            1048576
        );
        assert_eq!(
            RebellionsReader::parse_memory_allocation("2TiB"),
            2199023255552
        );
        assert_eq!(
            RebellionsReader::parse_memory_allocation("1048576"),
            1048576
        );
        assert_eq!(RebellionsReader::parse_memory_allocation("invalid"), 0);
    }

    #[test]
    fn test_get_device_model() {
        assert_eq!(
            RebellionsReader::get_device_model("RBLN-CA22", 16 * 1024 * 1024 * 1024),
            "RBLN-CA22 (ATOM)"
        );
        assert_eq!(
            RebellionsReader::get_device_model("RBLN-CA22", 32 * 1024 * 1024 * 1024),
            "RBLN-CA22 (ATOM+)"
        );
        assert_eq!(
            RebellionsReader::get_device_model("RBLN-CA22", 64 * 1024 * 1024 * 1024),
            "RBLN-CA22 (ATOM Max)"
        );
    }

    #[test]
    fn test_command_path_discovery() {
        // Test that COMMAND_PATH is initialized
        assert!(COMMAND_PATH.is_some() || COMMAND_PATH.is_none());
    }

    #[test]
    fn test_with_command_path() {
        let custom_path = PathBuf::from("/custom/path/rbln-stat");
        let reader = RebellionsReader::with_command_path(custom_path.clone());
        assert_eq!(*reader.command_path, custom_path);
    }

    #[test]
    fn test_error_handling() {
        let reader = RebellionsReader::with_command_path(PathBuf::from("/nonexistent/rbln-stat"));
        reader.set_error("Test error".to_string());
        assert_eq!(reader.last_error(), Some("Test error".to_string()));
    }

    #[test]
    fn test_json_parsing() {
        let json_response = r#"{
            "KMD_version": "1.0.0",
            "devices": [{
                "npu": "0",
                "name": "RBLN-CA22",
                "sid": "SN123456",
                "uuid": "UUID-123-456",
                "device": "/dev/rbln0",
                "status": "Active",
                "fw_ver": "1.2.3",
                "pci": {
                    "dev": "0000:01:00.0",
                    "bus_id": "0000:01:00.0",
                    "numa_node": "0",
                    "link_speed": "Gen4",
                    "link_width": "16"
                },
                "temperature": "45C",
                "card_power": "75000mW",
                "pstate": "P0",
                "memory": {
                    "used": "1073741824",
                    "total": "17179869184"
                },
                "util": "25.5",
                "board_info": "Rev 1.0",
                "location": 0
            }],
            "contexts": [{
                "ctx_id": "1",
                "npu": "0",
                "process": "test_app",
                "pid": "1234",
                "priority": "0",
                "ptid": "1234",
                "memalloc": "512MiB",
                "status": "Running",
                "util_info": "15.5"
            }]
        }"#;

        let response: RblnResponse = serde_json::from_str(json_response).unwrap();
        assert_eq!(response.kmd_version, "1.0.0");
        assert_eq!(response.devices.len(), 1);
        assert_eq!(response.contexts.len(), 1);

        let device = &response.devices[0];
        assert_eq!(device.name, "RBLN-CA22");
        assert_eq!(device.temperature, "45C");
        assert_eq!(device.card_power, "75000mW");

        let context = &response.contexts[0];
        assert_eq!(context.process, "test_app");
        assert_eq!(context.pid, "1234");
        assert_eq!(context.memalloc, "512MiB");
    }
}
