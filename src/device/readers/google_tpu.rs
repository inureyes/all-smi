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

//! Google TPU (Tensor Processing Unit) reader module.
//!
//! This module provides support for monitoring Google Cloud TPU accelerators.
//! TPUs are custom-designed ASICs developed by Google for accelerating machine
//! learning workloads.
//!
//! # Platform Detection
//!
//! TPU devices are detected by:
//! - Checking for `/dev/accel*` device files
//! - Checking for libtpu library availability
//! - Verifying TPU device presence via sysfs
//!
//! # Supported TPU Generations
//!
//! | Generation | Codename | HBM | Notes |
//! |------------|----------|-----|-------|
//! | TPU v2 | - | 8 GB | Legacy |
//! | TPU v3 | - | 16 GB | Legacy |
//! | TPU v4 | - | 32 GB | Production |
//! | TPU v5e | - | 16 GB | Cost-optimized |
//! | TPU v5p | - | 95 GB | High performance |
//! | TPU v6 | Trillium | 32 GB | 4.7x v5e performance |
//! | TPU v7 | Ironwood | 192 GB | Latest generation, HBM3e |

#[cfg(target_os = "linux")]
use crate::device::common::execute_command_default;
#[cfg(target_os = "linux")]
use crate::device::readers::common_cache::{DetailBuilder, DeviceStaticInfo};
use crate::device::types::{GpuInfo, ProcessInfo};
use crate::device::GpuReader;
#[cfg(target_os = "linux")]
use crate::utils::get_hostname;
#[cfg(target_os = "linux")]
use chrono::Local;
#[cfg(target_os = "linux")]
use once_cell::sync::Lazy;
#[cfg(target_os = "linux")]
use serde::Deserialize;
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex, OnceLock};

/// TPU generation enumeration with specifications
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg(target_os = "linux")]
pub enum TpuGeneration {
    V2,
    V3,
    V4,
    V5e,
    V5p,
    V6Trillium,
    V7Ironwood,
    Unknown,
}

#[cfg(target_os = "linux")]
impl TpuGeneration {
    /// Get HBM memory size in bytes for each TPU generation
    pub fn hbm_size_bytes(&self) -> u64 {
        match self {
            TpuGeneration::V2 => 8 * 1024 * 1024 * 1024,   // 8 GB
            TpuGeneration::V3 => 16 * 1024 * 1024 * 1024,  // 16 GB
            TpuGeneration::V4 => 32 * 1024 * 1024 * 1024,  // 32 GB
            TpuGeneration::V5e => 16 * 1024 * 1024 * 1024, // 16 GB
            TpuGeneration::V5p => 95 * 1024 * 1024 * 1024, // 95 GB
            TpuGeneration::V6Trillium => 32 * 1024 * 1024 * 1024, // 32 GB
            TpuGeneration::V7Ironwood => 192 * 1024 * 1024 * 1024, // 192 GB HBM3e
            TpuGeneration::Unknown => 16 * 1024 * 1024 * 1024, // Default 16 GB
        }
    }

    /// Get TensorCore count for each TPU generation
    pub fn tensor_cores(&self) -> u32 {
        match self {
            TpuGeneration::V2 => 2,
            TpuGeneration::V3 => 2,
            TpuGeneration::V4 => 2,
            TpuGeneration::V5e => 1,
            TpuGeneration::V5p => 2,
            TpuGeneration::V6Trillium => 2,
            TpuGeneration::V7Ironwood => 2, // Estimated based on architecture
            TpuGeneration::Unknown => 1,
        }
    }

