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

use crate::device::powermetrics::get_powermetrics_manager;
use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use once_cell::sync::{Lazy, OnceCell};
use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

// Type alias to simplify the complex type
type GpuInfoCache = (String, Option<String>, Option<u32>);

// Cache GPU info to avoid expensive system_profiler calls on every initialization
static CACHED_GPU_INFO: Lazy<Mutex<Option<GpuInfoCache>>> = Lazy::new(|| Mutex::new(None));

pub struct AppleSiliconGpuReader {
    name: OnceCell<String>,
    driver_version: OnceCell<Option<String>>,
    gpu_core_count: OnceCell<Option<u32>>,
    initialized: AtomicBool,
}

impl Default for AppleSiliconGpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl AppleSiliconGpuReader {
    pub fn new() -> Self {
        // Don't fetch GPU info during initialization - defer it to first use
        AppleSiliconGpuReader {
            name: OnceCell::new(),
            driver_version: OnceCell::new(),
            gpu_core_count: OnceCell::new(),
            initialized: AtomicBool::new(false),
        }
    }

    fn ensure_initialized(&self) {
        if self.initialized.load(Ordering::Acquire) {
            return;
        }

        // Check cache first to avoid expensive system_profiler calls
        let mut cache = CACHED_GPU_INFO.lock().unwrap();

        if let Some((name, driver_version, gpu_core_count)) = cache.as_ref() {
            // Use cached values - safe initialization via OnceCell
            let _ = self.name.set(name.clone());
            let _ = self.driver_version.set(driver_version.clone());
            let _ = self.gpu_core_count.set(*gpu_core_count);
            self.initialized.store(true, Ordering::Release);
            return;
        }

        // If not cached, fetch the information (this is slow but only happens once)
        let (name, driver_version) = get_gpu_name_and_version();
        let gpu_core_count = get_gpu_core_count();

        // Store in cache for future use
        *cache = Some((name.clone(), driver_version.clone(), gpu_core_count));

        // Update self - safe initialization via OnceCell
        let _ = self.name.set(name);
        let _ = self.driver_version.set(driver_version);
        let _ = self.gpu_core_count.set(gpu_core_count);
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

                // Create minimal ProcessInfo for GPU data
                // The rest will be filled by sysinfo
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
        let metrics = if let Some(mgr) = &manager {
            // Get the latest powermetrics data
            if let Ok(data) = mgr.get_latest_data_result() {
                GpuMetrics {
                    utilization: Some(data.gpu_active_residency),
                    ane_utilization: Some(data.ane_power_mw),
                    frequency: Some(data.gpu_frequency),
                    power_consumption: Some(data.gpu_power_mw / 1000.0), // Convert mW to W
                    thermal_pressure_level: data.thermal_pressure_level,
                }
            } else {
                get_gpu_metrics_fallback()
            }
        } else {
            // Fallback to creating temporary powermetrics reader
            get_gpu_metrics_fallback()
        };

        let mut detail = HashMap::new();
        detail.insert(
            "Driver Version".to_string(),
            self.driver_version
                .get()
                .and_then(|v| v.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
        );
        detail.insert("GPU Type".to_string(), "Integrated".to_string());
        detail.insert("Architecture".to_string(), "Apple Silicon".to_string());
        if let Some(ref thermal_level) = metrics.thermal_pressure_level {
            detail.insert("Thermal Pressure".to_string(), thermal_level.clone());
        }

        vec![GpuInfo {
            uuid: "AppleSiliconGPU".to_string(),
            time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            name: self
                .name
                .get()
                .cloned()
                .unwrap_or_else(|| "Apple Silicon GPU".to_string()),
            device_type: "GPU".to_string(),
            host_id: get_hostname(), // For local mode, host_id is just the hostname
            hostname: get_hostname(), // DNS hostname
            instance: get_hostname(),
            utilization: metrics.utilization.unwrap_or(0.0),
            ane_utilization: metrics.ane_utilization.unwrap_or(0.0),
            dla_utilization: None,
            temperature: 0, // Apple Silicon reports pressure level as text, not numeric temp
            used_memory: 0, // Apple Silicon doesn't report dedicated GPU memory
            total_memory: 0, // Using unified memory
            frequency: metrics.frequency.unwrap_or(0),
            power_consumption: metrics.power_consumption.unwrap_or(0.0),
            gpu_core_count: self.gpu_core_count.get().copied().flatten(),
            detail,
        }]
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // For Apple Silicon, we only return GPU process information from powermetrics
        // The main process list will be collected separately to avoid duplication
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
    // This is a simplified version that runs powermetrics once
    let output = Command::new("sudo")
        .args([
            "powermetrics",
            "--samplers",
            "gpu_power",
            "-n",
            "1",
            "-i",
            "1000",
        ])
        .output()
        .ok();

    if let Some(output) = output {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            return parse_gpu_metrics(&output_str);
        }
    }

    GpuMetrics {
        utilization: None,
        ane_utilization: None,
        frequency: None,
        power_consumption: None,
        thermal_pressure_level: None,
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
            if let Some(percent_str) = line.split(':').nth(1) {
                if let Some(percent) = percent_str.split_whitespace().next() {
                    utilization = percent.trim_end_matches('%').parse::<f64>().ok();
                }
            }
        } else if line.contains("ANE Power:") {
            if let Some(power_str) = line.split(':').nth(1) {
                ane_utilization = power_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<f64>()
                    .ok();
            }
        } else if line.contains("GPU HW active frequency:") {
            if let Some(freq_str) = line.split(':').nth(1) {
                frequency = freq_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<u32>()
                    .ok();
            }
        } else if line.contains("GPU Power:") && !line.contains("CPU + GPU") {
            if let Some(power_str) = line.split(':').nth(1) {
                power_consumption = power_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<f64>()
                    .ok();
                // Convert power from mW to W
                if let Some(p) = power_consumption {
                    power_consumption = Some(p / 1000.0);
                }
            }
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
    // Try to get chip name from a faster source first
    if let Ok(output) = Command::new("sysctl")
        .arg("-n")
        .arg("machdep.cpu.brand_string")
        .output()
    {
        let cpu_brand = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Extract chip name from CPU brand string (e.g., "Apple M1 Pro" from full string)
        if cpu_brand.contains("Apple M") {
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
                    return (gpu_name, Some("Metal 3".to_string()));
                }
            }
            // If we found "Apple M" but couldn't parse the model, return a default
            return ("Apple Silicon GPU".to_string(), Some("Metal 3".to_string()));
        }
    }

