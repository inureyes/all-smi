// SPDX-FileCopyrightText: © 2025 All-SMI Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tenstorrent device implementation with proper initialization
//! Based on TT-REPORT.md specifications

use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{
    arc_protocol::{ArcMessageHandler, ArcProtocol, ArcRegisters},
    arch::Arch,
    bar::BarManager,
    chip::Telemetry,
    error::PlatformError,
    ttkmd::{ioctl::ioctl_get_device_info, kmdif::DeviceInfo},
};

/// Telemetry collection interval for caching
const TELEMETRY_CACHE_DURATION: Duration = Duration::from_millis(500);

/// ARC initialization timeout
const ARC_INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Heartbeat check interval
const HEARTBEAT_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// Minimum heartbeat increment to consider healthy
const MIN_HEARTBEAT_INCREMENT: u32 = 1;

/// Chip initialization status
#[derive(Debug, Clone)]
pub struct InitStatus {
    pub arc_ready: bool,
    pub dram_ready: bool,
    pub eth_ready: bool,
    pub pcie_ready: bool,
    pub init_complete: bool,
    pub error_message: Option<String>,
}

impl InitStatus {
    pub fn new() -> Self {
        Self {
            arc_ready: false,
            dram_ready: false,
            eth_ready: false,
            pcie_ready: false,
            init_complete: false,
            error_message: None,
        }
    }

    pub fn has_error(&self) -> bool {
        self.error_message.is_some()
    }
}

/// Cached telemetry data
struct TelemetryCache {
    data: Option<Telemetry>,
    last_update: Instant,
    telemetry_addr: Option<u32>,
}

/// Tenstorrent device implementation
pub struct TenstorrentDevice {
    pub device_id: usize,
    pub arch: Arch,
    pub device_info: DeviceInfo,
    bar_manager: BarManager,
    telemetry_cache: Arc<Mutex<TelemetryCache>>,
    init_status: InitStatus,
}

impl TenstorrentDevice {
    /// Open a Tenstorrent device
    pub fn open(device_id: usize) -> Result<Self, PlatformError> {
        eprintln!("[DEBUG] Opening Tenstorrent device {device_id}");

        // Open device file for ioctl
        let path = format!("/dev/tenstorrent/{device_id}");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| PlatformError::IoError(format!("Failed to open device {path}: {e}")))?;

        // Get device info via ioctl
        let device_info = unsafe {
            let mut info = std::mem::MaybeUninit::<DeviceInfo>::uninit();
            ioctl_get_device_info(file.as_raw_fd(), info.as_mut_ptr())
                .map_err(|e| PlatformError::IoError(format!("Failed to get device info: {e}")))?;
            info.assume_init()
        };

        // Determine architecture from device ID
        let arch = match device_info.device_id {
            0xfaca => Arch::Grayskull,
            0x401e => Arch::Wormhole,
            0xb140 => Arch::Blackhole,
            _ => {
                return Err(PlatformError::InvalidParameter(format!(
                    "Unknown device ID: 0x{:x}",
                    device_info.device_id
                )))
            }
        };

        eprintln!("[DEBUG] Device architecture: {arch:?}");

        // Create BAR manager and map BARs
        let mut bar_manager = BarManager::new(device_id)?;
        bar_manager.map_bars()?;

        let device = Self {
            device_id,
            arch,
            device_info,
            bar_manager,
            telemetry_cache: Arc::new(Mutex::new(TelemetryCache {
                data: None,
                last_update: Instant::now() - TELEMETRY_CACHE_DURATION * 2,
                telemetry_addr: None,
            })),
            init_status: InitStatus::new(),
        };

