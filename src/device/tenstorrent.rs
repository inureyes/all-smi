use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use all_smi_luwen_if::chip::{Chip, ChipImpl};
use all_smi_luwen_if::ChipDetectOptions;
use chrono::Local;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Collection method for Tenstorrent NPU metrics
#[derive(Debug, Clone, Copy)]
pub enum CollectionMethod {
    /// Read directly from device files in /dev
    DeviceFile,
}

/// Configuration for Tenstorrent reader
pub struct TenstorrentConfig {
    /// Primary method to use for collecting metrics
    pub primary_method: CollectionMethod,
}

impl Default for TenstorrentConfig {
    fn default() -> Self {
        Self {
            primary_method: CollectionMethod::DeviceFile,
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

        match self.config.primary_method {
            CollectionMethod::DeviceFile => self.collect_via_device_files(),
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
        match all_smi_luwen_ref::detect_chips_silent(options) {
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
                        all_smi_luwen_core::Arch::Grayskull => "Grayskull",
                        all_smi_luwen_core::Arch::Wormhole => "Wormhole",
                        all_smi_luwen_core::Arch::Blackhole => "Blackhole",
                    },
                    board_type
                );

                let mut detail = HashMap::new();
                detail.insert("board_type".to_string(), board_type.to_string());
                detail.insert("board_id".to_string(), telemetry.board_serial_number_hex());
                detail.insert("collection_method".to_string(), "luwen".to_string());

                // Add PCIe device information if available
                if let Ok(Some(device_info)) = chip.get_device_info() {
                    detail.insert(
                        "pcie_address".to_string(),
                        format!(
                            "{:04x}:{:02x}:{:02x}.{:x}",
                            device_info.domain,
                            device_info.bus,
                            device_info.slot,
                            device_info.function
                        ),
                    );
                    detail.insert(
                        "pcie_vendor_id".to_string(),
                        format!("0x{:04x}", device_info.vendor),
                    );
                    detail.insert(
                        "pcie_device_id".to_string(),
                        format!("0x{:04x}", device_info.device_id),
                    );

                    // Get PCIe link information
                    detail.insert(
                        "pcie_link_width".to_string(),
                        format!("x{}", device_info.pcie_current_link_width()),
                    );
                    detail.insert(
                        "pcie_link_gen".to_string(),
                        format!("Gen{}", device_info.pcie_current_link_gen()),
                    );
                }

                // Add firmware versions
                detail.insert("arc_fw_version".to_string(), telemetry.arc_fw_version());
                detail.insert("eth_fw_version".to_string(), telemetry.eth_fw_version());
                detail.insert("fw_date".to_string(), telemetry.firmware_date());

                // Add additional firmware versions from TT-REPORT.md
                if telemetry.ddr_fw_version != 0 {
                    detail.insert(
                        "ddr_fw_version".to_string(),
                        format!(
                            "{}.{}.{}",
                            (telemetry.ddr_fw_version >> 16) & 0xFF,
                            (telemetry.ddr_fw_version >> 8) & 0xFF,
                            telemetry.ddr_fw_version & 0xFF
                        ),
                    );
                }
                if telemetry.spibootrom_fw_version != 0 {
                    detail.insert(
                        "spibootrom_fw_version".to_string(),
                        format!(
                            "{}.{}.{}",
                            (telemetry.spibootrom_fw_version >> 16) & 0xFF,
                            (telemetry.spibootrom_fw_version >> 8) & 0xFF,
                            telemetry.spibootrom_fw_version & 0xFF
                        ),
                    );
                }

                // Add detailed power/thermal info
                detail.insert(
                    "voltage".to_string(),
                    format!("{:.2}", telemetry.voltage()), // Use luwen's voltage() method
                );
                detail.insert(
                    "current".to_string(),
                    format!("{:.1}", telemetry.current()), // Use luwen's current() method
                );

                // Add TDP/TDC limits from upper 16 bits as per TT-REPORT.md
                let tdp_limit = ((telemetry.tdp >> 16) & 0xFFFF) as f64;
                let tdc_limit = ((telemetry.tdc >> 16) & 0xFFFF) as f64;
                if tdp_limit > 0.0 {
                    detail.insert("tdp_limit".to_string(), format!("{tdp_limit:.0}"));
                }
                if tdc_limit > 0.0 {
                    detail.insert("tdc_limit".to_string(), format!("{tdc_limit:.0}"));
                }

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

                // Calculate utilization based on power consumption vs TDP
                // TT-REPORT.md indicates that TDP limit is in upper 16 bits of tdp register
                let utilization = {
                    // First try to get TDP limit from telemetry (upper 16 bits)
                    let tdp_limit_from_telemetry = ((telemetry.tdp >> 16) & 0xFFFF) as f64;

                    let tdp_limit = if tdp_limit_from_telemetry > 0.0 {
                        // Use the actual TDP limit from device telemetry
                        tdp_limit_from_telemetry
                    } else {
                        // Fallback to board-specific TDP estimates based on TT-REPORT.md
                        match telemetry.board_type() {
                            // Grayskull boards
                            "e75" => 75.0,
                            "e150" => 75.0,
                            "e300" | "e300_R2" | "e300_R3" => 100.0,
                            "GALAXY" => 300.0,
                            // Wormhole boards
                            "n150" => 150.0,
                            "n300" => 160.0,
                            "NEBULA_CB" => 150.0,
                            "galaxy-wormhole" => 200.0,
                            // Blackhole boards
                            "p100" | "p100a" => 300.0,
                            "p150a" | "p150b" | "p150c" => 350.0,
                            "p300a" | "p300b" | "p300c" => 400.0,
                            "galaxy-blackhole" => 450.0,
                            _ => {
                                // Fallback based on architecture
                                match telemetry.arch {
                                    all_smi_luwen_core::Arch::Grayskull => 75.0,
                                    all_smi_luwen_core::Arch::Wormhole => 160.0,
                                    all_smi_luwen_core::Arch::Blackhole => 350.0,
                                }
                            }
                        }
                    };

                    // Calculate utilization percentage
                    ((power / tdp_limit) * 100.0).min(100.0)
                };

                // Add raw telemetry values for debugging
                detail.insert("power_watts".to_string(), format!("{power:.2}"));
                detail.insert("aiclk_mhz".to_string(), format!("{frequency}"));
                detail.insert("axiclk_mhz".to_string(), format!("{}", telemetry.axi_clk()));
                detail.insert("arcclk_mhz".to_string(), format!("{}", telemetry.arc_clk()));

                // Add PCIe/Ethernet/DDR status fields as per TT-REPORT.md
                detail.insert(
                    "pcie_status".to_string(),
                    format!("0x{:08x}", telemetry.pcie_status),
                );
                detail.insert(
                    "eth_status0".to_string(),
                    format!("0x{:08x}", telemetry.eth_status0),
                );
                detail.insert(
                    "eth_status1".to_string(),
                    format!("0x{:08x}", telemetry.eth_status1),
                );
                detail.insert(
                    "ddr_status".to_string(),
                    format!("0x{:08x}", telemetry.ddr_status),
                );

                // Add health/heartbeat counters
                let heartbeat = telemetry.telemetry_heartbeat();
                detail.insert("heartbeat".to_string(), format!("{heartbeat}"));
                detail.insert(
                    "arc0_health".to_string(),
                    format!("{}", telemetry.arc0_health),
                );
                detail.insert(
                    "arc3_health".to_string(),
                    format!("{}", telemetry.arc3_health),
                );

                // Add fault and throttler information
                if telemetry.faults != 0 {
                    detail.insert("faults".to_string(), format!("0x{:08x}", telemetry.faults));
                }
                if telemetry.throttler != 0 {
                    detail.insert(
                        "throttler".to_string(),
                        format!("0x{:08x}", telemetry.throttler),
                    );
                }

                // Add fan information if available
                if telemetry.fan_speed != 0 {
                    detail.insert("fan_speed".to_string(), format!("{}", telemetry.fan_speed));
                }
                if telemetry.fan_rpm != 0 {
                    detail.insert("fan_rpm".to_string(), format!("{}", telemetry.fan_rpm));
                }

                // DDR memory info (if available)
                let (used_memory, total_memory) = if telemetry.ddr_status != 0 {
                    // Get memory information based on board type
                    // Memory sizes are based on Tenstorrent board specifications
                    let total_mem = match telemetry.board_type() {
                        // Grayskull boards
                        "e75" => 16 * 1024 * 1024 * 1024,  // 16GB
                        "e150" => 32 * 1024 * 1024 * 1024, // 32GB
                        "e300" | "e300_R2" | "e300_R3" => 48 * 1024 * 1024 * 1024, // 48GB
                        "GALAXY" => 96 * 1024 * 1024 * 1024, // 96GB (Galaxy has 2x48GB)
                        // Wormhole boards
                        "n150" => 32 * 1024 * 1024 * 1024, // 32GB
                        "n300" => 64 * 1024 * 1024 * 1024, // 64GB
                        "NEBULA_CB" => 32 * 1024 * 1024 * 1024, // 32GB
                        "galaxy-wormhole" => 96 * 1024 * 1024 * 1024, // 96GB per board
                        // Blackhole boards
                        "p100" | "p100a" => 96 * 1024 * 1024 * 1024, // 96GB
                        "p150a" | "p150b" | "p150c" => 144 * 1024 * 1024 * 1024, // 144GB
                        "p300a" | "p300b" | "p300c" => 288 * 1024 * 1024 * 1024, // 288GB
                        "galaxy-blackhole" => 576 * 1024 * 1024 * 1024, // 576GB
                        _ => {
                            // Try to extract from DDR speed if available
                            if let Some(ddr_speed) = telemetry.ddr_speed {
                                // Add DDR speed to details
                                detail.insert("ddr_speed".to_string(), format!("{ddr_speed}"));
                                // Conservative memory estimates based on architecture
                                match telemetry.arch {
                                    all_smi_luwen_core::Arch::Grayskull => 16 * 1024 * 1024 * 1024,
                                    all_smi_luwen_core::Arch::Wormhole => 32 * 1024 * 1024 * 1024,
                                    all_smi_luwen_core::Arch::Blackhole => 96 * 1024 * 1024 * 1024,
                                }
                            } else {
                                0
                            }
                        }
                    };

                    // Improved memory usage estimation based on multiple factors from TT-REPORT.md
                    // Consider power consumption, throttler state, and utilization
                    let memory_utilization_estimate = {
                        // Start with power-based estimate
                        let power_factor = (power / tdp_limit).min(1.0);

                        // Adjust based on throttler state (non-zero indicates memory pressure)
                        let throttler_factor = if telemetry.throttler != 0 { 0.9 } else { 0.7 };

                        // Consider AI clock frequency (higher frequency = more memory bandwidth usage)
                        let freq_factor = (frequency as f64 / 1000.0).min(1.0); // Assuming 1000MHz as nominal

                        // Combine factors with weights
                        let combined =
                            (power_factor * 0.5) + (freq_factor * 0.3) + (throttler_factor * 0.2);

                        // Ensure reasonable bounds
                        combined.clamp(0.05, 0.95)
                    };

                    let used_mem = (total_mem as f64 * memory_utilization_estimate) as u64;
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

    /// Get processes using Tenstorrent NPUs via device files
    fn get_processes_via_device_files(&self) -> Vec<ProcessInfo> {
        // NOTE: Process information is not directly available through the device interface
        // The luwen API and device files don't expose per-process GPU usage information
        // This is different from NVIDIA GPUs where nvidia-smi can track process usage
        //
        // Future improvement: Could potentially track which processes have the device file open
        // using lsof or /proc, but this wouldn't show actual GPU utilization per process
        vec![]
    }

    /// Collect process info using the configured method with fallback
    fn collect_process_info(&self) -> Vec<ProcessInfo> {
        // Try primary method first

        match self.config.primary_method {
            CollectionMethod::DeviceFile => self.get_processes_via_device_files(),
        }
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
