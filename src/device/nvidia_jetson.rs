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

use crate::device::process_list::{get_all_processes, merge_gpu_processes};
use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::{get_hostname, hz_to_mhz, millicelsius_to_celsius};
use chrono::Local;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::process::Command;
use sysinfo::System;

pub struct NvidiaJetsonGpuReader;

impl GpuReader for NvidiaJetsonGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        let name = fs::read_to_string("/proc/device-tree/model")
            .unwrap_or_else(|_| "NVIDIA Jetson".to_string())
            .trim_end_matches('\0')
            .to_string();

        let utilization = fs::read_to_string("/sys/devices/platform/tegra-soc/gpu.0/load")
            .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0) / 10.0);

        let frequency = fs::read_to_string("/sys/devices/platform/tegra-soc/gpu.0/cur_freq")
            .map_or(0, |s| s.trim().parse::<u64>().map(hz_to_mhz).unwrap_or(0));

        let temperature = fs::read_to_string("/sys/devices/virtual/thermal/thermal_zone0/temp")
            .map_or(0, |s| {
                s.trim()
                    .parse::<u32>()
                    .map(millicelsius_to_celsius)
                    .unwrap_or(0)
            });

        let power_consumption =
            fs::read_to_string("/sys/bus/i2c/drivers/ina3221x/0-0040/iio:device0/in_power0_input")
                .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0) / 1000.0);

        let dla0_util = fs::read_to_string("/sys/kernel/debug/dla_0/load")
            .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0));
        let dla1_util = fs::read_to_string("/sys/kernel/debug/dla_1/load")
            .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0));
        let dla_utilization = if dla0_util > 0.0 || dla1_util > 0.0 {
            Some(dla0_util + dla1_util)
        } else {
            None
        };

        let (total_memory, used_memory) = get_memory_info();

        // Get Jetson-specific information
        let mut detail = HashMap::new();

        // Try to get CUDA version from nvidia-smi if available
        if let Ok(output) = Command::new("nvidia-smi").output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Parse CUDA version from header
                for line in output_str.lines() {
                    if line.contains("CUDA Version:") {
                        if let Some(version_part) = line.split("CUDA Version:").nth(1) {
                            let cuda_version = version_part
                                .split_whitespace()
                                .next()
                                .unwrap_or("Unknown")
                                .to_string();
                            detail.insert("CUDA Version".to_string(), cuda_version);
                        }
                        break;
                    }
                }
            }
        }

        // Get JetPack version if available
        if let Ok(jetpack) = fs::read_to_string("/etc/nv_jetpack_release") {
            let version = jetpack
                .lines()
                .find(|line| line.starts_with("JETPACK_VERSION"))
                .and_then(|line| line.split('=').nth(1))
                .map(|v| v.trim().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            detail.insert("JetPack Version".to_string(), version);
        }

        // Get L4T version
        if let Ok(l4t) = fs::read_to_string("/etc/nv_tegra_release") {
            if let Some(version) = l4t.split_whitespace().nth(1) {
                detail.insert("L4T Version".to_string(), version.to_string());
            }
        }

        detail.insert("GPU Type".to_string(), "Integrated".to_string());
        detail.insert("Architecture".to_string(), "Tegra".to_string());

        let info = GpuInfo {
            uuid: "JetsonGPU".to_string(),
            time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            name,
            device_type: "GPU".to_string(),
            host_id: get_hostname(), // For local mode, host_id is just the hostname
            hostname: get_hostname(), // DNS hostname
            instance: get_hostname(),
            utilization,
            ane_utilization: 0.0,
            dla_utilization,
            temperature,
            used_memory,
            total_memory,
            frequency,
            power_consumption,
            gpu_core_count: None,
            detail,
        };

        gpu_info.push(info);
        gpu_info
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // Create a lightweight system instance and only refresh what we need
        use sysinfo::ProcessesToUpdate;
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
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

impl NvidiaJetsonGpuReader {
    /// Get GPU processes for Jetson
    fn get_gpu_processes(&self) -> (Vec<ProcessInfo>, HashSet<u32>) {
        let mut gpu_processes = Vec::new();
        let mut gpu_pids = HashSet::new();

        // Jetson doesn't have a direct way to query GPU processes
        // We can try nvidia-smi if available (on newer Jetson models)
        if let Ok(output) = Command::new("nvidia-smi")
            .args([
                "--query-compute-apps=pid,used_memory",
                "--format=csv,noheader,nounits",
            ])
            .output()
        {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                for line in output_str.lines() {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        if let Ok(pid) = parts[0].trim().parse::<u32>() {
                            if let Ok(used_memory_mb) = parts[1].trim().parse::<u64>() {
                                gpu_pids.insert(pid);

                                gpu_processes.push(ProcessInfo {
                                    device_id: 0,
                                    device_uuid: "JetsonGPU".to_string(),
                                    pid,
                                    process_name: String::new(), // Will be filled by sysinfo
                                    used_memory: used_memory_mb * 1024 * 1024, // Convert MB to bytes
                                    cpu_percent: 0.0,    // Will be filled by sysinfo
                                    memory_percent: 0.0, // Will be filled by sysinfo
                                    memory_rss: 0,       // Will be filled by sysinfo
                                    memory_vms: 0,       // Will be filled by sysinfo
                                    user: String::new(), // Will be filled by sysinfo
                                    state: String::new(), // Will be filled by sysinfo
                                    start_time: String::new(), // Will be filled by sysinfo
                                    cpu_time: 0,         // Will be filled by sysinfo
                                    command: String::new(), // Will be filled by sysinfo
                                    ppid: 0,             // Will be filled by sysinfo
                                    threads: 0,          // Will be filled by sysinfo
                                    uses_gpu: true,
                                    priority: 0,          // Will be filled by sysinfo
                                    nice_value: 0,        // Will be filled by sysinfo
                                    gpu_utilization: 0.0, // nvidia-smi on Jetson doesn't provide per-process GPU utilization
                                });
                            }
                        }
                    }
                }
            }
        }

        // If nvidia-smi is not available or doesn't return processes,
        // we can look for known GPU-using processes by name
        if gpu_processes.is_empty() {
            // Look for common GPU applications on Jetson
            let gpu_process_names = vec![
                "nvargus-daemon",
                "nvgstcapture",
                "deepstream",
                "tensorrt",
                "cuda",
            ];

            let mut system = System::new();
            system.refresh_memory();
            for (pid, process) in system.processes() {
                let process_name = process.name().to_string_lossy().to_lowercase();
                for gpu_name in &gpu_process_names {
                    if process_name.contains(gpu_name) {
                        let pid_u32 = pid.as_u32();
                        gpu_pids.insert(pid_u32);

                        gpu_processes.push(ProcessInfo {
                            device_id: 0,
                            device_uuid: "JetsonGPU".to_string(),
                            pid: pid_u32,
                            process_name: String::new(), // Will be filled by sysinfo
                            used_memory: 0, // Can't determine GPU memory usage without nvidia-smi
                            cpu_percent: 0.0, // Will be filled by sysinfo
                            memory_percent: 0.0, // Will be filled by sysinfo
                            memory_rss: 0,  // Will be filled by sysinfo
                            memory_vms: 0,  // Will be filled by sysinfo
                            user: String::new(), // Will be filled by sysinfo
                            state: String::new(), // Will be filled by sysinfo
                            start_time: String::new(), // Will be filled by sysinfo
                            cpu_time: 0,    // Will be filled by sysinfo
                            command: String::new(), // Will be filled by sysinfo
                            ppid: 0,        // Will be filled by sysinfo
                            threads: 0,     // Will be filled by sysinfo
                            uses_gpu: true,
                            priority: 0,          // Will be filled by sysinfo
                            nice_value: 0,        // Will be filled by sysinfo
                            gpu_utilization: 0.0, // Can't determine per-process GPU utilization
                        });
                        break;
                    }
                }
            }
        }

        (gpu_processes, gpu_pids)
    }
}

