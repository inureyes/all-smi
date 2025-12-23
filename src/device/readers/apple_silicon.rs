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

use crate::device::common::command_executor::execute_command_default;
use crate::device::powermetrics::get_powermetrics_manager;
use crate::device::readers::common_cache::{DetailBuilder, DeviceStaticInfo};
use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use once_cell::sync::{Lazy, OnceCell};
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use sysinfo::System;

// Type alias to simplify the complex type
type GpuInfoCache = DeviceStaticInfo;

// Cache GPU info to avoid expensive system_profiler calls on every initialization
static CACHED_GPU_INFO: Lazy<Mutex<Option<GpuInfoCache>>> = Lazy::new(|| Mutex::new(None));

// Apple Silicon specific info that needs to be cached separately
struct AppleSiliconInfo {
    gpu_core_count: Option<u32>,
}

pub struct AppleSiliconGpuReader {
    static_info: OnceCell<DeviceStaticInfo>,
    apple_info: OnceCell<AppleSiliconInfo>,
    initialized: AtomicBool,
}

impl Default for AppleSiliconGpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl AppleSiliconGpuReader {
    pub fn new() -> Self {
        AppleSiliconGpuReader {
            static_info: OnceCell::new(),
            apple_info: OnceCell::new(),
            initialized: AtomicBool::new(false),
        }
    }

    fn ensure_initialized(&self) {
        if self.initialized.load(Ordering::Acquire) {
            return;
        }

        // Check cache first to avoid expensive system_profiler calls
        let mut cache = match CACHED_GPU_INFO.lock() {
            Ok(guard) => guard,
            Err(e) => {
                eprintln!("Failed to acquire lock for Apple Silicon GPU cache: {e}");
                return;
            }
        };

        if let Some(static_info) = cache.as_ref() {
            // Use cached values - safe initialization via OnceCell
            let _ = self.static_info.set(static_info.clone());
            // Extract gpu_core_count from detail if present
            let gpu_core_count = static_info
                .detail
                .get("GPU Core Count")
                .and_then(|s| s.parse::<u32>().ok());
            let _ = self.apple_info.set(AppleSiliconInfo { gpu_core_count });
            self.initialized.store(true, Ordering::Release);
            return;
        }

        // If not cached, fetch the information (this is slow but only happens once)
        let (name, driver_version) = get_gpu_name_and_version();
        let gpu_core_count = get_gpu_core_count();

        // Build DeviceStaticInfo using DetailBuilder
        let mut builder = DetailBuilder::new()
            .insert("gpu_type", "Integrated")
            .insert_optional("driver_version", driver_version.as_ref());

        if let Some(count) = gpu_core_count {
            builder = builder.insert("GPU Core Count", count.to_string());
        }

        let detail = builder.build();
        let static_info = DeviceStaticInfo::with_details(name, None, detail);

        // Store in cache for future use
        *cache = Some(static_info.clone());

        // Update self - safe initialization via OnceCell
        let _ = self.static_info.set(static_info);
        let _ = self.apple_info.set(AppleSiliconInfo { gpu_core_count });
        self.initialized.store(true, Ordering::Release);
    }

    /// Get GPU processes from powermetrics
    fn get_gpu_processes(&self) -> (Vec<ProcessInfo>, HashSet<u32>) {
        let mut gpu_processes = Vec::new();
        let mut gpu_pids = HashSet::new();

        // Try to get process info from PowerMetricsManager
        let process_data = if let Some(manager) = get_powermetrics_manager() {
            manager.get_process_info()
        } else {
            vec![]
        };

        // Convert GPU process data to ProcessInfo
        for (process_name, pid, gpu_usage) in process_data {
            if gpu_usage > 0.0 {
                gpu_pids.insert(pid);

                gpu_processes.push(ProcessInfo {
                    device_id: 0,
                    device_uuid: "AppleSiliconGPU".to_string(),
                    pid,
                    process_name: process_name.clone(),
                    used_memory: gpu_usage as u64, // Using GPU ms/s as a proxy for memory
                    cpu_percent: 0.0,              // Will be filled by sysinfo
                    memory_percent: 0.0,           // Will be filled by sysinfo
                    memory_rss: 0,                 // Will be filled by sysinfo
                    memory_vms: 0,                 // Will be filled by sysinfo
                    user: String::new(),           // Will be filled by sysinfo
                    state: String::new(),          // Will be filled by sysinfo
                    start_time: String::new(),     // Will be filled by sysinfo
                    cpu_time: 0,                   // Will be filled by sysinfo
                    command: String::new(),        // Will be filled by sysinfo
                    ppid: 0,                       // Will be filled by sysinfo
                    threads: 0,                    // Will be filled by sysinfo
                    uses_gpu: true,
                    priority: 0,          // Will be filled by sysinfo
                    nice_value: 0,        // Will be filled by sysinfo
                    gpu_utilization: 0.0, // Apple Silicon doesn't provide per-process GPU utilization
                });
            }
        }

        (gpu_processes, gpu_pids)
    }
}

