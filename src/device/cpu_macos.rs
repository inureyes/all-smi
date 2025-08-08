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
use crate::device::{
    AppleSiliconCpuInfo, CoreType, CoreUtilization, CpuInfo, CpuPlatformType, CpuReader,
    CpuSocketInfo,
};
use crate::utils::system::get_hostname;
use chrono::Local;
use std::cell::RefCell;
use std::process::Command;

type CpuHardwareParseResult = Result<(String, u32, u32, u32, u32, u32), Box<dyn std::error::Error>>;
type IntelCpuInfo = (String, u32, u32, u32, u32, u32);

pub struct MacOsCpuReader {
    is_apple_silicon: bool,
    // Cached hardware info for Apple Silicon
    cached_cpu_model: RefCell<Option<String>>,
    cached_p_core_count: RefCell<Option<u32>>,
    cached_e_core_count: RefCell<Option<u32>>,
    cached_gpu_core_count: RefCell<Option<u32>>,
    cached_p_core_l2_cache_mb: RefCell<Option<u32>>,
    cached_e_core_l2_cache_mb: RefCell<Option<u32>>,
    // Cached hardware info for Intel
    cached_intel_info: RefCell<Option<IntelCpuInfo>>,
}

impl Default for MacOsCpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsCpuReader {
    pub fn new() -> Self {
        let is_apple_silicon = Self::detect_apple_silicon();
        Self {
            is_apple_silicon,
            cached_cpu_model: RefCell::new(None),
            cached_p_core_count: RefCell::new(None),
            cached_e_core_count: RefCell::new(None),
            cached_gpu_core_count: RefCell::new(None),
            cached_p_core_l2_cache_mb: RefCell::new(None),
            cached_e_core_l2_cache_mb: RefCell::new(None),
            cached_intel_info: RefCell::new(None),
        }
    }

    fn detect_apple_silicon() -> bool {
        if let Ok(output) = Command::new("uname").arg("-m").output() {
            let architecture = String::from_utf8_lossy(&output.stdout);
            return architecture.trim() == "arm64";
        }
        false
    }

    fn get_cpu_info_from_system(&self) -> Result<CpuInfo, Box<dyn std::error::Error>> {
        let hostname = get_hostname();
        let instance = hostname.clone();
        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if self.is_apple_silicon {
            self.get_apple_silicon_cpu_info(hostname, instance, time)
        } else {
            self.get_intel_mac_cpu_info(hostname, instance, time)
        }
    }

