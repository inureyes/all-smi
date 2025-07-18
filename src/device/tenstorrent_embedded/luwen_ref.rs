// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use thiserror::Error;

use super::arch::Arch;
use super::chip::{ChipComms, ChipImpl, HlComms, Telemetry};
use super::error::PlatformError;
use super::interface::{FnDriver, FnOptions};
use super::ttkmd::error::PciError;
use super::ttkmd::kmdif::PciDevice;
use super::ttkmd::kmdif::{AxiData, ChipInterface};
use super::ttkmd::tlb::Tlb;
use super::ttkmd::{DmaBuffer, PossibleTlbAllocation};

#[derive(Debug, Error)]
pub enum LuwenError {
    #[error("PCI error: {0}")]
    Pci(#[from] PciError),
    #[error("Platform error: {0}")]
    Platform(#[from] PlatformError),
}

#[derive(Clone)]
pub struct ExtendedPciDeviceWrapper {
    inner: Arc<RwLock<ExtendedPciDevice>>,
}

impl ExtendedPciDeviceWrapper {
    pub fn borrow_mut(&self) -> RwLockWriteGuard<ExtendedPciDevice> {
        self.inner.as_ref().write().unwrap()
    }

    pub fn borrow(&self) -> RwLockReadGuard<ExtendedPciDevice> {
        self.inner.as_ref().read().unwrap()
    }
}

pub struct ExtendedPciDevice {
    pub device: PciDevice,

    pub harvested_rows: u32,
    pub grid_size_x: u8,
    pub grid_size_y: u8,

    pub eth_x: u8,
    pub eth_y: u8,
    pub command_q_addr: u32,
    pub fake_block: bool,

    pub default_tlb: PossibleTlbAllocation,

    pub ethernet_dma_buffer: HashMap<(u8, u8), DmaBuffer>,
}

impl ExtendedPciDevice {
    pub fn open(pci_interface: usize) -> Result<ExtendedPciDeviceWrapper, PciError> {
        eprintln!("[DEBUG] ExtendedPciDevice::open() called with interface {pci_interface}");
        let device = PciDevice::open(pci_interface)?;
        eprintln!(
            "[DEBUG] PciDevice::open() succeeded, arch: {:?}, driver_version: {}",
            device.arch, device.driver_version
        );

        let (grid_size_x, grid_size_y) = match device.arch {
            Arch::Grayskull => (13, 12),
            Arch::Wormhole => (10, 12),
            Arch::Blackhole => (17, 12),
        };
        eprintln!("[DEBUG] Grid size: {grid_size_x}x{grid_size_y}");

        let default_tlb;

        // Driver API 2+ has TLB allocation APIs supporting WH & BH.
        eprintln!(
            "[DEBUG] Checking TLB allocation: arch != Grayskull: {}, driver_version >= 2: {}",
            device.arch != Arch::Grayskull,
            device.driver_version >= 2
        );
        if device.arch != Arch::Grayskull && device.driver_version >= 2 {
            let size = match device.arch {
                Arch::Wormhole => 1 << 24,  // 16 MiB
                Arch::Blackhole => 1 << 21, // 2 MiB
                _ => {
                    return Err(PciError::TlbAllocationError(
                        "Unsupported architecture for TLB allocation".to_string(),
                    ))
                }
            };

            eprintln!("[DEBUG] Attempting to allocate TLB of size: {size}");
            match device.allocate_tlb(size) {
                Ok(tlb) => {
                    eprintln!("[DEBUG] TLB allocation succeeded");
                    default_tlb = PossibleTlbAllocation::Allocation(tlb);
                }
                Err(e) => {
                    eprintln!("[DEBUG] TLB allocation failed: {e:?}");
                    // Couldn't get a tlb... ideally at this point we would fallback to using a slower but useable read/write API
                    // for now though, we will just fail
                    return Err(PciError::TlbAllocationError(format!(
                        "Failed to find a free tlb: {e:?}"
                    )));
                }
            }
        } else {
            // Otherwise fallback to default behaviour of just taking a constant one
            let hardcoded_tlb = match device.arch {
                Arch::Grayskull | Arch::Wormhole => 184,
                Arch::Blackhole => 190,
            };
            eprintln!("[DEBUG] Using hardcoded TLB: {hardcoded_tlb}");
            default_tlb = PossibleTlbAllocation::Hardcoded(hardcoded_tlb);
        }

        let wrapper = ExtendedPciDeviceWrapper {
            inner: Arc::new(RwLock::new(ExtendedPciDevice {
                harvested_rows: 0,
                grid_size_x,
                grid_size_y,
                eth_x: 4,
                eth_y: 6,
                command_q_addr: 0,
                fake_block: false,

                default_tlb,

                device,

                ethernet_dma_buffer: HashMap::with_capacity(16),
            })),
        };
        eprintln!("[DEBUG] ExtendedPciDevice::open() completed successfully");
        Ok(wrapper)
    }

    pub fn read_block(&mut self, addr: u32, data: &mut [u8]) -> Result<(), PciError> {
        self.device.read_block(addr, data)
    }

    pub fn write_block(&mut self, addr: u32, data: &[u8]) -> Result<(), PciError> {
        self.device.write_block(addr, data)
    }
}

pub fn comms_callback(
    ud: &ExtendedPciDeviceWrapper,
    op: FnOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(comms_callback_inner(ud, op)?)
}

pub fn comms_callback_inner(
    ud: &ExtendedPciDeviceWrapper,
    op: FnOptions,
) -> Result<(), LuwenError> {
    match op {
        FnOptions::Driver(op) => match op {
            FnDriver::DeviceInfo(info) => {
                let borrow = ud.borrow();
                if !info.is_null() {
                    unsafe {
                        *info = Some(crate::device::tenstorrent_embedded::interface::DeviceInfo {
                            bus: borrow.device.physical.pci_bus,
                            slot: borrow.device.physical.slot,
                            function: borrow.device.physical.pci_function,
                            domain: borrow.device.physical.pci_domain,

                            interface_id: borrow.device.id as u32,

                            vendor: borrow.device.physical.vendor_id,
                            device_id: borrow.device.physical.device_id,
                            board_id: borrow.device.physical.subsystem_id,
                            bar_size: borrow.device.pci_bar.as_ref().map(|v| v.bar_size_bytes),
                        });
                    }
                }
            }
        },
        FnOptions::Axi(op) => match op {
            crate::device::tenstorrent_embedded::interface::FnAxi::Read { addr, data, len } => {
                if len > 0 {
                    if len <= 4 {
                        let output = ud.borrow_mut().device.read32(addr)?;
                        let output = output.to_le_bytes();
                        unsafe {
                            data.copy_from_nonoverlapping(output.as_ptr(), len as usize);
                        }
                    } else {
                        unsafe {
                            ud.borrow_mut().read_block(
                                addr,
                                std::slice::from_raw_parts_mut(data, len as usize),
                            )?
                        };
                    }
                }
            }
            crate::device::tenstorrent_embedded::interface::FnAxi::Write { addr, data, len } => {
                if len > 0 {
                    // Assuming here that u32 is our fundamental unit of transfer
                    if len <= 4 {
                        let to_write = if len == 4 {
                            let slice = unsafe { std::slice::from_raw_parts(data, len as usize) };
                            u32::from_le_bytes(slice.try_into().unwrap())
                        } else {
                            // We are reading less than a u32, so we need to read the existing value first
                            // then writeback the new value with the lower len bytes replaced
                            let value = ud.borrow_mut().device.read32(addr)?;
                            let mut value = value.to_le_bytes();
                            unsafe {
                                value
                                    .as_mut_ptr()
                                    .copy_from_nonoverlapping(data, len as usize);
                            }

                            u32::from_le_bytes(value)
                        };

                        ud.borrow_mut().device.write32(addr, to_write)?;
                    } else {
                        unsafe {
                            ud.borrow_mut()
                                .write_block(addr, std::slice::from_raw_parts(data, len as usize))?
                        };
                    }
                }
            }
        },
        FnOptions::Noc(op) => match op {
            crate::device::tenstorrent_embedded::interface::FnNoc::Read {
                noc_id: _,
                x,
                y,
                addr,
                data,
                len,
            } => {
                let mut reader = ud.borrow_mut();
                let reader: &mut ExtendedPciDevice = &mut reader;

                reader.device.noc_read(
                    &reader.default_tlb,
                    Tlb::new(x as u8, y as u8, addr),
                    unsafe { std::slice::from_raw_parts_mut(data, len as usize) },
                )?;
            }
            crate::device::tenstorrent_embedded::interface::FnNoc::Write {
                noc_id: _,
                x,
                y,
                addr,
                data,
                len,
            } => {
                let mut writer = ud.borrow_mut();
                let writer: &mut ExtendedPciDevice = &mut writer;

                writer.device.noc_write(
                    &writer.default_tlb,
                    Tlb::new(x as u8, y as u8, addr),
                    unsafe { std::slice::from_raw_parts(data, len as usize) },
                )?;
            }
            crate::device::tenstorrent_embedded::interface::FnNoc::Broadcast {
                noc_id: _,
                addr,
                data,
                len,
            } => {
                let mut writer = ud.borrow_mut();
                let writer: &mut ExtendedPciDevice = &mut writer;

                let (x_start, y_start) = match writer.device.arch {
                    Arch::Grayskull => (0, 0),
                    Arch::Wormhole => (1, 0),
                    Arch::Blackhole => (0, 1),
                };

                writer.device.noc_write(
                    &writer.default_tlb,
                    Tlb::broadcast(
                        x_start,
                        y_start,
                        writer.grid_size_x - 1,
                        writer.grid_size_y - 1,
                        addr,
                    ),
                    unsafe { std::slice::from_raw_parts(data, len as usize) },
                )?;
            }
            crate::device::tenstorrent_embedded::interface::FnNoc::Multicast {
                noc_id: _,
                start_x,
                start_y,
                end_x,
                end_y,
                addr,
                data,
                len,
            } => {
                let mut writer = ud.borrow_mut();
                let writer: &mut ExtendedPciDevice = &mut writer;

                let (min_start_x, min_start_y) = match writer.device.arch {
                    Arch::Grayskull => (0, 0),
                    Arch::Wormhole => (1, 0),
                    Arch::Blackhole => (0, 1),
                };

                let (start_x, start_y) = (start_x.max(min_start_x), start_y.max(min_start_y));

                writer.device.noc_write(
                    &writer.default_tlb,
                    Tlb::multicast(start_x, start_y, end_x, end_y, addr),
                    unsafe { std::slice::from_raw_parts(data, len as usize) },
                )?;
            }
        },
    }

    Ok(())
}

pub fn axi_translate(addr_str: &str) -> Result<AxiData, Box<dyn std::error::Error>> {
    let mut data = AxiData::default();

    // Handle register name translations for common registers
    // These are the actual hardware addresses for these registers
    data.addr = match addr_str {
        // Wormhole/Grayskull scratch registers
        "ARC_RESET.SCRATCH[0]" => 0x1ff30060,
        "ARC_RESET.SCRATCH[1]" => 0x1ff30064,
        "ARC_RESET.SCRATCH[2]" => 0x1ff30068,
        "ARC_RESET.SCRATCH[3]" => 0x1ff3006c,
        "ARC_RESET.SCRATCH[4]" => 0x1ff30070,
        "ARC_RESET.SCRATCH[5]" => 0x1ff30074,
        "ARC_RESET.POST_CODE" => 0x1ff3007c,
        "ARC_RESET.ARC_MISC_CNTL" => 0x1ff30100,
        "ARC_CSM.ARC_PCIE_DMA_REQUEST" => 0x1fef83b0,
        "ARC_CSM.ARC_PCIE_DMA_REQUEST.trigger" => 0x1fef83b0,

        // Blackhole scratch registers
        "arc_ss.reset_unit.SCRATCH_0" => 0xffff0060,
        "arc_ss.reset_unit.SCRATCH_RAM[0]" => 0xffff0060,
        "arc_ss.reset_unit.SCRATCH_RAM[10]" => 0xffff0088,
        "arc_ss.reset_unit.SCRATCH_RAM[11]" => 0xffff008c,
        "arc_ss.reset_unit.SCRATCH_RAM[12]" => 0xffff0090,
        "arc_ss.reset_unit.SCRATCH_RAM[13]" => 0xffff0094,
        "arc_ss.reset_unit.ARC_MISC_CNTL" => 0xffff0100,

        // CSM data register
        "ARC_CSM.DATA[0]" => 0x1fef8000,

        // If not a known register name, try to parse as hex or decimal
        _ => {
            if let Some(hex_str) = addr_str.strip_prefix("0x") {
                u32::from_str_radix(hex_str, 16)?
            } else {
                addr_str.parse::<u32>()?
            }
        }
    };

    Ok(data)
}

pub struct LuwenChip {
    pub arch: Arch,
    pub comms: Box<dyn ChipInterface>,
}

impl LuwenChip {
    pub fn new(arch: Arch, ifc: impl ChipInterface + 'static) -> Result<Self, PlatformError> {
        Ok(Self {
            arch,
            comms: Box::new(ifc),
        })
    }

    /// Wait for chip initialization to complete
    pub fn wait_for_init(&self) -> Result<(), PlatformError> {
        eprintln!("[DEBUG] Waiting for chip initialization...");

        // Check ARC firmware is ready by reading scratch register
        let scratch_reg = if self.arch.is_blackhole() {
            "arc_ss.reset_unit.SCRATCH_0"
        } else {
            "ARC_RESET.SCRATCH[0]"
        };

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30); // 30 second timeout

        loop {
            match self.axi_sread32(scratch_reg) {
                Ok(val) => {
                    eprintln!("[DEBUG] Scratch register value: 0x{val:x}");
                    // If we can read the register, ARC is responsive
                    break;
                }
                Err(e) => {
                    if start.elapsed() > timeout {
                        return Err(PlatformError::Generic(
                            format!("Timeout waiting for chip initialization: {e}"),
                            super::error::BtWrapper::capture(),
                        ));
                    }
                    eprintln!("[DEBUG] Waiting for ARC firmware... ({e})");
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }

        // For Wormhole/Blackhole, check if DDR training is complete
        if self.arch == Arch::Wormhole || self.arch == Arch::Blackhole {
            eprintln!("[DEBUG] Checking DDR training status...");
            // Try to read telemetry to check DDR status
            match self.get_telemetry_no_retry() {
                Ok(telemetry) => {
                    if telemetry.ddr_status != 0 {
                        eprintln!(
                            "[DEBUG] DDR training complete (status: 0x{:x})",
                            telemetry.ddr_status
                        );
                    } else {
                        eprintln!("[DEBUG] DDR not initialized, continuing anyway");
                    }
                }
                Err(e) => {
                    eprintln!("[DEBUG] Could not check DDR status: {e}, continuing");
                }
            }
        }

        eprintln!("[DEBUG] Chip initialization complete");
        Ok(())
    }

    /// Get telemetry without retry logic (for internal use)
    fn get_telemetry_no_retry(&self) -> Result<Telemetry, PlatformError> {
        // For Wormhole, we need to send an ARC message to get the telemetry address
        if self.arch == Arch::Wormhole || self.arch == Arch::Grayskull {
            // Send GetSmbusTelemetryAddr message
            let msg_options = super::arc_msg::ArcMsgOptions {
                msg: super::arc_msg::ArcMsg::Typed(
                    super::arc_msg::TypedArcMsg::GetSmbusTelemetryAddr,
                ),
                wait_for_done: true,
                timeout: std::time::Duration::from_secs(1),
                use_second_mailbox: false,
                addrs: Some(super::arc_msg::ArcMsgAddr {
                    scratch_base: 0x1ff30060,  // ARC_RESET.SCRATCH[0]
                    arc_misc_cntl: 0x1ff30100, // ARC_RESET.ARC_MISC_CNTL
                }),
            };

            // Arc message handling - simplified version
            let (msg_reg, return_reg) = if msg_options.use_second_mailbox {
                (2, 4)
            } else {
                (5, 3)
            };

            let telemetry_addr = match super::arc_msg::arc_msg(
                self,
                &msg_options.msg,
                msg_options.wait_for_done,
                msg_options.timeout,
                msg_reg,
                return_reg,
                msg_options.addrs.as_ref().unwrap(),
            ) {
                Ok(super::arc_msg::ArcMsgOk::Ok { arg }) => {
                    eprintln!("[DEBUG] ARC message returned telemetry address: 0x{arg:x}");
                    arg
                }
                Ok(super::arc_msg::ArcMsgOk::OkNoWait) => {
                    eprintln!("[DEBUG] ARC message returned OkNoWait (unexpected)");
                    return Ok(Telemetry {
                        arch: self.arch,
                        ..Default::default()
                    });
                }
                Err(e) => {
                    eprintln!("[DEBUG] Failed to get telemetry address via ARC message: {e:?}");
                    return Ok(Telemetry {
                        arch: self.arch,
                        ..Default::default()
                    });
                }
            };

            // Calculate CSM offset
            let csm_offset = self.axi_translate("ARC_CSM.DATA[0]")?.addr;
            let telemetry_struct_offset = csm_offset + (telemetry_addr - 0x10000000);

            eprintln!("[DEBUG] CSM base offset: 0x{csm_offset:x}");
            eprintln!(
                "[DEBUG] Telemetry addr adjusted: 0x{:x}",
                telemetry_addr - 0x10000000
            );
            eprintln!("[DEBUG] Reading telemetry from offset: 0x{telemetry_struct_offset:x}");

            // Check if we're getting the magic number (uninitialized telemetry)
            let first_word = self.comms.axi_read32(telemetry_struct_offset)?;
            if first_word == 0 {
                // Try the raw address
                let first_word_raw = self.comms.axi_read32(telemetry_addr)?;
                if first_word_raw == 0x66666666 {
                    eprintln!(
                        "[DEBUG] Detected telemetry magic number 0x66666666, telemetry not ready"
                    );
                    return self.read_telemetry_from_offset(telemetry_addr);
                }
            }

            self.read_telemetry_from_offset(telemetry_struct_offset)
        } else {
            Err(PlatformError::Generic(
                format!("Telemetry not implemented for arch {:?}", self.arch),
                super::error::BtWrapper::capture(),
            ))
        }
    }
}

impl LuwenChip {
    fn read_telemetry_from_offset(
        &self,
        telemetry_struct_offset: u32,
    ) -> Result<Telemetry, PlatformError> {
        eprintln!("[DEBUG] Reading telemetry from offset: 0x{telemetry_struct_offset:x}");

        // Read telemetry fields
        let enum_version = self.comms.axi_read32(telemetry_struct_offset)?;
        let device_id = self.comms.axi_read32(telemetry_struct_offset + 4)?;
        let asic_ro = self.comms.axi_read32(telemetry_struct_offset + 8)?;
        let asic_idd = self.comms.axi_read32(telemetry_struct_offset + 12)?;
        let board_id_high = self.comms.axi_read32(telemetry_struct_offset + 16)?;
        let board_id_low = self.comms.axi_read32(telemetry_struct_offset + 20)?;

        // Following the exact offset mapping from luwen reference
        let arc0_fw_version = self.comms.axi_read32(telemetry_struct_offset + (6 * 4))?;
        let arc1_fw_version = self.comms.axi_read32(telemetry_struct_offset + (7 * 4))?;
        let arc2_fw_version = self.comms.axi_read32(telemetry_struct_offset + (8 * 4))?;
        let arc3_fw_version = self.comms.axi_read32(telemetry_struct_offset + (9 * 4))?;
        let spibootrom_fw_version = self.comms.axi_read32(telemetry_struct_offset + (10 * 4))?;
        let eth_fw_version = self.comms.axi_read32(telemetry_struct_offset + (11 * 4))?;
        let m3_bl_fw_version = self.comms.axi_read32(telemetry_struct_offset + (12 * 4))?;
        let m3_app_fw_version = self.comms.axi_read32(telemetry_struct_offset + (13 * 4))?;
        let ddr_status = self.comms.axi_read32(telemetry_struct_offset + (14 * 4))?;
        let eth_status0 = self.comms.axi_read32(telemetry_struct_offset + (15 * 4))?;
        let eth_status1 = self.comms.axi_read32(telemetry_struct_offset + (16 * 4))?;
        let pcie_status = self.comms.axi_read32(telemetry_struct_offset + (17 * 4))?;
        let faults = self.comms.axi_read32(telemetry_struct_offset + (18 * 4))?;
        let arc0_health = self.comms.axi_read32(telemetry_struct_offset + (19 * 4))?;
        let arc1_health = self.comms.axi_read32(telemetry_struct_offset + (20 * 4))?;
        let arc2_health = self.comms.axi_read32(telemetry_struct_offset + (21 * 4))?;
        let arc3_health = self.comms.axi_read32(telemetry_struct_offset + (22 * 4))?;
        let fan_speed = self.comms.axi_read32(telemetry_struct_offset + (23 * 4))?;

        let aiclk = self.comms.axi_read32(telemetry_struct_offset + (24 * 4))?;
        eprintln!(
            "[DEBUG] aiclk at offset 0x{:x}: 0x{:x} ({})",
            telemetry_struct_offset + (24 * 4),
            aiclk,
            aiclk & 0xffff
        );

        let axiclk = self.comms.axi_read32(telemetry_struct_offset + (25 * 4))?;
        let arcclk = self.comms.axi_read32(telemetry_struct_offset + (26 * 4))?;
        let throttler = self.comms.axi_read32(telemetry_struct_offset + (27 * 4))?;

        let vcore = self.comms.axi_read32(telemetry_struct_offset + (28 * 4))?;
        eprintln!(
            "[DEBUG] vcore at offset 0x{:x}: 0x{:x} ({}mV)",
            telemetry_struct_offset + (28 * 4),
            vcore,
            vcore
        );

        let asic_temperature = self.comms.axi_read32(telemetry_struct_offset + (29 * 4))?;
        eprintln!(
            "[DEBUG] asic_temperature at offset 0x{:x}: 0x{:x}",
            telemetry_struct_offset + (29 * 4),
            asic_temperature
        );

        let vreg_temperature = self.comms.axi_read32(telemetry_struct_offset + (30 * 4))?;
        let board_temperature = self.comms.axi_read32(telemetry_struct_offset + (31 * 4))?;

        let tdp = self.comms.axi_read32(telemetry_struct_offset + (32 * 4))?;
        eprintln!(
            "[DEBUG] tdp at offset 0x{:x}: 0x{:x} ({}W)",
            telemetry_struct_offset + (32 * 4),
            tdp,
            tdp & 0xffff
        );

        let tdc = self.comms.axi_read32(telemetry_struct_offset + (33 * 4))?;
        let vdd_limits = self.comms.axi_read32(telemetry_struct_offset + (34 * 4))?;
        let thm_limits = self.comms.axi_read32(telemetry_struct_offset + (35 * 4))?;
        let wh_fw_date = self.comms.axi_read32(telemetry_struct_offset + (36 * 4))?;
        let asic_tmon0 = self.comms.axi_read32(telemetry_struct_offset + (37 * 4))?;
        let asic_tmon1 = self.comms.axi_read32(telemetry_struct_offset + (38 * 4))?;

        eprintln!(
            "[DEBUG] Telemetry read: aiclk={}, vcore={}, tdp={}, temperature={}",
            aiclk & 0xffff,
            vcore,
            tdp & 0xffff,
            ((asic_temperature & 0xffff) >> 4)
        );

        Ok(Telemetry {
            arch: self.arch,
            board_id: ((board_id_high as u64) << 32) | (board_id_low as u64),
            enum_version,
            device_id,
            asic_ro,
            asic_idd,
            board_id_high,
            board_id_low,
            arc0_fw_version,
            arc1_fw_version,
            arc2_fw_version,
            arc3_fw_version,
            spibootrom_fw_version,
            eth_fw_version,
            m3_bl_fw_version,
            m3_app_fw_version,
            ddr_status,
            eth_status0,
            eth_status1,
            pcie_status,
            faults,
            arc0_health,
            arc1_health,
            arc2_health,
            arc3_health,
            fan_speed,
            aiclk,
            axiclk,
            arcclk,
            throttler,
            vcore,
            asic_temperature,
            vreg_temperature,
            board_temperature,
            tdp,
            tdc,
            vdd_limits,
            thm_limits,
            wh_fw_date,
            asic_tmon0,
            asic_tmon1,
            timer_heartbeat: arc0_health,
            ..Default::default()
        })
    }
}

impl ChipImpl for LuwenChip {
    fn get_arch(&self) -> Arch {
        self.arch
    }

    fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        // First try without retry
        match self.get_telemetry_no_retry() {
            Ok(telemetry) => {
                // Check if we got valid telemetry (not magic number)
                if telemetry.enum_version != 0x66666666 {
                    return Ok(telemetry);
                }
                eprintln!("[DEBUG] Telemetry not ready (magic number), implementing retry logic");
            }
            Err(e) => {
                eprintln!("[DEBUG] Initial telemetry read failed: {e}");
            }
        }

        // Retry logic for uninitialized telemetry
        let max_retries = 10;
        let retry_delay = std::time::Duration::from_millis(500);
        let mut last_heartbeat = 0u32;

        for retry in 0..max_retries {
            eprintln!(
                "[DEBUG] Retry {}/{} for telemetry reading",
                retry + 1,
                max_retries
            );
            std::thread::sleep(retry_delay);

            match self.get_telemetry_no_retry() {
                Ok(telemetry) => {
                    // Check if we got valid telemetry (not magic number)
                    if telemetry.enum_version != 0x66666666 {
                        // Verify heartbeat is changing
                        let current_heartbeat = telemetry.telemetry_heartbeat();
                        eprintln!(
                            "[DEBUG] Heartbeat value: {current_heartbeat} (previous: {last_heartbeat})"
                        );

                        if retry > 0 && current_heartbeat == last_heartbeat {
                            eprintln!(
                                "[DEBUG] Warning: Heartbeat not changing, telemetry may be stale"
                            );
                        }

                        eprintln!(
                            "[DEBUG] Successfully got valid telemetry on retry {}",
                            retry + 1
                        );
                        return Ok(telemetry);
                    }
                    eprintln!("[DEBUG] Still getting magic number on retry {}", retry + 1);

                    // Update heartbeat even for magic number
                    last_heartbeat = telemetry.telemetry_heartbeat();
                }
                Err(e) => {
                    eprintln!(
                        "[DEBUG] Error reading telemetry on retry {}: {e}",
                        retry + 1
                    );
                }
            }
        }

        eprintln!("[DEBUG] Max retries reached, returning last telemetry state");
        // Return whatever we have
        self.get_telemetry_no_retry()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// Remove all orphaned code below
/* ORPHANED CODE START
                wait_for_done: true,
                timeout: std::time::Duration::from_secs(1),
                use_second_mailbox: false,
                addrs: Some(super::arc_msg::ArcMsgAddr {
                    scratch_base: 0x1ff30060,  // ARC_RESET.SCRATCH[0]
                    arc_misc_cntl: 0x1ff30100, // ARC_RESET.ARC_MISC_CNTL
                }),
            };

            // Arc message handling - simplified version
            let (msg_reg, return_reg) = if msg_options.use_second_mailbox {
                (2, 4)
            } else {
                (5, 3)
            };

            eprintln!("[DEBUG] Sending ARC message GetSmbusTelemetryAddr (0x2C) to msg_reg={msg_reg}, return_reg={return_reg}");
            eprintln!(
                "[DEBUG] scratch_base=0x{:x}, arc_misc_cntl=0x{:x}",
                msg_options.addrs.as_ref().unwrap().scratch_base,
                msg_options.addrs.as_ref().unwrap().arc_misc_cntl
            );

            let telemetry_addr = match super::arc_msg::arc_msg(
                self,
                &msg_options.msg,
                msg_options.wait_for_done,
                msg_options.timeout,
                msg_reg,
                return_reg,
                msg_options.addrs.as_ref().unwrap(),
            ) {
                Ok(super::arc_msg::ArcMsgOk::Ok { arg }) => {
                    eprintln!("[DEBUG] ARC message returned telemetry address: 0x{arg:x}");
                    arg
                }
                Ok(super::arc_msg::ArcMsgOk::OkNoWait) => {
                    eprintln!("[DEBUG] ARC message returned OkNoWait (unexpected)");
                    return Ok(Telemetry {
                        arch: self.arch,
                        ..Default::default()
                    });
                }
                Err(e) => {
                    eprintln!("[DEBUG] Failed to get telemetry address via ARC message: {e:?}");
                    return Ok(Telemetry {
                        arch: self.arch,
                        ..Default::default()
                    });
                }
            };

            // Calculate CSM offset
            let csm_offset = self.axi_translate("ARC_CSM.DATA[0]")?.addr;
            let telemetry_struct_offset = csm_offset + (telemetry_addr - 0x10000000);

            eprintln!("[DEBUG] CSM base offset: 0x{csm_offset:x}");
            eprintln!(
                "[DEBUG] Telemetry addr adjusted: 0x{:x}",
                telemetry_addr - 0x10000000
            );
            eprintln!("[DEBUG] Reading telemetry from offset: 0x{telemetry_struct_offset:x}");

            // Also try reading the telemetry address as if it contains a pointer
            let potential_pointer = self.comms.axi_read32(telemetry_struct_offset)?;
            eprintln!(
                "[DEBUG] Value at telemetry offset (potential pointer): 0x{potential_pointer:x}"
            );
            if potential_pointer != 0 && potential_pointer != telemetry_addr {
                eprintln!(
                    "[DEBUG] Might be a pointer, trying to read from 0x{:x}",
                    csm_offset + (potential_pointer - 0x10000000)
                );
            }

            // Read telemetry fields
            let enum_version = self.comms.axi_read32(telemetry_struct_offset)?;
            eprintln!(
                "[DEBUG] enum_version at offset 0x{telemetry_struct_offset:x}: 0x{enum_version:x}"
            );

            // Check if telemetry structure is initialized
            if enum_version == 0 {
                eprintln!("[DEBUG] Telemetry structure not initialized (enum_version=0)");
                // Try reading from the raw telemetry address without adjustment
                let raw_offset = telemetry_addr;
                eprintln!("[DEBUG] Trying raw telemetry address: 0x{raw_offset:x}");
                let test_read = self.comms.axi_read32(raw_offset)?;
                eprintln!("[DEBUG] Value at raw address: 0x{test_read:x}");

                // If we see data at the raw address, it might be the telemetry structure
                if test_read != 0 {
                    eprintln!(
                        "[DEBUG] Found non-zero data at raw address, checking if it's telemetry"
                    );
                    // Read a few more values to verify
                    let test_device_id = self.comms.axi_read32(raw_offset + 4)?;
                    let test_asic_ro = self.comms.axi_read32(raw_offset + 8)?;
                    eprintln!("[DEBUG] Raw addr +4: 0x{test_device_id:x}, +8: 0x{test_asic_ro:x}");

                    // Check telemetry at offset 96 (aiclk)
                    let test_aiclk = self.comms.axi_read32(raw_offset + 96)?;
                    eprintln!("[DEBUG] Raw addr +96 (aiclk): 0x{test_aiclk:x}");

                    // If this looks like valid telemetry, use the raw address
                    if test_read == 0x66666666 {
                        eprintln!("[DEBUG] Detected telemetry magic number, using raw address for telemetry");
                        // Override the telemetry offset to use the raw address
                        let telemetry_struct_offset_override = raw_offset;
                        return self.read_telemetry_from_offset(telemetry_struct_offset_override);
                    }
                }
            }

            let device_id = self.comms.axi_read32(telemetry_struct_offset + 4)?;
            eprintln!(
                "[DEBUG] device_id at offset 0x{:x}: 0x{:x}",
                telemetry_struct_offset + 4,
                device_id
            );

            let asic_ro = self.comms.axi_read32(telemetry_struct_offset + 8)?;
            let asic_idd = self.comms.axi_read32(telemetry_struct_offset + 12)?;
            let board_id_high = self.comms.axi_read32(telemetry_struct_offset + 16)?;
            let board_id_low = self.comms.axi_read32(telemetry_struct_offset + 20)?;
            eprintln!("[DEBUG] board_id: 0x{board_id_high:08x}{board_id_low:08x}");
            // Note: Following the exact offset mapping from luwen reference
            let arc0_fw_version = self.comms.axi_read32(telemetry_struct_offset + (6 * 4))?;
            let arc1_fw_version = self.comms.axi_read32(telemetry_struct_offset + (7 * 4))?;
            let arc2_fw_version = self.comms.axi_read32(telemetry_struct_offset + (8 * 4))?;
            let arc3_fw_version = self.comms.axi_read32(telemetry_struct_offset + (9 * 4))?;
            let spibootrom_fw_version =
                self.comms.axi_read32(telemetry_struct_offset + (10 * 4))?;
            let eth_fw_version = self.comms.axi_read32(telemetry_struct_offset + (11 * 4))?;
            let m3_bl_fw_version = self.comms.axi_read32(telemetry_struct_offset + (12 * 4))?;
            let m3_app_fw_version = self.comms.axi_read32(telemetry_struct_offset + (13 * 4))?;
            let ddr_status = self.comms.axi_read32(telemetry_struct_offset + (14 * 4))?;
            let eth_status0 = self.comms.axi_read32(telemetry_struct_offset + (15 * 4))?;
            let eth_status1 = self.comms.axi_read32(telemetry_struct_offset + (16 * 4))?;
            let pcie_status = self.comms.axi_read32(telemetry_struct_offset + (17 * 4))?;
            let faults = self.comms.axi_read32(telemetry_struct_offset + (18 * 4))?;
            let arc0_health = self.comms.axi_read32(telemetry_struct_offset + (19 * 4))?;
            let arc1_health = self.comms.axi_read32(telemetry_struct_offset + (20 * 4))?;
            let arc2_health = self.comms.axi_read32(telemetry_struct_offset + (21 * 4))?;
            let arc3_health = self.comms.axi_read32(telemetry_struct_offset + (22 * 4))?;
            let fan_speed = self.comms.axi_read32(telemetry_struct_offset + (23 * 4))?;
            let aiclk = self.comms.axi_read32(telemetry_struct_offset + (24 * 4))?;
            eprintln!(
                "[DEBUG] aiclk at offset 0x{:x}: 0x{:x} ({})",
                telemetry_struct_offset + (24 * 4),
                aiclk,
                aiclk & 0xffff
            );

            let axiclk = self.comms.axi_read32(telemetry_struct_offset + (25 * 4))?;
            let arcclk = self.comms.axi_read32(telemetry_struct_offset + (26 * 4))?;
            let throttler = self.comms.axi_read32(telemetry_struct_offset + (27 * 4))?;

            let vcore = self.comms.axi_read32(telemetry_struct_offset + (28 * 4))?;
            eprintln!(
                "[DEBUG] vcore at offset 0x{:x}: 0x{:x} ({}mV)",
                telemetry_struct_offset + (28 * 4),
                vcore,
                vcore
            );

            let asic_temperature = self.comms.axi_read32(telemetry_struct_offset + (29 * 4))?;
            eprintln!(
                "[DEBUG] asic_temperature at offset 0x{:x}: 0x{:x}",
                telemetry_struct_offset + (29 * 4),
                asic_temperature
            );

            let vreg_temperature = self.comms.axi_read32(telemetry_struct_offset + (30 * 4))?;
            let board_temperature = self.comms.axi_read32(telemetry_struct_offset + (31 * 4))?;

            let tdp = self.comms.axi_read32(telemetry_struct_offset + (32 * 4))?;
            eprintln!(
                "[DEBUG] tdp at offset 0x{:x}: 0x{:x} ({}W)",
                telemetry_struct_offset + (32 * 4),
                tdp,
                tdp & 0xffff
            );

            let tdc = self.comms.axi_read32(telemetry_struct_offset + (33 * 4))?;
            let vdd_limits = self.comms.axi_read32(telemetry_struct_offset + (34 * 4))?;
            let thm_limits = self.comms.axi_read32(telemetry_struct_offset + (35 * 4))?;
            let wh_fw_date = self.comms.axi_read32(telemetry_struct_offset + (36 * 4))?;
            let asic_tmon0 = self.comms.axi_read32(telemetry_struct_offset + (37 * 4))?;
            let asic_tmon1 = self.comms.axi_read32(telemetry_struct_offset + (38 * 4))?;

            eprintln!(
                "[DEBUG] Telemetry read: aiclk={aiclk}, vcore={vcore}, tdp={tdp}, temperature={asic_temperature}"
            );

            Ok(Telemetry {
                arch: self.arch,
                board_id: ((board_id_high as u64) << 32) | (board_id_low as u64),
                enum_version,
                device_id,
                asic_ro,
                asic_idd,
                board_id_high,
                board_id_low,
                arc0_fw_version,
                arc1_fw_version,
                arc2_fw_version,
                arc3_fw_version,
                spibootrom_fw_version,
                eth_fw_version,
                m3_bl_fw_version,
                m3_app_fw_version,
                ddr_status,
                eth_status0,
                eth_status1,
                pcie_status,
                faults,
                arc0_health,
                arc1_health,
                arc2_health,
                arc3_health,
                fan_speed,
                aiclk,
                axiclk,
                arcclk,
                throttler,
                vcore,
                asic_temperature,
                vreg_temperature,
                board_temperature,
                tdp,
                tdc,
                vdd_limits,
                thm_limits,
                wh_fw_date,
                asic_tmon0,
                asic_tmon1,
                timer_heartbeat: arc0_health,
                ..Default::default()
            })
        } else {
            // For other architectures, return default for now
            Ok(Telemetry {
                arch: self.arch,
                ..Default::default()
            })
        self
    }
}
ORPHANED CODE END */

impl ChipComms for LuwenChip {
    fn axi_sread32(&self, addr: &str) -> Result<u32, Box<dyn std::error::Error>> {
        self.comms.axi_sread32(addr)
    }

    fn axi_write32(&self, addr: &str, value: u32) -> Result<(), Box<dyn std::error::Error>> {
        let addr_data = axi_translate(addr)?;
        let data = value.to_le_bytes();
        self.comms.axi_write(addr_data.addr, &data)
    }

    fn axi_translate(
        &self,
        addr: &str,
    ) -> Result<super::ttkmd::kmdif::AxiData, Box<dyn std::error::Error>> {
        axi_translate(addr)
    }
}

impl HlComms for LuwenChip {
    fn comms_obj(&self) -> (&dyn super::chip::ChipComms, &dyn ChipInterface) {
        (self, self.comms.as_ref())
    }
}
