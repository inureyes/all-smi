// Copyright 2025 Lablup Inc. and Jeongkyu Shin
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! libtpuinfo FFI bindings for TPU metrics collection.
//!
//! This module provides Rust bindings to libtpuinfo, a Go library compiled as
//! a shared object that exposes TPU metrics through C function pointers.
//!
//! See: https://github.com/rdyro/libtpuinfo
//!
//! # Available Metrics
//!
//! - Device count and IDs
//! - Memory usage and total memory per device
//! - Duty cycle percentage (utilization)
//! - Process IDs using TPU devices

#![allow(unused)]

#[cfg(target_os = "linux")]
use libloading::{Library, Symbol};
#[cfg(target_os = "linux")]
use once_cell::sync::OnceCell;
#[cfg(target_os = "linux")]
use std::sync::Mutex;

/// TPU device metrics from libtpuinfo
#[derive(Debug, Clone, Default)]
pub struct TpuDeviceMetrics {
    /// Device ID
    pub device_id: i64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Total memory in bytes
    pub total_memory: u64,
    /// Duty cycle percentage (0-100)
    pub duty_cycle_pct: f64,
    /// Process ID using this device (0 if none)
    #[allow(dead_code)]
    pub pid: u64,
}

/// libtpuinfo library wrapper
#[cfg(target_os = "linux")]
struct LibTpuInfo {
    _library: Library,
    tpu_chip_count: unsafe extern "C" fn() -> i32,
    tpu_pids: unsafe extern "C" fn(*mut i64, i32) -> i32,
    tpu_metrics: unsafe extern "C" fn(i32, *mut i64, *mut i64, *mut i64, *mut f64, i32) -> i32,
}

#[cfg(target_os = "linux")]
unsafe impl Send for LibTpuInfo {}
#[cfg(target_os = "linux")]
unsafe impl Sync for LibTpuInfo {}

/// Global library instance (loaded once)
#[cfg(target_os = "linux")]
static LIBTPUINFO: OnceCell<Mutex<Option<LibTpuInfo>>> = OnceCell::new();

/// Search paths for libtpuinfo.so
#[cfg(target_os = "linux")]
const LIBTPUINFO_PATHS: &[&str] = &[
    "libtpuinfo.so",
    "/usr/local/lib/libtpuinfo.so",
    "/usr/lib/libtpuinfo.so",
    "/opt/libtpuinfo/libtpuinfo.so",
];

/// Load libtpuinfo library from known paths
#[cfg(target_os = "linux")]
fn load_libtpuinfo() -> Option<LibTpuInfo> {
    for path in LIBTPUINFO_PATHS {
        if let Ok(lib) = unsafe { Library::new(path) } {
            // Try to load all required symbols and convert to raw function pointers
            let tpu_chip_count = unsafe {
                let sym: Symbol<unsafe extern "C" fn() -> i32> =
                    match lib.get(b"tpu_chip_count\0") {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                *sym
            };

            let tpu_pids = unsafe {
                let sym: Symbol<unsafe extern "C" fn(*mut i64, i32) -> i32> =
                    match lib.get(b"tpu_pids\0") {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                *sym
            };

            let tpu_metrics = unsafe {
                let sym: Symbol<
                    unsafe extern "C" fn(i32, *mut i64, *mut i64, *mut i64, *mut f64, i32) -> i32,
                > = match lib.get(b"tpu_metrics\0") {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                *sym
            };

            return Some(LibTpuInfo {
                _library: lib,
                tpu_chip_count,
                tpu_pids,
                tpu_metrics,
            });
        }
    }
    None
}

/// Initialize libtpuinfo library (called once)
#[cfg(target_os = "linux")]
fn get_libtpuinfo() -> Option<&'static Mutex<Option<LibTpuInfo>>> {
    Some(LIBTPUINFO.get_or_init(|| Mutex::new(load_libtpuinfo())))
}

/// Check if libtpuinfo is available
#[cfg(target_os = "linux")]
pub fn is_libtpuinfo_available() -> bool {
    if let Some(mutex) = get_libtpuinfo() {
        if let Ok(guard) = mutex.lock() {
            return guard.is_some();
        }
    }
    false
}

#[cfg(not(target_os = "linux"))]
pub fn is_libtpuinfo_available() -> bool {
    false
}

/// Get the number of TPU chips
#[cfg(target_os = "linux")]
pub fn get_tpu_chip_count() -> Option<i32> {
    let mutex = get_libtpuinfo()?;
    let guard = mutex.lock().ok()?;
    let lib = guard.as_ref()?;

    let count = unsafe { (lib.tpu_chip_count)() };
    if count >= 0 {
        Some(count)
    } else {
        None
    }
}

#[cfg(not(target_os = "linux"))]
pub fn get_tpu_chip_count() -> Option<i32> {
    None
}

/// Get TPU device metrics for all devices
#[cfg(target_os = "linux")]
pub fn get_tpu_metrics() -> Option<Vec<TpuDeviceMetrics>> {
    let chip_count = get_tpu_chip_count()?;
    if chip_count == 0 {
        return Some(Vec::new());
    }

    let n = chip_count as usize;
    let mutex = get_libtpuinfo()?;
    let guard = mutex.lock().ok()?;
    let lib = guard.as_ref()?;

    // Allocate buffers for metrics
    let mut device_ids = vec![0i64; n];
    let mut memory_usage = vec![0i64; n];
    let mut total_memory = vec![0i64; n];
    let mut duty_cycle = vec![0.0f64; n];
    let mut pids = vec![0i64; n];

    // Get PIDs first
    let pid_result = unsafe { (lib.tpu_pids)(pids.as_mut_ptr(), chip_count) };
    if pid_result < 0 {
        // PIDs may not be available, continue without them
        pids = vec![0i64; n];
    }

    // Get metrics (port <= 0 uses default 8431)
    let result = unsafe {
        (lib.tpu_metrics)(
            0, // Use default gRPC port
            device_ids.as_mut_ptr(),
            memory_usage.as_mut_ptr(),
            total_memory.as_mut_ptr(),
            duty_cycle.as_mut_ptr(),
            chip_count,
        )
    };

    if result < 0 {
        return None;
    }

    // Build result vector
    let metrics: Vec<TpuDeviceMetrics> = (0..n)
        .map(|i| TpuDeviceMetrics {
            device_id: device_ids[i],
            memory_usage: memory_usage[i].max(0) as u64,
            total_memory: total_memory[i].max(0) as u64,
            duty_cycle_pct: duty_cycle[i].clamp(0.0, 100.0),
            pid: pids[i].max(0) as u64,
        })
        .collect();

    Some(metrics)
}

#[cfg(not(target_os = "linux"))]
pub fn get_tpu_metrics() -> Option<Vec<TpuDeviceMetrics>> {
    None
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn test_libtpuinfo_availability() {
        // This test just checks that the function doesn't panic
        let available = is_libtpuinfo_available();
        println!("libtpuinfo available: {}", available);
    }

    #[test]
    fn test_chip_count() {
        if is_libtpuinfo_available() {
            let count = get_tpu_chip_count();
            println!("TPU chip count: {:?}", count);
            assert!(count.is_some());
        }
    }

    #[test]
    fn test_metrics() {
        if is_libtpuinfo_available() {
            let metrics = get_tpu_metrics();
            println!("TPU metrics: {:?}", metrics);
        }
    }
}
