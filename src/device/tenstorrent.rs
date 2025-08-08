// Copyright 2025 Lablup Inc. and Jeongkyu Shin
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::device::process_list::{get_all_processes, merge_gpu_processes};
use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use all_smi_luwen_if::chip::{Chip, ChipImpl};
use all_smi_luwen_if::ChipDetectOptions;
use chrono::Local;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use sysinfo::System;

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

// Static device information that doesn't change after initialization
#[derive(Clone)]
struct StaticDeviceInfo {
    uuid: String,
    device_name: String,
    board_type: String,
    board_id: String,
    pcie_address: Option<String>,
    pcie_vendor_id: Option<String>,
    pcie_device_id: Option<String>,
    pcie_link_width: Option<String>,
    pcie_link_gen: Option<String>,
    arc_fw_version: String,
    eth_fw_version: String,
    fw_date: String,
    ddr_fw_version: Option<String>,
    spibootrom_fw_version: Option<String>,
    total_memory: u64,
    tdp_limit: f64,
}

// Cache entry containing both chip and its static info
struct CachedChipInfo {
    chip: Chip,
    static_info: StaticDeviceInfo,
}

// Cache for initialized chips and their static info to avoid re-initialization on every measurement
static INITIALIZED_CHIPS: Lazy<Mutex<Option<Vec<CachedChipInfo>>>> = Lazy::new(|| Mutex::new(None));

pub struct TenstorrentReader {
    config: TenstorrentConfig,
}

