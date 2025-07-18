// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::os::fd::{AsRawFd, RawFd};

use super::{
    error::{PciError, PciOpenError},
    ioctl,
    kmdif::{self, DmaBuffer, PossibleTlbAllocation, TlbAllocation},
    PciDevice,
};

/// PCIe BAR mapping information
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct PciBarMapping {
    pub mapping_id: u32,
    pub base_address: u64,
    pub mapping_size: u64,
}

/// Query mappings result structure
#[derive(Debug, Default)]
#[repr(C)]
pub struct QueryMappings {
    pub mappings: [PciBarMapping; 6],
    pub mapping_count: u32,
}

const ERROR_VALUE: u32 = 0xffffffff;

pub(crate) fn read_bar0_base(config_space: &std::fs::File) -> u64 {
    const BAR_ADDRESS_MASK: u64 = !0xFu64;

    let bar0_config_offset = 0x10;

    let mut bar01 = [0u8; std::mem::size_of::<u64>()];
    let size = nix::sys::uio::pread(config_space, &mut bar01, bar0_config_offset);
    match size {
        Ok(size) => {
            if size != std::mem::size_of::<u64>() {
                panic!("Failed to read BAR0 config space: {size}");
            }
        }
        Err(err) => {
            panic!("Failed to read BAR0 config space: {err}");
        }
    }

    u64::from_ne_bytes(bar01) & BAR_ADDRESS_MASK
}

impl super::kmdif::BarMapping {
    unsafe fn register_address_mut<T>(&self, mut register_addr: u32) -> *mut T {
        let reg_mapping: *mut u8;

        if self.system_reg_mapping.is_some() && register_addr >= self.system_reg_start_offset {
            let mapping = self.system_reg_mapping.as_ref().unwrap_unchecked();

            register_addr -= self.system_reg_offset_adjust;
            reg_mapping = mapping.as_ptr() as *mut u8;
        } else if self.bar0_wc.is_some() && (register_addr as u64) < self.bar0_wc_size {
            let mapping = self.bar0_wc.as_ref().unwrap_unchecked();

            reg_mapping = mapping.as_ptr() as *mut u8;
        } else {
            register_addr -= self.bar0_uc_offset as u32;
            reg_mapping = self.bar0_uc.as_ptr() as *mut u8;
        }

        reg_mapping.offset(register_addr as isize) as *mut T
    }

    unsafe fn register_address<T>(&self, register_addr: u32) -> *const T {
        self.register_address_mut(register_addr) as *const T
    }
}

impl PciDevice {
    pub fn open(device_id: usize) -> Result<PciDevice, super::error::PciOpenError> {
        use super::error::PciOpenError;
        use super::ioctl::GetDeviceInfo;

        let device_path = format!("/dev/tenstorrent/{device_id}");
        eprintln!("[DEBUG] PciDevice::open() trying to open: '{device_path}'");
        eprintln!(
            "[DEBUG] Current user: uid={}, euid={}",
            unsafe { libc::getuid() },
            unsafe { libc::geteuid() }
        );

        // Check if file exists first
        if let Ok(metadata) = std::fs::metadata(&device_path) {
            eprintln!("[DEBUG] File exists: {:?}", metadata.file_type());
            eprintln!("[DEBUG] Is file: {}", metadata.is_file());
            eprintln!("[DEBUG] Is dir: {}", metadata.is_dir());

            // Try to check permissions
            use std::os::unix::fs::PermissionsExt;
            eprintln!("[DEBUG] File mode: {:o}", metadata.permissions().mode());
        } else {
            eprintln!("[DEBUG] Cannot get metadata for '{device_path}'");

            // Let's check what we can see in the parent directory
            if let Ok(entries) = std::fs::read_dir("/dev/tenstorrent") {
                eprintln!("[DEBUG] Contents of /dev/tenstorrent:");
                for entry in entries {
                    if let Ok(entry) = entry {
                        eprintln!("[DEBUG]   - {:?}", entry.path());
                    }
                }
            }
        }

        // First try opening with read-only
        eprintln!("[DEBUG] Trying to open with read-only permissions...");
        let fd_readonly = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(&device_path);

        match fd_readonly {
            Ok(_) => eprintln!("[DEBUG] Read-only open succeeded! Now trying read-write..."),
            Err(e) => eprintln!("[DEBUG] Read-only open failed: {e}"),
        }

        let fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&device_path);
        let fd = match fd {
            Ok(fd) => {
                eprintln!("[DEBUG] Read-write open SUCCEEDED!");
                fd
            }
            Err(err) => {
                eprintln!("[DEBUG] Failed to open '{device_path}': {err}");
                eprintln!("[DEBUG] Error kind: {:?}", err.kind());
                eprintln!("[DEBUG] Raw OS error: {:?}", err.raw_os_error());

                // Let's check if this is actually a symlink
                if let Ok(metadata) = std::fs::symlink_metadata(&device_path) {
                    eprintln!("[DEBUG] Is symlink: {}", metadata.file_type().is_symlink());
                    if metadata.file_type().is_symlink() {
                        if let Ok(target) = std::fs::read_link(&device_path) {
                            eprintln!("[DEBUG] Symlink target: {target:?}");
                        }
                    }
                }

                // Let's try using a raw file descriptor approach
                eprintln!("[DEBUG] Trying raw open() syscall...");
                use std::ffi::CString;
                use std::os::unix::io::FromRawFd;

                let c_path = CString::new(device_path.as_str()).unwrap();
                let raw_fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR) };

