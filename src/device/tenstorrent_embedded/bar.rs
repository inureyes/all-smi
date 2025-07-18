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

/// Memory protection flags for mmap
const PROT_READ: i32 = 0x1;
const PROT_WRITE: i32 = 0x2;

/// Mapping flags for mmap
const MAP_SHARED: i32 = 0x1;

/// Size of each mapping region (256MB)
const MAPPING_REGION_SIZE: i64 = 1 << 28;

// External mmap function binding
extern "C" {
    fn mmap(
        addr: *mut std::ffi::c_void,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut std::ffi::c_void;

    fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
}

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

            eprintln!(
                "[DEBUG] Mapping BAR {}: addr=0x{:x}, size=0x{:x}",
                mapping.mapping_id, mapping.base_address, mapping.mapping_size
            );

            // Memory map the BAR directly - no TLB allocation needed for BAR mapping
            // The offset is based on the mapping_id
            let mmap_offset = mapping.mapping_id as i64 * MAPPING_REGION_SIZE;
            let ptr = unsafe {
                mmap(
                    ptr::null_mut(),
                    mapping.mapping_size as usize,
                    PROT_READ | PROT_WRITE,
                    MAP_SHARED,
                    self.device_file.as_raw_fd(),
                    mmap_offset,
                )
            };

            if ptr.is_null() || ptr as isize == -1 {
                return Err(PlatformError::IoError(format!(
                    "Failed to mmap BAR {}",
                    mapping.mapping_id
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

            eprintln!(
                "[DEBUG] Successfully mapped BAR {} at {:p}",
                mapping.mapping_id, ptr
            );
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