impl Default for TenstorrentReader {
    fn default() -> Self {
        Self::new()
    }
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
            if let Some(ref cached_chips) = *cache {
                // Use cached chips and their static info
                let mut devices = Vec::new();
                for (idx, cached_info) in cached_chips.iter().enumerate() {
                    if let Some(info) = self.read_device_info_with_cache(
                        &cached_info.chip,
                        &cached_info.static_info,
                        idx,
                    ) {
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
                let mut cached_chips = Vec::new();
                let mut devices = Vec::new();

                // Initialize each chip and collect info
                for (idx, uninit_chip) in uninit_chips.into_iter().enumerate() {
                    // Initialize the chip without progress callbacks
                    match uninit_chip.init(&mut |_| Ok::<(), std::convert::Infallible>(())) {
                        Ok(chip) => {
                            // Extract static info on first initialization
                            if let Some(static_info) = self.extract_static_info(&chip, idx) {
                                // Read full device info for the first time
                                if let Some(device_info) =
                                    self.read_device_info_with_cache(&chip, &static_info, idx)
                                {
                                    devices.push(device_info);
                                }
                                cached_chips.push(CachedChipInfo { chip, static_info });
                            }
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

                    // Cache the initialized chips and their static info for future use
                    if let Ok(mut cache) = INITIALIZED_CHIPS.lock() {
                        *cache = Some(cached_chips);
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

    /// Extract static device information that doesn't change after initialization
    fn extract_static_info(&self, chip: &Chip, index: usize) -> Option<StaticDeviceInfo> {
        // Try to get telemetry from the chip
        match chip.get_telemetry() {
            Ok(telemetry) => {
                // Get board type name
                let board_type = telemetry.try_board_type().unwrap_or("Unknown");
                let device_name = format!(
                    "Tenstorrent {} {board_type}",
                    match telemetry.arch {
                        all_smi_luwen_core::Arch::Grayskull => "Grayskull",
                        all_smi_luwen_core::Arch::Wormhole => "Wormhole",
                        all_smi_luwen_core::Arch::Blackhole => "Blackhole",
                    }
                );

                // Extract PCIe information if available
                let (pcie_address, pcie_vendor_id, pcie_device_id, pcie_link_width, pcie_link_gen) =
                    if let Ok(Some(device_info)) = chip.get_device_info() {
                        (
                            Some(format!(
                                "{:04x}:{:02x}:{:02x}.{:x}",
                                device_info.domain,
                                device_info.bus,
                                device_info.slot,
                                device_info.function
                            )),
                            Some(format!("0x{:04x}", device_info.vendor)),
                            Some(format!("0x{:04x}", device_info.device_id)),
                            Some(format!("x{}", device_info.pcie_current_link_width())),
                            Some(format!("Gen{}", device_info.pcie_current_link_gen())),
                        )
                    } else {
                        (None, None, None, None, None)
                    };

                // Extract firmware versions
                let ddr_fw_version = if telemetry.ddr_fw_version != 0 {
                    Some(format!(
                        "{}.{}.{}",
                        (telemetry.ddr_fw_version >> 16) & 0xFF,
                        (telemetry.ddr_fw_version >> 8) & 0xFF,
                        telemetry.ddr_fw_version & 0xFF
                    ))
                } else {
                    None
                };

                let spibootrom_fw_version = if telemetry.spibootrom_fw_version != 0 {
                    Some(format!(
                        "{}.{}.{}",
                        (telemetry.spibootrom_fw_version >> 16) & 0xFF,
                        (telemetry.spibootrom_fw_version >> 8) & 0xFF,
                        telemetry.spibootrom_fw_version & 0xFF
                    ))
                } else {
                    None
                };

                // Calculate total memory based on board type
                let total_memory = match telemetry.board_type() {
                    // Grayskull boards
                    "e75" => 16 * 1024 * 1024 * 1024,  // 16GB
                    "e150" => 32 * 1024 * 1024 * 1024, // 32GB
                    "e300" | "e300_R2" | "e300_R3" => 48 * 1024 * 1024 * 1024, // 48GB
                    "GALAXY" => 96 * 1024 * 1024 * 1024, // 96GB (Galaxy has 2x48GB)
                    // Wormhole boards
                    "n150" => 32 * 1024 * 1024 * 1024,      // 32GB
                    "n300" => 64 * 1024 * 1024 * 1024,      // 64GB
                    "NEBULA_CB" => 32 * 1024 * 1024 * 1024, // 32GB
                    "galaxy-wormhole" => 96 * 1024 * 1024 * 1024, // 96GB per board
                    // Blackhole boards
                    "p100" | "p100a" => 96 * 1024 * 1024 * 1024, // 96GB
                    "p150a" | "p150b" | "p150c" => 144 * 1024 * 1024 * 1024, // 144GB
                    "p300a" | "p300b" | "p300c" => 288 * 1024 * 1024 * 1024, // 288GB
                    "galaxy-blackhole" => 576 * 1024 * 1024 * 1024, // 576GB
                    _ => {
                        // Conservative memory estimates based on architecture
                        match telemetry.arch {
                            all_smi_luwen_core::Arch::Grayskull => 16 * 1024 * 1024 * 1024,
                            all_smi_luwen_core::Arch::Wormhole => 32 * 1024 * 1024 * 1024,
                            all_smi_luwen_core::Arch::Blackhole => 96 * 1024 * 1024 * 1024,
                        }
                    }
                };

                // Calculate TDP limit
                let tdp_limit = {
                    // First try to get TDP limit from telemetry (upper 16 bits)
                    let tdp_limit_from_telemetry = ((telemetry.tdp >> 16) & 0xFFFF) as f64;

                    if tdp_limit_from_telemetry > 0.0 {
                        tdp_limit_from_telemetry
                    } else {
                        // Fallback to board-specific TDP estimates
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
                    }
                };

                Some(StaticDeviceInfo {
                    uuid: telemetry.board_serial_number_hex(),
                    device_name,
                    board_type: board_type.to_string(),
                    board_id: telemetry.board_serial_number_hex(),
                    pcie_address,
                    pcie_vendor_id,
                    pcie_device_id,
                    pcie_link_width,
                    pcie_link_gen,
                    arc_fw_version: telemetry.arc_fw_version(),
                    eth_fw_version: telemetry.eth_fw_version(),
                    fw_date: telemetry.firmware_date(),
                    ddr_fw_version,
                    spibootrom_fw_version,
                    total_memory,
                    tdp_limit,
                })
            }
            Err(e) => {
                eprintln!("Failed to get telemetry for device {index}: {e}");
                None
            }
        }
    }

    /// Read device information using cached static info and current dynamic telemetry
    fn read_device_info_with_cache(
        &self,
        chip: &Chip,
        static_info: &StaticDeviceInfo,
        index: usize,
    ) -> Option<GpuInfo> {
        // Try to get current telemetry from the chip
        match chip.get_telemetry() {
            Ok(telemetry) => {
                let hostname = get_hostname();
                let time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();

                let mut detail = HashMap::new();

                // Add static information from cache
                detail.insert("board_type".to_string(), static_info.board_type.clone());
                detail.insert("board_id".to_string(), static_info.board_id.clone());
                detail.insert("collection_method".to_string(), "luwen".to_string());

                if let Some(ref pcie_address) = static_info.pcie_address {
                    detail.insert("pcie_address".to_string(), pcie_address.clone());
                }
                if let Some(ref pcie_vendor_id) = static_info.pcie_vendor_id {
                    detail.insert("pcie_vendor_id".to_string(), pcie_vendor_id.clone());
                }
                if let Some(ref pcie_device_id) = static_info.pcie_device_id {
                    detail.insert("pcie_device_id".to_string(), pcie_device_id.clone());
                }
                if let Some(ref pcie_link_width) = static_info.pcie_link_width {
                    detail.insert("pcie_link_width".to_string(), pcie_link_width.clone());
                }
                if let Some(ref pcie_link_gen) = static_info.pcie_link_gen {
                    detail.insert("pcie_link_gen".to_string(), pcie_link_gen.clone());
                }

                // Add firmware versions from static cache
                detail.insert(
                    "arc_fw_version".to_string(),
                    static_info.arc_fw_version.clone(),
                );
                detail.insert(
                    "eth_fw_version".to_string(),
                    static_info.eth_fw_version.clone(),
                );
                detail.insert("fw_date".to_string(), static_info.fw_date.clone());

                if let Some(ref ddr_fw_version) = static_info.ddr_fw_version {
                    detail.insert("ddr_fw_version".to_string(), ddr_fw_version.clone());
                }
                if let Some(ref spibootrom_fw_version) = static_info.spibootrom_fw_version {
                    detail.insert(
                        "spibootrom_fw_version".to_string(),
                        spibootrom_fw_version.clone(),
                    );
                }

                // Add dynamic telemetry data
                detail.insert("voltage".to_string(), format!("{:.2}", telemetry.voltage()));
                detail.insert("current".to_string(), format!("{:.1}", telemetry.current()));

                // Add TDP/TDC limits if available
                let tdc_limit = ((telemetry.tdc >> 16) & 0xFFFF) as f64;
                if static_info.tdp_limit > 0.0 {
                    detail.insert(
                        "tdp_limit".to_string(),
                        format!("{:.0}", static_info.tdp_limit),
                    );
                }
                if tdc_limit > 0.0 {
                    detail.insert("tdc_limit".to_string(), format!("{tdc_limit:.0}"));
                }

                // Add dynamic temperature readings
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

                // Get dynamic metrics
                let temperature = telemetry.asic_temperature().round() as u32;
                let power = telemetry.power();
                let frequency = telemetry.ai_clk();

                // Calculate utilization based on power consumption vs TDP
                let utilization = ((power / static_info.tdp_limit) * 100.0).min(100.0);

                // Add dynamic power and clock info
                detail.insert("power_watts".to_string(), format!("{power:.2}"));
                detail.insert("aiclk_mhz".to_string(), format!("{frequency}"));
                detail.insert("axiclk_mhz".to_string(), format!("{}", telemetry.axi_clk()));
                detail.insert("arcclk_mhz".to_string(), format!("{}", telemetry.arc_clk()));

                // Add dynamic status fields
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

                // Add dynamic health/heartbeat counters
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

                // Add dynamic fault and throttler information
                if telemetry.faults != 0 {
                    detail.insert("faults".to_string(), format!("0x{:08x}", telemetry.faults));
                }
                if telemetry.throttler != 0 {
                    detail.insert(
                        "throttler".to_string(),
                        format!("0x{:08x}", telemetry.throttler),
                    );
                }

                // Add dynamic fan information if available
                if telemetry.fan_speed != 0 {
                    detail.insert("fan_speed".to_string(), format!("{}", telemetry.fan_speed));
                }
                if telemetry.fan_rpm != 0 {
                    detail.insert("fan_rpm".to_string(), format!("{}", telemetry.fan_rpm));
                }

                // Calculate dynamic memory usage
                let (used_memory, total_memory) = if telemetry.ddr_status != 0 {
                    // Add DDR speed if available
                    if let Some(ddr_speed) = telemetry.ddr_speed {
                        detail.insert("ddr_speed".to_string(), format!("{ddr_speed}"));
                    }

                    // Dynamic memory usage estimation
                    let memory_utilization_estimate = {
                        let power_factor = (power / static_info.tdp_limit).min(1.0);
                        let throttler_factor = if telemetry.throttler != 0 { 0.9 } else { 0.7 };
                        let freq_factor = (frequency as f64 / 1000.0).min(1.0);
                        let combined =
                            (power_factor * 0.5) + (freq_factor * 0.3) + (throttler_factor * 0.2);
                        combined.clamp(0.05, 0.95)
                    };

                    let used_mem =
                        (static_info.total_memory as f64 * memory_utilization_estimate) as u64;
                    (used_mem, static_info.total_memory)
                } else {
                    (0, 0)
                };

                Some(GpuInfo {
                    uuid: static_info.uuid.clone(),
                    time,
                    name: static_info.device_name.clone(),
                    device_type: "NPU".to_string(),
                    host_id: hostname.clone(), // For local mode, host_id is just the hostname
                    hostname: hostname.clone(), // DNS hostname
                    instance: hostname.clone(),
                    utilization,
                    ane_utilization: 0.0,
                    dla_utilization: None,
                    temperature,
                    used_memory,
                    total_memory,
                    frequency,
                    power_consumption: power,
                    gpu_core_count: None,
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
    fn get_processes_via_device_files(&self) -> (Vec<ProcessInfo>, HashSet<u32>) {
        let mut gpu_processes = Vec::new();
        let mut gpu_pids = HashSet::new();

        // Get cached device UUIDs for mapping
        let device_uuids = if let Ok(cache) = INITIALIZED_CHIPS.lock() {
            if let Some(ref cached_chips) = *cache {
                cached_chips
                    .iter()
                    .enumerate()
                    .map(|(idx, info)| (idx as u32, info.static_info.uuid.clone()))
                    .collect::<HashMap<u32, String>>()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        #[cfg(target_os = "linux")]
        {
            // On Linux, scan /proc/*/fd/* to find processes with Tenstorrent device files open
            if let Ok(entries) = std::fs::read_dir("/proc") {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Check if this is a PID directory
                        if let Some(pid_str) = path.file_name().and_then(|s| s.to_str()) {
                            if let Ok(pid) = pid_str.parse::<u32>() {
                                // Skip kernel threads (they don't have fd directory)
                                let fd_path = path.join("fd");
                                if !fd_path.exists() {
                                    continue;
                                }

                                // Check the file descriptors for this process
                                if let Ok(fd_entries) = std::fs::read_dir(&fd_path) {
                                    let mut process_devices = Vec::new();

                                    for fd_entry in fd_entries.flatten() {
                                        // Read the symlink to see what file it points to
                                        if let Ok(target) = std::fs::read_link(fd_entry.path()) {
                                            if let Some(target_str) = target.to_str() {
                                                // Check if this is a Tenstorrent device file
                                                if target_str.starts_with("/dev/tenstorrent/") {
                                                    // Extract device ID from path
                                                    if let Some(device_id) = target_str
                                                        .strip_prefix("/dev/tenstorrent/")
                                                        .and_then(|s| s.parse::<u32>().ok())
                                                    {
                                                        process_devices.push(device_id);
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Add process info for each device it's using
                                    for device_id in process_devices {
                                        gpu_pids.insert(pid);

                                        let device_uuid = device_uuids
                                            .get(&device_id)
                                            .cloned()
                                            .unwrap_or_else(|| format!("tt{device_id}"));

                                        gpu_processes.push(ProcessInfo {
                                            device_id: device_id as usize,
                                            device_uuid,
                                            pid,
                                            process_name: String::new(), // Will be filled by sysinfo
                                            used_memory: 0, // Can't determine without proper API
                                            cpu_percent: 0.0,
                                            memory_percent: 0.0,
                                            memory_rss: 0,
                                            memory_vms: 0,
                                            user: String::new(),
                                            state: String::new(),
                                            start_time: String::new(),
                                            cpu_time: 0,
                                            command: String::new(),
                                            ppid: 0,
                                            threads: 0,
                                            uses_gpu: true,
                                            priority: 0,
                                            nice_value: 0,
                                            gpu_utilization: 0.0,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, use lsof to find processes with Tenstorrent device files open
            use std::path::Path;
            use std::process::Command;

            // First, check if any Tenstorrent devices exist
            let dev_path = Path::new("/dev");
            let device_files: Vec<String> = if let Ok(entries) = std::fs::read_dir(dev_path) {
                entries
                    .flatten()
                    .filter_map(|entry| {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with("tenstorrent") {
                            Some(format!("/dev/{name}"))
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                vec![]
            };

            if !device_files.is_empty() {
                // Use lsof to find processes using specific Tenstorrent devices
                for device_file in &device_files {
                    if let Ok(output) = Command::new("lsof")
                        .args(["-F", "pn", device_file])
                        .output()
                    {
                        let output_str = String::from_utf8_lossy(&output.stdout);
                        let mut current_pid = None;

                        for line in output_str.lines() {
                            if let Some(pid_str) = line.strip_prefix('p') {
                                current_pid = pid_str.parse::<u32>().ok();
                            } else if let Some(device_path) = line.strip_prefix('n') {
                                if let Some(pid) = current_pid {
                                    // Extract device ID from path
                                    let device_id = device_path
                                        .strip_prefix("/dev/tenstorrent")
                                        .and_then(|s| s.parse::<u32>().ok())
                                        .unwrap_or(0);

                                    gpu_pids.insert(pid);

                                    let device_uuid = device_uuids
                                        .get(&device_id)
                                        .cloned()
                                        .unwrap_or_else(|| format!("tt{device_id}"));

                                    gpu_processes.push(ProcessInfo {
                                        device_id: device_id as usize,
                                        device_uuid,
                                        pid,
                                        process_name: String::new(),
                                        used_memory: 0,
                                        cpu_percent: 0.0,
                                        memory_percent: 0.0,
                                        memory_rss: 0,
                                        memory_vms: 0,
                                        user: String::new(),
                                        state: String::new(),
                                        start_time: String::new(),
                                        cpu_time: 0,
                                        command: String::new(),
                                        ppid: 0,
                                        threads: 0,
                                        uses_gpu: true,
                                        priority: 0,
                                        nice_value: 0,
                                        gpu_utilization: 0.0,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        (gpu_processes, gpu_pids)
    }

    /// Collect process info using the configured method with fallback
    fn collect_process_info(&self) -> (Vec<ProcessInfo>, HashSet<u32>) {
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
        // Create a new system instance and refresh it
        let mut system = System::new_all();
        system.refresh_all();

        // Get GPU processes and PIDs
        let (gpu_processes, gpu_pids) = self.collect_process_info();

        // Get all system processes
        let mut all_processes = get_all_processes(&system, &gpu_pids);

        // Merge GPU information into the process list
        merge_gpu_processes(&mut all_processes, gpu_processes);

        all_processes
    }
}

/// Get the current Tenstorrent status message (if any)
pub fn get_tenstorrent_status_message() -> Option<String> {
    TENSTORRENT_STATUS.lock().ok()?.clone()
}
