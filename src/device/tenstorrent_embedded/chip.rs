// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0
// Extracted and simplified from luwen-if for embedded use

use super::arch::Arch;
use std::any::Any;

/// Platform error type for chip operations
#[derive(Debug)]
pub struct PlatformError(pub String);

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PlatformError {}

/// Telemetry information from the chip
#[derive(Default, Debug)]
#[allow(dead_code)]
pub struct Telemetry {
    pub arch: Arch,
    pub board_id: u64,
    pub enum_version: u32,
    pub entry_count: u32,
    pub device_id: u32,
    pub asic_id: u32,
    pub asic_ro: u32,
    pub asic_idd: u32,
    pub board_id_high: u32,
    pub board_id_low: u32,
    pub harvesting_state: u32,
    pub update_telem_speed: u32,
    pub arc0_fw_version: u32,
    pub arc1_fw_version: u32,
    pub arc2_fw_version: u32,
    pub arc3_fw_version: u32,
    pub spibootrom_fw_version: u32,
    pub eth_fw_version: u32,
    pub ddr_fw_version: u32,
    pub l2cpu_fw_version: u32,
    pub m3_bl_fw_version: u32,
    pub m3_app_fw_version: u32,
    pub ddr_speed: Option<u32>,
    pub ddr_status: u32,
    pub eth_status0: u32,
    pub eth_status1: u32,
    pub pcie_status: u32,
    pub faults: u32,
    pub arc0_health: u32,
    pub arc1_health: u32,
    pub arc2_health: u32,
    pub arc3_health: u32,
    pub fan_speed: u32,
    pub aiclk: u32,
    pub axiclk: u32,
    pub arcclk: u32,
    pub l2cpuclk0: u32,
    pub l2cpuclk1: u32,
    pub l2cpuclk2: u32,
    pub l2cpuclk3: u32,
    pub throttler: u32,
    pub vcore: u32,
    pub asic_temperature: u32,
    pub vreg_temperature: u32,
    pub board_temperature: u32,
    pub tdp: u32,
    pub tdc: u32,
    pub vdd_limits: u32,
    pub thm_limits: u32,
    pub wh_fw_date: u32,
    pub asic_tmon0: u32,
    pub asic_tmon1: u32,
    pub mvddq_power: u32,
    pub gddr_train_temp0: u32,
    pub gddr_train_temp1: u32,
    pub asic_power: Option<u32>,
    pub aux_status: Option<u32>,
    pub boot_date: u32,
    pub rt_seconds: u32,
    pub eth_debug_status0: u32,
    pub eth_debug_status1: u32,
    pub tt_flash_version: u32,
    pub fw_bundle_version: u32,
    pub timer_heartbeat: u32,
    pub noc_translation_enabled: bool,
    pub tensix_enabled_col: u32,
    pub enabled_eth: u32,
    pub enabled_gddr: u32,
    pub enabled_l2cpu: u32,
    pub enabled_pcie: u32,
    pub fan_rpm: u32,
}

impl Telemetry {
    /// Return the AI clock speed in MHz.
    pub fn ai_clk(&self) -> u32 {
        self.aiclk & 0xffff
    }

    /// Return the core voltage in volts.
    pub fn voltage(&self) -> f64 {
        self.vcore as f64 / 1000.0
    }

    /// Return the ASIC temperature in degrees celsius.
    pub fn asic_temperature(&self) -> f64 {
        if self.arch.is_blackhole() {
            let frac: f64 = (self.asic_temperature & 0xFFFF).into();
            let frac = frac / 65536.0;

            let int: f64 = (self.asic_temperature >> 16).into();

            int + frac
        } else {
            ((self.asic_temperature & 0xffff) >> 4) as f64
        }
    }

    /// Return the power consumption in watts.
    pub fn power(&self) -> f64 {
        (self.tdp & 0xffff) as f64
    }

    /// Return the current consumption in amperes.
    pub fn current(&self) -> f64 {
        (self.tdc & 0xffff) as f64
    }

    /// Return the board serial number as an integer.
    pub fn board_serial_number(&self) -> u64 {
        ((self.board_id_high as u64) << 32) | self.board_id_low as u64
    }

