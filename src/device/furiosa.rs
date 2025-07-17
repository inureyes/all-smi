use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use std::process::Command;

/// Collection method for Furiosa NPU metrics
#[derive(Debug, Clone, Copy)]
pub enum CollectionMethod {
    /// Use furiosactl command-line tool
    Furiosactl,
    /// Read directly from device files in /dev
    DeviceFile,
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
        match Command::new("furiosactl").arg("info").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    self.parse_furiosactl_output(&output_str)
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

    /// Parse furiosactl output to extract NPU information
    fn parse_furiosactl_output(&self, _output: &str) -> Vec<GpuInfo> {
        // TODO: Parse furiosactl output to extract NPU information
        // This will be implemented once we know the exact output format
        vec![]
    }

    /// Get processes using Furiosa NPUs via furiosactl
    fn get_furiosa_processes_via_furiosactl(&self) -> Vec<ProcessInfo> {
        // TODO: Get processes using Furiosa NPUs via furiosactl
        vec![]
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