    fn get_apple_silicon_cpu_info(
        &self,
        hostname: String,
        instance: String,
        time: String,
    ) -> Result<CpuInfo, Box<dyn std::error::Error>> {
        // Get CPU model and core counts using system_profiler
        let output = Command::new("system_profiler")
            .arg("SPHardwareDataType")
            .output()?;

        let hardware_info = String::from_utf8_lossy(&output.stdout);
        let (cpu_model, p_core_count, e_core_count, gpu_core_count) =
            self.parse_apple_silicon_hardware_info(&hardware_info)?;

        // Get CPU utilization using powermetrics
        let cpu_utilization = self.get_cpu_utilization_powermetrics()?;
        let (p_core_utilization, e_core_utilization) = self.get_apple_silicon_core_utilization()?;

        // Get CPU frequency information and per-core data from PowerMetricsManager if available
        let (base_frequency, max_frequency, p_cluster_freq, e_cluster_freq, per_core_utilization) =
            if let Some(manager) = get_powermetrics_manager() {
                if let Ok(data) = manager.get_latest_data_result() {
                    // Use actual frequencies from powermetrics
                    let avg_freq = (data.p_cluster_frequency + data.e_cluster_frequency) / 2;

                    // Convert per-core data
                    let mut cores = Vec::new();
                    for (i, residency) in data.core_active_residencies.iter().enumerate() {
                        let core_type = if i < data.core_cluster_types.len() {
                            match data.core_cluster_types[i] {
                                crate::device::powermetrics_parser::CoreType::Performance => {
                                    CoreType::Performance
                                }
                                crate::device::powermetrics_parser::CoreType::Efficiency => {
                                    CoreType::Efficiency
                                }
                            }
                        } else {
                            CoreType::Standard
                        };

                        cores.push(CoreUtilization {
                            core_id: i as u32,
                            core_type,
                            utilization: *residency,
                        });
                    }

                    (
                        avg_freq,
                        data.p_cluster_frequency, // P-cluster frequency as max
                        Some(data.p_cluster_frequency),
                        Some(data.e_cluster_frequency),
                        cores,
                    )
                } else {
                    (
                        self.get_cpu_base_frequency()?,
                        self.get_cpu_max_frequency()?,
                        None,
                        None,
                        Vec::new(),
                    )
                }
            } else {
                (
                    self.get_cpu_base_frequency()?,
                    self.get_cpu_max_frequency()?,
                    None,
                    None,
                    Vec::new(),
                )
            };

        // Get CPU temperature (may not be available)
        let temperature = self.get_cpu_temperature();

        // Power consumption from powermetrics
        let power_consumption = self.get_cpu_power_consumption();

        let total_cores = p_core_count + e_core_count;
        let total_threads = total_cores; // Apple Silicon doesn't use hyperthreading

        // Get cache sizes for P and E cores
        let p_core_l2_cache_mb = self.get_p_core_l2_cache_size().ok();
        let e_core_l2_cache_mb = self.get_e_core_l2_cache_size().ok();

        let apple_silicon_info = Some(AppleSiliconCpuInfo {
            p_core_count,
            e_core_count,
            gpu_core_count,
            p_core_utilization,
            e_core_utilization,
            ane_ops_per_second: None, // ANE metrics are complex to get
            p_cluster_frequency_mhz: p_cluster_freq,
            e_cluster_frequency_mhz: e_cluster_freq,
            p_core_l2_cache_mb,
            e_core_l2_cache_mb,
        });

        // Create per-socket info (Apple Silicon typically has 1 socket)
        let per_socket_info = vec![CpuSocketInfo {
            socket_id: 0,
            utilization: cpu_utilization,
            cores: total_cores,
            threads: total_threads,
            temperature,
            frequency_mhz: base_frequency,
        }];

        Ok(CpuInfo {
            host_id: hostname.clone(), // For local mode, host_id is just the hostname
            hostname,
            instance,
            cpu_model,
            architecture: "arm64".to_string(),
            platform_type: CpuPlatformType::AppleSilicon,
            socket_count: 1,
            total_cores,
            total_threads,
            base_frequency_mhz: base_frequency,
            max_frequency_mhz: max_frequency,
            cache_size_mb: p_core_l2_cache_mb.unwrap_or(0) + e_core_l2_cache_mb.unwrap_or(0), // Total L2 cache
            utilization: cpu_utilization,
            temperature,
            power_consumption,
            per_socket_info,
            apple_silicon_info,
            per_core_utilization,
            time,
        })
    }

