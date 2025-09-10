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
use crate::device::types::{GpuInfo, ProcessInfo};
use crate::device::GpuReader;
use crate::utils::get_hostname;
use all_smi_luwen_core;
use all_smi_luwen_if::chip::{Chip, ChipImpl, Telemetry};
use all_smi_luwen_if::ChipDetectOptions;
use all_smi_luwen_ref;
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
    /// Primary method to use for collecting metrics (reserved for future use)
    pub _primary_method: CollectionMethod,
}

impl Default for TenstorrentConfig {
    fn default() -> Self {
        Self {
            _primary_method: CollectionMethod::DeviceFile,
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
    _config: TenstorrentConfig,
}

impl Default for TenstorrentReader {
    fn default() -> Self {
        Self::new()
    }
}

impl TenstorrentReader {
    pub fn new() -> Self {
        Self {
            _config: TenstorrentConfig::default(),
        }
    }

    #[allow(dead_code)]
    pub fn with_config(config: TenstorrentConfig) -> Self {
        Self { _config: config }
    }

    /// Get or initialize chips with caching
    fn ensure_chips_initialized() {
        let mut chips_guard = match INITIALIZED_CHIPS.lock() {
            Ok(guard) => guard,
            Err(e) => {
                eprintln!("Failed to acquire lock for Tenstorrent chips: {e}");
                return;
            }
        };

        if chips_guard.is_some() {
            return;
        }

        // Detect and initialize chips
        let options = ChipDetectOptions {
            local_only: true,
            ..Default::default()
        };
        let uninit_chips = match all_smi_luwen_ref::detect_chips_silent(options) {
            Ok(chips) => chips,
            Err(e) => {
                set_tenstorrent_status(format!("Failed to detect Tenstorrent chips: {e}"));
                return;
            }
        };

        let cached_chips: Vec<CachedChipInfo> = uninit_chips
            .into_iter()
            .filter_map(|uninit_chip| {
                // Initialize the chip
                match uninit_chip.init(&mut |_| Ok::<(), std::convert::Infallible>(())) {
                    Ok(chip) => {
                        let static_info = extract_static_info(&chip)?;
                        Some(CachedChipInfo { chip, static_info })
                    }
                    Err(_) => None, // This should never happen with Infallible
                }
            })
            .collect();

        if cached_chips.is_empty() {
            set_tenstorrent_status("No Tenstorrent chips detected".to_string());
        } else {
            clear_tenstorrent_status();
        }

        *chips_guard = Some(cached_chips);
    }

    /// Invalidate cache to force re-detection on next access
    #[allow(dead_code)]
    pub fn invalidate_cache() {
        if let Ok(mut chips_guard) = INITIALIZED_CHIPS.lock() {
            *chips_guard = None;
        } else {
            eprintln!("Failed to acquire lock to invalidate Tenstorrent cache");
        }
    }

    /// Get NPU processes (currently returns empty - Tenstorrent doesn't provide process info)
    fn get_npu_processes(&self) -> (Vec<ProcessInfo>, HashSet<u32>) {
        (Vec::new(), HashSet::new())
    }
}

impl GpuReader for TenstorrentReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        Self::ensure_chips_initialized();

        let chips_guard = match INITIALIZED_CHIPS.lock() {
            Ok(guard) => guard,
            Err(e) => {
                eprintln!("Failed to acquire lock for Tenstorrent chips: {e}");
                return Vec::new();
            }
        };
        let cached_chips = match chips_guard.as_ref() {
            Some(chips) => chips,
            None => return Vec::new(),
        };

        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let hostname = get_hostname();

        cached_chips
            .iter()
            .enumerate()
            .filter_map(|(index, cached)| {
                create_gpu_info(&cached.chip, &cached.static_info, index, &time, &hostname)
            })
            .collect()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // Create system instance and refresh processes
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything().with_user(UpdateKind::Always),
        );
        system.refresh_memory();

        // Get NPU processes (currently empty for Tenstorrent)
        let (npu_processes, npu_pids) = self.get_npu_processes();

        // Get all system processes
        let mut all_processes = get_all_processes(&system, &npu_pids);

        // Merge NPU information
        merge_gpu_processes(&mut all_processes, npu_processes);

