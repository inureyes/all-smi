use crate::device::{container_utils, process_utils, GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
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
    process: String,
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

pub struct RebellionsReader {
    command_path: String,
}

impl RebellionsReader {
    pub fn new() -> Self {
        // Try to find rbln-stat first, then fall back to rbln-smi
        let command_path = if std::path::Path::new("/usr/local/bin/rbln-stat").exists() {
            "/usr/local/bin/rbln-stat".to_string()
        } else if std::path::Path::new("/usr/bin/rbln-stat").exists() {
            "/usr/bin/rbln-stat".to_string()
        } else if Self::is_command_available("rbln-stat") {
            "rbln-stat".to_string()
        } else if std::path::Path::new("/usr/local/bin/rbln-smi").exists() {
            "/usr/local/bin/rbln-smi".to_string()
        } else if std::path::Path::new("/usr/bin/rbln-smi").exists() {
            "/usr/bin/rbln-smi".to_string()
        } else {
            // Final fallback to PATH lookup
            "rbln-smi".to_string()
        };

        RebellionsReader { command_path }
    }

    /// Check if a command is available in PATH
    fn is_command_available(cmd: &str) -> bool {
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
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

    /// Execute rbln-stat/rbln-smi command and parse the output
    fn get_rbln_info(&self) -> Result<RblnResponse, String> {
        let cmd_name = std::path::Path::new(&self.command_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rbln-stat/rbln-smi");

        let output = Command::new(&self.command_path)
            .arg("-j")
            .output()
            .map_err(|e| format!("Failed to execute {cmd_name}: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("{cmd_name} failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse JSON response
        let response: RblnResponse = serde_json::from_str(&stdout)
            .map_err(|e| format!("Failed to parse {cmd_name} JSON: {e}"))?;

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

impl RebellionsReader {
    /// Parse memory allocation string (e.g., "66.0MiB") to bytes
    fn parse_memory_allocation(mem_str: &str) -> u64 {
        let mem_str = mem_str.trim();
        if let Some(mib_pos) = mem_str.find("MiB") {
            if let Ok(mib_val) = mem_str[..mib_pos].parse::<f64>() {
                return (mib_val * 1024.0 * 1024.0) as u64;
            }
        } else if let Some(gib_pos) = mem_str.find("GiB") {
            if let Ok(gib_val) = mem_str[..gib_pos].parse::<f64>() {
                return (gib_val * 1024.0 * 1024.0 * 1024.0) as u64;
            }
        } else if let Some(kib_pos) = mem_str.find("KiB") {
            if let Ok(kib_val) = mem_str[..kib_pos].parse::<f64>() {
                return (kib_val * 1024.0) as u64;
            }
        }
        0
    }

    /// Get all processes from the system
    fn get_all_processes() -> Vec<ProcessInfo> {
        let mut processes = Vec::new();

        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name() {
                    if let Some(pid_str) = filename.to_str() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            if let Some(proc_info) = Self::create_process_info(pid) {
                                processes.push(proc_info);
                            }
                        }
                    }
                }
            }
        }

        processes
    }

    /// Create ProcessInfo from pid using process_utils
    fn create_process_info(pid: u32) -> Option<ProcessInfo> {
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
            // Extract process name from command or use comm file
            let mut process_name = fs::read_to_string(format!("/proc/{pid}/comm"))
                .ok()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| {
                    command
                        .split_whitespace()
                        .next()
                        .unwrap_or("unknown")
                        .to_string()
                });

            // Add container indicators to process name
            process_name =
                container_utils::format_process_name_with_container_info(process_name, pid);

            Some(ProcessInfo {
                device_id: 0,
                device_uuid: String::new(),
                pid,
                process_name,
                used_memory: 0, // Will be filled from NPU data
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
                uses_gpu: false, // Will be updated from NPU data
                priority: 0,
                nice_value: 0,
                gpu_utilization: 0.0, // Will be filled from NPU data
            })
        } else {
            None
        }
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
                    .map(|device| {
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
                            instance: hostname.clone(),
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
        // First, get all processes from the system
        let mut all_processes = Self::get_all_processes();

        // Then get NPU usage information
        if let Ok(response) = self.get_rbln_info() {
            let devices = response.devices;

            // Create a map to aggregate NPU usage by PID
            type NpuUsageData = (u64, f64, Vec<(usize, String)>);
            let mut npu_usage_map: HashMap<u32, NpuUsageData> = HashMap::new();

            let running_in_container = container_utils::is_running_in_container();

            // Aggregate NPU contexts by PID
            for context in response.contexts {
                let reported_pid = match context.pid.parse::<u32>() {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let pid = if running_in_container {
                    // We're running inside a container
                    // The NPU driver likely reports host PIDs, we need container PIDs
                    match container_utils::map_host_to_container_pid(reported_pid) {
                        Some(container_pid) => {
                            if reported_pid != container_pid {
                                eprintln!(
                                    "Mapped host PID {reported_pid} to container PID {container_pid}"
                                );
                            }
                            container_pid
                        }
                        None => {
                            eprintln!(
                                "Warning: Could not map host PID {reported_pid} to container namespace"
                            );
                            continue;
                        }
                    }
                } else {
                    // We're running on the host
                    // Check if this PID exists in /proc
                    if !std::path::Path::new(&format!("/proc/{reported_pid}")).exists() {
                        // This might be a container PID reported by the NPU driver
                        // Try to find the corresponding host PID
                        if let Some(host_pid) =
                            container_utils::find_host_pid_from_container_pid(reported_pid, None)
                        {
                            eprintln!("Mapped container PID {reported_pid} to host PID {host_pid}");
                            host_pid
                        } else {
                            // Can't find host PID, skip this context
                            eprintln!(
                                "Warning: PID {reported_pid} not found in /proc, might be a container PID"
                            );
                            continue;
                        }
                    } else {
                        reported_pid
                    }
                };

                let npu_idx = context.npu.parse::<usize>().unwrap_or(0);
                let device_uuid = devices
                    .get(npu_idx)
                    .map(|d| d.uuid.clone())
                    .unwrap_or_default();
                let memory_used = Self::parse_memory_allocation(&context.memalloc);
                let gpu_util = context.util_info.parse::<f64>().unwrap_or(0.0);

                let entry = npu_usage_map.entry(pid).or_insert((0, 0.0, Vec::new()));
                entry.0 += memory_used; // Sum memory usage
                entry.1 = entry.1.max(gpu_util); // Take max utilization
                entry.2.push((npu_idx, device_uuid)); // Track all devices used
            }

            // Update processes with NPU usage information
            for process in &mut all_processes {
                if let Some((total_memory, gpu_util, devices_used)) =
                    npu_usage_map.get(&process.pid)
                {
                    process.uses_gpu = true;
                    process.used_memory = *total_memory;
                    process.gpu_utilization = *gpu_util;

                    // Use the first device for device_id and device_uuid
                    if let Some((device_id, device_uuid)) = devices_used.first() {
                        process.device_id = *device_id;
                        process.device_uuid = device_uuid.clone();
                    }
                }
            }
        } else if let Err(e) = self.get_rbln_info() {
            Self::set_status(format!("Failed to get NPU info: {e}"));
        }

        all_processes
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