    fn get_intel_mac_cpu_info(
        &self,
        hostname: String,
        instance: String,
        time: String,
    ) -> Result<CpuInfo, Box<dyn std::error::Error>> {
        // Get CPU information using system_profiler
        let output = Command::new("system_profiler")
            .arg("SPHardwareDataType")
            .output()?;

        let hardware_info = String::from_utf8_lossy(&output.stdout);
        let (cpu_model, socket_count, total_cores, total_threads, base_frequency, cache_size) =
            self.parse_intel_mac_hardware_info(&hardware_info)?;

        // Get CPU utilization using iostat or top
        let cpu_utilization = self.get_cpu_utilization_iostat()?;

        // Get CPU temperature (may not be available)
        let temperature = self.get_cpu_temperature();

        // Power consumption is not easily available on Intel Macs
        let power_consumption = None;

        // Create per-socket info
        let mut per_socket_info = Vec::new();
        for socket_id in 0..socket_count {
            per_socket_info.push(CpuSocketInfo {
                socket_id,
                utilization: cpu_utilization,
                cores: total_cores / socket_count,
                threads: total_threads / socket_count,
                temperature,
                frequency_mhz: base_frequency,
            });
        }

        Ok(CpuInfo {
            host_id: hostname.clone(), // For local mode, host_id is just the hostname
            hostname,
            instance,
            cpu_model,
            architecture: "x86_64".to_string(),
            platform_type: CpuPlatformType::Intel,
            socket_count,
            total_cores,
            total_threads,
            base_frequency_mhz: base_frequency,
            max_frequency_mhz: base_frequency, // Max frequency not easily available
            cache_size_mb: cache_size,
            utilization: cpu_utilization,
            temperature,
            power_consumption,
            per_socket_info,
            apple_silicon_info: None,
            per_core_utilization: Vec::new(), // Intel Macs don't have easy per-core data
            time,
        })
    }

    fn parse_apple_silicon_hardware_info(
        &self,
        hardware_info: &str,
    ) -> Result<(String, u32, u32, u32), Box<dyn std::error::Error>> {
        // Check if we have cached values
        if let (Some(cpu_model), Some(p_core_count), Some(e_core_count), Some(gpu_core_count)) = (
            self.cached_cpu_model.borrow().clone(),
            *self.cached_p_core_count.borrow(),
            *self.cached_e_core_count.borrow(),
            *self.cached_gpu_core_count.borrow(),
        ) {
            return Ok((cpu_model, p_core_count, e_core_count, gpu_core_count));
        }

        let mut cpu_model = String::new();

        // Extract CPU model from system_profiler output
        for line in hardware_info.lines() {
            let line = line.trim();
            if line.starts_with("Chip:") {
                cpu_model = line.split(':').nth(1).unwrap_or("").trim().to_string();
                break;
            }
        }

        // Get both P and E core counts in a single sysctl call for better performance
        let output = Command::new("sysctl")
            .args(["hw.perflevel0.physicalcpu", "hw.perflevel1.physicalcpu"])
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut p_core_count = 0u32;
        let mut e_core_count = 0u32;

        for line in output_str.lines() {
            if line.starts_with("hw.perflevel0.physicalcpu:") {
                if let Some(value) = line.split(':').nth(1) {
                    p_core_count = value.trim().parse().unwrap_or(0);
                }
            } else if line.starts_with("hw.perflevel1.physicalcpu:") {
                if let Some(value) = line.split(':').nth(1) {
                    e_core_count = value.trim().parse().unwrap_or(0);
                }
            }
        }

        // Get GPU core count separately (still needed)
        let gpu_core_count = self.get_gpu_core_count().unwrap_or(0);

        // Validate we got valid counts
        if p_core_count == 0 || e_core_count == 0 {
            return Err("Failed to get core counts".into());
        }

        // Cache the values
        *self.cached_cpu_model.borrow_mut() = Some(cpu_model.clone());
        *self.cached_p_core_count.borrow_mut() = Some(p_core_count);
        *self.cached_e_core_count.borrow_mut() = Some(e_core_count);
        *self.cached_gpu_core_count.borrow_mut() = Some(gpu_core_count);

        Ok((cpu_model, p_core_count, e_core_count, gpu_core_count))
    }

    // These methods are no longer used since we fetch both values in a single sysctl call
    #[allow(dead_code)]
    fn get_p_core_count(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let output = Command::new("sysctl")
            .arg("hw.perflevel0.physicalcpu")
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(value_str) = output_str.split(':').nth(1) {
            let count = value_str.trim().parse::<u32>()?;
            Ok(count)
        } else {
            Err("Failed to parse P-core count".into())
        }
    }