impl GpuReader for AppleSiliconGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        // Ensure GPU info is initialized (happens on first call)
        self.ensure_initialized();

        let manager = get_powermetrics_manager();
        let (metrics, combined_power_mw) = if let Some(mgr) = &manager {
            // Get the latest powermetrics data
            if let Ok(data) = mgr.get_latest_data_result() {
                let combined_power = data.combined_power_mw;
                (
                    GpuMetrics {
                        utilization: Some(data.gpu_active_residency),
                        ane_utilization: Some(data.ane_power_mw),
                        frequency: Some(data.gpu_frequency),
                        power_consumption: Some(data.gpu_power_mw / 1000.0), // Convert mW to W
                        thermal_pressure_level: data.thermal_pressure_level,
                    },
                    Some(combined_power),
                )
            } else {
                (get_gpu_metrics_fallback(), None)
            }
        } else {
            // Fallback to creating temporary powermetrics reader
            (get_gpu_metrics_fallback(), None)
        };

        // Get cached static info
        let static_info = self
            .static_info
            .get()
            .expect("ensure_initialized should have set static_info");
        let apple_info = self
            .apple_info
            .get()
            .expect("ensure_initialized should have set apple_info");

        let mut detail = static_info.detail.clone();
        detail.insert("architecture".to_string(), "Apple Silicon".to_string());
        if let Some(ref thermal_level) = metrics.thermal_pressure_level {
            detail.insert("thermal_pressure".to_string(), thermal_level.clone());
        }

        // Add combined power (CPU + GPU + ANE) for metrics export
        if let Some(combined_power) = combined_power_mw {
            detail.insert("combined_power_mw".to_string(), combined_power.to_string());
        }

        // Add unified AI acceleration library labels
        detail.insert("lib_name".to_string(), "Metal".to_string());
        // For Apple Silicon, use the Metal version if available
        if let Some(driver_ver) = static_info.detail.get("driver_version") {
            if driver_ver != "Unknown" {
                // Extract numeric version from "Metal X.Y" format
                let lib_ver = driver_ver
                    .strip_prefix("Metal ")
                    .unwrap_or(driver_ver)
                    .to_string();
                detail.insert("lib_version".to_string(), lib_ver);
            }
        }

        vec![GpuInfo {
            uuid: static_info
                .uuid
                .clone()
                .unwrap_or_else(|| "AppleSiliconGPU".to_string()),
            time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            name: static_info.name.clone(),
            device_type: "GPU".to_string(),
            host_id: get_hostname(),
            hostname: get_hostname(),
            instance: get_hostname(),
            utilization: metrics.utilization.unwrap_or(0.0),
            ane_utilization: metrics.ane_utilization.unwrap_or(0.0),
            dla_utilization: None,
            tensorcore_utilization: None,
            temperature: 0, // Apple Silicon reports pressure level as text, not numeric temp
            used_memory: get_used_memory(), // Get system memory usage (unified memory)
            total_memory: get_total_memory(), // Get total system memory (unified memory)
            frequency: metrics.frequency.unwrap_or(0),
            power_consumption: metrics.power_consumption.unwrap_or(0.0),
            gpu_core_count: apple_info.gpu_core_count,
            detail,
        }]
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // For Apple Silicon, we only return GPU process information from powermetrics
        let (gpu_processes, _gpu_pids) = self.get_gpu_processes();
        gpu_processes
    }
}

struct GpuMetrics {
    utilization: Option<f64>,
    ane_utilization: Option<f64>,
    frequency: Option<u32>,
    power_consumption: Option<f64>,
    thermal_pressure_level: Option<String>,
}