    /// Get human-readable name for the TPU generation
    pub fn display_name(&self) -> &'static str {
        match self {
            TpuGeneration::V2 => "Google TPU v2",
            TpuGeneration::V3 => "Google TPU v3",
            TpuGeneration::V4 => "Google TPU v4",
            TpuGeneration::V5e => "Google TPU v5e",
            TpuGeneration::V5p => "Google TPU v5p",
            TpuGeneration::V6Trillium => "Google TPU v6 Trillium",
            TpuGeneration::V7Ironwood => "Google TPU v7 Ironwood 192GB HBM3e",
            TpuGeneration::Unknown => "Google TPU",
        }
    }

    /// Get memory type string for the TPU generation
    pub fn memory_type(&self) -> &'static str {
        match self {
            TpuGeneration::V7Ironwood => "HBM3e",
            TpuGeneration::V5p | TpuGeneration::V6Trillium => "HBM2e",
            _ => "HBM2",
        }
    }

    /// Parse TPU generation from chip version string
    pub fn from_chip_version(version: &str) -> Self {
        let version_lower = version.to_lowercase();
        if version_lower.contains("v7") || version_lower.contains("ironwood") {
            TpuGeneration::V7Ironwood
        } else if version_lower.contains("v6") || version_lower.contains("trillium") {
            TpuGeneration::V6Trillium
        } else if version_lower.contains("v5p") {
            TpuGeneration::V5p
        } else if version_lower.contains("v5e") {
            TpuGeneration::V5e
        } else if version_lower.contains("v4") {
            TpuGeneration::V4
        } else if version_lower.contains("v3") {
            TpuGeneration::V3
        } else if version_lower.contains("v2") {
            TpuGeneration::V2
        } else {
            TpuGeneration::Unknown
        }
    }
}

/// JSON structure for TPU device information from Python/CLI output
#[derive(Debug, Deserialize)]
#[cfg(target_os = "linux")]
struct TpuDeviceInfo {
    /// Device index (0, 1, 2, ...)
    index: u32,
    /// Chip version (e.g., "v4", "v5e", "v5p", "v6", "v7")
    #[serde(default)]
    chip_version: String,
    /// Device UUID
    #[serde(default)]
    uuid: String,
    /// Core count per chip
    #[serde(default)]
    core_count: u32,
    /// Current utilization percentage (0-100)
    #[serde(default)]
    utilization: f64,
    /// HBM memory used in bytes
    #[serde(default)]
    memory_used: u64,
    /// HBM memory total in bytes
    #[serde(default)]
    memory_total: u64,
    /// Current temperature in Celsius
    #[serde(default)]
    temperature: u32,
    /// Power consumption in Watts
    #[serde(default)]
    power_draw: f64,
    /// Maximum power limit in Watts
    #[serde(default)]
    power_max: f64,
    /// TPU runtime version
    #[serde(default)]
    tpu_runtime_version: String,
    /// Accelerator type string
    #[serde(default)]
    accelerator_type: String,
}

/// JSON structure for TPU process information
#[derive(Debug, Deserialize)]
#[cfg(target_os = "linux")]
struct TpuProcessInfo {
    /// Device index
    device_index: u32,
    /// Process ID
    pid: u32,
    /// Command line
    #[serde(default)]
    command: String,
    /// Memory used by the process
    #[serde(default)]
    memory_used: u64,
}

/// Cache for TPU Python script availability
#[cfg(target_os = "linux")]
static TPU_SCRIPT_AVAILABLE: Lazy<Arc<Mutex<Option<bool>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

/// Google TPU Reader implementation
pub struct GoogleTpuReader {
    /// Cached static device information per UUID
    #[cfg(target_os = "linux")]
    device_static_info: OnceLock<HashMap<String, DeviceStaticInfo>>,
}

