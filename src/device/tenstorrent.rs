use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use std::collections::HashMap;
use std::process::Command;

/// Collection method for Tenstorrent NPU metrics
#[derive(Debug, Clone, Copy)]
pub enum CollectionMethod {
    /// Use tt-smi command-line tool
    TtSmi,
    /// Read directly from device files in /dev
    DeviceFile,
}

/// Configuration for Tenstorrent reader
pub struct TenstorrentConfig {
    /// Primary method to use for collecting metrics
    pub primary_method: CollectionMethod,
    /// Fallback method if primary fails
    pub fallback_method: Option<CollectionMethod>,
}

impl Default for TenstorrentConfig {
    fn default() -> Self {
        Self {
            primary_method: CollectionMethod::TtSmi,
            fallback_method: Some(CollectionMethod::DeviceFile),
        }
    }
}

pub struct TenstorrentReader {
    config: TenstorrentConfig,
}

impl TenstorrentReader {
    pub fn new() -> Self {
        Self::with_config(TenstorrentConfig::default())
    }

    pub fn with_config(config: TenstorrentConfig) -> Self {
        TenstorrentReader { config }
    }

    /// Extract base device name from Tenstorrent device string
    /// e.g., "wh0" or similar patterns
    #[allow(dead_code)]
    fn get_base_device_name(device: &str) -> String {
        device.to_string()
    }

    /// Collect NPU info using the configured method with fallback
    fn collect_npu_info(&self) -> Vec<GpuInfo> {
        // Try primary method first
        let mut result = match self.config.primary_method {
            CollectionMethod::TtSmi => self.collect_via_tt_smi(),
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
                    CollectionMethod::TtSmi => self.collect_via_tt_smi(),
                    CollectionMethod::DeviceFile => self.collect_via_device_files(),
                };
            }
        }

        result
    }

    /// Collect NPU information using tt-smi
    fn collect_via_tt_smi(&self) -> Vec<GpuInfo> {
        // Try tt-smi first
        match Command::new("tt-smi").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    return self.parse_tt_smi_output(&output_str);
                } else {
                    eprintln!(
                        "tt-smi command failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => {
                eprintln!("Failed to execute tt-smi: {e}");
            }
        }

        // If tt-smi fails, try tensix-stat as alternative
        match Command::new("tensix-stat").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    self.parse_tensix_stat_output(&output_str)
                } else {
                    eprintln!(
                        "tensix-stat command failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    vec![]
                }
            }
            Err(e) => {
                eprintln!("Failed to execute tensix-stat: {e}");
                vec![]
            }
        }
    }

    /// Collect NPU information by reading device files
    fn collect_via_device_files(&self) -> Vec<GpuInfo> {
        // TODO: Implement device file reading
        // This will read from /dev/tenstorrent* or similar device files
        eprintln!("Device file collection not yet implemented for Tenstorrent");
        vec![]
    }

    /// Parse tt-smi output
    fn parse_tt_smi_output(&self, _output: &str) -> Vec<GpuInfo> {
        // TODO: Parse tt-smi output to extract NPU information
        // This will be implemented once we know the exact output format
        let hostname = get_hostname();
        let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        // Placeholder implementation
        // Real implementation would parse the actual output
        vec![GpuInfo {
            uuid: "TT-PLACEHOLDER-UUID".to_string(),
            time,
            name: "Tenstorrent Wormhole".to_string(),
            device_type: "NPU".to_string(),
            hostname: hostname.clone(),
            instance: "wh0".to_string(),
            utilization: 0.0,
            ane_utilization: 0.0,
            dla_utilization: None,
            temperature: 0,
            used_memory: 0,
            total_memory: 0,
            frequency: 0,
            power_consumption: 0.0,
            detail: HashMap::new(),
        }]
    }

    /// Parse tensix-stat output
    fn parse_tensix_stat_output(&self, _output: &str) -> Vec<GpuInfo> {
        // TODO: Parse tensix-stat output to extract NPU information
        // This will be implemented once we know the exact output format
        let hostname = get_hostname();
        let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        // Placeholder implementation
        vec![GpuInfo {
            uuid: "TT-PLACEHOLDER-UUID".to_string(),
            time,
            name: "Tenstorrent Wormhole".to_string(),
            device_type: "NPU".to_string(),
            hostname: hostname.clone(),
            instance: "wh0".to_string(),
            utilization: 0.0,
            ane_utilization: 0.0,
            dla_utilization: None,
            temperature: 0,
            used_memory: 0,
            total_memory: 0,
            frequency: 0,
            power_consumption: 0.0,
            detail: HashMap::new(),
        }]
    }

    /// Get processes using Tenstorrent NPUs via tt-smi
    fn get_processes_via_tt_smi(&self) -> Vec<ProcessInfo> {
        // TODO: Get processes using Tenstorrent NPUs via tt-smi
        vec![]
    }

    /// Get processes using Tenstorrent NPUs via device files
    fn get_processes_via_device_files(&self) -> Vec<ProcessInfo> {
        // TODO: Get processes using Tenstorrent NPUs via /dev
        vec![]
    }

    /// Collect process info using the configured method with fallback
    fn collect_process_info(&self) -> Vec<ProcessInfo> {
        // Try primary method first
        let mut result = match self.config.primary_method {
            CollectionMethod::TtSmi => self.get_processes_via_tt_smi(),
            CollectionMethod::DeviceFile => self.get_processes_via_device_files(),
        };

        // If primary method failed and we have a fallback, try it
        if result.is_empty() {
            if let Some(fallback) = self.config.fallback_method {
                result = match fallback {
                    CollectionMethod::TtSmi => self.get_processes_via_tt_smi(),
                    CollectionMethod::DeviceFile => self.get_processes_via_device_files(),
                };
            }
        }

        result
    }
}

impl GpuReader for TenstorrentReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        self.collect_npu_info()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.collect_process_info()
    }
}