fn get_memory_info() -> (u64, u64) {
    // Try to get GPU memory from tegrastats
    if let Ok(output) = Command::new("tegrastats").arg("--once").output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Parse memory info from tegrastats output
            // Format: RAM 2298/3964MB (lfb 25x4MB) SWAP 0/1982MB (cached 0MB)
            if let Some(ram_part) = output_str.split("RAM ").nth(1) {
                if let Some(ram_info) = ram_part.split("MB").next() {
                    let parts: Vec<&str> = ram_info.split('/').collect();
                    if parts.len() == 2 {
                        let used = parts[0].parse::<u64>().unwrap_or(0) * 1024 * 1024;
                        let total = parts[1].parse::<u64>().unwrap_or(0) * 1024 * 1024;
                        return (total, used);
                    }
                }
            }
        }
    }

    // Fallback to system memory
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        let mut total = 0;
        let mut available = 0;

        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(value) = line.split_whitespace().nth(1) {
                    total = value.parse::<u64>().unwrap_or(0) * 1024; // Convert KB to bytes
                }
            } else if line.starts_with("MemAvailable:") {
                if let Some(value) = line.split_whitespace().nth(1) {
                    available = value.parse::<u64>().unwrap_or(0) * 1024; // Convert KB to bytes
                }
            }
        }

        let used = total.saturating_sub(available);
        (total, used)
    } else {
        (0, 0)
    }
}
