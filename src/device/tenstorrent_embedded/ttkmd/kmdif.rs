// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

pub const MAX_DMA_BYTES: u32 = 4 * 1024 * 1024;
pub const GS_BAR0_WC_MAPPING_SIZE: u64 = (156 << 20) + (10 << 21) + (18 << 24);
pub const BH_BAR0_WC_MAPPING_SIZE: u64 = 188 << 21;

pub const GS_WH_ARC_SCRATCH6_ADDR: u32 = 0x1ff30078;
pub const BH_NOC_NODE_ID_OFFSET: u32 = 0x1FD04044;

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum MappingId {
    Unused = 0,
    Resource0Uc = 1,
    Resource0Wc = 2,
    Resource1Uc = 3,
    Resource1Wc = 4,
    Resource2Uc = 5,
    Resource2Wc = 6,
    Unknown(u32),
}

impl MappingId {
    pub fn from_u32(value: u32) -> MappingId {
        match value {
            0 => MappingId::Unused,
            1 => MappingId::Resource0Uc,
            2 => MappingId::Resource0Wc,
            3 => MappingId::Resource1Uc,
            4 => MappingId::Resource1Wc,
            5 => MappingId::Resource2Uc,
            6 => MappingId::Resource2Wc,
            v => MappingId::Unknown(v),
        }
    }

    pub fn as_u32(&self) -> u32 {
        // SAFTEY: Need to ensure that the enum has a primitive representation for this to be defined
        unsafe { *(self as *const Self as *const u32) }
    }
}

pub fn getpagesize() -> Option<i64> {
    nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE)
        .ok()
        .flatten()
}

/// Device information structure
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub bus_dev_fn: u16,
    pub max_dma_buf_size_log2: u16,
    pub pci_domain: u16,
}

#[bitfield_struct::bitfield(u32)]
pub struct DmaPack {
    #[bits(28)]
    pub size_bytes: u32, // Transfer size in bytes
    pub write: bool,              // 0 = Chip -> Host, 1 = Host -> Chip
    pub pcie_msi_on_done: bool, // Whether to configure DMA engine to send MSI on completion. pcie_msi_on_done and pcie_write_on_done are exclusive.
    pub pcie_write_on_done: bool, // Instead of triggering an MSI, write to a location stored in pcie_config_t.completion_flag_phys_addr. pcie_msi_on_done and pcie_write_on_done are exclusive.
    pub trigger: bool,            // 1 = Start transfer. The handler should reset it to 0.
}

#[repr(C)]
pub struct ArcPcieCtrlDmaRequest {
    pub chip_addr: u32,                 // Local address (on the device)
    pub host_phys_addr_lo: u32,         // Host physical address (this is physical address)
    pub completion_flag_phys_addr: u32, // Pointer to the completion flag - the dma engine will write to this address to report completion
    pub dma_pack: DmaPack,
    pub repeat: u32, // How many times to repeat the oparation (for debug only) bit31 indicates whether the request is 64 bit transfer
} // 5 * 4 = 20B

#[derive(Clone)]
pub struct DmaConfig {
    /// Address in CSM where the DMA request structure resides
    pub csm_pcie_ctrl_dma_request_offset: u32,

    /// To trigger ARC interrupt
    pub arc_misc_cntl_addr: u32,

    /// DMA host phys addr high
    pub dma_host_phys_addr_high: u32,

    pub support_64_bit_dma: bool,

    pub use_msi_for_dma: bool,

    pub read_threshold: u32,
    pub write_threshold: u32,
}

pub struct PhysicalDevice {
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,

    pub pci_bus: u16,
    pub slot: u16,
    pub pci_function: u16,
    pub pci_domain: u16,
}

pub struct BarMapping {
    pub bar_addr: u64,
    pub bar_size_bytes: u64,

    pub bar0_uc: memmap2::MmapMut,
    #[allow(dead_code)]
    pub bar0_uc_size: u64,
    pub bar0_uc_offset: u64,

    pub bar0_wc: Option<memmap2::MmapMut>,
    pub bar0_wc_size: u64,

    pub bar1_uc: Option<memmap2::MmapMut>,
    pub bar1_uc_size: u64,

