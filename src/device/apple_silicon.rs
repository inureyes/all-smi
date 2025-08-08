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

use crate::device::powermetrics_manager::get_powermetrics_manager;
use crate::device::process_list::{get_all_processes, merge_gpu_processes};
use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use sysinfo::System;

pub struct AppleSiliconGpuReader {
    name: String,
    driver_version: Option<String>,
    gpu_core_count: Option<u32>,
}

impl Default for AppleSiliconGpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl AppleSiliconGpuReader {
    pub fn new() -> Self {
        let (name, driver_version) = get_gpu_name_and_version();
        let gpu_core_count = get_gpu_core_count();

        AppleSiliconGpuReader {
            name,
            driver_version,
            gpu_core_count,
        }
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
                .clone()
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
            name: self.name.clone(),
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
            gpu_core_count: self.gpu_core_count,
            detail,
        }]
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // Create a lightweight system instance and only refresh what we need
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
        let mut system = System::new();
        // Refresh processes with user information
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything().with_user(UpdateKind::Always),
        );
        system.refresh_memory();

        // Get GPU processes and PIDs
        let (gpu_processes, gpu_pids) = self.get_gpu_processes();

        // Get all system processes
        let mut all_processes = get_all_processes(&system, &gpu_pids);

        // Merge GPU information into the process list
        merge_gpu_processes(&mut all_processes, gpu_processes);

        all_processes
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
    let output = Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output()
        .expect("Failed to execute system_profiler command");

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
    // Run the ioreg command to get GPU core count
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