fn get_gpu_metrics_fallback() -> GpuMetrics {
    // Fallback implementation when PowerMetricsManager is not available
    let output = execute_command_default(
        "sudo",
        &[
            "powermetrics",
            "--samplers",
            "gpu_power",
            "-n",
            "1",
            "-i",
            "1000",
        ],
    );

    match output {
        Ok(cmd_output) => parse_gpu_metrics(&cmd_output.stdout),
        Err(_) => GpuMetrics {
            utilization: None,
            ane_utilization: None,
            frequency: None,
            power_consumption: None,
            thermal_pressure_level: None,
        },
    }
}

fn parse_gpu_metrics(output: &str) -> GpuMetrics {
    let mut utilization = None;
    let mut ane_utilization = None;
    let mut frequency = None;
    let mut power_consumption = None;
    let mut thermal_pressure_level = None;

    for line in output.lines() {
        let line = line.trim();

        if line.contains("GPU HW active residency:") {
            utilization = crate::parse_metric!(line, "%", f64);
        } else if line.contains("ANE Power:") {
            ane_utilization = crate::parse_metric!(line, "mW", f64);
        } else if line.contains("GPU HW active frequency:") {
            frequency = crate::parse_metric!(line, "MHz", u32);
        } else if line.contains("GPU Power:") && !line.contains("CPU + GPU") {
            power_consumption = crate::parse_metric!(line, "mW", f64).map(|p| p / 1000.0);
        // Convert mW to W
        } else if line.contains("pressure level:") {
            if let Some(pressure_str) = line.split(':').nth(1) {
                thermal_pressure_level = Some(pressure_str.trim().to_string());
            }
        }
    }

    GpuMetrics {
        utilization,
        ane_utilization,
        frequency,
        power_consumption,
        thermal_pressure_level,
    }
}

fn get_gpu_name_and_version() -> (String, Option<String>) {
    // Try to get GPU name from sysctl first (fast path for name only)
    let gpu_name = if let Ok(output) =
        execute_command_default("sysctl", &["-n", "machdep.cpu.brand_string"])
    {
        let cpu_brand = output.stdout.trim().to_string();
        // Extract chip name from CPU brand string (e.g., "Apple M1 Pro" from full string)
        if cpu_brand.contains("Apple M") {
            let mut name = None;
            for part in cpu_brand.split_whitespace() {
                if part.starts_with("M") && part.chars().nth(1).is_some_and(|c| c.is_numeric()) {
                    // Found the chip model (M1, M2, M3, etc.)
                    let mut gpu_name = format!("Apple {part} GPU");
                    // Check for Pro/Max/Ultra suffix
                    let parts: Vec<&str> = cpu_brand.split_whitespace().collect();
                    if let Some(pos) = parts.iter().position(|&x| x == part) {
                        if pos + 1 < parts.len() {
                            let suffix = parts[pos + 1];
                            if suffix == "Pro" || suffix == "Max" || suffix == "Ultra" {
                                gpu_name = format!("Apple {part} {suffix} GPU");
                            }
                        }
                    }
                    name = Some(gpu_name);
                    break;
                }
            }
            name.unwrap_or_else(|| "Apple Silicon GPU".to_string())
        } else {
            "Apple Silicon GPU".to_string()
        }
    } else {
        "Apple Silicon GPU".to_string()
    };

    // Get Metal version from system_profiler (always call to get accurate version)
    let metal_version = match execute_command_default("system_profiler", &["SPDisplaysDataType"]) {
        Ok(cmd_output) => {
            let (_profiler_name, profiler_version) =
                parse_system_profiler_output(&cmd_output.stdout);
            // Use the Metal version from system_profiler if available
            if let Some(version) = profiler_version {
                Some(version)
            } else {
                // If system_profiler didn't provide version, try to infer from Metal framework
                get_metal_version_from_framework()
            }
        }
        Err(_) => {
            // If system_profiler fails, try to get version from Metal framework
            get_metal_version_from_framework()
        }
    };

    (gpu_name, metal_version)
}

