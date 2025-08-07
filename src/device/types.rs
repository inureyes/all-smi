use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Type aliases for complex return types
#[allow(dead_code)]
pub type ProcessInfoResult = Option<(
    f64,
    f64,
    u64,
    u64,
    String,
    String,
    String,
    u64,
    String,
    u32,
    u32,
)>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuInfo {
    pub uuid: String,
    pub time: String,
    pub name: String,
    pub device_type: String, // "GPU", "NPU", etc.
    pub host_id: String,     // Host identifier (e.g., "10.82.128.41:9090")
    pub hostname: String,    // DNS hostname of the server
    pub instance: String,    // Instance name from metrics
    pub utilization: f64,
    pub ane_utilization: f64,
    pub dla_utilization: Option<f64>,
    pub temperature: u32,
    pub used_memory: u64,
    pub total_memory: u64,
    pub frequency: u32,
    pub power_consumption: f64,
    pub gpu_core_count: Option<u32>, // Number of GPU cores (e.g., Apple Silicon)
    pub detail: HashMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub device_id: usize,     // GPU index (internal)
    pub device_uuid: String,  // GPU UUID
    pub pid: u32,             // Process ID
    pub process_name: String, // Process name
    pub used_memory: u64,     // GPU memory usage in bytes
    pub cpu_percent: f64,     // CPU usage percentage
    pub memory_percent: f64,  // System memory usage percentage
    pub memory_rss: u64,      // Resident Set Size in bytes
    pub memory_vms: u64,      // Virtual Memory Size in bytes
    pub user: String,         // User name
    pub state: String,        // Process state (R, S, D, etc.)
    pub start_time: String,   // Process start time
    pub cpu_time: u64,        // Total CPU time in seconds
    pub command: String,      // Full command line
    pub ppid: u32,            // Parent process ID
    pub threads: u32,         // Number of threads
    pub uses_gpu: bool,       // Whether the process uses GPU
    pub priority: i32,        // Process priority (PRI)
    pub nice_value: i32,      // Nice value (NI)
    pub gpu_utilization: f64, // GPU utilization percentage
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuInfo {
    pub host_id: String,  // Host identifier (e.g., "10.82.128.41:9090")
    pub hostname: String, // DNS hostname of the server
    pub instance: String, // Instance name from metrics
    pub cpu_model: String,
    pub architecture: String, // "x86_64", "arm64", etc.
    pub platform_type: CpuPlatformType,
    pub socket_count: u32,                   // Number of CPU sockets
    pub total_cores: u32,                    // Total logical cores
    pub total_threads: u32,                  // Total threads (with hyperthreading)
    pub base_frequency_mhz: u32,             // Base CPU frequency
    pub max_frequency_mhz: u32,              // Maximum CPU frequency
    pub cache_size_mb: u32,                  // Total cache size in MB
    pub utilization: f64,                    // Overall CPU utilization percentage
    pub temperature: Option<u32>,            // CPU temperature (if available)
    pub power_consumption: Option<f64>,      // Power consumption in watts (if available)
    pub per_socket_info: Vec<CpuSocketInfo>, // Per-socket information
    pub apple_silicon_info: Option<AppleSiliconCpuInfo>, // Apple Silicon specific info
    pub per_core_utilization: Vec<CoreUtilization>, // Per-core utilization data
    pub time: String,                        // Timestamp
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoreUtilization {
    pub core_id: u32,        // Core identifier (0-based)
    pub core_type: CoreType, // Type of core (Performance, Efficiency, Standard)
    pub utilization: f64,    // Core utilization percentage (0-100)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CoreType {
    Performance, // P-cores (Apple Silicon) or Performance cores (Intel/AMD)
    Efficiency,  // E-cores (Apple Silicon) or Efficiency cores (Intel/AMD)
    Standard,    // Regular cores (no P/E distinction)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CpuPlatformType {
    Intel,
    Amd,
    AppleSilicon,
    Arm,
    Other(String), // For unknown or other CPU types
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuSocketInfo {
    pub socket_id: u32,           // Socket identifier
    pub utilization: f64,         // Per-socket utilization
    pub cores: u32,               // Number of cores in this socket
    pub threads: u32,             // Number of threads in this socket
    pub temperature: Option<u32>, // Socket temperature (if available)
    pub frequency_mhz: u32,       // Current frequency
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppleSiliconCpuInfo {
    pub p_core_count: u32,                    // Performance core count
    pub e_core_count: u32,                    // Efficiency core count
    pub gpu_core_count: u32,                  // GPU core count
    pub p_core_utilization: f64,              // Performance core utilization
    pub e_core_utilization: f64,              // Efficiency core utilization
    pub ane_ops_per_second: Option<f64>,      // ANE operations per second
    pub p_cluster_frequency_mhz: Option<u32>, // P-cluster frequency in MHz
    pub e_cluster_frequency_mhz: Option<u32>, // E-cluster frequency in MHz
    pub p_core_l2_cache_mb: Option<u32>,      // P-core L2 cache size in MB
    pub e_core_l2_cache_mb: Option<u32>,      // E-core L2 cache size in MB
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryInfo {
    pub host_id: String,       // Host identifier (e.g., "10.82.128.41:9090")
    pub hostname: String,      // DNS hostname of the server
    pub instance: String,      // Instance name from metrics
    pub total_bytes: u64,      // Total system memory in bytes
    pub used_bytes: u64,       // Used memory in bytes
    pub available_bytes: u64,  // Available memory in bytes
    pub free_bytes: u64,       // Free memory in bytes
    pub buffers_bytes: u64,    // Buffer memory in bytes (Linux specific)
    pub cached_bytes: u64,     // Cached memory in bytes (Linux specific)
    pub swap_total_bytes: u64, // Total swap space in bytes
    pub swap_used_bytes: u64,  // Used swap space in bytes
    pub swap_free_bytes: u64,  // Free swap space in bytes
    pub utilization: f64,      // Memory utilization percentage
    pub time: String,          // Timestamp
}
