use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

// Import furiosa-smi-rs if available on Linux
#[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
use furiosa_smi_rs::{Device, SmiResult};

/// Collection method for Furiosa NPU metrics
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum CollectionMethod {
    /// Use furiosa-smi command-line tool
    FuriosaSmi,
    /// Use furiosa-smi-rs crate
    FuriosaSmiRs,
}

/// JSON structure for furiosa-smi info output
#[derive(Debug, Deserialize)]
struct FuriosaSmiInfoJson {
    index: String,
    arch: String,
    dev_name: String,
    device_uuid: String,
    device_sn: String,
    firmware: String,
    pert: String,
    temperature: String,
    power: String,
    core_clock: String,
    governor: String,
    pci_bdf: String,
    pci_dev: String,
}

/// JSON structure for furiosa-smi status output
#[derive(Debug, Deserialize)]
struct FuriosaSmiStatusJson {
    index: String,
    #[allow(dead_code)]
    arch: String,
    #[allow(dead_code)]
    device: String,
    #[allow(dead_code)]
    liveness: String,
    #[allow(dead_code)]
    cores: Vec<FuriosaCoreInfo>,
    pe_utilizations: Vec<FuriosaPeUtilization>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FuriosaCoreInfo {
    idx: u32,
    status: String,
}

#[derive(Debug, Deserialize)]
struct FuriosaPeUtilization {
    #[allow(dead_code)]
    pe_core: u32,
    pe_utilization: f64,
}

/// JSON structure for furiosa-smi ps output
#[derive(Debug, Deserialize)]
struct FuriosaSmiProcessJson {
    pid: u32,
    device: String,
    memory_usage: Option<u64>,
    command: String,
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
            #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
            primary_method: CollectionMethod::FuriosaSmiRs,
            #[cfg(not(all(target_os = "linux", feature = "furiosa-smi-rs")))]
            primary_method: CollectionMethod::FuriosaSmi,
            #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
            fallback_method: Some(CollectionMethod::FuriosaSmi),
            #[cfg(not(all(target_os = "linux", feature = "furiosa-smi-rs")))]
            fallback_method: None,
        }
    }
}

pub struct FuriosaReader {
    config: FuriosaConfig,
    #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
    initialized: std::cell::Cell<bool>,
}

impl FuriosaReader {
    pub fn new() -> Self {
        Self::with_config(FuriosaConfig::default())
    }

    pub fn with_config(config: FuriosaConfig) -> Self {
        FuriosaReader {
            config,
            #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
            initialized: std::cell::Cell::new(false),
        }
    }

    #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
    fn ensure_initialized(&self) -> SmiResult<()> {
        if !self.initialized.get() {
            furiosa_smi_rs::init()?;
            self.initialized.set(true);
        }
        Ok(())
    }

    /// Get NPU status including utilization
    fn get_npu_status(&self) -> Option<Vec<FuriosaSmiStatusJson>> {
        match Command::new("furiosa-smi")
            .args(["status", "--format", "json"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    match serde_json::from_str::<Vec<FuriosaSmiStatusJson>>(&output_str) {
                        Ok(status) => Some(status),
                        Err(e) => {
                            eprintln!("Failed to parse furiosa-smi status JSON: {e}");
                            None
                        }
                    }
                } else {
                    eprintln!(
                        "furiosa-smi status failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    None
                }
            }
            Err(e) => {
                eprintln!("Failed to execute furiosa-smi status: {e}");
                None
            }
        }
    }

    /// Calculate average PE utilization for a device
    fn calculate_avg_utilization(pe_utilizations: &[FuriosaPeUtilization]) -> f64 {
        if pe_utilizations.is_empty() {
            return 0.0;
        }
        let sum: f64 = pe_utilizations.iter().map(|pe| pe.pe_utilization).sum();
        sum / pe_utilizations.len() as f64
    }

    /// Collect NPU info using the configured method with fallback
    fn collect_npu_info(&self) -> Vec<GpuInfo> {
        // Try primary method first
        let mut result = match self.config.primary_method {
            CollectionMethod::FuriosaSmi => self.collect_via_furiosa_smi(),
            CollectionMethod::FuriosaSmiRs => self.collect_via_furiosa_smi_rs(),
        };

        // If primary method failed and we have a fallback, try it
        if result.is_empty() {
            if let Some(fallback) = self.config.fallback_method {
                eprintln!(
                    "Primary method {:?} failed, trying fallback {:?}",
                    self.config.primary_method, fallback
                );
                result = match fallback {
                    CollectionMethod::FuriosaSmi => self.collect_via_furiosa_smi(),
                    CollectionMethod::FuriosaSmiRs => self.collect_via_furiosa_smi_rs(),
                };
            }
        }

        result
    }