    // Only use system_profiler as absolute last resort (this should rarely happen)
    let output = Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output()
        .unwrap_or_else(|_| {
            // If system_profiler fails, return default values
            std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }
        });

    if output.stdout.is_empty() {
        return ("Apple Silicon GPU".to_string(), Some("Metal 3".to_string()));
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
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
    // This is faster than calling ioreg
    if let Ok(output) = Command::new("sysctl")
        .arg("-n")
        .arg("machdep.cpu.brand_string")
        .output()
    {
        let cpu_brand = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Common GPU core counts for Apple Silicon chips
        if cpu_brand.contains("M1 ")
            && !cpu_brand.contains("Pro")
            && !cpu_brand.contains("Max")
            && !cpu_brand.contains("Ultra")
        {
            return Some(8); // M1 base
        } else if cpu_brand.contains("M1 Pro") {
            return Some(16); // M1 Pro
        } else if cpu_brand.contains("M1 Max") {
            return Some(32); // M1 Max
        } else if cpu_brand.contains("M1 Ultra") {
            return Some(64); // M1 Ultra
        } else if cpu_brand.contains("M2 ")
            && !cpu_brand.contains("Pro")
            && !cpu_brand.contains("Max")
            && !cpu_brand.contains("Ultra")
        {
            return Some(10); // M2 base
        } else if cpu_brand.contains("M2 Pro") {
            return Some(19); // M2 Pro
        } else if cpu_brand.contains("M2 Max") {
            return Some(38); // M2 Max
        } else if cpu_brand.contains("M2 Ultra") {
            return Some(76); // M2 Ultra
        } else if cpu_brand.contains("M3 ")
            && !cpu_brand.contains("Pro")
            && !cpu_brand.contains("Max")
        {
            return Some(10); // M3 base
        } else if cpu_brand.contains("M3 Pro") {
            return Some(18); // M3 Pro (14-core) or 14 (11-core)
        } else if cpu_brand.contains("M3 Max") {
            return Some(40); // M3 Max
        } else if cpu_brand.contains("M4 Pro") {
            return Some(20); // M4 Pro (estimated based on M3 Pro pattern)
        } else if cpu_brand.contains("M4 Max") {
            return Some(40); // M4 Max (estimated based on M3 Max pattern)
        } else if cpu_brand.contains("M4")
            && !cpu_brand.contains("Pro")
            && !cpu_brand.contains("Max")
        {
            return Some(10); // M4 base
        }
    }

    // Fallback to ioreg only if we can't determine from CPU brand
    let output = Command::new("ioreg")
        .arg("-rc")
        .arg("AGXAccelerator")
        .arg("-d1")
        .output()
        .ok()?;

    if output.status.success() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        // Find the line containing "gpu-core-count"
        for line in output_str.lines() {
            if line.contains("\"gpu-core-count\"") {
                // Split the line into whitespace-separated fields and get the third field
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(core_count) = parts[2].parse::<u32>() {
                        return Some(core_count);
                    }
                }
            }
        }
        None
    } else {
        None
    }
}