/// Try to get Metal version from the Metal framework
fn get_metal_version_from_framework() -> Option<String> {
    // Metal 3 is available on macOS 13+ (Ventura)
    // We can check the macOS version to infer Metal version
    if let Ok(output) = execute_command_default("sw_vers", &["-productVersion"]) {
        let version_str = output.stdout.trim();
        if let Some(major_version) = version_str.split('.').next() {
            if let Ok(major) = major_version.parse::<u32>() {
                // macOS version to Metal version mapping
                // Note: Version numbering jumped from 15 to 26 (year-based)
                let metal_version = match major {
                    26.. => "Metal 4",    // macOS 26+ (Tahoe and later) - Metal 4
                    15..=25 => "Metal 3", // macOS 15-25 (Sequoia era) - Metal 3
                    14 => "Metal 3",      // macOS 14 (Sonoma) - Metal 3
                    13 => "Metal 3",      // macOS 13 (Ventura) - Metal 3
                    12 => "Metal 2.4",    // macOS 12 (Monterey) - Metal 2.4
                    11 => "Metal 2.3",    // macOS 11 (Big Sur) - Metal 2.3
                    _ => "Metal 2",       // Older versions
                };
                return Some(metal_version.to_string());
            }
        }
    }
    Some("Metal 3".to_string()) // Default fallback
}

fn parse_system_profiler_output(output_str: &str) -> (String, Option<String>) {
    let mut gpu_name = "Apple Silicon GPU".to_string();
    let mut driver_version = None;

    let lines: Vec<&str> = output_str.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains("Chipset Model:") {
            if let Some(name) = line.split(':').nth(1) {
                gpu_name = name.trim().to_string();
            }
        } else if line.contains("Metal") && i + 1 < lines.len() {
            // Look for Metal version in the next line or current line
            let version_line = if lines[i + 1].contains("Version:") {
                lines[i + 1]
            } else {
                line
            };
            if let Some(version) = version_line.split(':').nth(1) {
                driver_version = Some(version.trim().to_string());
            }
        }
    }

    (gpu_name, driver_version)
}

fn get_gpu_core_count() -> Option<u32> {
    // Try to determine GPU core count based on chip model
    if let Ok(output) = execute_command_default("sysctl", &["-n", "machdep.cpu.brand_string"]) {
        let cpu_brand = output.stdout.trim().to_string();

        // Use pattern matching for cleaner code
        let core_count = match cpu_brand.as_str() {
            s if s.contains("M1 ")
                && !s.contains("Pro")
                && !s.contains("Max")
                && !s.contains("Ultra") =>
            {
                Some(8)
            }
            s if s.contains("M1 Pro") => Some(16),
            s if s.contains("M1 Max") => Some(32),
            s if s.contains("M1 Ultra") => Some(64),
            s if s.contains("M2 ")
                && !s.contains("Pro")
                && !s.contains("Max")
                && !s.contains("Ultra") =>
            {
                Some(10)
            }
            s if s.contains("M2 Pro") => Some(19),
            s if s.contains("M2 Max") => Some(38),
            s if s.contains("M2 Ultra") => Some(76),
            s if s.contains("M3 ") && !s.contains("Pro") && !s.contains("Max") => Some(10),
            s if s.contains("M3 Pro") => Some(18),
            s if s.contains("M3 Max") => Some(40),
            s if s.contains("M4 ") && !s.contains("Pro") && !s.contains("Max") => Some(10),
            s if s.contains("M4 Pro") => Some(20),
            s if s.contains("M4 Max") => Some(40),
            _ => None,
        };

        if core_count.is_some() {
            return core_count;
        }
    }

    // Fallback to ioreg only if we can't determine from CPU brand
    match execute_command_default("ioreg", &["-rc", "AGXAccelerator", "-d1"]) {
        Ok(cmd_output) => parse_ioreg_gpu_cores(&cmd_output.stdout),
        Err(_) => None,
    }
}

fn parse_ioreg_gpu_cores(output_str: &str) -> Option<u32> {
    for line in output_str.lines() {
        if line.contains("\"gpu-core-count\"") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(core_count) = parts[2].parse::<u32>() {
                    return Some(core_count);
                }
            }
        }
    }
    None
}

/// Get total system memory for Apple Silicon (unified memory)
fn get_total_memory() -> u64 {
    let mut system = System::new();
    system.refresh_memory();
    system.total_memory()
}

/// Get used memory for Apple Silicon (approximation of GPU memory usage)
fn get_used_memory() -> u64 {
    let mut system = System::new();
    system.refresh_memory();
    system.used_memory()
}
