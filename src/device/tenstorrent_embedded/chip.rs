// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use super::arch::Arch;
use crate::device::tenstorrent_embedded::{
    error::PlatformError,
    interface::CallbackStorage,
    luwen_ref::{ExtendedPciDeviceWrapper, LuwenChip},
    ttkmd::kmdif::ChipInterface,
};

pub trait ChipComms {
    fn axi_sread32(&self, addr: &str) -> Result<u32, Box<dyn std::error::Error>>;
    fn axi_write32(&self, addr: &str, value: u32) -> Result<(), Box<dyn std::error::Error>>;
    fn axi_read32(
        &self,
        ifc: &dyn ChipInterface,
        addr: u32,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        ifc.axi_read32(addr)
    }
    fn axi_translate(
        &self,
        addr: &str,
    ) -> Result<
        crate::device::tenstorrent_embedded::ttkmd::kmdif::AxiData,
        Box<dyn std::error::Error>,
    > {
        crate::device::tenstorrent_embedded::luwen_ref::axi_translate(addr)
    }
}

impl<T: ChipInterface> ChipComms for T {
    fn axi_sread32(&self, addr: &str) -> Result<u32, Box<dyn std::error::Error>> {
        self.axi_sread32(addr)
    }

    fn axi_write32(&self, addr: &str, value: u32) -> Result<(), Box<dyn std::error::Error>> {
        let addr_data = crate::device::tenstorrent_embedded::luwen_ref::axi_translate(addr)?;
        let data = value.to_le_bytes();
        self.axi_write(addr_data.addr, &data)
    }
}

/// Defines common functionality for all chips.
pub trait ChipImpl: Send + Sync + 'static {
    /// Returns the current arch of the chip
    fn get_arch(&self) -> Arch;

    /// Get telemetry information from the chip.
    fn get_telemetry(&self) -> Result<Telemetry, PlatformError>;

    /// Convinence function to downcast to a concrete type.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// A wrapper around a chip that implements `ChipImpl`.
/// This allows us to create and use chips without knowing their type,
/// but we can still downcast to the concrete type if we need to.
pub struct Chip {
    pub inner: Box<dyn ChipImpl>,
}

impl Chip {
    pub fn open(
        arch: Arch,
        ifc: CallbackStorage<ExtendedPciDeviceWrapper>,
    ) -> Result<Self, PlatformError> {
        Ok(Self {
            inner: Box::new(LuwenChip::new(arch, ifc)?),
        })
    }

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

pub trait HlComms {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface);
}

impl HlComms for Chip {
    fn comms_obj(&self) -> (&dyn ChipComms, &dyn ChipInterface) {
        self.inner
            .as_any()
            .downcast_ref::<LuwenChip>()
            .unwrap()
            .comms_obj()
    }
}

impl ChipComms for Chip {
    fn axi_sread32(&self, addr: &str) -> Result<u32, Box<dyn std::error::Error>> {
        let (comms, _) = self.comms_obj();
        comms.axi_sread32(addr)
    }

    fn axi_write32(&self, addr: &str, value: u32) -> Result<(), Box<dyn std::error::Error>> {
        let (comms, _) = self.comms_obj();
        comms.axi_write32(addr, value)
    }
}