    /// Collect NPU information using furiosa-smi-rs crate
    fn collect_via_furiosa_smi_rs(&self) -> Vec<GpuInfo> {
        #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
        {
            // Initialize library if needed
            if let Err(e) = self.ensure_initialized() {
                eprintln!("Failed to initialize furiosa-smi-rs: {e}");
                return vec![];
            }

            // Get driver version
            let driver_version = match furiosa_smi_rs::driver_info() {
                Ok(version) => version.to_string(),
                Err(_) => "Unknown".to_string(),
            };

            // Get all NPU devices
            match furiosa_smi_rs::list_devices() {
                Ok(devices) => {
                    let hostname = get_hostname();
                    let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                    let mut gpu_infos = Vec::new();

                    for device in devices {
                        if let Some(mut gpu_info) =
                            self.device_to_gpu_info(&device, &hostname, &time)
                        {
                            // Add driver version to detail
                            gpu_info
                                .detail
                                .insert("driver_version".to_string(), driver_version.clone());
                            gpu_infos.push(gpu_info);
                        }
                    }

                    gpu_infos
                }
                Err(e) => {
                    eprintln!("Failed to list Furiosa devices: {e}");
                    vec![]
                }
            }
        }
        #[cfg(not(all(target_os = "linux", feature = "furiosa-smi-rs")))]
        {
            eprintln!("furiosa-smi-rs crate support not compiled in");
            vec![]
        }
    }

    #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
    fn device_to_gpu_info(&self, device: &Device, hostname: &str, time: &str) -> Option<GpuInfo> {
        // Get device information
        let device_info = device.device_info().ok()?;
        let index = device_info.index();
        let name = format!("npu{}", index);
        let uuid = device_info.uuid();
        let arch = format!("{:?}", device_info.arch());

        // Get device details
        let mut detail = HashMap::new();
        detail.insert("device_index".to_string(), index.to_string());
        detail.insert("architecture".to_string(), arch.clone());
        detail.insert("core_count".to_string(), device_info.core_num().to_string());
        detail.insert("serial_number".to_string(), device_info.serial());
        detail.insert("device_name".to_string(), device_info.name());
        detail.insert("pci_bdf".to_string(), device_info.bdf());
        detail.insert("numa_node".to_string(), device_info.numa_node().to_string());
        detail.insert("major".to_string(), device_info.major().to_string());
        detail.insert("minor".to_string(), device_info.minor().to_string());

        // Get firmware and pert versions
        let firmware_ver = device_info.firmware_version();
        detail.insert("firmware_version".to_string(), firmware_ver.to_string());

        let pert_ver = device_info.pert_version();
        detail.insert("pert_version".to_string(), pert_ver.to_string());

        // Get temperature
        let temperature = match device.device_temperature() {
            Ok(temp) => temp.soc_peak() as u32,
            Err(_) => 0,
        };

        // Add ambient temperature if available
        if let Ok(temp) = device.device_temperature() {
            detail.insert(
                "ambient_temperature".to_string(),
                format!("{:.1}", temp.ambient()),
            );
        }

        // Get power consumption
        let power = device.power_consumption().unwrap_or(0.0);

        // Get memory frequency
        if let Ok(mem_freq) = device.memory_frequency() {
            detail.insert(
                "memory_frequency_mhz".to_string(),
                mem_freq.frequency().to_string(),
            );
        }

        // Get liveness status
        if let Ok(liveness) = device.liveness() {
            detail.insert("liveness".to_string(), liveness.to_string());
        }

        // Get device files information
        if let Ok(device_files) = device.device_files() {
            let file_paths: Vec<String> =
                device_files.iter().map(|f| f.path().to_string()).collect();
            if !file_paths.is_empty() {
                detail.insert("device_files".to_string(), file_paths.join(", "));
            }
        }

        // Get core status
        if let Ok(core_status) = device.core_status() {
            let pe_statuses: Vec<String> = core_status
                .pe_status()
                .iter()
                .map(|pe| format!("core{}:{:?}", pe.core(), pe.status()))
                .collect();
            detail.insert("core_status".to_string(), pe_statuses.join(", "));
        }

        // Get frequency
        let frequency = match device.core_frequency() {
            Ok(core_freq) => {
                // Get average frequency from all PE cores
                let pe_freqs = core_freq.pe_frequency();
                if pe_freqs.is_empty() {
                    1000
                } else {
                    // Calculate average frequency across all PE cores
                    let sum: u32 = pe_freqs.iter().map(|pe| pe.frequency()).sum();
                    sum / pe_freqs.len() as u32
                }
            }
            Err(_) => 1000,
        };

        // Get utilization from core utilization with additional details
        let utilization = match device.core_utilization() {
            Ok(core_util) => {
                let pe_utils = core_util.pe_utilization();
                if pe_utils.is_empty() {
                    0.0
                } else {
                    // Store per-core utilization details
                    let per_core_utils: Vec<String> = pe_utils
                        .iter()
                        .map(|pe| format!("core{}:{:.1}%", pe.core(), pe.pe_usage_percentage()))
                        .collect();
                    detail.insert(
                        "per_core_utilization".to_string(),
                        per_core_utils.join(", "),
                    );

                    // Calculate average utilization
                    let sum: f64 = pe_utils
                        .iter()
                        .map(|pe| pe.pe_usage_percentage() as f64)
                        .sum();
                    sum / pe_utils.len() as f64
                }
            }
            Err(_) => 0.0,
        };

        // Get performance counter information
        if let Ok(perf_counter) = device.device_performance_counter() {
            let pe_counters = perf_counter.pe_performance_counters();
            if !pe_counters.is_empty() {
                // Get first PE's performance info as sample
                if let Some(first_pe) = pe_counters.first() {
                    detail.insert(
                        "task_execution_cycles".to_string(),
                        first_pe.task_execution_cycle().to_string(),
                    );
                    detail.insert(
                        "cycle_count".to_string(),
                        first_pe.cycle_count().to_string(),
                    );
                }
            }
        }

        // Get governor profile
        if let Ok(governor) = device.governor_profile() {
            detail.insert("governor".to_string(), format!("{:?}", governor));
        }

        // Note about memory
        detail.insert("memory_type".to_string(), "HBM3".to_string());
        detail.insert("memory_capacity".to_string(), "48GB".to_string());
        detail.insert("memory_bandwidth".to_string(), "1.5TB/s".to_string());
        detail.insert("on_chip_sram".to_string(), "256MB".to_string());

        Some(GpuInfo {
            uuid,
            time: time.to_string(),
            name: format!("Furiosa {}", arch.to_uppercase()),
            device_type: "NPU".to_string(),
            host_id: hostname.to_string(),
            hostname: hostname.to_string(),
            instance: name,
            utilization,
            ane_utilization: 0.0,
            dla_utilization: None,
            temperature,
            used_memory: 0, // TODO: Get memory usage from device
            total_memory: 48 * 1024 * 1024 * 1024, // 48GB HBM3
            frequency: frequency as u32,
            power_consumption: power,
            detail,
        })
    }

