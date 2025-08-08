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
use crate::utils::{get_hostname, run_command_fast_fail};
use chrono::Local;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::{cuda_driver_version_major, cuda_driver_version_minor, Nvml};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use sysinfo::System;

// Global status for NVML error messages
static NVML_STATUS: Mutex<Option<String>> = Mutex::new(None);

pub struct NvidiaGpuReader;

impl GpuReader for NvidiaGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        // Try NVML first
        match Nvml::init() {
            Ok(nvml) => {
                // Clear any previous error status on success
                if let Ok(mut status) = NVML_STATUS.lock() {
                    *status = None;
                }
                self.get_gpu_info_nvml(&nvml)
            }
            Err(e) => {
                // Store the error status for notification
                set_nvml_status(e);
                self.get_gpu_info_nvidia_smi()
            }
        }
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

impl NvidiaGpuReader {
    /// Get GPU processes using NVML or nvidia-smi
    fn get_gpu_processes(&self) -> (Vec<ProcessInfo>, HashSet<u32>) {
        // Try NVML first
        match Nvml::init() {
            Ok(nvml) => self.get_gpu_processes_nvml(&nvml),
            Err(e) => {
                // Store the error status for notification
                set_nvml_status(e);
                self.get_gpu_processes_nvidia_smi()
            }
        }
    }

    /// Get GPU processes using NVML
    fn get_gpu_processes_nvml(&self, nvml: &Nvml) -> (Vec<ProcessInfo>, HashSet<u32>) {
        let mut gpu_processes = Vec::new();
        let mut gpu_pids = HashSet::new();

        if let Ok(device_count) = nvml.device_count() {
            for device_index in 0..device_count {
                if let Ok(device) = nvml.device_by_index(device_index) {
                    let device_uuid = device
                        .uuid()
                        .unwrap_or_else(|_| format!("GPU-{device_index}"));

                    // Get running compute processes
                    if let Ok(compute_procs) = device.running_compute_processes() {
                        for proc in compute_procs {
                            gpu_pids.insert(proc.pid);

                            gpu_processes.push(ProcessInfo {
                                device_id: device_index as usize,
                                device_uuid: device_uuid.clone(),
                                pid: proc.pid,
                                process_name: String::new(), // Will be filled by sysinfo
                                used_memory: match proc.used_gpu_memory {
                                    UsedGpuMemory::Used(bytes) => bytes,
                                    _ => 0,
                                },
                                cpu_percent: 0.0,     // Will be filled by sysinfo
                                memory_percent: 0.0,  // Will be filled by sysinfo
                                memory_rss: 0,        // Will be filled by sysinfo
                                memory_vms: 0,        // Will be filled by sysinfo
                                user: String::new(),  // Will be filled by sysinfo
                                state: String::new(), // Will be filled by sysinfo
                                start_time: String::new(), // Will be filled by sysinfo
                                cpu_time: 0,          // Will be filled by sysinfo
                                command: String::new(), // Will be filled by sysinfo
                                ppid: 0,              // Will be filled by sysinfo
                                threads: 0,           // Will be filled by sysinfo
                                uses_gpu: true,
                                priority: 0,          // Will be filled by sysinfo
                                nice_value: 0,        // Will be filled by sysinfo
                                gpu_utilization: 0.0, // NVML doesn't provide per-process GPU utilization
                            });
                        }
                    }

                    // Get graphics processes
                    if let Ok(graphics_procs) = device.running_graphics_processes() {
                        for proc in graphics_procs {
                            if !gpu_pids.contains(&proc.pid) {
                                gpu_pids.insert(proc.pid);

                                gpu_processes.push(ProcessInfo {
                                    device_id: device_index as usize,
                                    device_uuid: device_uuid.clone(),
                                    pid: proc.pid,
                                    process_name: String::new(), // Will be filled by sysinfo
                                    used_memory: match proc.used_gpu_memory {
                                        UsedGpuMemory::Used(bytes) => bytes,
                                        _ => 0,
                                    },
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
                                    gpu_utilization: 0.0, // NVML doesn't provide per-process GPU utilization
                                });
                            }
                        }
                    }
                }
            }
        }

        (gpu_processes, gpu_pids)
    }