    /// Return the board type or None if unknown
    pub fn try_board_type(&self) -> Option<&'static str> {
        let serial_num = self.board_serial_number();
        let output = match (serial_num >> 36) & 0xFFFFF {
            0x1 => match (serial_num >> 32) & 0xF {
                0x2 => "E300_R2",
                0x3 | 0x4 => "E300_R3",
                _ => return None,
            },
            0x3 => "e150",
            0x7 => "e75",
            0x8 => "NEBULA_CB",
            0xA => "e300",
            0xB => "GALAXY",
            0x14 => "n300",
            0x18 => "n150",
            0x35 => "galaxy-wormhole",
            0x36 => "p100",
            0x40 => "p150a",
            0x41 => "p150b",
            0x42 => "p150c",
            0x43 => "p100a",
            0x44 => "p300b",
            0x45 => "p300a",
            0x46 => "p300c",
            0x47 => "galaxy-blackhole",
            _ => return None,
        };

        Some(output)
    }

    /// Return the board type or "UNSUPPORTED"
    pub fn board_type(&self) -> &'static str {
        self.try_board_type().unwrap_or("UNSUPPORTED")
    }

    /// Return the board serial number as a hex-formatted string
    pub fn board_serial_number_hex(&self) -> String {
        format!("{:016x}", self.board_serial_number())
    }

    /// Return firmware date in YYYY-MM-DD format
    pub fn firmware_date(&self) -> String {
        let year = ((self.wh_fw_date >> 28) & 0xF) + 2020;
        let month = (self.wh_fw_date >> 24) & 0xF;
        let day = (self.wh_fw_date >> 16) & 0xFF;
        format!("{year:04}-{month:02}-{day:02}")
    }

    /// Return ARC firmware version in MAJOR.MINOR.PATCH format
    pub fn arc_fw_version(&self) -> String {
        let major = (self.arc0_fw_version >> 16) & 0xFF;
        let minor = (self.arc0_fw_version >> 8) & 0xFF;
        let patch = self.arc0_fw_version & 0xFF;
        format!("{major}.{minor}.{patch}")
    }

    /// Return Ethernet firmware version in MAJOR.MINOR.PATCH format
    pub fn eth_fw_version(&self) -> String {
        let major = (self.eth_fw_version >> 16) & 0x0FF;
        let minor = (self.eth_fw_version >> 12) & 0x00F;
        let patch = self.eth_fw_version & 0xFFF;
        format!("{major}.{minor}.{patch}")
    }

    /// Return the AXI clock speed in MHz
    pub fn axi_clk(&self) -> u32 {
        self.axiclk
    }

    /// Return the ARC clock speed in MHz
    pub fn arc_clk(&self) -> u32 {
        self.arcclk
    }

    /// Return the voltage regulator temperature in degrees celsius
    pub fn vreg_temperature(&self) -> f64 {
        (self.vreg_temperature & 0xffff) as f64
    }

    /// Return the inlet temperature in degrees celsius
    pub fn inlet_temperature(&self) -> f64 {
        ((self.board_temperature >> 0x10) & 0xff) as f64
    }

    /// Return the first outlet temperature in degrees celsius
    pub fn outlet_temperature1(&self) -> f64 {
        ((self.board_temperature >> 0x08) & 0xff) as f64
    }

    /// Return the second outlet temperature in degrees celsius
    pub fn outlet_temperature2(&self) -> f64 {
        (self.board_temperature & 0xff) as f64
    }
}

/// Simplified chip interface trait
#[allow(dead_code)]
pub trait ChipImpl: Send + Sync + 'static {
    /// Returns the current arch of the chip
    fn get_arch(&self) -> Arch;

    /// Get telemetry information from the chip
    fn get_telemetry(&self) -> Result<Telemetry, PlatformError>;

    /// Convenience function to downcast to a concrete type
    fn as_any(&self) -> &dyn Any;
}

/// A wrapper around a chip that implements `ChipImpl`
pub struct Chip {
    pub inner: Box<dyn ChipImpl>,
}

impl Chip {
    /// Get the architecture of this chip
    #[allow(dead_code)]
    pub fn get_arch(&self) -> Arch {
        self.inner.get_arch()
    }

    /// Get telemetry from this chip
    pub fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        self.inner.get_telemetry()
    }
}
