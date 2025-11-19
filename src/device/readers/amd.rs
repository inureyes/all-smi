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
use libamdgpu_top::AMDGPU::{DeviceHandle, GpuMetrics, MetricsInfo};
use libamdgpu_top::{AppDeviceInfo, DevicePath, VramUsage};
use std::collections::HashMap;

pub struct AmdGpuReader;

impl AmdGpuReader {
    pub fn new() -> Self {
        Self
    }
}

impl GpuReader for AmdGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();
        let device_path_list = DevicePath::get_device_path_list();

        for device_path in device_path_list {
            if let Ok(amdgpu_dev) = device_path.init() {
                let ext_info = amdgpu_dev.device_info().unwrap();
                let memory_info = amdgpu_dev.memory_info().unwrap();
                let sensors = libamdgpu_top::stat::Sensors::new(&amdgpu_dev, &device_path.pci, &ext_info);
                
                let app_device_info = AppDeviceInfo::new(
                    &amdgpu_dev,
                    &ext_info,
                    &memory_info,
                    &sensors,
                    &device_path,
                );

                let mut detail = HashMap::new();
                detail.insert("Device Name".to_string(), app_device_info.marketing_name.clone());
                detail.insert("PCI Bus".to_string(), app_device_info.pci_bus.to_string());
                
                if let Some(ver) = libamdgpu_top::get_rocm_version() {
                     detail.insert("ROCm Version".to_string(), ver);
                }

                // Add more details
                detail.insert("Device ID".to_string(), format!("{:#06x}", ext_info.device_id()));
                detail.insert("Revision ID".to_string(), format!("{:#04x}", ext_info.pci_rev_id()));
                detail.insert("ASIC Name".to_string(), app_device_info.asic_name.to_string());
                
                if let Some(ref vbios) = app_device_info.vbios {
                    detail.insert("VBIOS Version".to_string(), vbios.version.clone());
                    detail.insert("VBIOS Date".to_string(), vbios.date.clone());
                }

                if let Some(ref cap) = app_device_info.power_cap {
                    detail.insert("Power Cap".to_string(), format!("{} W", cap.current));
                    detail.insert("Power Cap (Min)".to_string(), format!("{} W", cap.min));
                    detail.insert("Power Cap (Max)".to_string(), format!("{} W", cap.max));
                }

                if let Some(link) = app_device_info.max_gpu_link {
                    detail.insert("Max GPU Link".to_string(), format!("Gen{} x{}", link.gen, link.width));
                }
                
                if let Some(link) = app_device_info.max_system_link {
                    detail.insert("Max System Link".to_string(), format!("Gen{} x{}", link.gen, link.width));
                }

                if let Some(min_dpm_link) = app_device_info.min_dpm_link {
                    detail.insert("Min DPM Link".to_string(), format!("Gen{} x{}", min_dpm_link.gen, min_dpm_link.width));
                }

                if let Some(max_dpm_link) = app_device_info.max_dpm_link {
                    detail.insert("Max DPM Link".to_string(), format!("Gen{} x{}", max_dpm_link.gen, max_dpm_link.width));
                }
                
                if let Some(ref sensors) = sensors {
                    if let Some(link) = sensors.current_link {
                        detail.insert("Current Link".to_string(), format!("Gen{} x{}", link.gen, link.width));
                    }
                }

                let mut utilization = 0.0;
                let mut power_consumption = 0.0;
                let mut temperature = 0;
                let mut frequency = 0;

                // Try to get metrics from GpuMetrics first
                if let Ok(metrics) = GpuMetrics::get_from_sysfs_path(&device_path.sysfs_path) {
                     if let Some(gfx_activity) = metrics.get_average_gfx_activity() {
                         utilization = gfx_activity as f64;
                     }
                     if let Some(power) = metrics.get_average_socket_power() {
                         power_consumption = power as f64;
                     }
                     if let Some(temp) = metrics.get_temperature_edge() {
                         temperature = temp as i64;
                     }
                     if let Some(freq) = metrics.get_current_gfxclk() {
                         frequency = freq as u64;
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
                            power_consumption = p.value as f64;
                        } else if let Some(ref p) = s.input_power {
                            power_consumption = p.value as f64;
                        }
                    }
                    if temperature == 0 {
                         if let Some(ref t) = s.edge_temp {
                             temperature = t.current as i64;
                         }
                    }
                    if frequency == 0 {
                        if let Some(clk) = s.sclk {
                            frequency = clk as u64;
                        }
                    }
                }

                let mut vram_usage = VramUsage::new(&memory_info);
                vram_usage.update_usage(&amdgpu_dev);

                let info = GpuInfo {
                    uuid: format!("GPU-{}", device_path.pci), // AMD doesn't have UUIDs like NVIDIA, use PCI
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
                    used_memory: vram_usage.0.vram.heap_usage,
                    total_memory: vram_usage.0.vram.total_heap_size,
                    frequency,
                    power_consumption,
                    gpu_core_count: None,
                    detail,
                };
                gpu_info.push(info);
            }
        }

        gpu_info
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // TODO: Implement process info using fdinfo from libamdgpu_top
        // This requires more complex fdinfo parsing which libamdgpu_top provides
        // For now, return empty list
        Vec::new()
    }
}
