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
use serde::Deserialize;
use std::sync::RwLock;
use sysinfo::{CpuRefreshKind, System};
use wmi::{COMLibrary, WMIConnection};

// WMI structures for thermal zone temperature
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct ThermalZoneTemperature {
    current_temperature: Option<u32>, // Temperature in tenths of Kelvin
}

// WMI structures for processor information
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32Processor {
    max_clock_speed: Option<u32>,
    l2_cache_size: Option<u32>,
    l3_cache_size: Option<u32>,
    #[allow(dead_code)]
    number_of_cores: Option<u32>,
    #[allow(dead_code)]
    number_of_logical_processors: Option<u32>,
}

// Thread-local WMI connections for reuse within the same thread
thread_local! {
    static WMI_CIMV2_CONNECTION: std::cell::RefCell<Option<WMIConnection>> = std::cell::RefCell::new(None);
    static WMI_ROOT_WMI_CONNECTION: std::cell::RefCell<Option<WMIConnection>> = std::cell::RefCell::new(None);
}

/// Helper to get or create CIMV2 connection
fn with_cimv2_connection<T, F: FnOnce(&WMIConnection) -> T>(f: F) -> Option<T> {
    WMI_CIMV2_CONNECTION.with(|cell| {
        let mut conn_ref = cell.borrow_mut();
        if conn_ref.is_none() {
            match COMLibrary::new() {
                Ok(com) => match WMIConnection::new(com) {
                    Ok(wmi_con) => {
                        *conn_ref = Some(wmi_con);
                    }
                    Err(e) => {
                        eprintln!("Failed to create WMI CIMV2 connection: {e}");
                    }
                },
                Err(e) => {
                    eprintln!("Failed to initialize COM library for CIMV2: {e}");
                }
            }
        }
        conn_ref.as_ref().map(f)
    })
}

/// Helper to get or create root\WMI connection
fn with_root_wmi_connection<T, F: FnOnce(&WMIConnection) -> T>(f: F) -> Option<T> {
    WMI_ROOT_WMI_CONNECTION.with(|cell| {
        let mut conn_ref = cell.borrow_mut();
        if conn_ref.is_none() {
            match COMLibrary::new() {
                Ok(com) => match WMIConnection::with_namespace_path("root\\WMI", com) {
                    Ok(wmi_con) => {
                        *conn_ref = Some(wmi_con);
                    }
                    Err(e) => {
                        eprintln!("Failed to create WMI root\\WMI connection: {e}");
                    }
                },
                Err(e) => {
                    eprintln!("Failed to initialize COM library for root\\WMI: {e}");
                }
            }
        }
        conn_ref.as_ref().map(f)
    })
}

pub struct WindowsCpuReader {
    system: RwLock<System>,
    first_refresh_done: RwLock<bool>,
    // Cached WMI data (static info)
    cached_max_frequency: RwLock<Option<u32>>,
    cached_cache_size: RwLock<Option<u32>>,
}

impl Default for WindowsCpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsCpuReader {
    pub fn new() -> Self {
        let mut system = System::new();

        // Perform initial CPU refresh during construction to establish baseline.
        // This moves the 100ms blocking delay from the first get_cpu_info() call
        // to initialization time, preventing UI freezing during runtime queries.
        system.refresh_cpu_specifics(CpuRefreshKind::everything());
        std::thread::sleep(std::time::Duration::from_millis(100));
        system.refresh_cpu_specifics(CpuRefreshKind::everything());

        Self {
            system: RwLock::new(system),
            first_refresh_done: RwLock::new(true), // Already initialized
            cached_max_frequency: RwLock::new(None),
            cached_cache_size: RwLock::new(None),
        }
    }

