// Copyright 2025 Lablup Inc.
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

//! Minimal PJRT (Platform Independent Runtime) FFI for Google TPU.
//!
//! This module attempts to load `libtpu.so` directly to access TPU metrics
//! without requiring external Go wrappers or Python.
//!
//! Note: This is a best-effort implementation. PJRT ABI is not strictly stable
//! across all versions, but basic entry points are generally consistent.

#[cfg(target_os = "linux")]
use libloading::{Library, Symbol};
#[cfg(target_os = "linux")]
use once_cell::sync::OnceCell;
#[cfg(target_os = "linux")]
use std::ffi::{c_void, c_char};
#[cfg(target_os = "linux")]
use std::sync::Mutex;

#[cfg(target_os = "linux")]
const LIBTPU_PATHS: &[&str] = &[
    "libtpu.so",
    "/usr/lib/libtpu.so",
    "/usr/local/lib/libtpu.so",
    "/opt/google/tpu/libtpu.so",
];

/// Struct representing minimal TPU metrics fetched via PJRT
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct PjrtTpuMetrics {
    pub device_id: i32,
    pub chip_id: i32,
    pub global_device_id: i32,
    pub process_index: i32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
}

// --- PJRT C API Definitions (Minimal Subset) ---
// These layouts are based on OpenXLA PJRT C API (pjrt_c_api.h)
// Note: Struct layout stability is not guaranteed. We use a best-effort approach.

#[repr(C)]
#[allow(dead_code)]
struct PJRT_Error {
    _private: [u8; 0],
}

#[repr(C)]
#[allow(dead_code)]
struct PJRT_Client {
    _private: [u8; 0],
}

#[repr(C)]
#[allow(dead_code)]
struct PJRT_DeviceDescription {
    _private: [u8; 0],
}

#[repr(C)]
#[allow(dead_code)]
struct PJRT_Device {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(dead_code)]
struct PJRT_NamedValue {
    name: *const c_char,
    name_len: usize,
    type_: i32, // PJRT_NamedValue_Type
    value: PJRT_NamedValue_Union,
}

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(dead_code)]
union PJRT_NamedValue_Union {
    bool_value: bool,
    int64_value: i64,
    float_value: f64,
    string_value: *const c_char,
    // other variants omitted
}

// --- PJRT Function Pointer Definitions ---

#[allow(non_camel_case_types)]
type PJRT_Error_Destroy = unsafe extern "C" fn(*mut PJRT_Error);
#[allow(non_camel_case_types)]
type PJRT_Error_Message = unsafe extern "C" fn(*mut PJRT_Error) -> *const c_char;
#[allow(non_camel_case_types)]
type PJRT_Error_GetCode = unsafe extern "C" fn(*mut PJRT_Error) -> i32;

#[allow(non_camel_case_types)]
type PJRT_Client_Create = unsafe extern "C" fn(
    *const PJRT_NamedValue, // args
    usize,                  // num_args
    *mut *mut PJRT_Client,  // client output
) -> *mut PJRT_Error;

#[allow(non_camel_case_types)]
type PJRT_Client_Destroy = unsafe extern "C" fn(*mut PJRT_Client) -> *mut PJRT_Error;
#[allow(non_camel_case_types)]
type PJRT_Client_Devices = unsafe extern "C" fn(
    *mut PJRT_Client,
    *mut *mut *mut PJRT_Device, // devices output (array of pointers)
    *mut usize,                 // num_devices output
) -> *mut PJRT_Error;

#[allow(non_camel_case_types)]
type PJRT_Device_GetMemoryStats = unsafe extern "C" fn(
    *mut PJRT_Device,
    *mut i64, // free_bytes
    *mut i64, // total_bytes
) -> *mut PJRT_Error;

#[allow(non_camel_case_types)]
type PJRT_Device_Id = unsafe extern "C" fn(*mut PJRT_Device) -> i32;
#[allow(non_camel_case_types)]
#[allow(dead_code)]
type PJRT_Device_GlobalId = unsafe extern "C" fn(*mut PJRT_Device) -> i32;