    #[allow(dead_code)]
    fn get_e_core_count(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let output = Command::new("sysctl")
            .arg("hw.perflevel1.physicalcpu")
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(value_str) = output_str.split(':').nth(1) {
            let count = value_str.trim().parse::<u32>()?;
            Ok(count)
        } else {
            Err("Failed to parse E-core count".into())
        }
    }

    fn get_gpu_core_count(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let output = Command::new("system_profiler")
            .arg("SPDisplaysDataType")
            .arg("-json")
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Parse JSON to find GPU core count
        // Look for "sppci_cores" field in the JSON output
        for line in output_str.lines() {
            if line.contains("sppci_cores") {
                // Extract the value between quotes after the colon
                if let Some(value_part) = line.split(':').nth(1) {
                    if let Some(start_quote) = value_part.find('"') {
                        if let Some(end_quote) = value_part[start_quote + 1..].find('"') {
                            let core_str =
                                &value_part[start_quote + 1..start_quote + 1 + end_quote];
                            if let Ok(count) = core_str.parse::<u32>() {
                                return Ok(count);
                            }
                        }
                    }
                }
            }
        }

        Err("Failed to parse GPU core count".into())
    }

    fn get_p_core_l2_cache_size(&self) -> Result<u32, Box<dyn std::error::Error>> {
        // Check if we have cached value
        if let Some(cached) = *self.cached_p_core_l2_cache_mb.borrow() {
            return Ok(cached);
        }

        let output = Command::new("sysctl")
            .arg("hw.perflevel0.l2cachesize")
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(value_str) = output_str.split(':').nth(1) {
            let cache_bytes = value_str.trim().parse::<u64>()?;
            let cache_mb = (cache_bytes / 1024 / 1024) as u32; // Convert bytes to MB

            // Cache the value
            *self.cached_p_core_l2_cache_mb.borrow_mut() = Some(cache_mb);
            Ok(cache_mb)
        } else {
            Err("Failed to parse P-core L2 cache size".into())
        }
    }

    fn get_e_core_l2_cache_size(&self) -> Result<u32, Box<dyn std::error::Error>> {
        // Check if we have cached value
        if let Some(cached) = *self.cached_e_core_l2_cache_mb.borrow() {
            return Ok(cached);
        }

        let output = Command::new("sysctl")
            .arg("hw.perflevel1.l2cachesize")
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Some(value_str) = output_str.split(':').nth(1) {
            let cache_bytes = value_str.trim().parse::<u64>()?;
            let cache_mb = (cache_bytes / 1024 / 1024) as u32; // Convert bytes to MB

            // Cache the value
            *self.cached_e_core_l2_cache_mb.borrow_mut() = Some(cache_mb);
            Ok(cache_mb)
        } else {
            Err("Failed to parse E-core L2 cache size".into())
        }
    }

    fn parse_intel_mac_hardware_info(&self, hardware_info: &str) -> CpuHardwareParseResult {
        // Check if we have cached values
        if let Some(cached_info) = self.cached_intel_info.borrow().clone() {
            return Ok(cached_info);
        }

        let mut cpu_model = String::new();
        let mut socket_count = 1u32;
        let mut total_cores = 0u32;
        let mut total_threads = 0u32;
        let mut base_frequency = 0u32;
        let mut cache_size = 0u32;

        for line in hardware_info.lines() {
            let line = line.trim();
            if line.starts_with("Processor Name:") {
                cpu_model = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Processor Speed:") {
                if let Some(speed_str) = line.split(':').nth(1) {
                    let speed_str = speed_str.trim();
                    if let Some(ghz_str) = speed_str.split_whitespace().next() {
                        if let Ok(ghz) = ghz_str.parse::<f64>() {
                            base_frequency = (ghz * 1000.0) as u32;
                        }
                    }
                }
            } else if line.starts_with("Number of Processors:") {
                if let Some(proc_str) = line.split(':').nth(1) {
                    if let Ok(procs) = proc_str.trim().parse::<u32>() {
                        socket_count = procs;
                    }
                }
            } else if line.starts_with("Total Number of Cores:") {
                if let Some(cores_str) = line.split(':').nth(1) {
                    if let Ok(cores) = cores_str.trim().parse::<u32>() {
                        total_cores = cores;
                        total_threads = cores * 2; // Assume hyperthreading
                    }
                }
            } else if line.starts_with("L3 Cache:") {
                if let Some(cache_str) = line.split(':').nth(1) {
                    let cache_str = cache_str.trim();
                    if let Some(size_str) = cache_str.split_whitespace().next() {
                        if let Ok(size) = size_str.parse::<u32>() {
                            cache_size = size;
                        }
                    }
                }
            }
        }

        let result = (
            cpu_model,
            socket_count,
            total_cores,
            total_threads,
            base_frequency,
            cache_size,
        );

        // Cache the values
        *self.cached_intel_info.borrow_mut() = Some(result.clone());

        Ok(result)
    }