    /// Get CPU temperature from WMI thermal zones (using thread-local connection)
    fn get_cpu_temperature(&self) -> Option<u32> {
        with_root_wmi_connection(|wmi_con| {
            let results: Result<Vec<ThermalZoneTemperature>, _> = wmi_con
                .raw_query("SELECT CurrentTemperature FROM MSAcpi_ThermalZoneTemperature");

            if let Ok(zones) = results {
                for zone in zones {
                    if let Some(temp_tenths_kelvin) = zone.current_temperature {
                        // Convert from tenths of Kelvin to Celsius
                        // Formula: (K / 10) - 273.15 = C
                        let celsius = (temp_tenths_kelvin as f64 / 10.0) - 273.15;
                        if celsius > 0.0 && celsius < 150.0 {
                            return Some(celsius as u32);
                        }
                    }
                }
            }
            None
        })
        .flatten()
    }

    /// Get static CPU info from WMI (max frequency, cache size) - using thread-local connection
    fn get_wmi_processor_info(&self) -> (Option<u32>, Option<u32>) {
        // Check cache first
        let cached_freq = *self
            .cached_max_frequency
            .read()
            .expect("cached_max_frequency lock poisoned");
        let cached_cache = *self
            .cached_cache_size
            .read()
            .expect("cached_cache_size lock poisoned");

        if cached_freq.is_some() && cached_cache.is_some() {
            return (cached_freq, cached_cache);
        }

        // Query WMI for processor info using thread-local connection
        let result = with_cimv2_connection(|wmi_con| {
            let results: Result<Vec<Win32Processor>, _> = wmi_con
                .raw_query("SELECT MaxClockSpeed, L2CacheSize, L3CacheSize FROM Win32_Processor");

            if let Ok(procs) = results {
                if let Some(proc) = procs.first() {
                    let max_freq = proc.max_clock_speed.unwrap_or(0);
                    // Cache size in KB, convert to MB
                    let l2 = proc.l2_cache_size.unwrap_or(0);
                    let l3 = proc.l3_cache_size.unwrap_or(0);
                    let cache_mb = (l2 + l3) / 1024;

                    return Some((max_freq, cache_mb));
                }
            }
            None
        })
        .flatten();

        if let Some((freq, cache)) = result {
            *self
                .cached_max_frequency
                .write()
                .expect("cached_max_frequency lock poisoned") = Some(freq);
            *self
                .cached_cache_size
                .write()
                .expect("cached_cache_size lock poisoned") = Some(cache);
            (Some(freq), Some(cache))
        } else {
            (None, None)
        }
    }

    fn get_cpu_info_from_system(&self) -> Result<CpuInfo, Box<dyn std::error::Error>> {
        // On first call, do two refreshes to establish baseline for delta calculation
        if !*self
            .first_refresh_done
            .read()
            .expect("first_refresh_done lock poisoned")
        {
            self.system
                .write()
                .expect("system lock poisoned")
                .refresh_cpu_specifics(CpuRefreshKind::everything());
            std::thread::sleep(std::time::Duration::from_millis(100));
            *self
                .first_refresh_done
                .write()
                .expect("first_refresh_done lock poisoned") = true;
        }

        // Regular refresh for current data
        self.system
            .write()
            .expect("system lock poisoned")
            .refresh_cpu_specifics(CpuRefreshKind::everything());

        let hostname = get_hostname();
        let instance = hostname.clone();
        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let system = self.system.read().expect("system lock poisoned");

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
        let socket_count = 1u32;

        // Get CPU temperature from WMI
        let temperature = self.get_cpu_temperature();

        // Get static info from WMI (max frequency, cache size)
        let (wmi_max_freq, wmi_cache_size) = self.get_wmi_processor_info();
        let max_frequency = wmi_max_freq.unwrap_or(base_frequency);
        let cache_size_mb = wmi_cache_size.unwrap_or(0);

        // Create per-socket info
        let per_socket_info = vec![CpuSocketInfo {
            socket_id: 0,
            utilization: overall_utilization,
            cores: total_cores,
            threads: total_threads,
            temperature,
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
            max_frequency_mhz: max_frequency,
            cache_size_mb,
            utilization: overall_utilization,
            temperature,
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