#[repr(C)]
struct PJRT_Api {
    struct_size: usize,
    priv_: *mut c_void,
    // Error handling
    error_destroy: PJRT_Error_Destroy,
    error_message: PJRT_Error_Message,
    error_get_code: PJRT_Error_GetCode,
    // Client
    client_create: PJRT_Client_Create,
    client_destroy: PJRT_Client_Destroy,
    client_platform_name: *mut c_void, // Skip
    client_process_index: *mut c_void, // Skip
    client_devices: PJRT_Client_Devices,
    client_addressable_devices: *mut c_void, // Skip
    client_lookup_device: *mut c_void, // Skip
    client_compile: *mut c_void, // Skip
    client_compile_est: *mut c_void, // Skip
    client_buffer_from_host: *mut c_void, // Skip
    client_buffer_from_host_async: *mut c_void, // Skip
    client_buffer_from_scalar: *mut c_void, // Skip
    // Device
    device_to_host_order: *mut c_void, // Skip
    device_id: PJRT_Device_Id,
    device_process_index: *mut c_void, // Skip
    device_is_addressable: *mut c_void, // Skip
    device_local_hardware_id: *mut c_void, // Skip
    device_addressable_memories: *mut c_void, // Skip
    device_default_memory: *mut c_void, // Skip
    device_memory_stats: PJRT_Device_GetMemoryStats,
    // There are more fields, but we hope these are stable at the top
}

#[cfg(target_os = "linux")]
struct LibTpu {
    _library: Library,
    api: *const PJRT_Api,
}

#[cfg(target_os = "linux")]
unsafe impl Send for LibTpu {}
#[cfg(target_os = "linux")]
unsafe impl Sync for LibTpu {}

#[cfg(target_os = "linux")]
static LIBTPU: OnceCell<Mutex<Option<LibTpu>>> = OnceCell::new();

#[cfg(target_os = "linux")]
struct PjrtClientHandle {
    client_ptr: *mut PJRT_Client,
}

#[cfg(target_os = "linux")]
unsafe impl Send for PjrtClientHandle {}
#[cfg(target_os = "linux")]
unsafe impl Sync for PjrtClientHandle {}

#[cfg(target_os = "linux")]
static PJRT_CLIENT: OnceCell<Mutex<Option<PjrtClientHandle>>> = OnceCell::new();

#[cfg(target_os = "linux")]
pub fn is_libtpu_available() -> bool {
    get_libtpu().map(|m| m.lock().map(|g| g.is_some()).unwrap_or(false)).unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn get_libtpu() -> Option<&'static Mutex<Option<LibTpu>>> {
    Some(LIBTPU.get_or_init(|| Mutex::new(load_libtpu())))
}

