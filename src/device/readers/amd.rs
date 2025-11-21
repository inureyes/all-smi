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

use crate::device::types::{GpuInfo, ProcessInfo};
use crate::device::GpuReader;
use crate::utils::get_hostname;
use chrono::Local;
use libamdgpu_top::stat::{self, FdInfoStat, ProcInfo};
use libamdgpu_top::AMDGPU::{DeviceHandle, GpuMetrics, MetricsInfo, GPU_INFO};
use libamdgpu_top::{AppDeviceInfo, DevicePath, VramUsage};
use std::collections::HashMap;
use std::sync::Mutex;

// GPU metric validation constants
const MAX_GPU_UTILIZATION: f64 = 100.0; // Maximum utilization percentage
const MAX_GPU_POWER_WATTS: f64 = 1000.0; // Maximum power consumption in watts
const MAX_GPU_TEMP_CELSIUS: u32 = 125; // Maximum temperature in Celsius
const MAX_GPU_FREQ_MHZ: u32 = 5000; // Maximum frequency in MHz
const MAX_GPU_MEMORY_BYTES: u64 = 512 * 1024 * 1024 * 1024; // 512GB max memory

/// Per-device state that needs to be cached
///
/// SAFETY: The vram_usage Mutex protects VramUsage which must be updated
/// atomically. We handle mutex poisoning by recreating the VramUsage from
/// fresh memory_info if a thread panics while holding the lock.
struct AmdGpuDevice {
    device_path: DevicePath,
    device_handle: DeviceHandle,
    vram_usage: Mutex<VramUsage>, // Protected by mutex for thread-safe updates
}

pub struct AmdGpuReader {
    devices: Vec<AmdGpuDevice>,
}