impl Default for GoogleTpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleTpuReader {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            device_static_info: OnceLock::new(),
        }
    }

    /// Initialize static device cache on first access
    #[cfg(target_os = "linux")]
    fn ensure_static_cache_initialized(&self, devices: &[TpuDeviceInfo]) {
        self.device_static_info.get_or_init(|| {
            let mut device_map = HashMap::new();
            const MAX_DEVICES: usize = crate::device::readers::common_cache::MAX_DEVICES;
            let devices_to_process: Vec<_> = devices.iter().take(MAX_DEVICES).collect();

            for device in devices_to_process {
                let generation = TpuGeneration::from_chip_version(&device.chip_version);

                // Build detail HashMap using DetailBuilder
                let detail = DetailBuilder::new()
                    .insert("Device Index", device.index.to_string())
                    .insert("Chip Version", &device.chip_version)
                    .insert("Accelerator Type", &device.accelerator_type)
                    .insert("Core Count", device.core_count.to_string())
                    .insert("TensorCore Count", generation.tensor_cores().to_string())
                    .insert("Memory Type", generation.memory_type())
                    .insert(
                        "Total Memory",
                        format_memory_size(generation.hbm_size_bytes()),
                    )
                    .insert("Max Power", format!("{:.0} W", device.power_max))
                    .insert_optional(
                        "TPU Runtime Version",
                        if device.tpu_runtime_version.is_empty() {
                            None
                        } else {
                            Some(&device.tpu_runtime_version)
                        },
                    )
                    // Add unified AI acceleration library labels
                    .insert("lib_name", "libtpu")
                    .insert_optional(
                        "lib_version",
                        if device.tpu_runtime_version.is_empty() {
                            None
                        } else {
                            Some(&device.tpu_runtime_version)
                        },
                    )
                    .build();

                let uuid = if device.uuid.is_empty() {
                    format!("TPU-{}", device.index)
                } else {
                    device.uuid.clone()
                };

                let static_info = DeviceStaticInfo::with_details(
                    generation.display_name().to_string(),
                    Some(uuid.clone()),
                    detail,
                );

                device_map.insert(uuid, static_info);
            }
            device_map
        });
    }

    /// Get cached static device info
    #[cfg(target_os = "linux")]
    fn get_device_static_info(&self, uuid: &str) -> Option<&DeviceStaticInfo> {
        self.device_static_info.get().and_then(|map| map.get(uuid))
    }

    /// Check if TPU Python integration is available
    #[cfg(target_os = "linux")]
    fn is_tpu_script_available() -> bool {
        // Check cache first
        if let Ok(cache) = TPU_SCRIPT_AVAILABLE.lock() {
            if let Some(available) = *cache {
                return available;
            }
        }

        // Check if we can import jax.tools.colab_tpu or cloud-tpu-diagnostics
        let result = Self::check_tpu_python_availability();

        // Cache the result
        if let Ok(mut cache) = TPU_SCRIPT_AVAILABLE.lock() {
            *cache = Some(result);
        }

        result
    }

    /// Check Python TPU library availability
    #[cfg(target_os = "linux")]
    fn check_tpu_python_availability() -> bool {
        // First check if libtpu.so exists
        const LIBTPU_PATHS: &[&str] = &[
            "/usr/local/lib/libtpu.so",
            "/usr/lib/libtpu.so",
            "/opt/google/libtpu/libtpu.so",
        ];

        for path in LIBTPU_PATHS {
            if Path::new(path).exists() {
                return true;
            }
        }

        // Check if /dev/accel* devices exist
        if let Ok(entries) = std::fs::read_dir("/dev") {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("accel") {
                        // Verify it's a TPU device by checking sysfs
                        let sysfs_path = format!("/sys/class/accel/{}/device/vendor", name);
                        if let Ok(vendor) = std::fs::read_to_string(&sysfs_path) {
                            // Google vendor ID: 0x1ae0
                            if vendor.trim() == "0x1ae0" {
                                return true;
                            }
                        }
                        // If we can't verify vendor, check if the device exists anyway
                        if Path::new(&format!("/dev/{}", name)).exists() {
                            return true;
                        }
                    }
                }
            }
        }

        // Try Python check as fallback
        if let Ok(output) =
            execute_command_default("python3", &["-c", "import jax; jax.devices('tpu')"])
        {
            if output.status == 0 && !output.stderr.contains("error") {
                return true;
            }
        }

        false
    }

    /// Get TPU info by executing Python script
    #[cfg(target_os = "linux")]
    fn get_tpu_info_internal(&self) -> Vec<GpuInfo> {
        if !Self::is_tpu_script_available() {
            return Vec::new();
        }

        // Try to get TPU information via Python
        let devices = match Self::query_tpu_devices() {
            Some(d) => d,
            None => return Vec::new(),
        };

        if devices.is_empty() {
            return Vec::new();
        }

        // Initialize static cache on first call
        self.ensure_static_cache_initialized(&devices);

        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let hostname = get_hostname();

        devices
            .into_iter()
            .filter_map(|device| {
                let uuid = if device.uuid.is_empty() {
                    format!("TPU-{}", device.index)
                } else {
                    device.uuid.clone()
                };
                let static_info = self.get_device_static_info(&uuid);
                create_gpu_info_from_device(device, static_info, &time, &hostname)
            })
            .collect()
    }

    /// Query TPU devices using Python integration
    #[cfg(target_os = "linux")]
    fn query_tpu_devices() -> Option<Vec<TpuDeviceInfo>> {
        // Python script to query TPU devices
        let python_script = r#"
import json
import sys

def get_tpu_info():
    devices = []
    try:
        import jax
        tpu_devices = jax.devices('tpu')

        for i, device in enumerate(tpu_devices):
            device_info = {
                "index": i,
                "chip_version": getattr(device, 'device_kind', 'unknown'),
                "uuid": str(device.id) if hasattr(device, 'id') else f"TPU-{i}",
                "core_count": 1,  # Per-chip core count
                "utilization": 0.0,  # JAX doesn't provide utilization directly
                "memory_used": 0,
                "memory_total": 0,
                "temperature": 0,
                "power_draw": 0.0,
                "power_max": 0.0,
                "tpu_runtime_version": "",
                "accelerator_type": getattr(device, 'platform', 'TPU'),
            }

            # Try to get memory stats if available
            try:
                stats = device.memory_stats()
                if stats:
                    device_info["memory_used"] = stats.get("bytes_in_use", 0)
                    device_info["memory_total"] = stats.get("peak_bytes_in_use", 0) or stats.get("bytes_limit", 0)
            except:
                pass

            devices.append(device_info)
    except ImportError:
        # Try alternative detection via sysfs
        import os
        import glob

        accel_devices = glob.glob("/dev/accel*")
        for i, dev_path in enumerate(sorted(accel_devices)):
            dev_name = os.path.basename(dev_path)
            sysfs_base = f"/sys/class/accel/{dev_name}/device"

            # Read device info from sysfs
            vendor = ""
            try:
                with open(f"{sysfs_base}/vendor") as f:
                    vendor = f.read().strip()
            except:
                pass

            # Only include Google TPU devices (vendor 0x1ae0)
            if vendor != "0x1ae0":
                continue

            device_info = {
                "index": i,
                "chip_version": "unknown",
                "uuid": f"TPU-{i}",
                "core_count": 1,
                "utilization": 0.0,
                "memory_used": 0,
                "memory_total": 0,
                "temperature": 0,
                "power_draw": 0.0,
                "power_max": 0.0,
                "tpu_runtime_version": "",
                "accelerator_type": "TPU",
            }
            devices.append(device_info)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return []

    return devices

if __name__ == "__main__":
    devices = get_tpu_info()
    print(json.dumps(devices))
"#;

        let output = execute_command_default("python3", &["-c", python_script]).ok()?;

        if output.status != 0 {
            return None;
        }

        let stdout = output.stdout.trim();
        if stdout.is_empty() || stdout == "[]" {
            return None;
        }

        serde_json::from_str(stdout).ok()
    }

    /// Get TPU process information
    #[cfg(target_os = "linux")]
    fn get_process_info_internal(&self) -> Vec<ProcessInfo> {
        // TPU process tracking would require integration with cloud-tpu-diagnostics
        // or parsing /proc for processes using /dev/accel* devices
        // For now, return empty as TPU process info is not readily available
        Vec::new()
    }
}