#[derive(Default, Debug)]
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
    /// Return firmware date in YYYY-MM-DD format.
    pub fn firmware_date(&self) -> String {
        let year = ((self.wh_fw_date >> 28) & 0xF) + 2020;
        let month = (self.wh_fw_date >> 24) & 0xF;
        let day = (self.wh_fw_date >> 16) & 0xFF;
        let _hour = (self.wh_fw_date >> 8) & 0xFF;
        let _minute = self.wh_fw_date & 0xFF;
        format!("{year:04}-{month:02}-{day:02}")
    }

    /// Return ARC firmware version in MAJOR.MINOR.PATCH format.
    pub fn arc_fw_version(&self) -> String {
        let major = (self.arc0_fw_version >> 16) & 0xFF;
        let minor = (self.arc0_fw_version >> 8) & 0xFF;
        let patch = self.arc0_fw_version & 0xFF;
        format!("{major}.{minor}.{patch}")
    }

    /// Return Ethernet firmware version in MAJOR.MINOR.PATCH format.
    pub fn eth_fw_version(&self) -> String {
        let major = (self.eth_fw_version >> 16) & 0x0FF;
        let minor = (self.eth_fw_version >> 12) & 0x00F;
        let patch = self.eth_fw_version & 0xFFF;
        format!("{major}.{minor}.{patch}")
    }

    /// Return the board serial number as an integer.
    pub fn board_serial_number(&self) -> u64 {
        ((self.board_id_high as u64) << 32) | self.board_id_low as u64
    }

    /// Return the board serial number as a hex-formatted string.
    pub fn board_serial_number_hex(&self) -> String {
        format!("{:016x}", self.board_serial_number())
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

    /// Return the board type of UNSUPPORTED
    pub fn board_type(&self) -> &'static str {
        self.try_board_type().unwrap_or("UNSUPPORTED")
    }

    /// Return the AI clock speed in MHz.
    pub fn ai_clk(&self) -> u32 {
        self.aiclk & 0xffff
    }

    /// Return the AXI clock speed in MHz.
    pub fn axi_clk(&self) -> u32 {
        self.axiclk
    }

    /// Return the ARC clock speed in MHz.
    pub fn arc_clk(&self) -> u32 {
        self.arcclk
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

    /// Return the voltage regulator temperature in degrees celsius.
    pub fn vreg_temperature(&self) -> f64 {
        (self.vreg_temperature & 0xffff) as f64
    }

    /// Return the inlet temperature in degrees celsius.
    pub fn inlet_temperature(&self) -> f64 {
        ((self.board_temperature >> 0x10) & 0xff) as f64
    }

    /// Return the first outlet temperature in degrees celsius.
    pub fn outlet_temperature1(&self) -> f64 {
        ((self.board_temperature >> 0x08) & 0xff) as f64
    }

    /// Return the second outlet temperature in degrees celsius.
    pub fn outlet_temperature2(&self) -> f64 {
        (self.board_temperature & 0xff) as f64
    }

    /// Return the power consumption in watts.
    pub fn power(&self) -> f64 {
        (self.tdp & 0xffff) as f64
    }

    /// Return the current consumption in amperes.
    pub fn current(&self) -> f64 {
        (self.tdc & 0xffff) as f64
    }

    pub fn telemetry_heartbeat(&self) -> u32 {
        if self.arch.is_blackhole() {
            self.timer_heartbeat
        } else {
            self.arc0_health
        }
    }

    /// Get board-specific TDP (Thermal Design Power) in watts
    pub fn get_board_tdp(&self) -> f64 {
        match self.try_board_type() {
            // Grayskull boards
            Some("e75") => 75.0,
            Some("e150") => 75.0,
            Some("e300") | Some("E300_R2") | Some("E300_R3") => 100.0,
            Some("GALAXY") => 300.0, // Grayskull Galaxy
            // Wormhole boards
            Some("n150") => 150.0,
            Some("n300") => 160.0,
            Some("NEBULA_CB") => 150.0,
            Some("galaxy-wormhole") => 200.0,
            // Blackhole boards
            Some("p100") | Some("p100a") => 300.0,
            Some("p150a") | Some("p150b") | Some("p150c") => 350.0,
            Some("p300a") | Some("p300b") | Some("p300c") => 400.0,
            Some("galaxy-blackhole") => 450.0,
            _ => {
                // Fallback based on architecture
                match self.arch {
                    Arch::Grayskull => 75.0,
                    Arch::Wormhole => 160.0,
                    Arch::Blackhole => 350.0,
                }
            }
        }
    }

    /// Get board-specific memory size in bytes
    pub fn get_board_memory_size(&self) -> u64 {
        match self.try_board_type() {
            // Grayskull boards
            Some("e75") => 16 * 1024 * 1024 * 1024,  // 16GB
            Some("e150") => 32 * 1024 * 1024 * 1024, // 32GB
            Some("e300") | Some("E300_R2") | Some("E300_R3") => 48 * 1024 * 1024 * 1024, // 48GB
            Some("GALAXY") => 128 * 1024 * 1024 * 1024, // 128GB (Galaxy with multiple chips)
            // Wormhole boards
            Some("n150") => 32 * 1024 * 1024 * 1024, // 32GB
            Some("n300") => 64 * 1024 * 1024 * 1024, // 64GB
            Some("NEBULA_CB") => 32 * 1024 * 1024 * 1024, // 32GB
            Some("galaxy-wormhole") => 96 * 1024 * 1024 * 1024, // 96GB per board
            // Blackhole boards
            Some("p100") | Some("p100a") => 96 * 1024 * 1024 * 1024, // 96GB
            Some("p150a") | Some("p150b") | Some("p150c") => 144 * 1024 * 1024 * 1024, // 144GB
            Some("p300a") | Some("p300b") | Some("p300c") => 288 * 1024 * 1024 * 1024, // 288GB
            Some("galaxy-blackhole") => 576 * 1024 * 1024 * 1024,    // 576GB
            _ => {
                // Conservative fallback based on architecture
                match self.arch {
                    Arch::Grayskull => 16 * 1024 * 1024 * 1024,
                    Arch::Wormhole => 32 * 1024 * 1024 * 1024,
                    Arch::Blackhole => 96 * 1024 * 1024 * 1024,
                }
            }
        }
    }
}