impl Default for AmdGpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl AmdGpuReader {
    pub fn new() -> Self {
        // Check if we have permission to access AMD GPU devices
        // This prevents panic from libamdgpu_top when running without sudo
        // If no permission, silently return empty device list
        // The main program will handle showing sudo message before TUI starts
        if !Self::check_amd_gpu_permissions() {
            return Self {
                devices: Vec::new(),
            };
        }

        let device_path_list = DevicePath::get_device_path_list();
        let mut devices = Vec::new();

        for device_path in device_path_list {
            match device_path.init() {
                Ok(amdgpu_dev) => {
                    // Get initial memory_info to create VramUsage
                    match amdgpu_dev.memory_info() {
                        Ok(memory_info) => {
                            let vram_usage = VramUsage::new(&memory_info);
                            devices.push(AmdGpuDevice {
                                device_path: device_path.clone(),
                                device_handle: amdgpu_dev,
                                vram_usage: Mutex::new(vram_usage),
                            });
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: Failed to get memory info for AMD GPU {}: {e}",
                                device_path.pci
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to initialize AMD GPU {}: {e}",
                        device_path.pci
                    );
                }
            }
        }

        Self { devices }
    }

    /// Check if we have permission to access AMD GPU devices
    /// Returns false if /dev/dri devices are not accessible
    fn check_amd_gpu_permissions() -> bool {
        use std::fs;

        // Check if /dev/dri directory exists and is accessible
        let dri_path = std::path::Path::new("/dev/dri");
        if !dri_path.exists() {
            return false;
        }

        // Try to read the directory to check permissions
        match fs::read_dir(dri_path) {
            Ok(entries) => {
                // Check if we can read at least one render or card device
                for entry in entries.flatten() {
                    let path = entry.path();
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    // Check card or render devices
                    if file_name.starts_with("card") || file_name.starts_with("render") {
                        // Check if we have read/write permissions
                        // For root: always has access
                        if unsafe { libc::geteuid() } == 0 {
                            return true; // Root always has access
                        }

                        // For non-root, check if we can actually open the device
                        if let Ok(_file) = fs::OpenOptions::new().read(true).write(true).open(&path)
                        {
                            return true; // We have access
                        }
                    }
                }
                false // No accessible devices found
            }
            Err(_) => false, // Cannot read /dev/dri directory
        }
    }
}

impl GpuReader for AmdGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        for device in &self.devices {
            // Get device info with error handling
            let ext_info = match device.device_handle.device_info() {
                Ok(info) => info,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to get device info for AMD GPU {}: {e}",
                        device.device_path.pci
                    );
                    continue; // Skip this GPU if we can't get device info
                }
            };

            // Update the VramUsage from the driver (following libamdgpu-top pattern)
            // SAFETY: We handle mutex poisoning by recreating the VramUsage from fresh memory_info
            let memory_info = {
                let vram_usage_result = device.vram_usage.lock();

                match vram_usage_result {
                    Ok(mut vram_usage) => {
                        // Normal path: update and read
                        vram_usage.update_usage(&device.device_handle);
                        vram_usage.update_usable_heap_size(&device.device_handle);
                        vram_usage.0 // VramUsage is a tuple struct wrapping drm_amdgpu_memory_info
                    }
                    Err(poisoned) => {
                        // Mutex was poisoned - recover by getting fresh memory info
                        // This prevents denial of service from panics in other threads
                        eprintln!(
                            "Warning: VramUsage mutex was poisoned for device {}, recovering...",
                            device.device_path.pci
                        );

                        // Try to get fresh memory info from the device
                        match device.device_handle.memory_info() {
                            Ok(fresh_memory_info) => {
                                // Clear the poison and update with fresh data
                                let mut guard = poisoned.into_inner();
                                *guard = VramUsage::new(&fresh_memory_info);
                                guard.update_usage(&device.device_handle);
                                guard.update_usable_heap_size(&device.device_handle);
                                guard.0
                            }
                            Err(e) => {
                                eprintln!("Failed to recover from poisoned mutex: {e}");
                                continue; // Skip this GPU if we can't recover
                            }
                        }
                    }
                }
            };

            let sensors = libamdgpu_top::stat::Sensors::new(
                &device.device_handle,
                &device.device_path.pci,
                &ext_info,
            );

            let app_device_info = AppDeviceInfo::new(
                &device.device_handle,
                &ext_info,
                &memory_info,
                &sensors,
                &device.device_path,
            );

            let mut detail = HashMap::new();
            detail.insert(
                "Device Name".to_string(),
                app_device_info.marketing_name.clone(),
            );
            detail.insert("PCI Bus".to_string(), app_device_info.pci_bus.to_string());

            if let Some(ver) = libamdgpu_top::get_rocm_version() {
                detail.insert("ROCm Version".to_string(), ver);
            }

            // Add driver version from DRM
            if let Ok(drm) = device.device_handle.get_drm_version_struct() {
                let driver_version = format!(
                    "{}.{}.{}",
                    drm.version_major, drm.version_minor, drm.version_patchlevel
                );
                detail.insert("Driver Version".to_string(), driver_version);
            }

            // Add more details
            detail.insert(
                "Device ID".to_string(),
                format!("{:#06x}", ext_info.device_id()),
            );
            detail.insert(
                "Revision ID".to_string(),
                format!("{:#04x}", ext_info.pci_rev_id()),
            );
            detail.insert(
                "ASIC Name".to_string(),
                app_device_info.asic_name.to_string(),
            );

            if let Some(ref vbios) = app_device_info.vbios {
                detail.insert("VBIOS Version".to_string(), vbios.ver.clone());
                detail.insert("VBIOS Date".to_string(), vbios.date.clone());
            }

            if let Some(ref cap) = app_device_info.power_cap {
                detail.insert("Power Cap".to_string(), format!("{} W", cap.current));
                detail.insert("Power Cap (Min)".to_string(), format!("{} W", cap.min));
                detail.insert("Power Cap (Max)".to_string(), format!("{} W", cap.max));
            }

            if let Some(link) = app_device_info.max_gpu_link {
                detail.insert(
                    "Max GPU Link".to_string(),
                    format!("Gen{} x{}", link.gen, link.width),
                );
            }

            if let Some(link) = app_device_info.max_system_link {
                detail.insert(
                    "Max System Link".to_string(),
                    format!("Gen{} x{}", link.gen, link.width),
                );
            }

            if let Some(min_dpm_link) = app_device_info.min_dpm_link {
                detail.insert(
                    "Min DPM Link".to_string(),
                    format!("Gen{} x{}", min_dpm_link.gen, min_dpm_link.width),
                );
            }

            if let Some(max_dpm_link) = app_device_info.max_dpm_link {
                detail.insert(
                    "Max DPM Link".to_string(),
                    format!("Gen{} x{}", max_dpm_link.gen, max_dpm_link.width),
                );
            }

            if let Some(ref sensors) = sensors {
                if let Some(link) = sensors.current_link {
                    detail.insert(
                        "Current Link".to_string(),
                        format!("Gen{} x{}", link.gen, link.width),
                    );
                }
                if let Some(fan) = sensors.fan_rpm {
                    detail.insert("Fan Speed".to_string(), format!("{fan} RPM"));
                }
                if let Some(mclk) = sensors.mclk {
                    detail.insert("Memory Clock".to_string(), format!("{mclk} MHz"));
                }
            }

            let mut utilization = 0.0;
            let mut power_consumption = 0.0;
            let mut temperature: u32 = 0;
            let mut frequency: u32 = 0;

            // Try to get metrics from GpuMetrics first with validation
            if let Ok(metrics) = GpuMetrics::get_from_sysfs_path(&device.device_path.sysfs_path) {
                if let Some(gfx_activity) = metrics.get_average_gfx_activity() {
                    // Validate utilization is within reasonable bounds
                    utilization = (gfx_activity as f64).clamp(0.0, MAX_GPU_UTILIZATION);
                }
                if let Some(power) = metrics.get_average_socket_power() {
                    // Validate power consumption
                    let watts = power as f64 / 1000.0; // Convert mW to W
                    power_consumption = watts.clamp(0.0, MAX_GPU_POWER_WATTS);
                }
                if let Some(temp) = metrics.get_temperature_edge() {
                    // Validate temperature
                    temperature = (temp as u32).min(MAX_GPU_TEMP_CELSIUS);
                }
                if let Some(freq) = metrics.get_current_gfxclk() {
                    // Validate frequency
                    frequency = (freq as u32).min(MAX_GPU_FREQ_MHZ);
                }
            }

            // Fallback to sensors if metrics failed or missing (with validation)
            if let Some(ref s) = sensors {
                if utilization == 0.0 {
                    // Approximate utilization from load if available, or leave 0
                    // libamdgpu_top doesn't expose a simple "gpu load" sensor easily without GpuMetrics or fdinfo
                }
                if power_consumption == 0.0 {
                    if let Some(ref p) = s.average_power {
                        let watts = p.value as f64 / 1000.0; // Convert mW to W
                        power_consumption = watts.clamp(0.0, MAX_GPU_POWER_WATTS);
                    } else if let Some(ref p) = s.input_power {
                        let watts = p.value as f64 / 1000.0; // Convert mW to W
                        power_consumption = watts.clamp(0.0, MAX_GPU_POWER_WATTS);
                    }
                }
                if temperature == 0 {
                    if let Some(ref t) = s.edge_temp {
                        temperature = (t.current as u32).min(MAX_GPU_TEMP_CELSIUS);
                    }
                }
                if frequency == 0 {
                    if let Some(clk) = s.sclk {
                        frequency = clk.min(MAX_GPU_FREQ_MHZ);
                    }
                }
            }

            // Use memory_info from VramUsage (already updated above)
            // The update_usable_heap_size() call updates total_heap_size from vram_gtt_info()
            // but we do it once per update cycle, not repeated queries

            // Get VRAM size - try multiple sources in order with validation
            // Current max is MI325X with 288GB, but we allow headroom for future models
            let total_memory = if memory_info.vram.total_heap_size > 0 {
                memory_info.vram.total_heap_size.min(MAX_GPU_MEMORY_BYTES)
            } else if memory_info.vram.usable_heap_size > 0 {
                memory_info.vram.usable_heap_size.min(MAX_GPU_MEMORY_BYTES)
            } else {
                0
            };

            // Validate used memory doesn't exceed total
            let used_memory = memory_info.vram.heap_usage.min(total_memory);

            let info = GpuInfo {
                uuid: format!("GPU-{}", device.device_path.pci), // AMD doesn't have UUIDs like NVIDIA, use PCI
                time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                name: app_device_info.marketing_name,
                device_type: "GPU".to_string(),
                host_id: get_hostname(),
                hostname: get_hostname(),
                instance: get_hostname(),
                utilization,
                ane_utilization: 0.0,
                dla_utilization: None,
                temperature,
                used_memory,
                total_memory,
                frequency,
                power_consumption,
                gpu_core_count: None,
                detail,
            };
            gpu_info.push(info);
        }

        gpu_info
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        use std::collections::{HashMap, HashSet};
        use sysinfo::System;

        let mut process_info_list = Vec::new();

        // Get process list once for fdinfo parsing
        let proc_list = stat::get_process_list();

        // Collect all GPU process data in a single pass
        struct GpuProcessData {
            device_id: usize,
            device_uuid: String,
            pid: u32,
            name: String,
            vram_usage_kib: u64,
            gtt_usage_kib: u64,
        }

        let mut gpu_processes = Vec::new();
        let mut gpu_pids = HashSet::new();

        // Single pass: collect all GPU process data
        for (device_idx, device) in self.devices.iter().enumerate() {
            // Build process index for this device
            let mut proc_index: Vec<ProcInfo> = Vec::new();
            stat::update_index_by_all_proc(
                &mut proc_index,
                &[&device.device_path.render, &device.device_path.card],
                &proc_list,
            );

            // Get fdinfo usage for all processes
            let mut fdinfo = FdInfoStat::default();
            fdinfo.get_all_proc_usage(&proc_index);

            // Collect process data
            for proc_usage in fdinfo.proc_usage {
                let vram_usage_kib = proc_usage.usage.vram_usage;
                let gtt_usage_kib = proc_usage.usage.gtt_usage;

                // Include process if it uses VRAM or GTT (GPU memory)
                if vram_usage_kib > 0 || gtt_usage_kib > 0 {
                    let pid = proc_usage.pid as u32;
                    gpu_pids.insert(pid);

                    gpu_processes.push(GpuProcessData {
                        device_id: device_idx,
                        device_uuid: format!("GPU-{}", device.device_path.pci),
                        pid,
                        name: proc_usage.name,
                        vram_usage_kib,
                        gtt_usage_kib,
                    });
                }
            }
        }

        // Get system process information once for all GPU processes
        let mut system = System::new_all();
        system.refresh_all();
        let system_processes = crate::device::process_list::get_all_processes(&system, &gpu_pids);
        let process_map: HashMap<u32, _> = system_processes.iter().map(|p| (p.pid, p)).collect();

        // Build final ProcessInfo list efficiently
        for gpu_proc in gpu_processes {
            // Convert to bytes and prioritize VRAM, fallback to GTT
            let gpu_memory_bytes = if gpu_proc.vram_usage_kib > 0 {
                gpu_proc.vram_usage_kib * 1024
            } else {
                gpu_proc.gtt_usage_kib * 1024
            };

            // Get system process info or use defaults
            let sys_proc = process_map.get(&gpu_proc.pid);

            let process_info = ProcessInfo {
                device_id: gpu_proc.device_id,
                device_uuid: gpu_proc.device_uuid,
                pid: gpu_proc.pid,
                process_name: gpu_proc.name,
                used_memory: gpu_memory_bytes,
                cpu_percent: sys_proc.map(|p| p.cpu_percent).unwrap_or(0.0),
                memory_percent: sys_proc.map(|p| p.memory_percent).unwrap_or(0.0),
                memory_rss: sys_proc.map(|p| p.memory_rss).unwrap_or(0),
                memory_vms: sys_proc.map(|p| p.memory_vms).unwrap_or(0),
                user: sys_proc.map(|p| p.user.clone()).unwrap_or_default(),
                state: sys_proc.map(|p| p.state.clone()).unwrap_or_default(),
                start_time: sys_proc.map(|p| p.start_time.clone()).unwrap_or_default(),
                cpu_time: sys_proc.map(|p| p.cpu_time).unwrap_or(0),
                command: sys_proc.map(|p| p.command.clone()).unwrap_or_default(),
                ppid: sys_proc.map(|p| p.ppid).unwrap_or(0),
                threads: sys_proc.map(|p| p.threads).unwrap_or(0),
                uses_gpu: true,
                priority: sys_proc.map(|p| p.priority).unwrap_or(0),
                nice_value: sys_proc.map(|p| p.nice_value).unwrap_or(0),
                gpu_utilization: 0.0, // fdinfo doesn't directly provide this per-process
            };

            process_info_list.push(process_info);
        }

        process_info_list
    }
}
