use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use serde::Deserialize;
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

// JSON structures for parsing tt-smi output
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TtSmiOutput {
    time: String,
    #[serde(default)]
    host_info: Option<HostInfo>,
    device_info: Vec<DeviceInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HostInfo {
    #[serde(rename = "OS")]
    os: Option<String>,
    #[serde(rename = "Distro")]
    distro: Option<String>,
    #[serde(rename = "Kernel")]
    kernel: Option<String>,
    #[serde(rename = "Hostname")]
    hostname: Option<String>,
    #[serde(rename = "Platform")]
    platform: Option<String>,
    #[serde(rename = "Driver")]
    driver: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeviceInfo {
    board_info: BoardInfo,
    telemetry: Telemetry,
    #[serde(default)]
    firmwares: Option<Firmwares>,
    #[serde(default)]
    limits: Option<Limits>,
}

#[derive(Debug, Deserialize)]
struct BoardInfo {
    bus_id: String,
    board_type: String,
    board_id: String,
    coords: String,
    dram_status: String,
    dram_speed: String,
    pcie_speed: String,
    pcie_width: String,
}

#[derive(Debug, Deserialize)]
struct Telemetry {
    voltage: String,
    current: String,
    aiclk: String,
    power: String,
    asic_temperature: String,
    heartbeat: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Firmwares {
    fw_bundle_version: Option<String>,
    tt_flash_version: Option<String>,
    cm_fw: Option<String>,
    cm_fw_date: Option<String>,
    eth_fw: Option<String>,
    bm_bl_fw: Option<String>,
    bm_app_fw: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Limits {
    vdd_min: Option<String>,
    vdd_max: Option<String>,
    tdp_limit: Option<String>,
    tdc_limit: Option<String>,
    asic_fmax: Option<String>,
    therm_trip_l1_limit: Option<String>,
    thm_limit: Option<String>,
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
        // Try tt-smi with JSON snapshot mode
        match Command::new("tt-smi")
            .args(["-s", "--snapshot_no_tty"])
            .output()
        {
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
    fn parse_tt_smi_output(&self, output: &str) -> Vec<GpuInfo> {
        // Parse JSON output from tt-smi
        match serde_json::from_str::<TtSmiOutput>(output) {
            Ok(tt_output) => {
                let hostname = get_hostname();
                let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

                tt_output
                    .device_info
                    .into_iter()
                    .enumerate()
                    .map(|(idx, device)| {
                        let mut detail = HashMap::new();

                        // Extract board info
                        detail.insert(
                            "board_type".to_string(),
                            device.board_info.board_type.clone(),
                        );
                        detail.insert("board_id".to_string(), device.board_info.board_id.clone());
                        detail.insert("bus_id".to_string(), device.board_info.bus_id.clone());
                        detail.insert("coords".to_string(), device.board_info.coords.clone());
                        detail.insert(
                            "dram_status".to_string(),
                            device.board_info.dram_status.clone(),
                        );
                        detail.insert(
                            "dram_speed".to_string(),
                            device.board_info.dram_speed.clone(),
                        );
                        detail.insert(
                            "pcie_speed".to_string(),
                            format!("Gen{}", device.board_info.pcie_speed),
                        );
                        detail.insert(
                            "pcie_width".to_string(),
                            format!("x{}", device.board_info.pcie_width),
                        );

                        // Extract firmware versions
                        if let Some(ref fw) = device.firmwares {
                            if let Some(ref bundle) = fw.fw_bundle_version {
                                detail.insert("firmware".to_string(), bundle.clone());
                            }
                            if let Some(ref cm_fw) = fw.cm_fw {
                                detail.insert("cm_firmware".to_string(), cm_fw.clone());
                            }
                            if let Some(ref eth_fw) = fw.eth_fw {
                                detail.insert("eth_firmware".to_string(), eth_fw.clone());
                            }
                        }

                        // Extract power limits if available
                        if let Some(ref limits) = device.limits {
                            if let Some(ref tdp) = limits.tdp_limit {
                                detail.insert("power_limit_tdp".to_string(), tdp.clone());
                            }
                            if let Some(ref tdc) = limits.tdc_limit {
                                detail.insert("power_limit_tdc".to_string(), tdc.clone());
                            }
                            if let Some(ref thm) = limits.thm_limit {
                                detail.insert("thermal_limit".to_string(), thm.clone());
                            }
                        }

                        // Extract telemetry metrics
                        let telemetry = &device.telemetry;
                        let temperature =
                            telemetry.asic_temperature.parse::<f64>().unwrap_or(0.0) as u32;
                        let power = telemetry.power.parse::<f64>().unwrap_or(0.0);
                        let frequency = telemetry.aiclk.parse::<u32>().unwrap_or(0);

                        // Calculate memory usage - for now we just indicate DRAM status
                        let (used_memory, total_memory) = if device.board_info.dram_status == "Y" {
                            // Extract memory size from dram_speed (e.g., "16G" -> 16GB)
                            let mem_size = device
                                .board_info
                                .dram_speed
                                .trim_end_matches('G')
                                .parse::<u64>()
                                .unwrap_or(0)
                                * 1024
                                * 1024
                                * 1024; // Convert to bytes
                            (0, mem_size) // We don't have actual usage data
                        } else {
                            (0, 0)
                        };

                        // Heartbeat can be used as a proxy for device activity
                        // A changing heartbeat indicates the device is active
                        detail.insert("heartbeat".to_string(), telemetry.heartbeat.clone());

                        // Store voltage and current for diagnostics
                        detail.insert("voltage".to_string(), telemetry.voltage.clone());
                        detail.insert("current".to_string(), telemetry.current.clone());

                        // Generate device name from board type
                        let device_name = match device.board_info.board_type.as_str() {
                            "e150" => "Tenstorrent Grayskull e150",
                            "e300" => "Tenstorrent Grayskull e300",
                            "e75" => "Tenstorrent Grayskull e75",
                            "n300 L" | "n300 R" => "Tenstorrent Wormhole n300",
                            "n150" => "Tenstorrent Wormhole n150",
                            "nb_cb" => "Tenstorrent Wormhole NB CB",
                            "wh_4u" => "Tenstorrent Wormhole 4U",
                            "p100a" => "Tenstorrent Blackhole p100a",
                            "p150a" => "Tenstorrent Blackhole p150a",
                            "p150b" => "Tenstorrent Blackhole p150b",
                            _ => "Tenstorrent Unknown",
                        };

                        GpuInfo {
                            uuid: device.board_info.board_id.clone(),
                            time: time.clone(),
                            name: device_name.to_string(),
                            device_type: "NPU".to_string(),
                            hostname: hostname.clone(),
                            instance: format!("tt{idx}"),
                            utilization: 0.0, // Not directly available from tt-smi
                            ane_utilization: 0.0,
                            dla_utilization: None,
                            temperature,
                            used_memory,
                            total_memory,
                            frequency,
                            power_consumption: power,
                            detail,
                        }
                    })
                    .collect()
            }
            Err(e) => {
                eprintln!("Failed to parse tt-smi JSON output: {e}");
                vec![]
            }
        }
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