    pub system_reg_mapping: Option<memmap2::MmapMut>,
    #[allow(dead_code)]
    pub system_reg_mapping_size: usize,
    pub system_reg_start_offset: u32, // Registers >= this are system regs, use the mapping.
    pub system_reg_offset_adjust: u32, // This is the offset of the first reg in the system reg mapping.
}

#[derive(Debug)]
pub struct TlbAllocation {
    pub id: u32,
    pub uc_mapping: memmap2::MmapMut,
    pub size: u64,
}

#[derive(Debug)]
pub enum PossibleTlbAllocation {
    Allocation(TlbAllocation),
    Hardcoded(u32),
    NoAllocation,
}

pub struct DmaBuffer {
    pub buffer: memmap2::MmapMut,
    pub physical_address: u64,
    pub size: u64,
}

use crate::device::tenstorrent_embedded::arch::Arch;

pub trait ChipInterface: Send + Sync {
    fn axi_read(&self, addr: u32, data: &mut [u8]) -> Result<(), Box<dyn std::error::Error>>;
    fn axi_write(&self, addr: u32, data: &[u8]) -> Result<(), Box<dyn std::error::Error>>;
    fn noc_read(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn noc_write(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn noc_broadcast(
        &self,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn noc_multicast(
        &self,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;

    fn axi_sread32(&self, addr_str: &str) -> Result<u32, Box<dyn std::error::Error>> {
        let mut data = [0u8; 4];
        self.axi_read(
            crate::device::tenstorrent_embedded::luwen_ref::axi_translate(addr_str)?.addr,
            &mut data,
        )?;
        Ok(u32::from_le_bytes(data))
    }

    fn axi_read32(&self, addr: u32) -> Result<u32, Box<dyn std::error::Error>> {
        let mut data = [0u8; 4];
        self.axi_read(addr, &mut data)?;
        Ok(u32::from_le_bytes(data))
    }
}

impl<T: ChipInterface> ChipInterface for &T {
    fn axi_read(&self, addr: u32, data: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        (*self).axi_read(addr, data)
    }

    fn axi_write(&self, addr: u32, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        (*self).axi_write(addr, data)
    }

    fn noc_read(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (*self).noc_read(noc_id, x, y, addr, data)
    }

    fn noc_write(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (*self).noc_write(noc_id, x, y, addr, data)
    }

    fn noc_broadcast(
        &self,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (*self).noc_broadcast(noc_id, addr, data)
    }

    fn noc_multicast(
        &self,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (*self).noc_multicast(noc_id, start, end, addr, data)
    }
}

#[allow(dead_code)]
pub struct PciDevice {
    pub id: usize,

    pub physical: PhysicalDevice,
    pub arch: Arch,

    pub read_checking_enabled: bool,
    pub read_checking_addr: u32,

    pub(crate) next_dma_buf: usize,

    pub(crate) device_fd: std::fs::File,
    pub driver_version: u32,

    pub(crate) config_space: std::fs::File,

    pub(crate) max_dma_buf_size_log2: u16,

    #[allow(dead_code)]
    pub(crate) dma_buffer_mappings: Vec<std::sync::Arc<DmaBuffer>>,
    pub(crate) completion_flag_buffer: Option<DmaBuffer>,
    pub(crate) transfer_buffer: Option<DmaBuffer>,

    pub dma_config: Option<DmaConfig>,
    pub pci_bar: Option<BarMapping>,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct AxiData {
    pub addr: u32,
    pub size_bytes: u32,
}

pub fn axi_translate(addr_str: &str) -> Result<AxiData, Box<dyn std::error::Error>> {
    let mut data = AxiData::default();
    data.addr = addr_str.parse::<u32>()?;
    Ok(data)
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C)]
pub struct TlbData {
    pub local_offset: u64,
    pub x_end: u32,
    pub y_end: u32,
    pub x_start: u32,
    pub y_start: u32,
    pub noc_sel: u32,
    pub mcast: bool,
    pub ordering: TlbOrdering,
    pub linked: bool,
    pub static_vc: u32,
}

#[derive(Default, Debug, Clone, Copy)]
pub enum TlbOrdering {
    #[default]
    Posted,
    Relaxed,
    Strict,
}

impl From<TlbOrdering> for u8 {
    fn from(val: TlbOrdering) -> Self {
        match val {
            TlbOrdering::Posted => 0,
            TlbOrdering::Relaxed => 1,
            TlbOrdering::Strict => 2,
        }
    }
}