impl GpuReader for GoogleTpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        #[cfg(target_os = "linux")]
        {
            self.get_tpu_info_internal()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        #[cfg(target_os = "linux")]
        {
            self.get_process_info_internal()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }
}

// Helper functions

#[cfg(target_os = "linux")]
fn format_memory_size(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= GB {
        format!("{} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{} MB", bytes / MB)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(target_os = "linux")]
fn create_gpu_info_from_device(
    device: TpuDeviceInfo,
    static_info: Option<&DeviceStaticInfo>,
    time: &str,
    hostname: &str,
) -> Option<GpuInfo> {
    let generation = TpuGeneration::from_chip_version(&device.chip_version);

    // Use cached static info if available, otherwise build from current device data
    let (uuid, name, mut detail) = if let Some(info) = static_info {
        let uuid = info
            .uuid
            .clone()
            .unwrap_or_else(|| format!("TPU-{}", device.index));
        (uuid, info.name.clone(), info.detail.clone())
    } else {
        // Build detail HashMap if no cache available (first call)
        let detail = DetailBuilder::new()
            .insert("Device Index", device.index.to_string())
            .insert("Chip Version", &device.chip_version)
            .insert("Accelerator Type", &device.accelerator_type)
            .insert("Core Count", device.core_count.to_string())
            .insert("TensorCore Count", generation.tensor_cores().to_string())
            .insert("Memory Type", generation.memory_type())
            .insert(
                "Total Memory",
                format_memory_size(generation.hbm_size_bytes()),
            )
            .insert("Max Power", format!("{:.0} W", device.power_max))
            // Add unified AI acceleration library labels
            .insert("lib_name", "libtpu")
            .insert_optional(
                "lib_version",
                if device.tpu_runtime_version.is_empty() {
                    None
                } else {
                    Some(&device.tpu_runtime_version)
                },
            )
            .build();

        let uuid = if device.uuid.is_empty() {
            format!("TPU-{}", device.index)
        } else {
            device.uuid.clone()
        };

        (uuid, generation.display_name().to_string(), detail)
    };

    // Dynamic values - update with current readings
    detail.insert(
        "Current Power".to_string(),
        format!("{:.1} W", device.power_draw),
    );
    detail.insert(
        "Used Memory".to_string(),
        format_memory_size(device.memory_used),
    );

    // Get memory total - use device reported if available, otherwise use generation default
    let total_memory = if device.memory_total > 0 {
        device.memory_total
    } else {
        generation.hbm_size_bytes()
    };

    Some(GpuInfo {
        uuid,
        time: time.to_string(),
        name,
        device_type: "TPU".to_string(),
        host_id: hostname.to_string(),
        hostname: hostname.to_string(),
        instance: hostname.to_string(),
        utilization: device.utilization,
        ane_utilization: 0.0,
        dla_utilization: None,
        temperature: device.temperature,
        used_memory: device.memory_used,
        total_memory,
        frequency: 0, // TPU doesn't report frequency in the same way
        power_consumption: device.power_draw,
        gpu_core_count: Some(device.core_count),
        detail,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn test_reader_creation() {
        let reader = GoogleTpuReader::new();
        // Just verify we can create the reader
        let _ = reader.get_gpu_info();
    }

    #[test]
    fn test_tpu_generation_from_chip_version() {
        assert_eq!(TpuGeneration::from_chip_version("v2"), TpuGeneration::V2);
        assert_eq!(TpuGeneration::from_chip_version("v3"), TpuGeneration::V3);
        assert_eq!(TpuGeneration::from_chip_version("v4"), TpuGeneration::V4);
        assert_eq!(TpuGeneration::from_chip_version("v5e"), TpuGeneration::V5e);
        assert_eq!(TpuGeneration::from_chip_version("v5p"), TpuGeneration::V5p);
        assert_eq!(
            TpuGeneration::from_chip_version("trillium"),
            TpuGeneration::V6Trillium
        );
        assert_eq!(
            TpuGeneration::from_chip_version("v6"),
            TpuGeneration::V6Trillium
        );
        assert_eq!(
            TpuGeneration::from_chip_version("ironwood"),
            TpuGeneration::V7Ironwood
        );
        assert_eq!(
            TpuGeneration::from_chip_version("v7"),
            TpuGeneration::V7Ironwood
        );
        assert_eq!(
            TpuGeneration::from_chip_version("unknown"),
            TpuGeneration::Unknown
        );
    }

    #[test]
    fn test_tpu_generation_hbm_size() {
        assert_eq!(TpuGeneration::V2.hbm_size_bytes(), 8 * 1024 * 1024 * 1024);
        assert_eq!(TpuGeneration::V3.hbm_size_bytes(), 16 * 1024 * 1024 * 1024);
        assert_eq!(TpuGeneration::V4.hbm_size_bytes(), 32 * 1024 * 1024 * 1024);
        assert_eq!(TpuGeneration::V5e.hbm_size_bytes(), 16 * 1024 * 1024 * 1024);
        assert_eq!(TpuGeneration::V5p.hbm_size_bytes(), 95 * 1024 * 1024 * 1024);
        assert_eq!(
            TpuGeneration::V6Trillium.hbm_size_bytes(),
            32 * 1024 * 1024 * 1024
        );
        assert_eq!(
            TpuGeneration::V7Ironwood.hbm_size_bytes(),
            192 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_tpu_generation_display_name() {
        assert_eq!(TpuGeneration::V2.display_name(), "Google TPU v2");
        assert_eq!(TpuGeneration::V3.display_name(), "Google TPU v3");
        assert_eq!(TpuGeneration::V4.display_name(), "Google TPU v4");
        assert_eq!(TpuGeneration::V5e.display_name(), "Google TPU v5e");
        assert_eq!(TpuGeneration::V5p.display_name(), "Google TPU v5p");
        assert_eq!(
            TpuGeneration::V6Trillium.display_name(),
            "Google TPU v6 Trillium"
        );
        assert_eq!(
            TpuGeneration::V7Ironwood.display_name(),
            "Google TPU v7 Ironwood 192GB HBM3e"
        );
    }

    #[test]
    fn test_tpu_generation_memory_type() {
        assert_eq!(TpuGeneration::V2.memory_type(), "HBM2");
        assert_eq!(TpuGeneration::V5p.memory_type(), "HBM2e");
        assert_eq!(TpuGeneration::V6Trillium.memory_type(), "HBM2e");
        assert_eq!(TpuGeneration::V7Ironwood.memory_type(), "HBM3e");
    }

    #[test]
    fn test_format_memory_size() {
        assert_eq!(format_memory_size(1024), "1024 B");
        assert_eq!(format_memory_size(1024 * 1024), "1 MB");
        assert_eq!(format_memory_size(1024 * 1024 * 1024), "1 GB");
        assert_eq!(format_memory_size(16 * 1024 * 1024 * 1024), "16 GB");
        assert_eq!(format_memory_size(192 * 1024 * 1024 * 1024), "192 GB");
    }

    #[test]
    fn test_create_gpu_info_from_mock_device() {
        let device = TpuDeviceInfo {
            index: 0,
            chip_version: "v4".to_string(),
            uuid: "TPU-0-test".to_string(),
            core_count: 2,
            utilization: 75.5,
            memory_used: 16 * 1024 * 1024 * 1024,  // 16 GB
            memory_total: 32 * 1024 * 1024 * 1024, // 32 GB
            temperature: 65,
            power_draw: 150.0,
            power_max: 200.0,
            tpu_runtime_version: "2.13.0".to_string(),
            accelerator_type: "TPU v4".to_string(),
        };

        let time = "2025-01-01 00:00:00";
        let hostname = "test-host";

        let gpu_info = create_gpu_info_from_device(device, None, time, hostname);

        assert!(gpu_info.is_some());
        let info = gpu_info.unwrap();

        assert_eq!(info.uuid, "TPU-0-test");
        assert_eq!(info.name, "Google TPU v4");
        assert_eq!(info.device_type, "TPU");
        assert_eq!(info.utilization, 75.5);
        assert_eq!(info.temperature, 65);
        assert_eq!(info.used_memory, 16 * 1024 * 1024 * 1024);
        assert_eq!(info.total_memory, 32 * 1024 * 1024 * 1024);
        assert_eq!(info.power_consumption, 150.0);
        assert_eq!(info.gpu_core_count, Some(2));
        assert_eq!(info.hostname, "test-host");

        // Check detail fields
        assert_eq!(info.detail.get("lib_name"), Some(&"libtpu".to_string()));
        assert_eq!(info.detail.get("lib_version"), Some(&"2.13.0".to_string()));
    }

    #[test]
    fn test_create_gpu_info_with_empty_uuid() {
        let device = TpuDeviceInfo {
            index: 5,
            chip_version: "v7".to_string(),
            uuid: "".to_string(), // Empty UUID should be auto-generated
            core_count: 2,
            utilization: 0.0,
            memory_used: 0,
            memory_total: 0, // Should default to generation size
            temperature: 45,
            power_draw: 0.0,
            power_max: 400.0,
            tpu_runtime_version: "".to_string(),
            accelerator_type: "TPU v7 Ironwood".to_string(),
        };

        let gpu_info = create_gpu_info_from_device(device, None, "2025-01-01 00:00:00", "host");

        assert!(gpu_info.is_some());
        let info = gpu_info.unwrap();

        // UUID should be auto-generated from index
        assert_eq!(info.uuid, "TPU-5");
        // Name should reflect v7 Ironwood
        assert_eq!(info.name, "Google TPU v7 Ironwood 192GB HBM3e");
        // Memory should default to v7 Ironwood size (192 GB)
        assert_eq!(info.total_memory, 192 * 1024 * 1024 * 1024);
    }
}