                if raw_fd >= 0 {
                    eprintln!("[DEBUG] Raw open() succeeded with fd: {raw_fd}");
                    unsafe { std::fs::File::from_raw_fd(raw_fd) }
                } else {
                    let errno = std::io::Error::last_os_error();
                    eprintln!("[DEBUG] Raw open() failed with error: {errno}");
                    eprintln!("[DEBUG] errno code: {}", errno.raw_os_error().unwrap_or(-1));

                    // Common errno values:
                    // ENOENT (2): No such file or directory
                    // EACCES (13): Permission denied
                    // EBUSY (16): Device or resource busy
                    // EINVAL (22): Invalid argument

                    return Err(PciOpenError::DeviceOpenFailed {
                        id: device_id,
                        source: err,
                    });
                }
            }
        };

        eprintln!("[DEBUG] About to call get_device_info ioctl...");
        let mut device_info = GetDeviceInfo::default();
        if let Err(errorno) = unsafe { ioctl::get_device_info(fd.as_raw_fd(), &mut device_info) } {
            eprintln!("[DEBUG] get_device_info ioctl FAILED: {errorno}");
            return Err(PciOpenError::IoctlError {
                name: "get_device_info".to_string(),
                id: device_id,
                source: errorno,
            });
        }
        eprintln!("[DEBUG] get_device_info ioctl succeeded");

        // Get PCI bus information from device_info for sysfs config space path
        let pci_bus = device_info.output.bus_dev_fn >> 8;
        let slot = ((device_info.output.bus_dev_fn) >> 3) & 0x1f; // The definition of PCI_SLOT from include/uapi/linux/pci.h
        let pci_function = (device_info.output.bus_dev_fn) & 0x7; // The definition of PCI_FUNC from include/uapi/linux/pci.h
        let pci_domain = device_info.output.pci_domain;

        eprintln!(
            "[DEBUG] Opening config space from sysfs: /sys/bus/pci/devices/{pci_domain:04x}:{pci_bus:02x}:{slot:02x}.{pci_function:01x}/config"
        );
        let config_space = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(format!(
                "/sys/bus/pci/devices/{pci_domain:04x}:{pci_bus:02x}:{slot:02x}.{pci_function:01x}/config"
            ));
        let config_space = match config_space {
            Ok(fd) => {
                eprintln!("[DEBUG] Config space opened successfully from sysfs");
                fd
            }
            Err(err) => {
                eprintln!("[DEBUG] Failed to open config space from sysfs: {err}");
                return Err(PciOpenError::DeviceOpenFailed {
                    id: device_id,
                    source: err,
                });
            }
        };

        let arch = match device_info.output.device_id {
            0xfaca => super::super::arch::Arch::Grayskull,
            0x401e => super::super::arch::Arch::Wormhole,
            0xb140 => super::super::arch::Arch::Blackhole,
            _ => {
                eprintln!(
                    "[DEBUG] Unrecognized device ID: 0x{:04x}",
                    device_info.output.device_id
                );
                return Err(PciOpenError::UnrecognizedDeviceId {
                    pci_id: device_id,
                    device_id: device_info.output.device_id,
                });
            }
        };

        // Extract bus, device, function from bus_dev_fn field
        let bus_dev_fn = device_info.output.bus_dev_fn;
        let pci_function = bus_dev_fn & 0x7;
        let slot = (bus_dev_fn >> 3) & 0x1f;
        let pci_bus = (bus_dev_fn >> 8) & 0xff;

        // Get driver version separately
        let mut driver_info = super::ioctl::GetDriverInfo::default();
        let driver_version =
            if unsafe { ioctl::get_driver_info(fd.as_raw_fd(), &mut driver_info) }.is_ok() {
                driver_info.output.driver_version
            } else {
                0 // Default if we can't get driver info
            };

        let mut device = PciDevice {
            id: device_id,
            physical: super::kmdif::PhysicalDevice {
                vendor_id: device_info.output.vendor_id,
                device_id: device_info.output.device_id,
                subsystem_vendor_id: device_info.output.subsystem_vendor_id,
                subsystem_id: device_info.output.subsystem_id,

                pci_bus,
                slot,
                pci_function,
                pci_domain: device_info.output.pci_domain,
            },
            arch,

            read_checking_enabled: false,
            read_checking_addr: 0,

            next_dma_buf: 0,

            device_fd: fd,
            driver_version,

            config_space,

            max_dma_buf_size_log2: device_info.output.max_dma_buf_size_log2,

            dma_buffer_mappings: Vec::new(),
            completion_flag_buffer: None,
            transfer_buffer: None,

            dma_config: None,
            pci_bar: None,
        };

        // To avoid needing a warmup when performing the first dma access, try to allocate the
        // buffers now.
        device.allocate_transfer_buffers();

        // Initialize BAR mapping
        device.map_bar()?;

        Ok(device)
    }

    pub fn read_cfg(&self, byte_offset: u32, data: &mut [u8]) -> Result<(), PciError> {
        let size = nix::sys::uio::pread(&self.config_space, data, byte_offset as i64);
        match size {
            Ok(size) => {
                if size != data.len() {
                    return Err(PciError::CfgReadFailed {
                        id: self.id,
                        offset: byte_offset as usize,
                        size: data.len(),
                        source: super::error::CfgFailType::SizeMismatch(size),
                    });
                }
            }
            Err(err) => {
                return Err(PciError::CfgReadFailed {
                    id: self.id,
                    offset: byte_offset as usize,
                    size: data.len(),
                    source: super::error::CfgFailType::Nix(err),
                });
            }
        }

        Ok(())
    }

    pub fn write_cfg(&self, byte_offset: u32, data: &[u8]) -> Result<(), PciError> {
        let size = nix::sys::uio::pwrite(&self.config_space, data, byte_offset as i64);
        match size {
            Ok(size) => {
                if size != data.len() {
                    return Err(PciError::CfgWriteFailed {
                        id: self.id,
                        offset: byte_offset as usize,
                        size: data.len(),
                        source: super::error::CfgFailType::SizeMismatch(size),
                    });
                }
            }
            Err(err) => {
                return Err(PciError::CfgWriteFailed {
                    id: self.id,
                    offset: byte_offset as usize,
                    size: data.len(),
                    source: super::error::CfgFailType::Nix(err),
                });
            }
        }

        Ok(())
    }

    #[inline]
    pub fn detect_ffffffff_read(&self, data_read: Option<u32>) -> Result<(), PciError> {
        let data_read = data_read.unwrap_or(ERROR_VALUE);

        if self.read_checking_enabled && data_read == ERROR_VALUE {
            let scratch_data = match &self.pci_bar {
                Some(bar) => unsafe {
                    bar.register_address::<u32>(self.read_checking_addr)
                        .read_volatile()
                },
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };

            if scratch_data == ERROR_VALUE {
                return Err(PciError::BrokenConnection);
            }
        }

        Ok(())
    }

    #[inline]
    pub fn read32_no_translation(&self, addr: usize) -> Result<u32, PciError> {
        let data = if addr % core::mem::align_of::<u32>() != 0 {
            unsafe {
                let aligned_read_pointer = addr & !(core::mem::align_of::<u32>() - 1);
                let a = (aligned_read_pointer as *const u32).read_volatile();
                let b = (aligned_read_pointer as *const u32).add(1).read_volatile();

                let byte_offset = addr % core::mem::align_of::<u32>();
                let shift = byte_offset * 8;
                let inverse_shift = (core::mem::size_of::<u32>() * 8) - shift;
                let inverse_mask = (1 << inverse_shift) - 1;
                let _mask = (1 << shift) - 1;

                ((a >> shift) & inverse_mask) | ((b << inverse_shift) & !inverse_mask)
            }
        } else {
            unsafe { (addr as *const u32).read_volatile() }
        };
        self.detect_ffffffff_read(Some(data))?;

        Ok(data)
    }

    #[inline]
    pub fn read32(&self, addr: u32) -> Result<u32, PciError> {
        let read_pointer = match &self.pci_bar {
            Some(bar) => unsafe { bar.register_address::<u32>(addr) as usize },
            None => {
                return Err(PciError::BarUnmapped);
            }
        };
        self.read32_no_translation(read_pointer)
    }

    #[inline]
    pub fn write32_no_translation(&mut self, addr: usize, data: u32) -> Result<(), PciError> {
        if addr % core::mem::align_of::<u32>() != 0 {
            unsafe {
                let aligned_write_pointer = addr & !(core::mem::align_of::<u32>() - 1);
                let a = (aligned_write_pointer as *const u32).read_volatile();
                let b = (aligned_write_pointer as *const u32).add(1).read_volatile();

                let byte_offset = addr % core::mem::align_of::<u32>();
                let shift = byte_offset * 8;
                let inverse_shift = (core::mem::size_of::<u32>() * 8) - shift;
                let inverse_mask = (1 << inverse_shift) - 1;
                let mask = (1 << shift) - 1;

                let a = (a & mask) | ((data & inverse_mask) << shift);
                let b = (b & !mask) | ((data & !inverse_mask) >> inverse_shift);

                (aligned_write_pointer as *mut u32).write_volatile(a);
                (aligned_write_pointer as *mut u32).add(1).write_volatile(b);
            }
        } else {
            unsafe { (addr as *mut u32).write_volatile(data) }
        };
        self.detect_ffffffff_read(None)?;

        Ok(())
    }

    #[inline]
    pub fn write32(&mut self, addr: u32, data: u32) -> Result<(), PciError> {
        let write_pointer = match &self.pci_bar {
            Some(bar) => unsafe { bar.register_address::<u32>(addr) as usize },
            None => {
                return Err(PciError::BarUnmapped);
            }
        };
        self.write32_no_translation(write_pointer, data)
    }

    pub fn write_no_dma<T>(&mut self, addr: u32, data: &[T]) -> Result<(), PciError> {
        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut::<T>(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            ptr.copy_from_nonoverlapping(data.as_ptr(), data.len());
        }

        Ok(())
    }
}

