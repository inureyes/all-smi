// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use thiserror::Error;

use super::arch::Arch;
use super::chip::{ChipImpl, HlComms, Telemetry};
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
        eprintln!(
            "[DEBUG] ExtendedPciDevice::open() called with interface {}",
            pci_interface
        );
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
        eprintln!("[DEBUG] Grid size: {}x{}", grid_size_x, grid_size_y);

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

            eprintln!("[DEBUG] Attempting to allocate TLB of size: {}", size);
            match device.allocate_tlb(size) {
                Ok(tlb) => {
                    eprintln!("[DEBUG] TLB allocation succeeded");
                    default_tlb = PossibleTlbAllocation::Allocation(tlb);
                }
                Err(e) => {
                    eprintln!("[DEBUG] TLB allocation failed: {:?}", e);
                    // Couldn't get a tlb... ideally at this point we would fallback to using a slower but useable read/write API
                    // for now though, we will just fail
                    return Err(PciError::TlbAllocationError(format!(
                        "Failed to find a free tlb: {:?}",
                        e
                    )));
                }
            }
        } else {
            // Otherwise fallback to default behaviour of just taking a constant one
            let hardcoded_tlb = match device.arch {
                Arch::Grayskull | Arch::Wormhole => 184,
                Arch::Blackhole => 190,
            };
            eprintln!("[DEBUG] Using hardcoded TLB: {}", hardcoded_tlb);
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
}

impl ChipImpl for LuwenChip {
    fn get_arch(&self) -> Arch {
        self.arch
    }

    fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        Ok(Telemetry::default())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl HlComms for LuwenChip {
    fn comms_obj(&self) -> (&dyn super::chip::ChipComms, &dyn ChipInterface) {
        (self, self.comms.as_ref())
    }
}

impl super::chip::ChipComms for LuwenChip {
    fn axi_sread32(&self, addr: &str) -> Result<u32, Box<dyn std::error::Error>> {
        self.comms.axi_sread32(addr)
    }
}
