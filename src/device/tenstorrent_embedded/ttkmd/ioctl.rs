// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

const TENSTORRENT_IOCTL_MAGIC: u8 = 0xFA;

use nix::request_code_none;

#[derive(Debug)]
#[repr(C)]
pub struct GetDeviceInfoIn {
    pub output_size_bytes: u32,
}

impl Default for GetDeviceInfoIn {
    fn default() -> Self {
        Self {
            output_size_bytes: std::mem::size_of::<GetDeviceInfoOut>() as u32,
        }
    }
}

#[derive(Default, Debug)]
#[repr(C)]
pub struct GetDeviceInfoOut {
    pub output_size_bytes: u32,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub bus_dev_fn: u16,            // [0:2] function, [3:7] device, [8:15] bus
    pub max_dma_buf_size_log2: u16, // Since 1.0
    pub pci_domain: u16,            // Since 1.23
}

#[derive(Default, Debug)]
#[repr(C)]
pub struct GetDeviceInfo {
    pub input: GetDeviceInfoIn,
    pub output: GetDeviceInfoOut,
}

nix::ioctl_readwrite_bad!(
    get_device_info,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 0),
    GetDeviceInfo
);

// Helper functions for easier use
pub unsafe fn ioctl_get_device_info(
    fd: nix::libc::c_int,
    info: *mut super::kmdif::DeviceInfo,
) -> nix::Result<()> {
    let mut ioctl_data = GetDeviceInfo::default();
    get_device_info(fd, &mut ioctl_data)?;

    // Convert output to DeviceInfo
    let device_info = super::kmdif::DeviceInfo {
        vendor_id: ioctl_data.output.vendor_id,
        device_id: ioctl_data.output.device_id,
        subsystem_vendor_id: ioctl_data.output.subsystem_vendor_id,
        subsystem_id: ioctl_data.output.subsystem_id,
        bus_dev_fn: ioctl_data.output.bus_dev_fn,
        max_dma_buf_size_log2: ioctl_data.output.max_dma_buf_size_log2,
        pci_domain: ioctl_data.output.pci_domain,
    };

    std::ptr::write(info, device_info);
    Ok(())
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct Mapping {
    pub mapping_id: u32,
    _reserved: u32,
    pub mapping_base: u64,
    pub mapping_size: u64,
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct QueryMappingsIn {
    pub output_mapping_count: u32,
    _reserved: u32,
}

#[derive(Debug)]
#[repr(C)]
pub struct QueryMappingsOut<const N: usize> {
    pub mappings: [Mapping; N],
}

impl<const N: usize> Default for QueryMappingsOut<N> {
    fn default() -> Self {
        Self {
            mappings: [Mapping::default(); N],
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct QueryMappings<const N: usize> {
    pub input: QueryMappingsIn,
    pub output: QueryMappingsOut<N>,
}

impl<const N: usize> Default for QueryMappings<N> {
    fn default() -> Self {
        Self {
            input: QueryMappingsIn {
                output_mapping_count: N as u32,
                ..Default::default()
            },
            output: QueryMappingsOut::<N>::default(),
        }
    }
}

/// # Safety
///
/// You must make sure that data is a valid pointer and that the file descriptor is valid
pub unsafe fn query_mappings<const N: usize>(
    fd: nix::libc::c_int,
    data: *mut QueryMappings<N>,
) -> nix::Result<nix::libc::c_int> {
    nix::convert_ioctl_res!(nix::libc::ioctl(
        fd,
        request_code_none!(TENSTORRENT_IOCTL_MAGIC, 2) as nix::sys::ioctl::ioctl_num_type,
        data
    ))
}

// Helper function for QueryMappings to work with PciBarMapping
pub unsafe fn ioctl_query_mappings(
    fd: nix::libc::c_int,
    mappings: *mut super::pci::QueryMappings,
) -> nix::Result<()> {
    const MAX_MAPPINGS: usize = 6;
    let mut ioctl_data = QueryMappings::<MAX_MAPPINGS>::default();
    query_mappings(fd, &mut ioctl_data)?;

    // Convert to PciBarMapping format
    let mut output = super::pci::QueryMappings::default();
    let count = ioctl_data
        .input
        .output_mapping_count
        .min(MAX_MAPPINGS as u32);
    output.mapping_count = count;

    for i in 0..count as usize {
        let mapping = &ioctl_data.output.mappings[i];
        output.mappings[i] = super::pci::PciBarMapping {
            mapping_id: mapping.mapping_id,
            base_address: mapping.mapping_base,
            mapping_size: mapping.mapping_size,
        };
    }

    std::ptr::write(mappings, output);
    Ok(())
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateDmaBufferIn {
    pub requested_size: u32,
    pub buf_index: u8, // [0,TENSTORRENT_MAX_DMA_BUFS)
    _reserved0: [u8; 3],
    _reserved1: [u64; 2],
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateDmaBufferOut {
    pub physical_address: u64,
    pub mapping_offset: u64,
    pub size: u32,
    _reserved0: u32,
    _reserved1: [u64; 2],
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateDmaBuffer {
    pub input: AllocateDmaBufferIn,
    pub output: AllocateDmaBufferOut,
}

nix::ioctl_readwrite_bad!(
    allocate_dma_buffer,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 3),
    AllocateDmaBuffer
);

#[derive(Default)]
#[repr(C)]
pub struct FreeDmaBufferIn;
#[derive(Default)]
#[repr(C)]
pub struct FreeDmaBufferOut;

#[derive(Default)]
#[repr(C)]
pub struct FreeDmaBuffer {
    pub input: FreeDmaBufferIn,
    pub output: FreeDmaBufferOut,
}

nix::ioctl_readwrite_bad!(
    free_dma_buffer,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 4),
    FreeDmaBuffer
);

#[repr(C)]
pub struct GetDriverInfoIn {
    pub output_size_bytes: u32,
}

impl Default for GetDriverInfoIn {
    fn default() -> Self {
        Self {
            output_size_bytes: std::mem::size_of::<GetDriverInfoOut>() as u32,
        }
    }
}

#[derive(Default)]
#[repr(C)]
pub struct GetDriverInfoOut {
    pub output_size_bytes: u32,
    pub driver_version: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct GetDriverInfo {
    pub input: GetDriverInfoIn,
    pub output: GetDriverInfoOut,
}

nix::ioctl_readwrite_bad!(
    get_driver_info,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 5),
    GetDriverInfo
);

pub const RESET_DEVICE_RESTORE_STATE: u32 = 0;
pub const RESET_DEVICE_RESET_PCIE_LINK: u32 = 1;
pub const RESET_DEVICE_RESET_CONFIG_WRITE: u32 = 2;

#[repr(C)]
pub struct ResetDeviceIn {
    pub output_size_bytes: u32,
    pub flags: u32,
}

impl Default for ResetDeviceIn {
    fn default() -> Self {
        Self {
            output_size_bytes: std::mem::size_of::<Self>() as u32,
            flags: 0,
        }
    }
}

#[derive(Default)]
#[repr(C)]
pub struct ResetDeviceOut {
    pub output_size_bytes: u32,
    pub result: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct ResetDevice {
    pub input: ResetDeviceIn,
    pub output: ResetDeviceOut,
}

nix::ioctl_readwrite_bad!(
    reset_device,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 6),
    ResetDevice
);

#[derive(Default)]
#[repr(C)]
pub struct AllocateTlbIn {
    pub size: u64,
    pub reserved: u64,
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateTlbOut {
    pub id: u32,
    pub reserved0: u32,
    pub mmap_offset_uc: u64,
    pub mmap_offset_wc: u64,
    pub reserved1: u64,
}

#[derive(Default)]
#[repr(C)]
pub struct AllocateTlb {
    pub input: AllocateTlbIn,
    pub output: AllocateTlbOut,
}

nix::ioctl_readwrite_bad!(
    allocate_tlb,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 11),
    AllocateTlb
);

// Helper function for allocating TLB for BAR mapping
pub unsafe fn ioctl_allocate_tlb(
    fd: nix::libc::c_int,
    buffer: *mut super::tlb::AllocateDmaBuffer,
) -> nix::Result<()> {
    let alloc_buffer = std::ptr::read(buffer);

    let mut ioctl_data = AllocateTlb {
        input: AllocateTlbIn {
            size: alloc_buffer.requested_size as u64,
            reserved: 0,
        },
        output: AllocateTlbOut::default(),
    };

    allocate_tlb(fd, &mut ioctl_data)?;

    // Update the buffer with the result
    let mut result = alloc_buffer;
    result.physical_address = ioctl_data.output.mmap_offset_wc; // Use WC mapping offset

    std::ptr::write(buffer, result);
    Ok(())
}

#[derive(Default)]
#[repr(C)]
pub struct FreeTlbIn {
    pub id: u32,
}

#[derive(Default)]
#[repr(C)]
pub struct FreeTlbOut {}

#[derive(Default)]
#[repr(C)]
pub struct FreeTlb {
    pub input: FreeTlbIn,
    pub output: FreeTlbOut,
}

nix::ioctl_readwrite_bad!(
    free_tlb,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 12),
    FreeTlb
);

#[derive(Default)]
#[repr(C)]
pub struct NocTlbConfig {
    pub addr: u64,
    pub x_end: u16,
    pub y_end: u16,
    pub x_start: u16,
    pub y_start: u16,
    pub noc: u8,
    pub mcast: u8,
    pub ordering: u8,
    pub linked: u8,
    pub static_vc: u8,
    pub reserved0: [u8; 3],
    pub reserved1: [u32; 2],
}

#[derive(Default)]
#[repr(C)]
pub struct ConfigureTlbIn {
    pub id: u32,
    pub config: NocTlbConfig,
}

#[derive(Default)]
#[repr(C)]
pub struct ConfigureTlbOut {
    pub reserved: u64,
}

#[derive(Default)]
#[repr(C)]
pub struct ConfigureTlb {
    pub input: ConfigureTlbIn,
    pub output: ConfigureTlbOut,
}

nix::ioctl_readwrite_bad!(
    configure_tlb,
    request_code_none!(TENSTORRENT_IOCTL_MAGIC, 13),
    ConfigureTlb
);