    fn get_cpu_utilization_powermetrics(&self) -> Result<f64, Box<dyn std::error::Error>> {
        // Try to get data from the PowerMetricsManager first
        if let Some(manager) = get_powermetrics_manager() {
            if let Ok(data) = manager.get_latest_data_result() {
                return Ok(data.cpu_utilization());
            }
        }

        // Fallback to iostat if PowerMetricsManager is not available
        self.get_cpu_utilization_iostat()
    }

    fn get_cpu_utilization_iostat(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let output = Command::new("iostat").args(["-c", "1"]).output()?;

        let iostat_output = String::from_utf8_lossy(&output.stdout);

        // Parse CPU utilization from iostat output
        for line in iostat_output.lines() {
            if line.contains("avg-cpu") {
                continue;
            }
            if line
                .trim()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
            {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() >= 6 {
                    // iostat format: %user %nice %system %iowait %steal %idle
                    let idle = fields[5].parse::<f64>().unwrap_or(0.0);
                    return Ok(100.0 - idle);
                }
            }
        }

        Ok(0.0)
    }

    fn get_apple_silicon_core_utilization(&self) -> Result<(f64, f64), Box<dyn std::error::Error>> {
        // Try to get data from the PowerMetricsManager first
        if let Some(manager) = get_powermetrics_manager() {
            if let Ok(data) = manager.get_latest_data_result() {
                return Ok((
                    data.p_cluster_active_residency,
                    data.e_cluster_active_residency,
                ));
            }
        }

        // Return default values if PowerMetricsManager is not available
        Ok((0.0, 0.0))
    }

    fn get_cpu_base_frequency(&self) -> Result<u32, Box<dyn std::error::Error>> {
        if self.is_apple_silicon {
            // Apple Silicon base frequencies are not easily available
            // Return typical values based on chip
            Ok(3000) // 3 GHz as default
        } else {
            // Try to get from system_profiler (already parsed in get_intel_mac_cpu_info)
            Ok(2400) // Default fallback
        }
    }

    fn get_cpu_max_frequency(&self) -> Result<u32, Box<dyn std::error::Error>> {
        if self.is_apple_silicon {
            // Apple Silicon max frequencies vary by core type
            Ok(3500) // Typical P-core max frequency
        } else {
            Ok(3000) // Default for Intel Macs
        }
    }

    fn get_cpu_temperature(&self) -> Option<u32> {
        // Temperature monitoring on macOS requires specialized tools
        // This is a placeholder - actual implementation might use external tools
        None
    }

    fn get_cpu_power_consumption(&self) -> Option<f64> {
        // Try to get data from the PowerMetricsManager first
        if let Some(manager) = get_powermetrics_manager() {
            if let Ok(data) = manager.get_latest_data_result() {
                return Some(data.cpu_power_mw / 1000.0); // Convert mW to W
            }
        }
        None
    }
}

impl CpuReader for MacOsCpuReader {
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