    /// Collect NPU information using furiosa-smi
    fn collect_via_furiosa_smi(&self) -> Vec<GpuInfo> {
        // First get status data for utilization
        let status_data = self.get_npu_status();

        // Then get info data
        match Command::new("furiosa-smi")
            .args(["info", "--format", "json"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    self.parse_furiosa_smi_info_json(&output_str, status_data.as_ref())
                } else {
                    eprintln!(
                        "furiosa-smi info failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    vec![]
                }
            }
            Err(e) => {
                eprintln!("Failed to execute furiosa-smi info: {e}");
                vec![]
            }
        }
    }

    /// Parse furiosa-smi info JSON output
    fn parse_furiosa_smi_info_json(
        &self,
        output: &str,
        status_data: Option<&Vec<FuriosaSmiStatusJson>>,
    ) -> Vec<GpuInfo> {
        match serde_json::from_str::<Vec<FuriosaSmiInfoJson>>(output) {
            Ok(devices) => {
                let hostname = get_hostname();
                let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

                devices
                    .into_iter()
                    .map(|device| {
                        let mut detail = HashMap::new();
                        detail.insert("serial_number".to_string(), device.device_sn);
                        detail.insert("firmware".to_string(), device.firmware);
                        detail.insert("pert".to_string(), device.pert);
                        detail.insert("pci_address".to_string(), device.pci_bdf);
                        detail.insert("pci_device".to_string(), device.pci_dev);
                        detail.insert("governor".to_string(), device.governor);
                        detail.insert("architecture".to_string(), device.arch.clone());
                        detail.insert("memory_type".to_string(), "HBM3".to_string());
                        detail.insert("memory_capacity".to_string(), "48GB".to_string());
                        detail.insert("memory_bandwidth".to_string(), "1.5TB/s".to_string());
                        detail.insert("on_chip_sram".to_string(), "256MB".to_string());

                        // Parse temperature (remove °C suffix)
                        let temperature = device
                            .temperature
                            .trim_end_matches("°C")
                            .parse::<f64>()
                            .unwrap_or(0.0) as u32;

                        // Parse power (remove W suffix)
                        let power = device
                            .power
                            .trim_end_matches(" W")
                            .parse::<f64>()
                            .unwrap_or(0.0);

                        // Parse frequency (remove MHz suffix)
                        let frequency = device
                            .core_clock
                            .trim_end_matches(" MHz")
                            .parse::<u32>()
                            .unwrap_or(0);

                        // Get utilization from status data if available
                        let utilization = status_data
                            .and_then(|status_vec| {
                                status_vec
                                    .iter()
                                    .find(|s| s.index == device.index)
                                    .map(|s| Self::calculate_avg_utilization(&s.pe_utilizations))
                            })
                            .unwrap_or(0.0);

                        GpuInfo {
                            uuid: device.device_uuid,
                            time: time.clone(),
                            name: format!("Furiosa {}", device.arch.to_uppercase()),
                            device_type: "NPU".to_string(),
                            host_id: hostname.clone(),
                            hostname: hostname.clone(),
                            instance: device.dev_name,
                            utilization,
                            ane_utilization: 0.0,
                            dla_utilization: None,
                            temperature,
                            used_memory: 0, // TODO: Get memory usage info
                            total_memory: 48 * 1024 * 1024 * 1024, // 48GB HBM3
                            frequency,
                            power_consumption: power,
                            detail,
                        }
                    })
                    .collect()
            }
            Err(e) => {
                eprintln!("Failed to parse furiosa-smi info JSON output: {e}");
                vec![]
            }
        }
    }