impl PciDevice {
    // HACK(drosen): Yes user data should be a mut slice,
    // but I don't really want to refactor the code righ now to make that possible
    pub fn pcie_dma_transfer_turbo(
        &mut self,
        chip_addr: u32,
        host_buffer_addr: u64,
        size: u32,
        write: bool,
    ) -> Result<(), PciError> {
        if self.dma_config.is_none() || !self.allocate_transfer_buffers() {
            return Err(PciError::DmaNotConfigured { id: self.id });
        }

        let dma_config = unsafe { self.dma_config.as_ref().unwrap_unchecked().clone() };

        let host_phys_addr_hi = (host_buffer_addr >> 32) as u32;

        if host_phys_addr_hi != 0 && !dma_config.support_64_bit_dma {
            return Err(PciError::No64bitDma { id: self.id });
        }

        if size > (1 << 28) - 1 {
            return Err(PciError::DmaTooLarge {
                id: self.id,
                size: size as usize,
            });
        }

        // SAFETY: Already checked that the completion_flag_buffer is Some in
        // self.allocate_transfer_buffers
        let completion_flag_buffer =
            unsafe { self.completion_flag_buffer.as_mut().unwrap_unchecked() };
        let req = kmdif::ArcPcieCtrlDmaRequest {
            chip_addr,
            host_phys_addr_lo: (host_buffer_addr & 0xffffffff) as u32,
            completion_flag_phys_addr: completion_flag_buffer.physical_address as u32,
            dma_pack: kmdif::DmaPack::new()
                .with_size_bytes(size)
                .with_write(write)
                .with_pcie_msi_on_done(dma_config.use_msi_for_dma)
                .with_pcie_write_on_done(!dma_config.use_msi_for_dma)
                .with_trigger(true),
            repeat: 1 | (((host_phys_addr_hi != 0) as u32) << 31), // 64-bit PCIe DMA transfer request
        };

        let complete_flag = completion_flag_buffer.buffer.as_ptr() as *mut u32;
        unsafe { complete_flag.write_volatile(0) };

        // Configure the DMA engine
        if dma_config.support_64_bit_dma {
            self.write32(dma_config.dma_host_phys_addr_high, host_phys_addr_hi)?;
        }

        let config_addr = dma_config.csm_pcie_ctrl_dma_request_offset;

        assert!(config_addr % 4 == 0);
        self.write_no_dma(config_addr, unsafe {
            std::slice::from_raw_parts(
                &req as *const _ as *const u32,
                std::mem::size_of::<kmdif::ArcPcieCtrlDmaRequest>() / 4,
            )
        })?;

        // Trigger ARC interrupt 0 on core 0
        let mut arc_misc_cntl_value = 0;

        // NOTE: Ideally, we should read the state of this register before writing to it, but that
        //       casues a lot of delay (reads have huge latencies)
        arc_misc_cntl_value |= 1 << 16; // Cause IRQ0 on core 0
        self.write32(dma_config.arc_misc_cntl_addr, arc_misc_cntl_value)?;

        if !dma_config.use_msi_for_dma {
            loop {
                // The complete flag is set ty by ARC (see src/hardware/soc/tb/arc_fw/lib/pcie_dma.c)
                unsafe {
                    if complete_flag.read_volatile() == 0xfaca {
                        break;
                    }
                }
            }
        } else {
            unimplemented!("Do not currently support MSI based dma");
        }

        Ok(())
    }

