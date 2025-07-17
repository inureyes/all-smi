use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

// Use embedded Tenstorrent modules instead of external luwen
use super::tenstorrent_embedded::detect::detect_chips_silent;
use super::tenstorrent_embedded::{Arch, Chip, ChipDetectOptions};

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
    telemetry: TtSmiTelemetry,
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
struct TtSmiTelemetry {
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

// Global status for error messages
static TENSTORRENT_STATUS: Mutex<Option<String>> = Mutex::new(None);

// Cache for initialized chips to avoid re-initialization on every measurement
static INITIALIZED_CHIPS: Lazy<Mutex<Option<Vec<Chip>>>> = Lazy::new(|| Mutex::new(None));

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

    /// Store an error message in the global status
    fn set_status(message: String) {
        if let Ok(mut status) = TENSTORRENT_STATUS.lock() {
            *status = Some(message);
        }
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
                // Don't log here, as we're trying fallback
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
                    // Don't set status for tt-smi failures since it's just a fallback
                    // The embedded telemetry reading should work without it
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Self::set_status(
                        "tt-smi not found - Tenstorrent tools not installed".to_string(),
                    );
                } else {
                    Self::set_status(format!("Failed to execute tt-smi: {e}"));
                }
            }
        }

        // If tt-smi fails, try tensix-stat as alternative
        match Command::new("tensix-stat").output() {
            Ok(output) => {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    self.parse_tensix_stat_output(&output_str)
                } else {
                    // Don't update status here, keep the tt-smi error
                    vec![]
                }
            }
            Err(_) => {
                // tensix-stat not found is expected if tt-smi is the primary tool
                vec![]
            }
        }
    }

    /// Collect NPU information by reading device files
    fn collect_via_device_files(&self) -> Vec<GpuInfo> {
        // Check if we have cached initialized chips
        if let Ok(cache) = INITIALIZED_CHIPS.lock() {
            if let Some(ref chips) = *cache {
                // Use cached chips
                let mut devices = Vec::new();
                for (idx, chip) in chips.iter().enumerate() {
                    if let Some(info) = self.read_device_info_luwen(chip, idx) {
                        devices.push(info);
                    }
                }
                return devices;
            }
        }

        // First time initialization - detect and initialize chips
        let options = ChipDetectOptions {
            local_only: true,
            ..Default::default()
        };

        // Use detect_chips_silent to avoid progress bars and messages
        match detect_chips_silent(options) {
            Ok(uninit_chips) => {
                let mut initialized_chips = Vec::new();
                let mut devices = Vec::new();

                // Initialize each chip and collect info
                for (idx, uninit_chip) in uninit_chips.into_iter().enumerate() {
                    // Initialize the chip without progress callbacks
                    match uninit_chip.init(&mut |_| Ok::<(), std::convert::Infallible>(())) {
                        Ok(chip) => {
                            if let Some(info) = self.read_device_info_luwen(&chip, idx) {
                                devices.push(info);
                            }
                            initialized_chips.push(chip);
                        }
                        Err(_) => {
                            // This should never happen with Infallible error type
                            eprintln!("Failed to initialize chip {idx}");
                        }
                    }
                }

                if devices.is_empty() {
                    Self::set_status("No Tenstorrent devices found".to_string());
                } else {
                    // Clear any previous error status
                    if let Ok(mut status) = TENSTORRENT_STATUS.lock() {
                        *status = None;
                    }

                    // Cache the initialized chips for future use
                    if let Ok(mut cache) = INITIALIZED_CHIPS.lock() {
                        *cache = Some(initialized_chips);
                    }
                }

                devices
            }
            Err(e) => {
                Self::set_status(format!("Failed to detect Tenstorrent devices: {e}"));
                vec![]
            }
        }
    }

    /// Read device information using luwen
    fn read_device_info_luwen(&self, chip: &Chip, index: usize) -> Option<GpuInfo> {
        // Try to get telemetry from the chip
        match chip.get_telemetry() {
            Ok(telemetry) => {
                let hostname = get_hostname();
                let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

                // Get board type name
                let board_type = telemetry.try_board_type().unwrap_or("Unknown");
                let device_name = format!(
                    "Tenstorrent {} {}",
                    match telemetry.arch {
                        Arch::Grayskull => "Grayskull",
                        Arch::Wormhole => "Wormhole",
                        Arch::Blackhole => "Blackhole",
                    },
                    board_type
                );

                let mut detail = HashMap::new();
                detail.insert("board_type".to_string(), board_type.to_string());
                detail.insert("board_id".to_string(), telemetry.board_serial_number_hex());
                detail.insert("collection_method".to_string(), "luwen".to_string());

                // Add firmware versions
                detail.insert("arc_fw_version".to_string(), telemetry.arc_fw_version());
                detail.insert("eth_fw_version".to_string(), telemetry.eth_fw_version());
                detail.insert("fw_date".to_string(), telemetry.firmware_date());

                // Add detailed power/thermal info
                detail.insert(
                    "voltage".to_string(),
                    format!("{:.2}", telemetry.voltage()), // Use luwen's voltage() method
                );
                detail.insert(
                    "current".to_string(),
                    format!("{:.1}", telemetry.current()), // Use luwen's current() method
                );

                // Add additional temperature readings
                detail.insert(
                    "asic_temperature".to_string(),
                    format!("{:.1}", telemetry.asic_temperature()),
                );
                detail.insert(
                    "vreg_temperature".to_string(),
                    format!("{:.1}", telemetry.vreg_temperature()),
                );
                if telemetry.board_temperature != 0 {
                    detail.insert(
                        "inlet_temperature".to_string(),
                        format!("{:.1}", telemetry.inlet_temperature()),
                    );
                    detail.insert(
                        "outlet_temperature1".to_string(),
                        format!("{:.1}", telemetry.outlet_temperature1()),
                    );
                    detail.insert(
                        "outlet_temperature2".to_string(),
                        format!("{:.1}", telemetry.outlet_temperature2()),
                    );
                }

                // Use luwen's built-in methods for proper temperature and power extraction
                let temperature = telemetry.asic_temperature().round() as u32; // Returns float in Celsius
                let power = telemetry.power(); // Returns watts as f64
                let frequency = telemetry.ai_clk(); // Use luwen's ai_clk() method

                // Debug logging to understand power reading
                if power == 0.0 {
                    eprintln!("Warning: Tenstorrent device {} has 0W power reading. Raw tdp value: 0x{:08x}", 
                        index, telemetry.tdp);
                }

                // Calculate utilization based on power consumption vs TDP
                // This is a proxy metric since Tenstorrent doesn't provide direct utilization
                // We use the ratio of current power to TDP (Thermal Design Power) limit
                // Note: This assumes the device scales power linearly with load, which is
                // a reasonable approximation for AI accelerators
                //
                // IMPORTANT: telemetry.tdp actually contains current power consumption, not TDP limit!
                // Since the actual TDP limit is not directly available in telemetry, we use
                // board-specific estimates based on Tenstorrent specifications
                let utilization = {
                    // Get board-specific TDP using the telemetry helper
                    let tdp_limit = telemetry.get_board_tdp();

                    // Calculate utilization percentage
                    ((power / tdp_limit) * 100.0).min(100.0)
                };

                // Add raw telemetry values for debugging
                detail.insert("power_watts".to_string(), format!("{power:.2}"));
                detail.insert("aiclk_mhz".to_string(), format!("{frequency}"));
                detail.insert("axiclk_mhz".to_string(), format!("{}", telemetry.axi_clk()));
                detail.insert("arcclk_mhz".to_string(), format!("{}", telemetry.arc_clk()));

                // DDR memory info (if available)
                let (used_memory, total_memory) = if telemetry.ddr_status != 0 {
                    // Get memory information using the telemetry helper
                    let total_mem = telemetry.get_board_memory_size();

                    // For used memory, we can estimate based on power consumption
                    // Higher power typically indicates more memory activity
                    // This is a rough estimate until we can get actual memory usage
                    let utilization_estimate = if power > 50.0 {
                        0.7 // High power usage suggests significant memory use
                    } else if power > 20.0 {
                        0.4 // Moderate power usage
                    } else if power > 5.0 {
                        0.2 // Low power usage
                    } else {
                        0.1 // Idle or very low usage
                    };

                    let used_mem = (total_mem as f64 * utilization_estimate) as u64;
                    (used_mem, total_mem)
                } else {
                    (0, 0)
                };

                Some(GpuInfo {
                    uuid: telemetry.board_serial_number_hex(),
                    time,
                    name: device_name,
                    device_type: "NPU".to_string(),
                    hostname: hostname.clone(),
                    instance: format!("tt{index}"),
                    utilization,
                    ane_utilization: 0.0,
                    dla_utilization: None,
                    temperature,
                    used_memory,
                    total_memory,
                    frequency,
                    power_consumption: power,
                    detail,
                })
            }
            Err(e) => {
                eprintln!("Failed to get telemetry for device {index}: {e}");
                None
            }
        }
    }

    /// Parse tt-smi output
    fn parse_tt_smi_output(&self, output: &str) -> Vec<GpuInfo> {
        // Parse JSON output from tt-smi
        match serde_json::from_str::<TtSmiOutput>(output) {
            Ok(tt_output) => {
                let hostname = get_hostname();
                let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

                // Clear any previous error status on successful parsing
                if let Ok(mut status) = TENSTORRENT_STATUS.lock() {
                    *status = None;
                }

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
                        // Parse temperature as float and round to nearest integer
                        let temperature = telemetry
                            .asic_temperature
                            .parse::<f64>()
                            .unwrap_or(0.0)
                            .round() as u32;
                        let power = telemetry.power.parse::<f64>().unwrap_or(0.0);
                        let frequency = telemetry.aiclk.parse::<u32>().unwrap_or(0);

                        // Calculate utilization based on power consumption vs TDP
                        // This is a proxy metric since Tenstorrent doesn't provide direct utilization
                        // We use the ratio of current power to TDP (Thermal Design Power) limit
                        let utilization = if let Some(ref limits) = device.limits {
                            if let Some(ref tdp_str) = limits.tdp_limit {
                                if let Ok(tdp_watts) = tdp_str.parse::<f64>() {
                                    if tdp_watts > 0.0 {
                                        // Clamp to 100% max as power can temporarily exceed TDP
                                        ((power / tdp_watts) * 100.0).min(100.0)
                                    } else {
                                        0.0
                                    }
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        } else {
                            // Fallback: estimate based on typical TDP values if limits not available
                            // These are conservative estimates based on known Tenstorrent boards
                            let estimated_tdp = match device.board_info.board_type.as_str() {
                                // Grayskull boards
                                "e75" => 75.0,
                                "e150" => 75.0,
                                "e300" => 100.0,
                                // Wormhole boards
                                "n150" => 150.0,
                                "n300 L" | "n300 R" => 160.0,
                                "nb_cb" => 150.0,
                                "wh_4u" => 200.0,
                                // Blackhole boards
                                "p100a" => 300.0,
                                "p150a" | "p150b" => 350.0,
                                _ => 150.0, // Conservative default
                            };
                            ((power / estimated_tdp) * 100.0).min(100.0)
                        };

                        // Calculate memory usage - extract from DRAM status and board type
                        let (used_memory, total_memory) = if device.board_info.dram_status == "Y" {
                            // First try to extract from dram_speed field if it contains size info
                            let mem_size = if device.board_info.dram_speed.contains('G') {
                                device
                                    .board_info
                                    .dram_speed
                                    .split_whitespace()
                                    .find(|s| s.ends_with('G'))
                                    .and_then(|s| s.trim_end_matches('G').parse::<u64>().ok())
                                    .unwrap_or(0)
                                    * 1024
                                    * 1024
                                    * 1024
                            } else {
                                // Fallback to board type based memory sizes
                                match device.board_info.board_type.as_str() {
                                    // Grayskull boards
                                    "e75" => 16 * 1024 * 1024 * 1024,
                                    "e150" => 32 * 1024 * 1024 * 1024,
                                    "e300" => 48 * 1024 * 1024 * 1024,
                                    // Wormhole boards
                                    "n150" => 32 * 1024 * 1024 * 1024,
                                    "n300 L" | "n300 R" => 64 * 1024 * 1024 * 1024,
                                    "nb_cb" => 32 * 1024 * 1024 * 1024,
                                    "wh_4u" => 96 * 1024 * 1024 * 1024,
                                    // Blackhole boards
                                    "p100a" => 96 * 1024 * 1024 * 1024,
                                    "p150a" | "p150b" => 144 * 1024 * 1024 * 1024,
                                    _ => 32 * 1024 * 1024 * 1024, // Default fallback
                                }
                            };

                            // Estimate used memory based on power consumption
                            let power_value = telemetry.power.parse::<f64>().unwrap_or(0.0);
                            let utilization_factor = if power_value > 50.0 {
                                0.7
                            } else if power_value > 20.0 {
                                0.4
                            } else if power_value > 5.0 {
                                0.2
                            } else {
                                0.1
                            };

                            let used_mem = (mem_size as f64 * utilization_factor) as u64;
                            (used_mem, mem_size)
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
                            // Grayskull boards
                            "e75" => "Tenstorrent Grayskull e75",
                            "e150" => "Tenstorrent Grayskull e150",
                            "e300" => "Tenstorrent Grayskull e300",
                            "E300_R2" => "Tenstorrent Grayskull e300 R2",
                            "E300_R3" => "Tenstorrent Grayskull e300 R3",
                            "GALAXY" => "Tenstorrent Grayskull Galaxy",
                            // Wormhole boards
                            "n150" => "Tenstorrent Wormhole n150",
                            "n300" | "n300 L" | "n300 R" => "Tenstorrent Wormhole n300",
                            "NEBULA_CB" | "nb_cb" => "Tenstorrent Wormhole Nebula CB",
                            "wh_4u" => "Tenstorrent Wormhole 4U",
                            "galaxy-wormhole" => "Tenstorrent Wormhole Galaxy",
                            // Blackhole boards
                            "p100" => "Tenstorrent Blackhole p100",
                            "p100a" => "Tenstorrent Blackhole p100a",
                            "p150a" => "Tenstorrent Blackhole p150a",
                            "p150b" => "Tenstorrent Blackhole p150b",
                            "p150c" => "Tenstorrent Blackhole p150c",
                            "p300a" => "Tenstorrent Blackhole p300a",
                            "p300b" => "Tenstorrent Blackhole p300b",
                            "p300c" => "Tenstorrent Blackhole p300c",
                            "galaxy-blackhole" => "Tenstorrent Blackhole Galaxy",
                            _ => "Tenstorrent Unknown",
                        };

                        GpuInfo {
                            uuid: device.board_info.board_id.clone(),
                            time: time.clone(),
                            name: device_name.to_string(),
                            device_type: "NPU".to_string(),
                            hostname: hostname.clone(),
                            instance: format!("tt{idx}"),
                            utilization,
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
                Self::set_status(format!("Failed to parse tt-smi output: {e}"));
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
        // TODO: Parse actual tensix-stat output when format is known
        vec![GpuInfo {
            uuid: "TT-PLACEHOLDER-UUID".to_string(),
            time,
            name: "Tenstorrent Wormhole".to_string(),
            device_type: "NPU".to_string(),
            hostname: hostname.clone(),
            instance: "wh0".to_string(),
            utilization: 0.0, // Will need to calculate from power/TDP when implemented
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

/// Get the current Tenstorrent status message (if any)
pub fn get_tenstorrent_status_message() -> Option<String> {
    TENSTORRENT_STATUS.lock().ok()?.clone()
}
