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
use std::ffi::c_void;
#[cfg(target_os = "linux")]
use std::sync::Mutex;

#[cfg(target_os = "linux")]
const LIBTPU_PATHS: &[&str] = &[
    "libtpu.so",
    "/usr/lib/libtpu.so",
    "/usr/local/lib/libtpu.so",
    "/opt/google/tpu/libtpu.so",
];

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PjrtTpuMetrics {
    pub device_id: i32,
    pub chip_id: i32,
    pub global_device_id: i32,
    pub process_index: i32,
    // Add memory/duty cycle if we can reverse engineer the struct
}

#[cfg(target_os = "linux")]
struct LibTpu {
    _library: Library,
    // We only map the most stable initialization functions for now to verify presence
    // Full metrics extraction requires matching the exact PJRT_Api struct layout
    // which varies by version.
    #[allow(dead_code)]
    pjrt_api_ptr: *const c_void, 
}

#[cfg(target_os = "linux")]
unsafe impl Send for LibTpu {}
#[cfg(target_os = "linux")]
unsafe impl Sync for LibTpu {}

#[cfg(target_os = "linux")]
static LIBTPU: OnceCell<Mutex<Option<LibTpu>>> = OnceCell::new();

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
    // Common pattern: ~/.local/lib/python3.*/site-packages/libtpu/libtpu.so
    if let Some(home) = std::env::var_os("HOME") {
        let local_lib = std::path::Path::new(&home).join(".local/lib");
        if let Some(lib) = scan_python_dirs_for_libtpu(&local_lib) {
            return Some(lib);
        }
    }

    // 2. Try system python paths
    // Common pattern: /usr/local/lib/python3.*/dist-packages/libtpu/libtpu.so
    if let Some(lib) = scan_python_dirs_for_libtpu(std::path::Path::new("/usr/local/lib")) {
        return Some(lib);
    }
    
    // Common pattern: /usr/lib/python3.*/site-packages/libtpu/libtpu.so
    if let Some(lib) = scan_python_dirs_for_libtpu(std::path::Path::new("/usr/lib")) {
        return Some(lib);
    }

    // 3. Try standard system paths (Fallback)
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
            // Look for python3.x directories
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("python") {
                    // Check site-packages and dist-packages
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
    if let Ok(lib) = Library::new(path) {
        // Try to find the PJRT entry point
        let get_api_sym: Option<Symbol<unsafe extern "C" fn() -> *const c_void>> = 
            lib.get(b"GetPjrtApi\0").ok()
            .or_else(|| lib.get(b"PJRT_GetApi\0").ok());

        if let Some(get_api) = get_api_sym {
            let api = get_api();
            return Some(LibTpu {
                _library: lib,
                pjrt_api_ptr: api,
            });
        }
    }
    None
}

// Since we cannot safely call complex PJRT functions without the exact C struct definitions
// (which change often), we primarily use this module to CONFIRM libtpu presence
// and theoretically we could extend this to call simple C-int return functions.

#[cfg(not(target_os = "linux"))]
pub fn is_libtpu_available() -> bool {
    false
}