    pub fn write_block(&mut self, addr: u32, data: &[u8]) -> Result<(), PciError> {
        if let Some(dma_config) = self.dma_config.clone() {
            #[allow(clippy::collapsible_if)] // I want to make it clear that these are seperate
            // types of checks
            if data.len() > dma_config.write_threshold as usize && dma_config.write_threshold > 0 {
                if self.allocate_transfer_buffers() {
                    let mut num_bytes = data.len();
                    let mut offset = 0;
                    while num_bytes > 0 {
                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer = unsafe { self.transfer_buffer.as_mut().unwrap_unchecked() };

                        let chunk_size = num_bytes.min(buffer.size as usize);
                        buffer.buffer[..chunk_size]
                            .copy_from_slice(&data[offset..(offset + chunk_size)]);

                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer_addr =
                            unsafe { self.transfer_buffer.as_mut().unwrap_unchecked() }
                                .physical_address;
                        self.pcie_dma_transfer_turbo(
                            addr + offset as u32,
                            buffer_addr,
                            chunk_size as u32,
                            true,
                        )?;
                        num_bytes = num_bytes.saturating_sub(chunk_size);
                        offset += chunk_size;
                    }

                    return Ok(());
                }
            }
        }

        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_to_device(ptr, data);
        }

        Ok(())
    }

    pub fn read_block(&mut self, addr: u32, data: &mut [u8]) -> Result<(), PciError> {
        if let Some(dma_config) = self.dma_config.clone() {
            #[allow(clippy::collapsible_if)] // I want to make it clear that these are seperate
            // types of checks
            if data.len() > dma_config.read_threshold as usize && dma_config.read_threshold > 0 {
                if self.allocate_transfer_buffers() {
                    let mut num_bytes = data.len();
                    let mut offset = 0;
                    while num_bytes > 0 {
                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer = unsafe { self.transfer_buffer.as_ref().unwrap_unchecked() };

                        let chunk_size = num_bytes.min(buffer.size as usize);

                        self.pcie_dma_transfer_turbo(
                            addr + offset as u32,
                            buffer.physical_address,
                            chunk_size as u32,
                            false,
                        )?;

                        // SAFETY: Already checked that the transfer_buffer is Some in
                        // self.allocate_transfer_buffers
                        let buffer = self.transfer_buffer.as_ref().unwrap();
                        data[offset..(offset + chunk_size)]
                            .copy_from_slice(&buffer.buffer[..chunk_size]);
                        num_bytes = num_bytes.saturating_sub(chunk_size);
                        offset += chunk_size;
                    }

                    return Ok(());
                }
            }
        }

        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_from_device(data, ptr);
        }

        if data.len() >= std::mem::size_of::<u32>() {
            self.detect_ffffffff_read(Some(unsafe { (data.as_ptr() as *const u32).read() }))?;
        }

        Ok(())
    }

    pub fn write_block_no_dma(&self, addr: u32, data: &[u8]) -> Result<(), PciError> {
        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address_mut(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_to_device(ptr, data);
        }

        Ok(())
    }

    pub fn read_block_no_dma(&self, addr: u32, data: &mut [u8]) -> Result<(), PciError> {
        unsafe {
            let ptr = match &self.pci_bar {
                Some(bar) => bar.register_address(addr),
                None => {
                    return Err(PciError::BarUnmapped);
                }
            };
            Self::memcpy_from_device(data, ptr);
        }

        if data.len() >= std::mem::size_of::<u32>() {
            self.detect_ffffffff_read(Some(unsafe { (data.as_ptr() as *const u32).read() }))?;
        }

        Ok(())
    }

    /// Map the PCI BARs for memory-mapped I/O
    fn map_bar(&mut self) -> Result<(), PciOpenError> {
        eprintln!("[DEBUG] map_bar() called for device {}", self.id);

        // Query mappings from the kernel driver
        let mut mappings = super::ioctl::QueryMappings::<8>::default();

        if let Err(errno) =
            unsafe { super::ioctl::query_mappings(self.device_fd.as_raw_fd(), &mut mappings) }
        {
            eprintln!("[DEBUG] query_mappings ioctl failed: {errno:?}");
            return Err(PciOpenError::IoctlError {
                name: "query_mappings".to_string(),
                id: self.id,
                source: errno,
            });
        }

        eprintln!(
            "[DEBUG] query_mappings returned {} mappings",
            mappings.input.output_mapping_count
        );

        let mut bar0_uc_mapping = None;
        let mut bar0_wc_mapping = None;
        let mut bar1_uc_mapping = None;
        let mut bar2_uc_mapping = None;

        // Process all mappings
        for i in 0..mappings.input.output_mapping_count as usize {
            let mapping = &mappings.output.mappings[i];
            eprintln!(
                "[DEBUG] Mapping {}: id={}, base=0x{:x}, size=0x{:x}",
                i, mapping.mapping_id, mapping.mapping_base, mapping.mapping_size
            );

            // Based on luwen reference, mapping IDs are:
            // id=0: Unused
            // id=1: Resource0Uc (BAR0 UC)
            // id=2: Resource0Wc (BAR0 WC)
            // id=3: Resource1Uc (BAR1 UC)
            // id=4: Resource1Wc (BAR1 WC)
            // id=5: Resource2Uc (BAR2 UC)
            // id=6: Resource2Wc (BAR2 WC)
            match mapping.mapping_id {
                0 => {}                                // Skip unused mapping
                1 => bar0_uc_mapping = Some(*mapping), // Resource0Uc
                2 => bar0_wc_mapping = Some(*mapping), // Resource0Wc
                3 => bar1_uc_mapping = Some(*mapping), // Resource1Uc
                5 => bar2_uc_mapping = Some(*mapping), // Resource2Uc
                _ => {}
            }
        }

        // Ensure we have at least BAR0 UC mapping
        let bar0_uc_mapping = bar0_uc_mapping.ok_or_else(|| {
            eprintln!("[DEBUG] No BAR0 UC mapping found");
            PciOpenError::BarMappingError {
                name: "bar0_uc_mapping".to_string(),
                id: self.id,
            }
        })?;

        eprintln!(
            "[DEBUG] Mapping BAR0 UC: offset=0x{:x}, size=0x{:x}",
            bar0_uc_mapping.mapping_base, bar0_uc_mapping.mapping_size
        );

        // Map the BAR0 UC region
        // Note: mapping_base of 0x0 is valid - it means mapping from the beginning of the device file
        eprintln!(
            "[DEBUG] About to mmap BAR0 UC with fd={}, offset=0x{:x}, size=0x{:x}",
            self.device_fd.as_raw_fd(),
            bar0_uc_mapping.mapping_base,
            bar0_uc_mapping.mapping_size
        );
        // Determine WC mapping size based on architecture
        let wc_mapping_size = if self.arch.is_blackhole() {
            super::kmdif::BH_BAR0_WC_MAPPING_SIZE
        } else {
            super::kmdif::GS_BAR0_WC_MAPPING_SIZE
        };

        // Map BAR0 WC first if available
        let mut bar0_wc_size = 0;
        let bar0_wc = if let Some(mapping) = bar0_wc_mapping {
            if mapping.mapping_id == 2 {
                // Resource0Wc
                bar0_wc_size = mapping.mapping_size.min(wc_mapping_size);
                eprintln!(
                    "[DEBUG] Mapping BAR0 WC: offset=0x{:x}, size=0x{:x}",
                    mapping.mapping_base, bar0_wc_size
                );
                let bar0_wc_map = unsafe {
                    memmap2::MmapOptions::default()
                        .len(bar0_wc_size as usize)
                        .offset(mapping.mapping_base)
                        .map_mut(self.device_fd.as_raw_fd())
                };
                match bar0_wc_map {
                    Ok(map) => Some(map),
                    Err(err) => {
                        eprintln!("[DEBUG] WARNING: Failed to map bar0_wc: {err}");
                        bar0_wc_size = 0;
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Adjust BAR0 UC mapping based on whether WC mapping exists
        let bar0_uc_size;
        let bar0_uc_offset;
        if bar0_wc.is_some() {
            // When WC mapping exists, UC mapping is reduced and offset
            bar0_uc_size = bar0_uc_mapping.mapping_size.saturating_sub(wc_mapping_size);
            bar0_uc_offset = wc_mapping_size;
        } else {
            // No WC mapping, map the entire BAR UC
            bar0_uc_size = bar0_uc_mapping.mapping_size;
            bar0_uc_offset = 0;
        }

        eprintln!(
            "[DEBUG] Mapping BAR0 UC: offset=0x{:x}, size=0x{:x}, adjusted_offset=0x{:x}",
            bar0_uc_mapping.mapping_base,
            bar0_uc_size,
            bar0_uc_mapping.mapping_base + bar0_uc_offset
        );

        let bar0_uc = unsafe {
            memmap2::MmapOptions::default()
                .len(bar0_uc_size as usize)
                .offset(bar0_uc_mapping.mapping_base + bar0_uc_offset)
                .map_mut(self.device_fd.as_raw_fd())
        }
        .map_err(|err| {
            eprintln!("[DEBUG] mmap failed for BAR0 UC: {err:?}");
            eprintln!("[DEBUG] errno: {:?}", err.raw_os_error());
            PciOpenError::BarMappingError {
                name: format!("bar0_uc mmap: {err}"),
                id: self.id,
            }
        })?;

        eprintln!("[DEBUG] BAR0 UC mapped successfully");

        // Map BAR1 UC if available (for Blackhole)
        let (bar1_uc, bar1_uc_size) = if let Some(mapping) = bar1_uc_mapping {
            if self.arch.is_blackhole() {
                eprintln!(
                    "[DEBUG] Mapping BAR1 UC for Blackhole: offset=0x{:x}, size=0x{:x}",
                    mapping.mapping_base, mapping.mapping_size
                );
                let mmap = unsafe {
                    memmap2::MmapOptions::default()
                        .len(mapping.mapping_size as usize)
                        .offset(mapping.mapping_base)
                        .map_mut(self.device_fd.as_raw_fd())
                }
                .map_err(|err| {
                    eprintln!("[DEBUG] mmap failed for BAR1 UC: {err:?}");
                    PciOpenError::BarMappingError {
                        name: format!("bar1_uc mmap: {err}"),
                        id: self.id,
                    }
                })?;
                (Some(mmap), mapping.mapping_size)
            } else {
                (None, 0)
            }
        } else {
            (None, 0)
        };

        // Handle system register mapping for Wormhole
        let (
            system_reg_mapping,
            system_reg_mapping_size,
            system_reg_start_offset,
            system_reg_offset_adjust,
        ) = if self.arch.is_wormhole() {
            if let Some(bar2_mapping) = bar2_uc_mapping {
                eprintln!(
                    "[DEBUG] Mapping system registers for Wormhole: offset=0x{:x}, size=0x{:x}",
                    bar2_mapping.mapping_base, bar2_mapping.mapping_size
                );
                let mmap = unsafe {
                    memmap2::MmapOptions::default()
                        .len(bar2_mapping.mapping_size as usize)
                        .offset(bar2_mapping.mapping_base)
                        .map_mut(self.device_fd.as_raw_fd())
                }
                .map_err(|err| {
                    eprintln!("[DEBUG] mmap failed for system registers: {err:?}");
                    PciOpenError::BarMappingError {
                        name: format!("system_reg mmap: {err}"),
                        id: self.id,
                    }
                })?;

                // Wormhole-specific offsets
                let start_offset = (512 - 16) * 1024 * 1024;
                let offset_adjust = (512 - 32) * 1024 * 1024;

                (
                    Some(mmap),
                    bar2_mapping.mapping_size as usize,
                    start_offset,
                    offset_adjust,
                )
            } else {
                (None, 0, 0, 0)
            }
        } else {
            (None, 0, 0, 0)
        };

        // Create the BarMapping structure
        self.pci_bar = Some(super::kmdif::BarMapping {
            bar_addr: read_bar0_base(&self.config_space),
            bar_size_bytes: bar0_uc_mapping.mapping_size,

            bar0_uc,
            bar0_uc_size,
            bar0_uc_offset,

            bar0_wc,
            bar0_wc_size,
            bar1_uc,
            bar1_uc_size,

            system_reg_mapping,
            system_reg_mapping_size,
            system_reg_start_offset,
            system_reg_offset_adjust,
        });

        eprintln!("[DEBUG] map_bar() completed successfully");
        Ok(())
    }

    pub fn allocate_transfer_buffers(&mut self) -> bool {
        // Try to allocate the transfer buffer first, if this fails then there is no point in
        // allocating the completion flag.
        if self.transfer_buffer.is_none() {
            self.transfer_buffer = self
                .allocate_dma_buffer_range(
                    kmdif::getpagesize().unwrap() as u32,
                    kmdif::MAX_DMA_BYTES,
                )
                .ok();
        }

        // If we didn't get the transfer buffer then there is no point in allocating the completion
        // flag
        if self.transfer_buffer.is_some() && self.completion_flag_buffer.is_none() {
            self.completion_flag_buffer = self
                .allocate_dma_buffer(std::mem::size_of::<u64>() as u32)
                .ok();
        }

        self.transfer_buffer.is_some() && self.completion_flag_buffer.is_some()
    }

    pub fn allocate_dma_buffer_range(
        &mut self,
        min_size: u32,
        max_size: u32,
    ) -> Result<DmaBuffer, PciError> {
        let page_size = kmdif::getpagesize().unwrap() as u32;

        let mut page_aligned_size = (max_size + page_size - 1) & !(page_size - 1);
        let min_aligned_page_size = (min_size + page_size - 1) & !(page_size - 1);

        loop {
            match allocate_dma_buffer(
                self.id,
                self.device_fd.as_raw_fd(),
                self.max_dma_buf_size_log2 as u32,
                self.next_dma_buf,
                page_aligned_size,
            ) {
                Ok(buf) => {
                    self.next_dma_buf += 1;
                    return Ok(buf);
                }
                Err(err) => {
                    if page_aligned_size <= min_aligned_page_size {
                        return Err(err);
                    }

                    page_aligned_size = (page_aligned_size / 2).max(min_aligned_page_size);
                }
            }
        }
    }

    pub fn allocate_dma_buffer(&mut self, size: u32) -> Result<DmaBuffer, PciError> {
        self.allocate_dma_buffer_range(size, size)
    }

    pub fn allocate_tlb(&self, size: u64) -> Result<TlbAllocation, PciError> {
        eprintln!("[DEBUG] allocate_tlb called with size: {size}");
        let mut data = ioctl::AllocateTlb {
            input: ioctl::AllocateTlbIn {
                size,
                ..Default::default()
            },
            ..Default::default()
        };
        eprintln!("[DEBUG] Calling allocate_tlb ioctl");
        let result =
            unsafe { ioctl::allocate_tlb(self.device_fd.as_raw_fd(), (&mut data) as *mut _) };
        eprintln!("[DEBUG] allocate_tlb ioctl returned: {result:?}");

        eprintln!(
            "[DEBUG] Attempting to mmap uc buffer with offset: {}, size: {}",
            data.output.mmap_offset_uc, size
        );
        let uc_mapping = unsafe {
            memmap2::MmapOptions::default()
                .len(size as usize)
                .offset(data.output.mmap_offset_uc)
                .map_mut(self.device_fd.as_raw_fd())
        }
        .map_err(|err| {
            eprintln!("[DEBUG] mmap failed: {err:?}");
            PciError::TlbAllocationError(format!("Failed to map uc buffer: {err:?}"))
        })?;
        eprintln!("[DEBUG] mmap succeeded");

        match result {
            Ok(rc) => match rc {
                0 => {
                    eprintln!("[DEBUG] TLB allocation successful, id: {}", data.output.id);
                    Ok(TlbAllocation {
                        id: data.output.id,
                        uc_mapping,
                        size,
                    })
                }
                errno => {
                    eprintln!("[DEBUG] TLB allocation failed with errno: {errno}");
                    Err(PciError::IoctlError(nix::errno::Errno::from_raw(errno)))
                }
            },
            Err(errno) => {
                eprintln!("[DEBUG] TLB allocation ioctl failed: {errno:?}");
                Err(PciError::IoctlError(errno))
            }
        }
    }

    pub fn noc_write(
        &mut self,
        _index: &PossibleTlbAllocation,
        _tlb: super::tlb::Tlb,
        _data: &[u8],
    ) -> Result<(), PciError> {
        // Simplified implementation - in production this would need full TLB handling
        Err(PciError::Generic(
            "noc_write not fully implemented".to_string(),
        ))
    }

    pub fn noc_read(
        &mut self,
        _index: &PossibleTlbAllocation,
        _tlb: super::tlb::Tlb,
        _data: &mut [u8],
    ) -> Result<(), PciError> {
        // Simplified implementation - in production this would need full TLB handling
        Err(PciError::Generic(
            "noc_read not fully implemented".to_string(),
        ))
    }

    pub fn scan() -> Vec<usize> {
        eprintln!("[DEBUG] PciDevice::scan() called");
        let output = std::fs::read_dir("/dev/tenstorrent");
        let output = match output {
            Ok(output) => output,
            Err(err) => {
                eprintln!("[DEBUG] When reading /dev/tenstorrent for a scan hit error: {err}");
                return Vec::new();
            }
        };

        let mut output = output
            .flatten()
            .filter_map(|f| {
                eprintln!("[DEBUG] Found entry: {:?}", f.path());
                if let Some(name) = f.file_name().to_str() {
                    eprintln!("[DEBUG] Entry name: '{name}'");
                    // The device files are just numeric IDs, not prefixed with "tenstorrent-"
                    if let Ok(id) = name.parse::<usize>() {
                        eprintln!("[DEBUG] Parsed as device ID: {id}");
                        return Some(id);
                    } else {
                        eprintln!("[DEBUG] Failed to parse '{name}' as number");
                    }
                }
                None
            })
            .collect::<Vec<_>>();

        output.sort_unstable();
        eprintln!("[DEBUG] Total devices found: {output:?}");
        output
    }
}

fn allocate_dma_buffer(
    device_id: usize,
    device_fd: RawFd,
    max_dma_buf_size_log2: u32,
    buffer_index: usize,
    size: u32,
) -> Result<DmaBuffer, PciError> {
    let mut allocate_dma_buf = ioctl::AllocateDmaBuffer::default();
    allocate_dma_buf.input.requested_size =
        (size.min(1 << max_dma_buf_size_log2)).max(kmdif::getpagesize().unwrap() as u32);
    allocate_dma_buf.input.buf_index = buffer_index as u8;

    if let Err(err) = unsafe { ioctl::allocate_dma_buffer(device_fd, &mut allocate_dma_buf) } {
        return Err(PciError::DmaAllocationFailed {
            id: device_id,
            size: allocate_dma_buf.input.requested_size,
            err,
        });
    }

    let map = unsafe {
        memmap2::MmapOptions::default()
            .len(allocate_dma_buf.output.size as usize)
            .offset(allocate_dma_buf.output.mapping_offset)
            .map_mut(device_fd)
    };
    let map = match map {
        Err(err) => {
            return Err(PciError::DmaBufferMappingFailed {
                id: device_id,
                source: err,
            })
        }
        Ok(map) => map,
    };

    let output = DmaBuffer {
        buffer: map,
        physical_address: allocate_dma_buf.output.physical_address,
        size: allocate_dma_buf.output.size as u64,
    };

    Ok(output)
}

impl PciDevice {
    /// Copy to a memory location mapped to the PciDevice from a buffer passed in by the host.
    /// Both dest and src may be unaligned.
    ///
    /// These the dest address is not bounds checked, passing in an unvalidated pointer may result
    /// in hangs, or system reboots.
    ///
    /// # Safety
    /// This function requires that dest is a value returned by the self.register_address
    /// function.
    pub unsafe fn memcpy_to_device(dest: *mut u8, src: &[u8]) {
        // Memcpy implementations on aarch64 systems seem to generate invalid code which does not
        // properly respect alignment requirements of the aarch64 "memmove" instruction.
        let align = if cfg!(target_arch = "aarch64") {
            4 * core::mem::align_of::<u32>()
        } else {
            core::mem::align_of::<u32>()
        };

        let mut offset = 0;
        while offset < src.len() {
            let bytes_left = src.len() - offset;

            let block_write_length = bytes_left & !(align - 1);

            let dest_misalign = ((dest as usize) + offset) % align;
            let src_misalign = ((src.as_ptr() as usize) + offset) % align;

            // Our device pcie controller requires that we write in a minimum of 4 byte chunks, and
            // that those chunks are aligned to 4 byte boundaries.
            if bytes_left < 4
                || dest_misalign != 0
                || src_misalign != 0
                || block_write_length < align
            {
                let addr = (dest as usize) + offset;
                let byte_offset = addr % core::mem::align_of::<u32>();

                let src_size_bytes = (core::mem::size_of::<u32>() - byte_offset).min(bytes_left);

                let mut src_data = 0u32;
                for i in (offset..(offset + src_size_bytes)).rev() {
                    src_data <<= 8;
                    src_data |= src[i] as u32;
                }

                let to_write = if byte_offset != 0 || src_size_bytes != 4 {
                    // cannot do an unaligned read
                    let dest_data =
                        ((addr & !(core::mem::align_of::<u32>() - 1)) as *mut u32).read();

                    let shift = byte_offset * 8;
                    let src_mask = ((1 << (src_size_bytes * 8)) - 1) << shift;

                    /*
                    println!(
                        "{dest_data:x} & {:x} = {:x}",
                        !src_mask,
                        dest_data & !src_mask
                    );

                    println!(
                        "({src_data:x} << {}) & {:x} = {:x}",
                        shift,
                        src_mask,
                        (src_data << shift) & src_mask
                    );
                    */

                    (dest_data & !src_mask) | ((src_data << shift) & src_mask)
                } else {
                    src_data
                };

                // println!("{to_write:x}");

                ((addr & !(core::mem::align_of::<u32>() - 1)) as *mut u32).write_volatile(to_write);

                offset += src_size_bytes;
            } else {
                // Everything is aligned!
                ((dest as usize + offset) as *mut u32).copy_from_nonoverlapping(
                    (src.as_ptr() as usize + offset) as *const u32,
                    block_write_length / core::mem::size_of::<u32>(),
                );
                offset += block_write_length;
            }
        }
    }

    /// Copy from a memory location mapped to the PciDevice to a buffer passed in by the host.
    /// Both dest and src may be unaligned.
    ///
    /// These the src address is not bounds checked, passing in an unvalidated pointer may result
    /// in hangs, or system reboots.
    ///
    /// # Safety
    /// This function requires that dest is a value returned by the self.register_address
    /// function.
    pub unsafe fn memcpy_from_device(dest: &mut [u8], src: *const u8) {
        let align = core::mem::align_of::<u32>();

        let mut offset = 0;
        while offset < dest.len() {
            let bytes_left = dest.len() - offset;

            let block_write_length = bytes_left & !(core::mem::align_of::<u32>() - 1);

            let dest_misalign = ((dest.as_ptr() as usize) + offset) % align;
            let src_misalign = ((src as usize) + offset) % align;

            // Our device pcie controller requires that we read in a minimum of 4 byte chunks, and
            // that those chunks are aligned to 4 byte boundaries.
            if bytes_left < 4
                || dest_misalign != 0
                || src_misalign != 0
                || block_write_length < align
            {
                let addr = (src as usize) + offset;
                let byte_offset = addr % core::mem::align_of::<u32>();
                let shift = byte_offset * 8;

                let src_data = ((addr & !(core::mem::align_of::<u32>() - 1)) as *mut u32).read();
                let read = src_data >> shift;

                let read_count = (core::mem::size_of::<u32>() - byte_offset).min(bytes_left);

                let read = read.to_le_bytes();
                dest[offset..(read_count + offset)].copy_from_slice(&read[..read_count]);

                offset += read_count
            } else {
                // Everything is aligned!
                ((dest.as_ptr() as usize + offset) as *mut u32).copy_from_nonoverlapping(
                    (src as usize + offset) as *const u32,
                    block_write_length / core::mem::size_of::<u32>(),
                );
                offset += block_write_length;
            }
        }
    }
}
