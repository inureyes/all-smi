// SPDX-FileCopyrightText: © 2025 All-SMI Contributors
// SPDX-License-Identifier: Apache-2.0

//! PCIe BAR (Base Address Register) mapping implementation for Tenstorrent devices
//! Based on TT-REPORT.md specifications

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::ptr;

use super::error::PlatformError;
use super::ttkmd::ioctl::ioctl_query_mappings;
use super::ttkmd::pci::QueryMappings;

// Import libc for system calls
use libc::{mmap, munmap, sysconf, MAP_FAILED, MAP_SHARED, PROT_READ, PROT_WRITE, _SC_PAGESIZE};

/// Represents a mapped BAR region
pub struct BarMapping {
    pub base_addr: *mut u8,
    pub size: u64,
    pub mapping_id: u32,
}

// SAFETY: BarMapping only contains a pointer to mapped memory which is safe to send between threads
// as long as proper synchronization is used (which is handled by Mutex in the global cache)
unsafe impl Send for BarMapping {}
unsafe impl Sync for BarMapping {}

impl Drop for BarMapping {
    fn drop(&mut self) {
        unsafe {
            munmap(self.base_addr as *mut std::ffi::c_void, self.size as usize);
        }
    }
}

/// Manages PCIe BAR mappings for a device
pub struct BarManager {
    device_file: std::fs::File,
    mappings: HashMap<u32, BarMapping>,
}

impl BarManager {
    /// Create a new BAR manager for a device
    pub fn new(device_id: usize) -> Result<Self, PlatformError> {
        let path = format!("/dev/tenstorrent/{device_id}");
        eprintln!("[DEBUG] Opening device file: {path}");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| PlatformError::IoError(format!("Failed to open device {path}: {e}")))?;

        Ok(Self {
            device_file: file,
            mappings: HashMap::new(),
        })
    }

    /// Query and map all available BARs
    pub fn map_bars(&mut self) -> Result<(), PlatformError> {
        // Query available mappings
        let mut query = QueryMappings::default();
        unsafe {
            ioctl_query_mappings(self.device_file.as_raw_fd(), &mut query)
                .map_err(|e| PlatformError::IoError(format!("Failed to query mappings: {e}")))?;
        }

        eprintln!("[DEBUG] Found {} BAR mappings", query.mapping_count);

        // Map each BAR
        for i in 0..query.mapping_count as usize {
            let mapping = &query.mappings[i];
            if mapping.mapping_id == 0 {
                continue; // Unused mapping
            }

            // Convert mapping_id to enum for clarity
            let mapping_type = match mapping.mapping_id {
                1 => "Resource0 UC (BAR0 uncached)",
                2 => "Resource0 WC (BAR0 write-combined)",
                3 => "Resource1 UC (BAR1 uncached)",
                4 => "Resource1 WC (BAR1 write-combined)",
                5 => "Resource2 UC (BAR2/4 uncached)",
                6 => "Resource2 WC (BAR2/4 write-combined)",
                _ => "Unknown",
            };

            eprintln!(
                "[DEBUG] Mapping ID {} ({}): offset=0x{:x}, size=0x{:x}",
                mapping.mapping_id, mapping_type, mapping.base_address, mapping.mapping_size
            );

            // Check if size is valid
            if mapping.mapping_size == 0 {
                eprintln!(
                    "[WARN] Skipping mapping {} with zero size",
                    mapping.mapping_id
                );
                continue;
            }

            // Memory map the BAR directly using the base_address from kernel driver
            // The kernel driver provides the appropriate offset for mmap
            let mmap_offset = mapping.base_address as i64;

            // Ensure offset is page-aligned
            let page_size = unsafe { sysconf(_SC_PAGESIZE) } as i64;
            if mmap_offset % page_size != 0 {
                eprintln!(
                    "[WARN] Mapping offset 0x{mmap_offset:x} is not page-aligned (page_size=0x{page_size:x})"
                );
            }

            eprintln!(
                "[DEBUG] Calling mmap: fd={}, offset=0x{:x}, size=0x{:x} ({}MB)",
                self.device_file.as_raw_fd(),
                mmap_offset,
                mapping.mapping_size,
                mapping.mapping_size / (1024 * 1024)
            );

            // Validate file descriptor
            let fd = self.device_file.as_raw_fd();
            if fd < 0 {
                return Err(PlatformError::IoError(format!(
                    "Invalid file descriptor: {fd}"
                )));
            }

            let ptr = unsafe {
                mmap(
                    ptr::null_mut(),
                    mapping.mapping_size as usize,
                    PROT_READ | PROT_WRITE,
                    MAP_SHARED,
                    fd,
                    mmap_offset,
                )
            };

            if ptr == MAP_FAILED {
                let err = std::io::Error::last_os_error();
                return Err(PlatformError::IoError(format!(
                    "Failed to mmap {} (id={}) at offset 0x{:x}, size 0x{:x}: {}",
                    mapping_type, mapping.mapping_id, mmap_offset, mapping.mapping_size, err
                )));
            }

            self.mappings.insert(
                mapping.mapping_id,
                BarMapping {
                    base_addr: ptr as *mut u8,
                    size: mapping.mapping_size,
                    mapping_id: mapping.mapping_id,
                },
            );

            eprintln!("[DEBUG] Successfully mapped {mapping_type} at {ptr:p}");
        }

        Ok(())
    }

    /// Read a 32-bit value from a mapped address
    pub fn read32(&self, bar_id: u32, offset: u64) -> Result<u32, PlatformError> {
        let mapping = self
            .mappings
            .get(&bar_id)
            .ok_or_else(|| PlatformError::InvalidParameter(format!("BAR {bar_id} not mapped")))?;

        if offset + 4 > mapping.size {
            return Err(PlatformError::InvalidParameter(format!(
                "Offset 0x{:x} out of bounds for BAR {} (size=0x{:x})",
                offset, bar_id, mapping.size
            )));
        }

        unsafe {
            let addr = mapping.base_addr.add(offset as usize) as *const u32;
            Ok(addr.read_volatile())
        }
    }

    /// Write a 32-bit value to a mapped address
    pub fn write32(&self, bar_id: u32, offset: u64, value: u32) -> Result<(), PlatformError> {
        let mapping = self
            .mappings
            .get(&bar_id)
            .ok_or_else(|| PlatformError::InvalidParameter(format!("BAR {bar_id} not mapped")))?;

        if offset + 4 > mapping.size {
            return Err(PlatformError::InvalidParameter(format!(
                "Offset 0x{:x} out of bounds for BAR {} (size=0x{:x})",
                offset, bar_id, mapping.size
            )));
        }

        unsafe {
            let addr = mapping.base_addr.add(offset as usize) as *mut u32;
            addr.write_volatile(value);
        }

        Ok(())
    }

    /// Get the base address of a mapped BAR
    pub fn get_bar_base(&self, bar_id: u32) -> Option<*mut u8> {
        self.mappings.get(&bar_id).map(|m| m.base_addr)
    }
}