        Ok(device)
    }

    /// Initialize the device and wait for it to be ready
    pub fn wait_for_init(&mut self) -> Result<(), PlatformError> {
        eprintln!("[DEBUG] Initializing device {}...", self.device_id);

        // First check if ARC is responsive
        match ArcProtocol::wait_for_arc_ready(self, ARC_INIT_TIMEOUT) {
            Ok(()) => {
                self.init_status.arc_ready = true;
                eprintln!("[DEBUG] ARC firmware is ready");
            }
            Err(e) => {
                self.init_status.error_message = Some(format!("ARC not ready: {e}"));
                return Err(e);
            }
        }

        // Verify heartbeat is incrementing
        if let Err(e) = self.verify_heartbeat() {
            self.init_status.error_message = Some(format!("Heartbeat check failed: {e}"));
            return Err(e);
        }

        // Get initial telemetry to check other subsystems
        match self.get_telemetry() {
            Ok(telemetry) => {
                // Check DRAM status
                if telemetry.ddr_status != 0 {
                    self.init_status.dram_ready = true;
                } else {
                    eprintln!("[WARN] DRAM not initialized (status=0)");
                }

                // Check PCIe status
                if telemetry.pcie_status != 0 {
                    self.init_status.pcie_ready = true;
                } else {
                    eprintln!("[WARN] PCIe not initialized (status=0)");
                }

                // Check Ethernet status (for Wormhole/Blackhole)
                if self.arch != Arch::Grayskull {
                    if telemetry.eth_status0 != 0 || telemetry.eth_status1 != 0 {
                        self.init_status.eth_ready = true;
                    } else {
                        eprintln!("[WARN] Ethernet not initialized");
                    }
                } else {
                    self.init_status.eth_ready = true; // N/A for Grayskull
                }

                self.init_status.init_complete = true;
                eprintln!("[DEBUG] Device initialization complete");
                Ok(())
            }
            Err(e) => {
                self.init_status.error_message = Some(format!("Failed to get telemetry: {e}"));
                Err(e)
            }
        }
    }

    /// Verify that the heartbeat counter is incrementing
    fn verify_heartbeat(&self) -> Result<(), PlatformError> {
        eprintln!("[DEBUG] Verifying heartbeat...");

        let first_telemetry = self.get_telemetry_uncached()?;
        let first_heartbeat = first_telemetry.telemetry_heartbeat();

        std::thread::sleep(HEARTBEAT_CHECK_INTERVAL);

        let second_telemetry = self.get_telemetry_uncached()?;
        let second_heartbeat = second_telemetry.telemetry_heartbeat();

        let increment = second_heartbeat.wrapping_sub(first_heartbeat);

        if increment >= MIN_HEARTBEAT_INCREMENT {
            eprintln!("[DEBUG] Heartbeat is healthy (increment={increment})");
            Ok(())
        } else {
            Err(PlatformError::ChipError(format!(
                "Heartbeat not incrementing ({first_heartbeat}->{second_heartbeat})"
            )))
        }
    }

    /// Get telemetry data (cached)
    pub fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        let cache = self
            .telemetry_cache
            .lock()
            .map_err(|_| PlatformError::ChipError("Failed to lock telemetry cache".to_string()))?;

        // Check if cache is still valid
        if cache.data.is_some() && cache.last_update.elapsed() < TELEMETRY_CACHE_DURATION {
            return Ok(cache.data.as_ref().unwrap().clone());
        }

        // Cache miss - read fresh telemetry
        drop(cache); // Release lock before reading
        let telemetry = self.get_telemetry_uncached()?;

        // Update cache
        let mut cache = self
            .telemetry_cache
            .lock()
            .map_err(|_| PlatformError::ChipError("Failed to lock telemetry cache".to_string()))?;
        cache.data = Some(telemetry.clone());
        cache.last_update = Instant::now();

        Ok(telemetry)
    }

    /// Get telemetry data (uncached)
    fn get_telemetry_uncached(&self) -> Result<Telemetry, PlatformError> {
        let mut cache = self
            .telemetry_cache
            .lock()
            .map_err(|_| PlatformError::ChipError("Failed to lock telemetry cache".to_string()))?;

        // Get telemetry address if not cached
        if cache.telemetry_addr.is_none() {
            let addr = ArcProtocol::get_telemetry_addr(self)?;
            cache.telemetry_addr = Some(addr);
        }

        let telemetry_addr = cache.telemetry_addr.unwrap();
        drop(cache); // Release lock

        // Calculate CSM offset
        let csm_base = ArcRegisters::translate("ARC_CSM.DATA[0]")?;
        let offset = csm_base + (telemetry_addr - 0x10000000) as u64;

        eprintln!("[DEBUG] Reading telemetry from CSM offset: 0x{offset:x}");

        // Read telemetry struct fields
        let mut telemetry = Telemetry::default();
        telemetry.arch = self.arch;

        // Read all fields at 4-byte offsets
        telemetry.enum_version = self.axi_read32(offset)?;
        telemetry.device_id = self.axi_read32(offset + 4)?;
        telemetry.asic_ro = self.axi_read32(offset + 8)?;
        telemetry.asic_idd = self.axi_read32(offset + 12)?;
        telemetry.board_id_high = self.axi_read32(offset + 16)?;
        telemetry.board_id_low = self.axi_read32(offset + 20)?;

        // Firmware versions
        telemetry.arc0_fw_version = self.axi_read32(offset + 24)?;
        telemetry.arc1_fw_version = self.axi_read32(offset + 28)?;
        telemetry.arc2_fw_version = self.axi_read32(offset + 32)?;
        telemetry.arc3_fw_version = self.axi_read32(offset + 36)?;
        telemetry.spibootrom_fw_version = self.axi_read32(offset + 40)?;
        telemetry.eth_fw_version = self.axi_read32(offset + 44)?;
        telemetry.m3_bl_fw_version = self.axi_read32(offset + 48)?;
        telemetry.m3_app_fw_version = self.axi_read32(offset + 52)?;

        // Status fields
        telemetry.ddr_status = self.axi_read32(offset + 56)?;
        telemetry.eth_status0 = self.axi_read32(offset + 60)?;
        telemetry.eth_status1 = self.axi_read32(offset + 64)?;
        telemetry.pcie_status = self.axi_read32(offset + 68)?;
        telemetry.faults = self.axi_read32(offset + 72)?;

        // Health counters
        telemetry.arc0_health = self.axi_read32(offset + 76)?;
        telemetry.arc1_health = self.axi_read32(offset + 80)?;
        telemetry.arc2_health = self.axi_read32(offset + 84)?;
        telemetry.arc3_health = self.axi_read32(offset + 88)?;

        // Clock frequencies
        telemetry.aiclk = self.axi_read32(offset + 96)?;
        telemetry.axiclk = self.axi_read32(offset + 100)?;
        telemetry.arcclk = self.axi_read32(offset + 104)?;

        // Power and thermal
        telemetry.vcore = self.axi_read32(offset + 112)?;
        telemetry.asic_temperature = self.axi_read32(offset + 116)?;
        telemetry.vreg_temperature = self.axi_read32(offset + 120)?;
        telemetry.board_temperature = self.axi_read32(offset + 124)?;
        telemetry.tdp = self.axi_read32(offset + 128)?;
        telemetry.tdc = self.axi_read32(offset + 132)?;

        // Additional fields
        telemetry.vdd_limits = self.axi_read32(offset + 136)?;
        telemetry.thm_limits = self.axi_read32(offset + 140)?;
        telemetry.wh_fw_date = self.axi_read32(offset + 144)?;

        // Blackhole specific
        if self.arch == Arch::Blackhole {
            telemetry.timer_heartbeat = self.axi_read32(offset + 176)?;
        }

        Ok(telemetry)
    }

    /// Get the initialization status
    pub fn get_init_status(&self) -> &InitStatus {
        &self.init_status
    }

    /// Get the device architecture
    pub fn get_arch(&self) -> Arch {
        self.arch
    }
}

impl ArcMessageHandler for TenstorrentDevice {
    fn axi_read32(&self, addr: u64) -> Result<u32, PlatformError> {
        // For now, assume BAR 0 for AXI access
        // In a real implementation, this would need proper BAR selection
        self.bar_manager.read32(0, addr)
    }

    fn axi_write32(&self, addr: u64, value: u32) -> Result<(), PlatformError> {
        // For now, assume BAR 0 for AXI access
        self.bar_manager.write32(0, addr, value)
    }

    fn get_scratch_base(&self) -> Result<&'static str, PlatformError> {
        Ok(match self.arch {
            Arch::Blackhole => "arc_ss.reset_unit.SCRATCH_0",
            _ => "ARC_RESET.SCRATCH[0]",
        })
    }

    fn get_misc_cntl(&self) -> Result<&'static str, PlatformError> {
        Ok(match self.arch {
            Arch::Blackhole => "arc_ss.reset_unit.ARC_MISC_CNTL",
            _ => "ARC_RESET.ARC_MISC_CNTL",
        })
    }
}
