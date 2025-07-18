// SPDX-FileCopyrightText: © 2025 All-SMI Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tenstorrent NPU reader implementation
//! Improved implementation based on TT-REPORT.md specifications

use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

use super::tenstorrent_embedded::{device::TenstorrentDevice, ttkmd::kmdif::PciDevice};

/// Global status for error messages
static TENSTORRENT_STATUS: Mutex<Option<String>> = Mutex::new(None);

/// Cache for initialized devices
static INITIALIZED_DEVICES: Lazy<Mutex<Option<Vec<TenstorrentDevice>>>> =
    Lazy::new(|| Mutex::new(None));

/// Tenstorrent NPU reader
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

    /// Clear the error status
    fn clear_status() {
        if let Ok(mut status) = TENSTORRENT_STATUS.lock() {
            *status = None;
        }
    }

    /// Collect NPU information from all devices
    fn collect_npu_info(&self) -> Vec<GpuInfo> {
        // Check if we have cached initialized devices
        if let Ok(cache) = INITIALIZED_DEVICES.lock() {
            if let Some(ref devices) = *cache {
                // Use cached devices
                let mut infos = Vec::new();
                for (idx, device) in devices.iter().enumerate() {
                    if let Some(info) = self.read_device_info(device, idx) {
                        infos.push(info);
                    }
                }
                return infos;
            }
        }

        // First time initialization - detect and initialize devices
        eprintln!("[DEBUG] Detecting Tenstorrent devices...");

        // Scan for device IDs
        let device_ids = PciDevice::scan();
        if device_ids.is_empty() {
            Self::set_status("No Tenstorrent devices found".to_string());
            return vec![];
        }

        eprintln!("[DEBUG] Found {} Tenstorrent device(s)", device_ids.len());

        let mut initialized_devices = Vec::new();
        let mut infos = Vec::new();

        // Initialize each device
        for (idx, device_id) in device_ids.into_iter().enumerate() {
            eprintln!("[DEBUG] Initializing device {idx} (ID: {device_id})");

            match TenstorrentDevice::open(device_id) {
                Ok(mut device) => {
                    // Wait for device initialization
                    match device.wait_for_init() {
                        Ok(()) => {
                            eprintln!("[DEBUG] Device {idx} initialized successfully");

                            // Read device info
                            if let Some(info) = self.read_device_info(&device, idx) {
                                infos.push(info);
                            }

                            initialized_devices.push(device);
                        }
                        Err(e) => {
                            eprintln!("[ERROR] Failed to initialize device {idx}: {e}");

                            // Check if it's partially initialized
                            let status = device.get_init_status();
                            if status.arc_ready {
                                eprintln!("[WARN] Device {idx} is partially initialized, attempting to read telemetry");

                                // Try to read telemetry anyway
                                if let Some(info) = self.read_device_info(&device, idx) {
                                    infos.push(info);
                                }

                                initialized_devices.push(device);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[ERROR] Failed to open device {idx}: {e}");
                }
            }
        }

        if infos.is_empty() {
            Self::set_status("Failed to initialize any Tenstorrent devices".to_string());
        } else {
            Self::clear_status();

            // Cache the initialized devices
            if let Ok(mut cache) = INITIALIZED_DEVICES.lock() {
                *cache = Some(initialized_devices);
            }
        }

        infos
    }

    /// Read device information from an initialized device
    fn read_device_info(&self, device: &TenstorrentDevice, index: usize) -> Option<GpuInfo> {
        match device.get_telemetry() {
            Ok(telemetry) => {
                let hostname = get_hostname();
                let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

                // Get board type and device name
                let board_type = telemetry.board_type();
                let device_name = format!("Tenstorrent {} {}", device.get_arch(), board_type);

                // Build detail map
                let mut detail = HashMap::new();
                detail.insert("board_type".to_string(), board_type.to_string());
                detail.insert("board_id".to_string(), telemetry.board_serial_number_hex());
                detail.insert("arch".to_string(), format!("{:?}", device.get_arch()));
                detail.insert("collection_method".to_string(), "bar_mapping".to_string());

                // Add firmware versions
                detail.insert("arc_fw_version".to_string(), telemetry.arc_fw_version());
                detail.insert("eth_fw_version".to_string(), telemetry.eth_fw_version());
                detail.insert("fw_date".to_string(), telemetry.firmware_date());

                // Add power and thermal details
                detail.insert("voltage".to_string(), format!("{:.2}", telemetry.voltage()));
                detail.insert("current".to_string(), format!("{:.1}", telemetry.current()));
                detail.insert(
                    "power_watts".to_string(),
                    format!("{:.2}", telemetry.power()),
                );
                detail.insert(
                    "tdp_limit".to_string(),
                    format!("{:.0}", telemetry.tdp_limit()),
                );
                detail.insert(
                    "tdc_limit".to_string(),
                    format!("{:.0}", telemetry.tdc_limit()),
                );

                // Temperature readings
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

                // Clock frequencies
                detail.insert("aiclk_mhz".to_string(), format!("{}", telemetry.ai_clk()));
                detail.insert("axiclk_mhz".to_string(), format!("{}", telemetry.axi_clk()));
                detail.insert("arcclk_mhz".to_string(), format!("{}", telemetry.arc_clk()));

                // Status fields
                detail.insert(
                    "ddr_status".to_string(),
                    format!("0x{:x}", telemetry.ddr_status),
                );
                detail.insert(
                    "pcie_status".to_string(),
                    format!("0x{:x}", telemetry.pcie_status),
                );
                detail.insert(
                    "eth_status0".to_string(),
                    format!("0x{:x}", telemetry.eth_status0),
                );
                detail.insert(
                    "eth_status1".to_string(),
                    format!("0x{:x}", telemetry.eth_status1),
                );

                // Health monitoring
                detail.insert(
                    "heartbeat".to_string(),
                    format!("{}", telemetry.telemetry_heartbeat()),
                );

                // Extract main metrics
                let temperature = telemetry.asic_temperature().round() as u32;
                let power = telemetry.power();
                let frequency = telemetry.ai_clk();

                // Calculate utilization based on power vs TDP
                let tdp_limit = telemetry.get_board_tdp();
                let utilization = ((power / tdp_limit) * 100.0).min(100.0);

                // Memory information
                let total_memory = telemetry.get_board_memory_size();
                let used_memory = if telemetry.ddr_status != 0 {
                    // Estimate memory usage based on power consumption
                    let mem_factor = if power > 50.0 {
                        0.7
                    } else if power > 20.0 {
                        0.4
                    } else if power > 5.0 {
                        0.2
                    } else {
                        0.1
                    };
                    (total_memory as f64 * mem_factor) as u64
                } else {
                    0
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
                eprintln!("[ERROR] Failed to get telemetry for device {index}: {e}");
                None
            }
        }
    }

    /// Get processes using Tenstorrent NPUs
    fn get_processes_with_info(&self) -> Vec<ProcessInfo> {
        let mut processes = Vec::new();

        // Read /proc to find processes with open /dev/tenstorrent/* files
        let Ok(proc_entries) = fs::read_dir("/proc") else {
            return processes;
        };

        for entry in proc_entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };

            let fd_path = format!("/proc/{pid}/fd");
            let Ok(fd_entries) = fs::read_dir(&fd_path) else {
                continue;
            };

            for fd_entry in fd_entries.flatten() {
                let Ok(link) = fs::read_link(fd_entry.path()) else {
                    continue;
                };

                if let Some(link_str) = link.to_str() {
                    if link_str.starts_with("/dev/tenstorrent/") {
                        // Found a process using Tenstorrent device
                        let device_id = link_str
                            .trim_start_matches("/dev/tenstorrent/")
                            .parse::<usize>()
                            .unwrap_or(0);

                        // Read process information
                        let cmdline_path = format!("/proc/{pid}/cmdline");
                        let comm_path = format!("/proc/{pid}/comm");
                        let status_path = format!("/proc/{pid}/status");
                        let stat_path = format!("/proc/{pid}/stat");

                        let cmdline = fs::read_to_string(&cmdline_path)
                            .unwrap_or_default()
                            .replace('\0', " ")
                            .trim()
                            .to_string();

                        let comm = fs::read_to_string(&comm_path)
                            .unwrap_or_default()
                            .trim()
                            .to_string();

                        // Parse additional info from status file
                        let mut user = String::new();
                        let mut state = String::new();
                        let mut ppid = 0;
                        let mut threads = 0;
                        let mut memory_rss = 0;
                        let mut memory_vms = 0;

                        if let Ok(status) = fs::read_to_string(&status_path) {
                            for line in status.lines() {
                                let parts: Vec<&str> = line.splitn(2, ':').collect();
                                if parts.len() == 2 {
                                    let key = parts[0].trim();
                                    let value = parts[1].trim();

                                    match key {
                                        "Uid" => {
                                            if let Some(uid_str) = value.split_whitespace().next() {
                                                if let Ok(uid) = uid_str.parse::<u32>() {
                                                    user = Self::get_username(uid);
                                                }
                                            }
                                        }
                                        "State" => {
                                            state = value.chars().next().unwrap_or('?').to_string()
                                        }
                                        "PPid" => ppid = value.parse().unwrap_or(0),
                                        "Threads" => threads = value.parse().unwrap_or(0),
                                        "VmRSS" => {
                                            if let Some(rss_str) = value.split_whitespace().next() {
                                                memory_rss =
                                                    rss_str.parse::<u64>().unwrap_or(0) * 1024;
                                            }
                                        }
                                        "VmSize" => {
                                            if let Some(vms_str) = value.split_whitespace().next() {
                                                memory_vms =
                                                    vms_str.parse::<u64>().unwrap_or(0) * 1024;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }

                        // Get CPU time from stat file
                        let cpu_time = if let Ok(stat) = fs::read_to_string(&stat_path) {
                            let fields: Vec<&str> = stat.split_whitespace().collect();
                            if fields.len() > 14 {
                                let utime = fields[13].parse::<u64>().unwrap_or(0);
                                let stime = fields[14].parse::<u64>().unwrap_or(0);
                                (utime + stime) / 100 // Convert from jiffies to seconds
                            } else {
                                0
                            }
                        } else {
                            0
                        };

                        let process_info = ProcessInfo {
                            device_id,
                            device_uuid: String::new(), // Could be populated if we match with device
                            pid,
                            process_name: comm,
                            used_memory: 0, // Device memory usage not easily available
                            cpu_percent: 0.0, // Would need sampling to calculate
                            memory_percent: 0.0, // Would need total system memory to calculate
                            memory_rss,
                            memory_vms,
                            user,
                            state,
                            start_time: String::new(), // Would need to calculate from boot time
                            cpu_time,
                            command: cmdline,
                            ppid,
                            threads,
                            uses_gpu: true,
                            priority: 0,          // Could read from stat file
                            nice_value: 0,        // Could read from stat file
                            gpu_utilization: 0.0, // Not available without device-specific metrics
                        };

                        processes.push(process_info);
                        break; // Found Tenstorrent usage, no need to check other FDs
                    }
                }
            }
        }

        processes
    }

    /// Get username from UID
    fn get_username(uid: u32) -> String {
        if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
            for line in passwd.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 3 {
                    if let Ok(file_uid) = parts[2].parse::<u32>() {
                        if file_uid == uid {
                            return parts[0].to_string();
                        }
                    }
                }
            }
        }
        format!("uid:{uid}")
    }
}

impl GpuReader for TenstorrentReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        self.collect_npu_info()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.get_processes_with_info()
    }
}

/// Get the current Tenstorrent status message (if any)
pub fn get_tenstorrent_status_message() -> Option<String> {
    TENSTORRENT_STATUS.lock().ok()?.clone()
}
