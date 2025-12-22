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

// Common constants for device readers

#![allow(dead_code)]

/// Memory conversion constants
pub const BYTES_PER_KB: u64 = 1024;
pub const BYTES_PER_MB: u64 = BYTES_PER_KB * 1024;
pub const BYTES_PER_GB: u64 = BYTES_PER_MB * 1024;
pub const BYTES_PER_TB: u64 = BYTES_PER_GB * 1024;

/// Default memory sizes for specific devices
pub const FURIOSA_HBM3_MEMORY_GB: u64 = 48;
pub const FURIOSA_HBM3_MEMORY_BYTES: u64 = FURIOSA_HBM3_MEMORY_GB * BYTES_PER_GB;

/// Power conversion constants
pub const MILLIWATTS_PER_WATT: f64 = 1000.0;

/// Temperature constants
pub const ABSOLUTE_ZERO_CELSIUS: i32 = -273;
pub const MAX_SAFE_TEMPERATURE_C: u32 = 100;

/// Frequency constants
pub const MHZ_PER_GHZ: u32 = 1000;

/// Default values for missing data
pub const DEFAULT_TEMPERATURE: u32 = 0;
pub const DEFAULT_POWER: f64 = 0.0;
pub const DEFAULT_FREQUENCY: u32 = 0;
pub const DEFAULT_UTILIZATION: f64 = 0.0;
pub const DEFAULT_MEMORY: u64 = 0;

/// Device-specific constants
pub mod furiosa {
    pub const CORE_COUNT: u32 = 8;
    pub const PE_COUNT_STR: &str = "64K";
    pub const MEMORY_BANDWIDTH: &str = "1.63TB/s";
    pub const ON_CHIP_SRAM: &str = "256MB";
}

/// Google TPU-specific constants
pub mod google_tpu {
    /// TPU v2 HBM memory size in bytes (8 GB)
    pub const TPU_V2_HBM_BYTES: u64 = 8 * 1024 * 1024 * 1024;
    /// TPU v3 HBM memory size in bytes (16 GB)
    pub const TPU_V3_HBM_BYTES: u64 = 16 * 1024 * 1024 * 1024;
    /// TPU v4 HBM memory size in bytes (32 GB)
    pub const TPU_V4_HBM_BYTES: u64 = 32 * 1024 * 1024 * 1024;
    /// TPU v5e HBM memory size in bytes (16 GB)
    pub const TPU_V5E_HBM_BYTES: u64 = 16 * 1024 * 1024 * 1024;
    /// TPU v5p HBM memory size in bytes (95 GB)
    pub const TPU_V5P_HBM_BYTES: u64 = 95 * 1024 * 1024 * 1024;
    /// TPU v6 Trillium HBM memory size in bytes (32 GB)
    pub const TPU_V6_TRILLIUM_HBM_BYTES: u64 = 32 * 1024 * 1024 * 1024;
    /// TPU v7 Ironwood HBM3e memory size in bytes (192 GB)
    pub const TPU_V7_IRONWOOD_HBM_BYTES: u64 = 192 * 1024 * 1024 * 1024;

    /// Google vendor ID for PCI devices
    pub const GOOGLE_VENDOR_ID: &str = "0x1ae0";
    /// Google vendor ID without 0x prefix (for lspci -n output)
    pub const GOOGLE_VENDOR_ID_SHORT: &str = "1ae0";

    /// Common libtpu library paths
    pub const LIBTPU_PATHS: &[&str] = &[
        "/usr/local/lib/libtpu.so",
        "/usr/lib/libtpu.so",
        "/opt/google/libtpu/libtpu.so",
    ];
}

/// Process information defaults
pub const DEFAULT_PID: u32 = 0;
pub const DEFAULT_CPU_PERCENT: f64 = 0.0;
pub const DEFAULT_PRIORITY: i32 = 0;
pub const DEFAULT_NICE_VALUE: i32 = 0;

/// String parsing constants
pub const DEVICE_ID_PREFIX_NPU: &str = "npu";
pub const DEVICE_ID_PREFIX_GPU: &str = "gpu";
pub const TEMPERATURE_SUFFIX_C: char = 'C';
pub const POWER_SUFFIX_W: char = 'W';
pub const PERCENTAGE_SUFFIX: char = '%';
pub const MEMORY_SUFFIX_MB: &str = "MB";
pub const MEMORY_SUFFIX_MIB: &str = "MiB";
pub const FREQUENCY_SUFFIX_MHZ: &str = "MHz";
