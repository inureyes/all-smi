use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// Global status for error messages
static REBELLIONS_STATUS: Mutex<Option<String>> = Mutex::new(None);

lazy_static! {
    static ref PROCESS_INFO_CACHE: Mutex<ProcessInfoCache> = Mutex::new(ProcessInfoCache::new());
}

/// Cache for process information to avoid excessive /proc queries
struct ProcessInfoCache {
    cache: HashMap<u32, CachedProcessInfo>,
    last_cleanup: Instant,
}

struct CachedProcessInfo {
    process_name: String,
    user: String,
    command: String,
    last_updated: Instant,
}

impl ProcessInfoCache {
    fn new() -> Self {
        ProcessInfoCache {
            cache: HashMap::new(),
            last_cleanup: Instant::now(),
        }
    }

    fn get_or_update(&mut self, pid: u32) -> (String, String, String) {
        // Clean up stale entries every 60 seconds
        if self.last_cleanup.elapsed() > Duration::from_secs(60) {
            self.cleanup_stale_entries();
            self.last_cleanup = Instant::now();
        }

        // Check if we have cached info that's less than 30 seconds old
        if let Some(cached) = self.cache.get(&pid) {
            if cached.last_updated.elapsed() < Duration::from_secs(30) {
                return (
                    cached.process_name.clone(),
                    cached.user.clone(),
                    cached.command.clone(),
                );
            }
        }

        // Fetch fresh info
        let process_name = Self::fetch_process_name(pid).unwrap_or_else(|| "Unknown".to_string());
        let user = Self::fetch_process_user(pid).unwrap_or_else(|| "N/A".to_string());
        let command = Self::fetch_process_command(pid).unwrap_or_else(|| process_name.clone());

        // Update cache
        self.cache.insert(
            pid,
            CachedProcessInfo {
                process_name: process_name.clone(),
                user: user.clone(),
                command: command.clone(),
                last_updated: Instant::now(),
            },
        );

        (process_name, user, command)
    }

    fn cleanup_stale_entries(&mut self) {
        let stale_threshold = Duration::from_secs(300); // 5 minutes
        self.cache
            .retain(|_, info| info.last_updated.elapsed() < stale_threshold);
    }

    fn fetch_process_name(pid: u32) -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()
            .map(|s| s.trim().to_string())
    }

    fn fetch_process_user(pid: u32) -> Option<String> {
        use std::os::unix::fs::MetadataExt;

        std::fs::metadata(format!("/proc/{pid}"))
            .ok()
            .and_then(|metadata| {
                let uid = metadata.uid();
                Self::fetch_username_by_uid(uid)
            })
    }

    fn fetch_username_by_uid(uid: u32) -> Option<String> {
        Command::new("id")
            .args(["-nu", &uid.to_string()])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }

    fn fetch_process_command(pid: u32) -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
            .ok()
            .map(|cmdline| cmdline.replace('\0', " ").trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

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
    process: String,
    pid: String,
    #[allow(dead_code)]
    priority: String,
    #[allow(dead_code)]
    ptid: String,
    memalloc: String,
    #[allow(dead_code)]
    status: String,
    #[allow(dead_code)]
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
        match self.get_rbln_info() {
            Ok(response) => {
                let devices = response.devices;

                response
                    .contexts
                    .into_iter()
                    .filter_map(|context| {
                        // Parse NPU index
                        let npu_idx = context.npu.parse::<usize>().unwrap_or(0);

                        // Find the device that matches this NPU index
                        let device = devices.get(npu_idx)?;
                        let device_uuid = device.uuid.clone();

                        // Parse memory allocation (e.g., "66.0MiB" -> bytes)
                        let memory_used = Self::parse_memory_allocation(&context.memalloc);

                        // Parse PID
                        let pid = context.pid.parse::<u32>().unwrap_or(0);

                        // Get cached process info or fetch if needed
                        let (process_name, user, command) = if context.process == "N/A" {
                            // Need to fetch from system - use cache
                            if let Ok(mut cache) = PROCESS_INFO_CACHE.lock() {
                                cache.get_or_update(pid)
                            } else {
                                ("Unknown".to_string(), "N/A".to_string(), "N/A".to_string())
                            }
                        } else {
                            // Process name provided by API, but still get user and command from cache
                            let provided_name = context.process.clone();
                            if let Ok(mut cache) = PROCESS_INFO_CACHE.lock() {
                                let (_, user, command) = cache.get_or_update(pid);
                                (provided_name, user, command)
                            } else {
                                (provided_name.clone(), "N/A".to_string(), provided_name)
                            }
                        };

                        Some(ProcessInfo {
                            device_id: npu_idx,
                            device_uuid,
                            pid,
                            process_name,
                            used_memory: memory_used,
                            cpu_percent: 0.0, // Not provided by rbln-stat/rbln-smi
                            memory_percent: 0.0, // Not provided by rbln-stat/rbln-smi
                            memory_rss: 0,    // Not provided by rbln-stat/rbln-smi
                            memory_vms: 0,    // Not provided by rbln-stat/rbln-smi
                            user,
                            state: if context.status == "run" { "R" } else { "S" }.to_string(),
                            start_time: "N/A".to_string(), // Not provided by rbln-stat/rbln-smi
                            cpu_time: 0,                   // Not provided by rbln-stat/rbln-smi
                            command,
                            ppid: 0,        // Not provided by rbln-stat/rbln-smi
                            threads: 0,     // Not provided by rbln-stat/rbln-smi
                            uses_gpu: true, // Using NPU, so yes
                            priority: 0,    // Not provided by rbln-stat/rbln-smi
                            nice_value: 0,  // Not provided by rbln-stat/rbln-smi
                            gpu_utilization: context.util_info.parse::<f64>().unwrap_or(0.0),
                        })
                    })
                    .collect()
            }
            Err(e) => {
                Self::set_status(format!("Failed to get process info: {e}"));
                vec![]
            }
        }
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