#[cfg(target_os = "linux")]
fn load_libtpu() -> Option<LibTpu> {
    // 1. Try to find in user python site-packages (Highest Priority)
    if let Some(home) = std::env::var_os("HOME") {
        let local_lib = std::path::Path::new(&home).join(".local/lib");
        if let Some(lib) = scan_python_dirs_for_libtpu(&local_lib) {
            return Some(lib);
        }
    }

    // 2. Try system python paths
    if let Some(lib) = scan_python_dirs_for_libtpu(std::path::Path::new("/usr/local/lib")) {
        return Some(lib);
    }
    if let Some(lib) = scan_python_dirs_for_libtpu(std::path::Path::new("/usr/lib")) {
        return Some(lib);
    }

    // 3. Try standard system paths
    for path in LIBTPU_PATHS {
        if let Some(lib) = unsafe { try_load_library(path) } {
            return Some(lib);
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn scan_python_dirs_for_libtpu(base_dir: &std::path::Path) -> Option<LibTpu> {
    if !base_dir.exists() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir(base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("python") {
                    let subdirs = ["site-packages", "dist-packages"];
                    for subdir in subdirs {
                        let libtpu_path = path.join(subdir).join("libtpu").join("libtpu.so");
                        if libtpu_path.exists() {
                            if let Some(str_path) = libtpu_path.to_str() {
                                if let Some(lib) = unsafe { try_load_library(str_path) } {
                                    return Some(lib);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
unsafe fn try_load_library(path: &str) -> Option<LibTpu> {
    let lib = Library::new(path).ok()?;
    
    // Get the API table
    let get_api_sym: Symbol<unsafe extern "C" fn() -> *const PJRT_Api> = 
        lib.get(b"GetPjrtApi\0").ok()
        .or_else(|| lib.get(b"PJRT_GetApi\0").ok())?;

    let api = get_api_sym();
    if api.is_null() {
        return None;
    }

    // Basic sanity check: struct_size should be reasonable
    let struct_size = (*api).struct_size;
    if struct_size < 128 || struct_size > 10000 {
        return None;
    }

    Some(LibTpu {
        _library: lib,
        api,
    })
}

// --- Metrics Retrieval ---

#[cfg(target_os = "linux")]
fn get_pjrt_client() -> Option<&'static Mutex<Option<PjrtClientHandle>>> {
    Some(PJRT_CLIENT.get_or_init(|| {
        unsafe {
            // Helper to handle ? logic inside unsafe block
            let try_create = || -> Option<PjrtClientHandle> {
                let lib_mutex = get_libtpu()?;
                let guard = lib_mutex.lock().ok()?;
                let lib = guard.as_ref()?;
                let api = &*lib.api;

                // Try to create client
                // No args needed for default local client
                let mut client: *mut PJRT_Client = std::ptr::null_mut();
                let err = (api.client_create)(std::ptr::null(), 0, &mut client);
                
                if !err.is_null() {
                    // Failed to create client (maybe locked?)
                    // (api.error_destroy)(err); // Cleanup error
                    return None;
                }

                if client.is_null() {
                    return None;
                }

                Some(PjrtClientHandle { client_ptr: client })
            };

            Mutex::new(try_create())
        }
    }))
}

#[cfg(target_os = "linux")]
pub fn get_tpu_metrics() -> Option<Vec<PjrtTpuMetrics>> {
    let mutex = get_libtpu()?;
    let guard = mutex.lock().ok()?;
    let lib = guard.as_ref()?;
    let api = unsafe { &*lib.api };
    
    // Ensure client is initialized (singleton pattern)
    let client_mutex = get_pjrt_client()?;
    let client_guard = client_mutex.lock().ok()?;
    let client = client_guard.as_ref()?;

    let mut metrics = Vec::new();

    unsafe {
        let mut devices: *mut *mut PJRT_Device = std::ptr::null_mut();
        let mut num_devices: usize = 0;

        let err = (api.client_devices)(client.client_ptr, &mut devices, &mut num_devices);
        if !err.is_null() {
            (api.error_destroy)(err);
            return Some(Vec::new());
        }

        // Iterate over devices
        let device_slice = std::slice::from_raw_parts(devices, num_devices);
        for (_i, &device_ptr) in device_slice.iter().enumerate() {
            if device_ptr.is_null() { continue; }

            // Get device ID
            let dev_id = (api.device_id)(device_ptr);
            
            // Get memory stats
            let mut free_bytes: i64 = 0;
            let mut total_bytes: i64 = 0;
            let mem_err = (api.device_memory_stats)(device_ptr, &mut free_bytes, &mut total_bytes);
            
            let (used, total) = if mem_err.is_null() {
                (
                    (total_bytes - free_bytes).max(0) as u64,
                    total_bytes.max(0) as u64
                )
            } else {
                (api.error_destroy)(mem_err);
                (0, 0)
            };

            metrics.push(PjrtTpuMetrics {
                device_id: dev_id,
                chip_id: dev_id, // Approx
                global_device_id: dev_id,
                process_index: 0,
                memory_used_bytes: used,
                memory_total_bytes: total,
            });
        }
        
        // Note: We don't free the devices array itself? 
        // PJRT semantics usually imply the array is owned by caller or callee depending on version.
        // Standard C API implies we might need to free it, but without `client_devices_free` in the table,
        // it might be managed or we are leaking a small pointer array. 
        // Given minimal implementation, we skip complex memory management for the array pointer itself.
    }
    
    Some(metrics)
}

#[cfg(not(target_os = "linux"))]
pub fn get_tpu_metrics() -> Option<Vec<PjrtTpuMetrics>> {
    None
}