    /// Get GPU processes using nvidia-smi
    fn get_gpu_processes_nvidia_smi(&self) -> (Vec<ProcessInfo>, HashSet<u32>) {
        let mut gpu_processes = Vec::new();
        let mut gpu_pids = HashSet::new();

        let output = run_command_fast_fail(
            "nvidia-smi",
            &[
                "--query-compute-apps=pid,used_memory",
                "--format=csv,noheader,nounits",
            ],
        );

        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                for line in output_str.lines() {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        if let Ok(pid) = parts[0].trim().parse::<u32>() {
                            if let Ok(used_memory_mb) = parts[1].trim().parse::<u64>() {
                                gpu_pids.insert(pid);

                                gpu_processes.push(ProcessInfo {
                                    device_id: 0, // Can't determine from nvidia-smi output
                                    device_uuid: "GPU".to_string(),
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
                                    gpu_utilization: 0.0, // NVIDIA doesn't provide per-process GPU utilization
                                });
                            }
                        }
                    }
                }
            }
        }

        (gpu_processes, gpu_pids)
    }

    // Implementation of get_gpu_info_nvml (same as original)
    fn get_gpu_info_nvml(&self, nvml: &Nvml) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        // Get CUDA version
        let cuda_version = format!(
            "{}.{}",
            cuda_driver_version_major(nvml.sys_cuda_driver_version().unwrap_or(0)),
            cuda_driver_version_minor(nvml.sys_cuda_driver_version().unwrap_or(0))
        );

        // Get driver version
        let driver_version = nvml
            .sys_driver_version()
            .unwrap_or_else(|_| "Unknown".to_string());

        if let Ok(device_count) = nvml.device_count() {
            for i in 0..device_count {
                if let Ok(device) = nvml.device_by_index(i) {
                    let mut detail = HashMap::new();
                    detail.insert("Driver Version".to_string(), driver_version.clone());
                    detail.insert("CUDA Version".to_string(), cuda_version.clone());

                    // Get additional details
                    if let Ok(brand) = device.brand() {
                        detail.insert("Brand".to_string(), format!("{brand:?}"));
                    }
                    if let Ok(arch) = device.architecture() {
                        detail.insert("Architecture".to_string(), format!("{arch:?}"));
                    }
                    if let Ok(pcie_gen) = device.current_pcie_link_gen() {
                        detail.insert("PCIe Generation".to_string(), pcie_gen.to_string());
                    }
                    if let Ok(pcie_width) = device.current_pcie_link_width() {
                        detail.insert("PCIe Width".to_string(), format!("x{pcie_width}"));
                    }

                    // Compute mode
                    if let Ok(compute_mode) = device.compute_mode() {
                        detail.insert("compute_mode".to_string(), format!("{compute_mode:?}"));
                    }

                    // PCIe max information
                    if let Ok(pcie_gen_max) = device.max_pcie_link_gen() {
                        detail.insert("pcie_gen_max".to_string(), pcie_gen_max.to_string());
                    }
                    if let Ok(pcie_width_max) = device.max_pcie_link_width() {
                        detail.insert("pcie_width_max".to_string(), pcie_width_max.to_string());
                    }

                    // Performance state
                    if let Ok(perf_state) = device.performance_state() {
                        detail.insert("performance_state".to_string(), format!("{perf_state:?}"));
                    }

                    // Power limits
                    if let Ok(power_limit) = device.power_management_limit() {
                        detail.insert(
                            "power_limit_current".to_string(),
                            format!("{:.2}", power_limit as f64 / 1000.0),
                        );
                    }
                    if let Ok(power_limit_default) = device.power_management_limit_default() {
                        detail.insert(
                            "power_limit_default".to_string(),
                            format!("{:.2}", power_limit_default as f64 / 1000.0),
                        );
                    }
                    if let Ok(constraints) = device.power_management_limit_constraints() {
                        detail.insert(
                            "power_limit_min".to_string(),
                            format!("{:.2}", constraints.min_limit as f64 / 1000.0),
                        );
                        detail.insert(
                            "power_limit_max".to_string(),
                            format!("{:.2}", constraints.max_limit as f64 / 1000.0),
                        );
                    }

                    // Max clocks - need to import Clock enum
                    use nvml_wrapper::enum_wrappers::device::Clock;
                    if let Ok(max_graphics_clock) = device.max_customer_boost_clock(Clock::Graphics)
                    {
                        detail.insert(
                            "clock_graphics_max".to_string(),
                            max_graphics_clock.to_string(),
                        );
                    }
                    if let Ok(max_memory_clock) = device.max_customer_boost_clock(Clock::Memory) {
                        detail.insert("clock_memory_max".to_string(), max_memory_clock.to_string());
                    }

                    // ECC mode
                    if let Ok(ecc_enabled) = device.is_ecc_enabled() {
                        detail.insert(
                            "ecc_mode_current".to_string(),
                            if ecc_enabled.currently_enabled {
                                "Enabled"
                            } else {
                                "Disabled"
                            }
                            .to_string(),
                        );
                        if ecc_enabled.currently_enabled != ecc_enabled.pending_enabled {
                            detail.insert(
                                "ecc_mode_pending".to_string(),
                                if ecc_enabled.pending_enabled {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }
                                .to_string(),
                            );
                        }
                    }

                    // MIG mode
                    if let Ok(mig_mode) = device.mig_mode() {
                        detail.insert(
                            "mig_mode_current".to_string(),
                            format!("{:?}", mig_mode.current),
                        );
                        if mig_mode.current != mig_mode.pending {
                            detail.insert(
                                "mig_mode_pending".to_string(),
                                format!("{:?}", mig_mode.pending),
                            );
                        }
                    }

                    // VBIOS version
                    if let Ok(vbios) = device.vbios_version() {
                        detail.insert("vbios_version".to_string(), vbios);
                    }

                    let info = GpuInfo {
                        uuid: device.uuid().unwrap_or_else(|_| format!("GPU-{i}")),
                        time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        name: device.name().unwrap_or_else(|_| "Unknown GPU".to_string()),
                        device_type: "GPU".to_string(),
                        host_id: get_hostname(), // For local mode, host_id is just the hostname
                        hostname: get_hostname(), // DNS hostname
                        instance: get_hostname(),
                        utilization: device
                            .utilization_rates()
                            .map(|u| u.gpu as f64)
                            .unwrap_or(0.0),
                        ane_utilization: 0.0,
                        dla_utilization: None,
                        temperature: device
                            .temperature(
                                nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu,
                            )
                            .unwrap_or(0),
                        used_memory: device.memory_info().map(|m| m.used).unwrap_or(0),
                        total_memory: device.memory_info().map(|m| m.total).unwrap_or(0),
                        frequency: device
                            .clock(
                                nvml_wrapper::enum_wrappers::device::Clock::Graphics,
                                nvml_wrapper::enum_wrappers::device::ClockId::Current,
                            )
                            .unwrap_or(0),
                        power_consumption: device
                            .power_usage()
                            .map(|p| p as f64 / 1000.0)
                            .unwrap_or(0.0),
                        gpu_core_count: None, // NVIDIA doesn't provide core count via NVML
                        detail,
                    };
                    gpu_info.push(info);
                }
            }
        }

        gpu_info
    }

    // Fallback implementation using nvidia-smi
    fn get_gpu_info_nvidia_smi(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        // First, get CUDA version using nvidia-smi without query (appears in header)
        let mut cuda_version = String::new();
        if let Ok(output) = run_command_fast_fail("nvidia-smi", &[]) {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Look for CUDA version in the header
                for line in output_str.lines() {
                    if line.contains("CUDA Version:") {
                        if let Some(version_part) = line.split("CUDA Version:").nth(1) {
                            cuda_version = version_part
                                .split_whitespace()
                                .next()
                                .unwrap_or("Unknown")
                                .to_string();
                        }
                        break;
                    }
                }
            }
        }

        let output = run_command_fast_fail(
            "nvidia-smi",
            &[
                "--query-gpu=index,uuid,name,utilization.gpu,memory.used,memory.total,temperature.gpu,clocks.current.graphics,power.draw,driver_version",
                "--format=csv,noheader,nounits"
            ],
        );

        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                for line in output_str.lines() {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 10 {
                        let index = parts[0].trim().parse::<u32>().unwrap_or(0);
                        let uuid = parts[1].trim().to_string();
                        let name = parts[2].trim().to_string();
                        let utilization = parts[3].trim().parse::<f64>().unwrap_or(0.0);
                        let used_memory_mb = parts[4].trim().parse::<u64>().unwrap_or(0);
                        let total_memory_mb = parts[5].trim().parse::<u64>().unwrap_or(0);
                        let temperature = parts[6].trim().parse::<u32>().unwrap_or(0);
                        let frequency = parts[7].trim().parse::<u32>().unwrap_or(0);
                        let power_draw = parts[8].trim().parse::<f64>().unwrap_or(0.0);
                        let driver_version = parts[9].trim().to_string();

                        let mut detail = HashMap::new();
                        detail.insert("Driver Version".to_string(), driver_version);
                        if !cuda_version.is_empty() {
                            detail.insert("CUDA Version".to_string(), cuda_version.clone());
                        }

                        let info = GpuInfo {
                            uuid: if uuid.is_empty() {
                                format!("GPU-{index}")
                            } else {
                                uuid
                            },
                            time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                            name,
                            device_type: "GPU".to_string(),
                            host_id: get_hostname(), // For local mode, host_id is just the hostname
                            hostname: get_hostname(), // DNS hostname
                            instance: get_hostname(),
                            utilization,
                            ane_utilization: 0.0,
                            dla_utilization: None,
                            temperature,
                            used_memory: used_memory_mb * 1024 * 1024, // Convert to bytes
                            total_memory: total_memory_mb * 1024 * 1024, // Convert to bytes
                            frequency,
                            power_consumption: power_draw,
                            gpu_core_count: None, // NVIDIA doesn't provide core count via nvidia-smi
                            detail,
                        };
                        gpu_info.push(info);
                    }
                }
            }
        }

        gpu_info
    }
}

/// Get a user-friendly message about NVML status
pub fn get_nvml_status_message() -> Option<String> {
    // Only return the stored status, don't try to initialize NVML here
    if let Ok(status) = NVML_STATUS.lock() {
        status.clone()
    } else {
        None
    }
}

/// Store NVML error status
fn set_nvml_status(error: NvmlError) {
    let message = match error {
        NvmlError::LibloadingError(_) => "NVML unavailable - using nvidia-smi fallback".to_string(),
        NvmlError::DriverNotLoaded => "NVIDIA driver not loaded".to_string(),
        NvmlError::NoPermission => "Insufficient permissions for NVML".to_string(),
        _ => "NVML unavailable - using nvidia-smi fallback".to_string(),
    };

    if let Ok(mut status) = NVML_STATUS.lock() {
        *status = Some(message);
    }
}
