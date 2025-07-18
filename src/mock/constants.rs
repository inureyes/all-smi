//! Constants used throughout the mock server

// General configuration constants
pub const DEFAULT_GPU_NAME: &str = "NVIDIA H200 141GB HBM3";
pub const DEFAULT_TENSTORRENT_NAME: &str = "Tenstorrent Grayskull e75 120W";
pub const NUM_GPUS: usize = 8;
pub const UPDATE_INTERVAL_SECS: u64 = 3;
pub const MAX_CONNECTIONS_PER_SERVER: usize = 10;

// Disk size options in bytes
pub const DISK_SIZE_1TB: u64 = 1024 * 1024 * 1024 * 1024;
pub const DISK_SIZE_4TB: u64 = 4 * 1024 * 1024 * 1024 * 1024;
pub const DISK_SIZE_12TB: u64 = 12 * 1024 * 1024 * 1024 * 1024;

// CPU placeholders
pub const PLACEHOLDER_CPU_UTIL: &str = "{{CPU_UTIL}}";
pub const PLACEHOLDER_CPU_SOCKET0_UTIL: &str = "{{CPU_SOCKET0_UTIL}}";
pub const PLACEHOLDER_CPU_SOCKET1_UTIL: &str = "{{CPU_SOCKET1_UTIL}}";
pub const PLACEHOLDER_CPU_P_CORE_UTIL: &str = "{{CPU_P_CORE_UTIL}}";
pub const PLACEHOLDER_CPU_E_CORE_UTIL: &str = "{{CPU_E_CORE_UTIL}}";
pub const PLACEHOLDER_CPU_TEMP: &str = "{{CPU_TEMP}}";
pub const PLACEHOLDER_CPU_POWER: &str = "{{CPU_POWER}}";

// System memory placeholders
pub const PLACEHOLDER_SYS_MEMORY_USED: &str = "{{SYS_MEMORY_USED}}";
pub const PLACEHOLDER_SYS_MEMORY_AVAILABLE: &str = "{{SYS_MEMORY_AVAILABLE}}";
pub const PLACEHOLDER_SYS_MEMORY_FREE: &str = "{{SYS_MEMORY_FREE}}";
pub const PLACEHOLDER_SYS_MEMORY_UTIL: &str = "{{SYS_MEMORY_UTIL}}";
pub const PLACEHOLDER_SYS_SWAP_USED: &str = "{{SYS_SWAP_USED}}";
pub const PLACEHOLDER_SYS_SWAP_FREE: &str = "{{SYS_SWAP_FREE}}";
pub const PLACEHOLDER_SYS_MEMORY_BUFFERS: &str = "{{SYS_MEMORY_BUFFERS}}";
pub const PLACEHOLDER_SYS_MEMORY_CACHED: &str = "{{SYS_MEMORY_CACHED}}";

// Disk placeholders
pub const PLACEHOLDER_DISK_AVAIL: &str = "{{DISK_AVAIL}}";
pub const PLACEHOLDER_DISK_TOTAL: &str = "{{DISK_TOTAL}}";
