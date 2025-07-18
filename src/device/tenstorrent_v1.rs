use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

// Use embedded Tenstorrent modules instead of external luwen
use super::tenstorrent_embedded::{
    chip::Chip,
    detect::{detect_chips_silent, ChipDetectOptions},
    Arch,
};

// Global status for error messages
static TENSTORRENT_STATUS: Mutex<Option<String>> = Mutex::new(None);

// Cache for initialized chips to avoid re-initialization on every measurement
static INITIALIZED_CHIPS: Lazy<Mutex<Option<Vec<Chip>>>> = Lazy::new(|| Mutex::new(None));

#[derive(Default)]
pub struct TenstorrentReader;

impl TenstorrentReader {
    pub fn new() -> Self {
        Self
    }

    /// Store an error message in the global status
    fn set_status(message: String) {
        if let Ok(mut status) = TENSTORRENT_STATUS.lock() {
            *status = Some(message);
        }
    }

    /// Collect NPU information by reading device files
    fn collect_npu_info(&self) -> Vec<GpuInfo> {
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

                // Note: 0x66666666 might be valid telemetry data that needs proper decoding
                // tt-smi processes these values normally

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

                // Add detailed power/thermal info - process all values normally
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
                // Process all values normally - the methods handle bit masking correctly
                let temperature = telemetry.asic_temperature().round() as u32; // Returns float in Celsius
                let power = telemetry.power(); // Returns watts as f64 (lower 16 bits of tdp)
                let frequency = telemetry.ai_clk(); // Use luwen's ai_clk() method

                // Note: If power is very low, it might indicate the device is idle or
                // telemetry reading is returning default values

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

    /// Get processes using Tenstorrent NPUs via device files
    fn get_processes_via_device_files(&self) -> Vec<ProcessInfo> {
        let mut processes = Vec::new();
        let Ok(proc_entries) = fs::read_dir("/proc") else {
            return processes;
        };

        for entry in proc_entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };

            let Ok(fd_entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };

            for fd_entry in fd_entries.flatten() {
                let Ok(link) = fs::read_link(fd_entry.path()) else {
                    continue;
                };

                if let Some(link_str) = link.to_str() {
                    if link_str.starts_with("/dev/tenstorrent/") {
                        let Ok(cmdline) = fs::read_to_string(format!("/proc/{pid}/cmdline")) else {
                            continue;
                        };
                        let Ok(comm) = fs::read_to_string(format!("/proc/{pid}/comm")) else {
                            continue;
                        };

                        let device_id = link_str
                            .trim_start_matches("/dev/tenstorrent/")
                            .parse::<usize>()
                            .unwrap_or(0);

                        let process_info = ProcessInfo {
                            device_id,
                            device_uuid: "".to_string(), // Not easily available
                            pid,
                            process_name: comm.trim().to_string(),
                            used_memory: 0,             // Not easily available
                            cpu_percent: 0.0,           // Not easily available
                            memory_percent: 0.0,        // Not easily available
                            memory_rss: 0,              // Not easily available
                            memory_vms: 0,              // Not easily available
                            user: "".to_string(),       // Not easily available
                            state: "".to_string(),      // Not easily available
                            start_time: "".to_string(), // Not easily available
                            cpu_time: 0,                // Not easily available
                            command: cmdline.replace('\0', " ").trim().to_string(),
                            ppid: 0,    // Not easily available
                            threads: 0, // Not easily available
                            uses_gpu: true,
                            priority: 0,          // Not easily available
                            nice_value: 0,        // Not easily available
                            gpu_utilization: 0.0, // Not easily available
                        };
                        processes.push(process_info);
                        // Found a tenstorrent process, no need to check other fds for this pid
                        break;
                    }
                }
            }
        }

        processes
    }
}

impl GpuReader for TenstorrentReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        self.collect_npu_info()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.get_processes_via_device_files()
    }
}

/// Get the current Tenstorrent status message (if any)
pub fn get_tenstorrent_status_message() -> Option<String> {
    TENSTORRENT_STATUS.lock().ok()?.clone()
}
