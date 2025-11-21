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

// Per-device state that needs to be cached
struct AmdGpuDevice {
    device_path: DevicePath,
    device_handle: DeviceHandle,
    vram_usage: Mutex<VramUsage>,
}

pub struct AmdGpuReader {
    devices: Vec<AmdGpuDevice>,
}

impl AmdGpuReader {
    pub fn new() -> Self {
        let device_path_list = DevicePath::get_device_path_list();
        let mut devices = Vec::new();

        for device_path in device_path_list {
            if let Ok(amdgpu_dev) = device_path.init() {
                // Get initial memory_info to create VramUsage
                if let Ok(memory_info) = amdgpu_dev.memory_info() {
                    let vram_usage = VramUsage::new(&memory_info);
                    devices.push(AmdGpuDevice {
                        device_path,
                        device_handle: amdgpu_dev,
                        vram_usage: Mutex::new(vram_usage),
                    });
                }
            }
        }

        Self { devices }
    }
}

impl GpuReader for AmdGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        for device in &self.devices {
            // Get device info with error handling
            let ext_info = match device.device_handle.device_info() {
                Ok(info) => info,
                Err(_) => continue, // Skip this GPU if we can't get device info
            };

            // Update the VramUsage from the driver (following libamdgpu-top pattern)
            {
                let mut vram_usage = match device.vram_usage.lock() {
                    Ok(guard) => guard,
                    Err(_) => continue, // Skip if mutex is poisoned
                };
                vram_usage.update_usage(&device.device_handle);
                vram_usage.update_usable_heap_size(&device.device_handle);
            }

            // Now read the updated memory info from VramUsage
            let memory_info = {
                let vram_usage = match device.vram_usage.lock() {
                    Ok(guard) => guard,
                    Err(_) => continue,
                };
                vram_usage.0 // VramUsage is a tuple struct wrapping drm_amdgpu_memory_info
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
                    detail.insert("Fan Speed".to_string(), format!("{} RPM", fan));
                }
                if let Some(mclk) = sensors.mclk {
                    detail.insert("Memory Clock".to_string(), format!("{} MHz", mclk));
                }
            }

            let mut utilization = 0.0;
            let mut power_consumption = 0.0;
            let mut temperature: u32 = 0;
            let mut frequency: u32 = 0;

            // Try to get metrics from GpuMetrics first
            if let Ok(metrics) = GpuMetrics::get_from_sysfs_path(&device.device_path.sysfs_path) {
                if let Some(gfx_activity) = metrics.get_average_gfx_activity() {
                    utilization = gfx_activity as f64;
                }
                if let Some(power) = metrics.get_average_socket_power() {
                    power_consumption = power as f64 / 1000.0; // Convert mW to W
                }
                if let Some(temp) = metrics.get_temperature_edge() {
                    temperature = temp as u32;
                }
                if let Some(freq) = metrics.get_current_gfxclk() {
                    frequency = freq as u32;
                }
            }

            // Fallback to sensors if metrics failed or missing
            if let Some(ref s) = sensors {
                if utilization == 0.0 {
                    // Approximate utilization from load if available, or leave 0
                    // libamdgpu_top doesn't expose a simple "gpu load" sensor easily without GpuMetrics or fdinfo
                }
                if power_consumption == 0.0 {
                    if let Some(ref p) = s.average_power {
                        power_consumption = p.value as f64 / 1000.0; // Convert mW to W
                    } else if let Some(ref p) = s.input_power {
                        power_consumption = p.value as f64 / 1000.0; // Convert mW to W
                    }
                }
                if temperature == 0 {
                    if let Some(ref t) = s.edge_temp {
                        temperature = t.current as u32;
                    }
                }
                if frequency == 0 {
                    if let Some(clk) = s.sclk {
                        frequency = clk as u32;
                    }
                }
            }

            // Use memory_info from VramUsage (already updated above)
            // The update_usable_heap_size() call updates total_heap_size from vram_gtt_info()
            // but we do it once per update cycle, not repeated queries

            // Get VRAM size - try multiple sources in order
            let total_memory = if memory_info.vram.total_heap_size > 0 {
                memory_info.vram.total_heap_size
            } else if memory_info.vram.usable_heap_size > 0 {
                memory_info.vram.usable_heap_size
            } else {
                0
            };

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
                used_memory: memory_info.vram.heap_usage,
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
        let mut process_info_list = Vec::new();

        // Get system process list for enriching with system metrics
        let system_processes = crate::device::process_list::get_all_processes();
        let process_map: HashMap<u32, _> = system_processes.iter().map(|p| (p.pid, p)).collect();

        // Get process list for fdinfo parsing
        let proc_list = stat::get_process_list();

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

            // Process each entry
            for proc_usage in fdinfo.proc_usage {
                let vram_usage_kib = proc_usage.usage.vram_usage;
                let gtt_usage_kib = proc_usage.usage.gtt_usage;

                // Include process if it uses VRAM or GTT (GPU memory)
                if vram_usage_kib == 0 && gtt_usage_kib == 0 {
                    continue;
                }

                // Convert to bytes and prioritize VRAM, fallback to GTT
                let gpu_memory_bytes = if vram_usage_kib > 0 {
                    vram_usage_kib * 1024
                } else {
                    gtt_usage_kib * 1024
                };

                // Get system process info or use defaults
                let sys_proc = process_map.get(&(proc_usage.pid as u32));

                let process_info = ProcessInfo {
                    device_id: device_idx,
                    device_uuid: format!("GPU-{}", device.device_path.pci),
                    pid: proc_usage.pid as u32,
                    process_name: proc_usage.name.clone(),
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
        }

        process_info_list
    }
}