        all_processes
    }
}

// Helper functions

fn set_tenstorrent_status(message: String) {
    if let Ok(mut status) = TENSTORRENT_STATUS.lock() {
        *status = Some(message);
    }
}

fn clear_tenstorrent_status() {
    if let Ok(mut status) = TENSTORRENT_STATUS.lock() {
        *status = None;
    }
}

/// Get a user-friendly message about Tenstorrent status
#[allow(dead_code)]
pub fn get_tenstorrent_status_message() -> Option<String> {
    TENSTORRENT_STATUS.lock().ok()?.clone()
}

fn extract_static_info(chip: &Chip) -> Option<StaticDeviceInfo> {
    // Get telemetry
    let telem = chip.get_telemetry().ok()?;

    // Get board type name
    let board_type = telem.try_board_type().unwrap_or("Unknown");
    let device_name = format!(
        "Tenstorrent {} {board_type}",
        match telem.arch {
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
                    device_info.domain, device_info.bus, device_info.slot, device_info.function
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
    let ddr_fw_version = if telem.ddr_fw_version != 0 {
        Some(format!(
            "{}.{}.{}",
            (telem.ddr_fw_version >> 16) & 0xFF,
            (telem.ddr_fw_version >> 8) & 0xFF,
            telem.ddr_fw_version & 0xFF
        ))
    } else {
        None
    };

    let spibootrom_fw_version = if telem.spibootrom_fw_version != 0 {
        Some(format!(
            "{}.{}.{}",
            (telem.spibootrom_fw_version >> 16) & 0xFF,
            (telem.spibootrom_fw_version >> 8) & 0xFF,
            telem.spibootrom_fw_version & 0xFF
        ))
    } else {
        None
    };

    // Determine memory size and TDP based on board type
    let (total_memory, tdp_limit) = determine_memory_and_tdp(board_type);

    Some(StaticDeviceInfo {
        uuid: telem.board_serial_number_hex(),
        device_name,
        board_type: board_type.to_string(),
        board_id: telem.board_serial_number_hex(),
        pcie_address,
        pcie_vendor_id,
        pcie_device_id,
        pcie_link_width,
        pcie_link_gen,
        arc_fw_version: telem.arc_fw_version(),
        eth_fw_version: telem.eth_fw_version(),
        fw_date: telem.firmware_date(),
        ddr_fw_version,
        spibootrom_fw_version,
        total_memory,
        tdp_limit,
    })
}

fn determine_memory_and_tdp(board_type: &str) -> (u64, f64) {
    match board_type {
        s if s.contains("e75") => (2 * 1024 * 1024 * 1024, 75.0), // 2GB, 75W
        s if s.contains("e150") => (8 * 1024 * 1024 * 1024, 200.0), // 8GB, 200W
        s if s.contains("e300") => (12 * 1024 * 1024 * 1024, 300.0), // 12GB, 300W
        s if s.contains("galaxy") => (32 * 1024 * 1024 * 1024, 200.0), // 32GB, 200W
        s if s.contains("n150") => (48 * 1024 * 1024 * 1024, 160.0), // 48GB, 160W
        s if s.contains("n300") => (96 * 1024 * 1024 * 1024, 300.0), // 96GB, 300W
        _ => (8 * 1024 * 1024 * 1024, 200.0),                     // Default: 8GB, 200W
    }
}

fn create_gpu_info(
    chip: &Chip,
    static_info: &StaticDeviceInfo,
    _index: usize,
    time: &str,
    hostname: &str,
) -> Option<GpuInfo> {
    // Get current telemetry
    let telem = chip.get_telemetry().ok()?;

    // Build device details
    let detail = build_device_details(static_info, &telem);

    // Get dynamic metrics with safe defaults
    let temperature = telem.asic_temperature().round() as u32;
    let power = calculate_power(&telem);
    let frequency = telem.ai_clk();
    let utilization = estimate_utilization(&telem, static_info.tdp_limit);

    Some(GpuInfo {
        uuid: static_info.uuid.clone(),
        time: time.to_string(),
        name: format!("Tenstorrent {}", static_info.device_name),
        device_type: "NPU".to_string(),
        host_id: hostname.to_string(),
        hostname: hostname.to_string(),
        instance: hostname.to_string(),
        utilization,
        ane_utilization: 0.0,
        dla_utilization: None,
        temperature,
        used_memory: 0, // TODO: Implement memory tracking
        total_memory: static_info.total_memory,
        frequency,
        power_consumption: power,
        gpu_core_count: None,
        detail,
    })
}

fn build_device_details(
    static_info: &StaticDeviceInfo,
    telem: &Telemetry,
) -> HashMap<String, String> {
    let mut detail = HashMap::new();

    // Static information
    detail.insert("Board Type".to_string(), static_info.board_type.clone());
    detail.insert("Board ID".to_string(), static_info.board_id.clone());
    detail.insert(
        "ARC Firmware".to_string(),
        static_info.arc_fw_version.clone(),
    );
    detail.insert(
        "ETH Firmware".to_string(),
        static_info.eth_fw_version.clone(),
    );
    detail.insert("Firmware Date".to_string(), static_info.fw_date.clone());

    // Optional static info
    if let Some(ref addr) = static_info.pcie_address {
        detail.insert("PCIe Address".to_string(), addr.clone());
    }
    if let Some(ref vid) = static_info.pcie_vendor_id {
        detail.insert("PCIe Vendor ID".to_string(), vid.clone());
    }
    if let Some(ref did) = static_info.pcie_device_id {
        detail.insert("PCIe Device ID".to_string(), did.clone());
    }
    if let Some(ref width) = static_info.pcie_link_width {
        detail.insert("PCIe Link Width".to_string(), width.clone());
    }
    if let Some(ref gen) = static_info.pcie_link_gen {
        detail.insert("PCIe Link Gen".to_string(), gen.clone());
    }
    if let Some(ref ddr_fw) = static_info.ddr_fw_version {
        detail.insert("DDR Firmware".to_string(), ddr_fw.clone());
    }
    if let Some(ref spi_fw) = static_info.spibootrom_fw_version {
        detail.insert("SPI Bootrom Firmware".to_string(), spi_fw.clone());
    }

    // Dynamic telemetry
    detail.insert(
        "VDD Voltage".to_string(),
        format!("{:.3}V", telem.voltage()),
    );
    detail.insert("Current".to_string(), format!("{:.2}A", telem.current()));
    detail.insert(
        "ASIC Temperature".to_string(),
        format!("{:.1}°C", telem.asic_temperature()),
    );
    detail.insert(
        "VR Temperature".to_string(),
        format!("{:.1}°C", telem.vreg_temperature()),
    );

    if telem.board_temperature != 0 {
        detail.insert(
            "Inlet Temperature".to_string(),
            format!("{:.1}°C", telem.inlet_temperature()),
        );
    }

    detail.insert("AI Clock".to_string(), format!("{}MHz", telem.ai_clk()));
    detail.insert("ARC Clock".to_string(), format!("{}MHz", telem.arc_clk()));
    detail.insert("AXI Clock".to_string(), format!("{}MHz", telem.axi_clk()));

    detail
}

fn calculate_power(telem: &Telemetry) -> f64 {
    // Calculate power from voltage and current
    // Use telem.power() which internally does voltage * current
    telem.power()
}

fn estimate_utilization(telem: &Telemetry, tdp_limit: f64) -> f64 {
    // Primary method: Power-based utilization
    let power = calculate_power(telem);
    let power_utilization = (power / tdp_limit * 100.0).min(100.0);

    // Secondary method: Clock frequency based
    // Assume max AI clock is around 1000-1200 MHz for most Tenstorrent chips
    let ai_clk = telem.ai_clk() as f64;
    let max_clk = 1200.0; // Conservative max frequency
    let clock_utilization = (ai_clk / max_clk * 100.0).min(100.0);

    // Tertiary method: Heartbeat counter as activity indicator
    // The heartbeat counter increments when the chip is active
    let heartbeat = telem.telemetry_heartbeat();
    let heartbeat_active = if heartbeat > 0 { 1.0 } else { 0.0 };

    // Combine methods with weighted average
    // Power is most reliable (60%), clock is secondary (30%), heartbeat is tertiary (10%)
    (power_utilization * 0.6 + clock_utilization * 0.3 + heartbeat_active * 10.0).min(100.0)
}
