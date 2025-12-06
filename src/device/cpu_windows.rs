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

use crate::device::{CoreType, CoreUtilization, CpuInfo, CpuPlatformType, CpuReader, CpuSocketInfo};
use crate::utils::system::get_hostname;
use chrono::Local;
use std::sync::RwLock;
use sysinfo::{CpuRefreshKind, System};

pub struct WindowsCpuReader {
    system: RwLock<System>,
    first_refresh_done: RwLock<bool>,
}

impl Default for WindowsCpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsCpuReader {
    pub fn new() -> Self {
        let system = System::new();

        Self {
            system: RwLock::new(system),
            first_refresh_done: RwLock::new(false),
        }
    }

    fn get_cpu_info_from_system(&self) -> Result<CpuInfo, Box<dyn std::error::Error>> {
        // On first call, do two refreshes to establish baseline for delta calculation
        if !*self.first_refresh_done.read().unwrap() {
            self.system
                .write()
                .unwrap()
                .refresh_cpu_specifics(CpuRefreshKind::everything());
            std::thread::sleep(std::time::Duration::from_millis(100));
            *self.first_refresh_done.write().unwrap() = true;
        }

        // Regular refresh for current data
        self.system
            .write()
            .unwrap()
            .refresh_cpu_specifics(CpuRefreshKind::everything());

        let hostname = get_hostname();
        let instance = hostname.clone();
        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let system = self.system.read().unwrap();

        // Get CPU information
        let cpus = system.cpus();
        let total_threads = cpus.len() as u32;

        // Get CPU model from the first CPU
        let cpu_model = if !cpus.is_empty() {
            cpus[0].brand().to_string()
        } else {
            "Unknown CPU".to_string()
        };

        // Determine platform type from CPU brand
        let platform_type = if cpu_model.to_lowercase().contains("intel") {
            CpuPlatformType::Intel
        } else if cpu_model.to_lowercase().contains("amd") {
            CpuPlatformType::Amd
        } else if cpu_model.to_lowercase().contains("arm") {
            CpuPlatformType::Arm
        } else {
            CpuPlatformType::Other("Unknown".to_string())
        };

        // Get architecture
        let architecture = if cfg!(target_arch = "x86_64") {
            "x86_64".to_string()
        } else if cfg!(target_arch = "x86") {
            "x86".to_string()
        } else if cfg!(target_arch = "aarch64") {
            "arm64".to_string()
        } else {
            std::env::consts::ARCH.to_string()
        };

        // Get physical core count using sysinfo (static method)
        let total_cores = System::physical_core_count().unwrap_or(total_threads as usize) as u32;

        // Get frequency from the first CPU (in MHz)
        let base_frequency = if !cpus.is_empty() {
            cpus[0].frequency() as u32
        } else {
            0
        };

        // Get overall CPU utilization
        let overall_utilization = system.global_cpu_usage() as f64;

        // Build per-core utilization
        let mut per_core_utilization = Vec::new();
        for (i, cpu) in cpus.iter().enumerate() {
            per_core_utilization.push(CoreUtilization {
                core_id: i as u32,
                core_type: CoreType::Standard,
                utilization: cpu.cpu_usage() as f64,
            });
        }

        // Windows typically has 1 socket for consumer machines
        // For more accurate socket count, we would need WMI
        let socket_count = 1u32;

        // Create per-socket info
        let per_socket_info = vec![CpuSocketInfo {
            socket_id: 0,
            utilization: overall_utilization,
            cores: total_cores,
            threads: total_threads,
            temperature: None, // Temperature requires WMI or specialized tools on Windows
            frequency_mhz: base_frequency,
        }];

        Ok(CpuInfo {
            host_id: hostname.clone(),
            hostname,
            instance,
            cpu_model,
            architecture,
            platform_type,
            socket_count,
            total_cores,
            total_threads,
            base_frequency_mhz: base_frequency,
            max_frequency_mhz: base_frequency, // Max frequency requires WMI
            cache_size_mb: 0,                  // Cache size requires WMI
            utilization: overall_utilization,
            temperature: None,
            power_consumption: None,
            per_socket_info,
            apple_silicon_info: None,
            per_core_utilization,
            time,
        })
    }
}

impl CpuReader for WindowsCpuReader {
    fn get_cpu_info(&self) -> Vec<CpuInfo> {
        match self.get_cpu_info_from_system() {
            Ok(cpu_info) => vec![cpu_info],
            Err(e) => {
                eprintln!("Error reading CPU info: {e}");
                vec![]
            }
        }
    }
}