    /// Get processes using Furiosa NPUs
    fn collect_process_info(&self) -> Vec<ProcessInfo> {
        // For now, only command-line method is available for process info
        self.get_furiosa_processes_via_furiosa_smi()
    }

    /// Get processes using Furiosa NPUs via furiosa-smi
    fn get_furiosa_processes_via_furiosa_smi(&self) -> Vec<ProcessInfo> {
        match Command::new("furiosa-smi")
            .args(["ps", "--format", "json"])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    self.parse_furiosa_smi_ps_json(&output_str)
                } else {
                    eprintln!(
                        "furiosa-smi ps failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    vec![]
                }
            }
            Err(e) => {
                eprintln!("Failed to execute furiosa-smi ps: {e}");
                vec![]
            }
        }
    }

    /// Parse furiosa-smi ps JSON output
    fn parse_furiosa_smi_ps_json(&self, output: &str) -> Vec<ProcessInfo> {
        match serde_json::from_str::<Vec<FuriosaSmiProcessJson>>(output) {
            Ok(processes) => {
                processes
                    .into_iter()
                    .map(|proc| {
                        // Extract process name from command
                        let process_name = proc
                            .command
                            .split_whitespace()
                            .next()
                            .and_then(|cmd| cmd.split('/').next_back())
                            .unwrap_or("unknown")
                            .to_string();

                        // Get system process info if available
                        let sys_info =
                            crate::device::process_utils::get_system_process_info(proc.pid);

                        ProcessInfo {
                            device_id: 0, // TODO: Map device name to index
                            device_uuid: proc.device.clone(),
                            pid: proc.pid,
                            process_name,
                            used_memory: proc.memory_usage.unwrap_or(0),
                            cpu_percent: sys_info.as_ref().map(|s| s.0).unwrap_or(0.0),
                            memory_percent: sys_info.as_ref().map(|s| s.1).unwrap_or(0.0),
                            memory_rss: sys_info.as_ref().map(|s| s.2).unwrap_or(0),
                            memory_vms: sys_info.as_ref().map(|s| s.3).unwrap_or(0),
                            user: sys_info.as_ref().map(|s| s.4.clone()).unwrap_or_default(),
                            state: sys_info.as_ref().map(|s| s.5.clone()).unwrap_or_default(),
                            start_time: sys_info.as_ref().map(|s| s.6.clone()).unwrap_or_default(),
                            cpu_time: sys_info.as_ref().map(|s| s.7).unwrap_or(0),
                            command: proc.command,
                            ppid: sys_info.as_ref().map(|s| s.9).unwrap_or(0),
                            threads: sys_info.as_ref().map(|s| s.10).unwrap_or(0),
                            uses_gpu: true, // Using NPU
                            priority: 0,
                            nice_value: 0,
                            gpu_utilization: 0.0, // TODO: Get from status data if per-process util is available
                        }
                    })
                    .collect()
            }
            Err(e) => {
                // Empty array is valid JSON, no need to log error
                if output.trim() != "[]" {
                    eprintln!("Failed to parse furiosa-smi ps JSON output: {e}");
                }
                vec![]
            }
        }
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
