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

use crate::device::common::constants::FURIOSA_HBM3_MEMORY_BYTES;
use crate::device::common::execute_command_default;
use crate::device::common::parsers::{
    parse_device_id, parse_frequency_mhz, parse_memory_mb_to_bytes, parse_power, parse_temperature,
};
use crate::device::types::{GpuInfo, ProcessInfo};
use crate::device::GpuReader;
use crate::utils::get_hostname;
use chrono::Local;
use serde::Deserialize;
use std::collections::HashMap;

// Import furiosa-smi-rs if available on Linux
#[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
use furiosa_smi_rs::{list_devices, Device};

/// Collection method for Furiosa NPU metrics
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum CollectionMethod {
    /// Use furiosa-smi command-line tool
    FuriosaSmi,
    /// Use furiosa-smi-rs crate
    FuriosaSmiRs,
}

/// JSON structures for furiosa-smi outputs
#[derive(Debug, Deserialize)]
struct FuriosaSmiInfoJson {
    index: String,
    arch: String,
    #[allow(dead_code)]
    dev_name: String,
    device_uuid: String,
    device_sn: String,
    firmware: String,
    pert: String,
    temperature: String,
    power: String,
    core_clock: String,
    governor: String,
    pci_bdf: String,
    pci_dev: String,
}

#[derive(Debug, Deserialize)]
struct FuriosaSmiStatusJson {
    index: String,
    #[allow(dead_code)]
    arch: String,
    #[allow(dead_code)]
    device: String,
    #[allow(dead_code)]
    liveness: String,
    #[allow(dead_code)]
    cores: Vec<FuriosaCoreInfo>,
    pe_utilizations: Vec<FuriosaPeUtilization>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FuriosaCoreInfo {
    idx: u32,
    status: String,
}

#[derive(Debug, Deserialize)]
struct FuriosaPeUtilization {
    #[allow(dead_code)]
    pe_core: u32,
    utilization: f64,
}

#[derive(Debug, Deserialize)]
struct FuriosaPsOutputJson {
    npu: String,
    pid: u32,
    cmd: String,
    memory: String,
}

pub struct FuriosaNpuReader {
    collection_method: CollectionMethod,
}

impl Default for FuriosaNpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl FuriosaNpuReader {
    pub fn new() -> Self {
        // Determine which collection method to use
        #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
        let collection_method = CollectionMethod::FuriosaSmiRs;

        #[cfg(not(all(target_os = "linux", feature = "furiosa-smi-rs")))]
        let collection_method = CollectionMethod::FuriosaSmi;

        Self { collection_method }
    }

    /// Get NPU info based on collection method
    fn get_npu_info_internal(&self) -> Vec<GpuInfo> {
        match self.collection_method {
            #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
            CollectionMethod::FuriosaSmiRs => self.get_npu_info_rs(),
            _ => self.get_npu_info_cli(),
        }
    }

    /// Get NPU info using furiosa-smi-rs crate
    #[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
    fn get_npu_info_rs(&self) -> Vec<GpuInfo> {
        // Initialize library and list devices
        let devices = match list_devices() {
            Ok(devices) => devices,
            Err(_) => return Vec::new(),
        };

        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let hostname = get_hostname();

        devices
            .iter()
            .filter_map(|device| {
                // Get device information using 2025.3.0 API
                let info = device.device_info().ok()?;

                // Get performance metrics individually
                let utilization = device.core_utilization().ok()?;
                let temperature = device.device_temperature().ok()?;
                let power = device.power_consumption().ok()?;
                let governor = device.governor_profile().ok()?;
                let core_freq = device.core_frequency().ok()?;

                create_gpu_info_from_device_2025(
                    &info,
                    &utilization,
                    &temperature,
                    &power,
                    &governor,
                    &core_freq,
                    &time,
                    &hostname,
                )
            })
            .collect()
    }

    /// Get NPU info using furiosa-smi command
    fn get_npu_info_cli(&self) -> Vec<GpuInfo> {
        // Get device info
        let info_output =
            match execute_command_default("furiosa-smi", &["info", "--output", "json"]) {
                Ok(output) => output,
                Err(_) => return Vec::new(),
            };

        let devices: Vec<FuriosaSmiInfoJson> = match serde_json::from_str(&info_output.stdout) {
            Ok(devices) => devices,
            Err(_) => return Vec::new(),
        };

        // Get status for utilization
        let status_output =
            match execute_command_default("furiosa-smi", &["status", "--output", "json"]) {
                Ok(output) => output,
                Err(_) => return Vec::new(),
            };

        let status_list: Vec<FuriosaSmiStatusJson> =
            serde_json::from_str(&status_output.stdout).unwrap_or_default();

        // Get memory usage
        let ps_output = execute_command_default("furiosa-smi", &["ps", "--output", "json"])
            .map(|o| o.stdout)
            .unwrap_or_default();
        let processes: Vec<FuriosaPsOutputJson> =
            serde_json::from_str(&ps_output).unwrap_or_default();
        let device_memory_usage = calculate_device_memory_usage(&processes);

        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let hostname = get_hostname();

        devices
            .into_iter()
            .filter_map(|device| {
                let status = status_list.iter().find(|s| s.index == device.index);
                create_gpu_info_from_cli(&device, status, &device_memory_usage, &time, &hostname)
            })
            .collect()
    }

    /// Get process info using furiosa-smi ps
    fn get_process_info_internal(&self) -> Vec<ProcessInfo> {
        let output = match execute_command_default("furiosa-smi", &["ps", "--output", "json"]) {
            Ok(output) => output,
            Err(_) => return Vec::new(),
        };

        let processes: Vec<FuriosaPsOutputJson> = match serde_json::from_str(&output.stdout) {
            Ok(procs) => procs,
            Err(_) => return Vec::new(),
        };

        processes
            .into_iter()
            .map(|proc| create_process_info_from_ps(&proc))
            .collect()
    }
}

impl GpuReader for FuriosaNpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        self.get_npu_info_internal()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.get_process_info_internal()
    }
}

// Helper functions

fn calculate_device_memory_usage(processes: &[FuriosaPsOutputJson]) -> HashMap<String, u64> {
    let mut device_memory_usage: HashMap<String, u64> = HashMap::new();

    for proc in processes {
        let memory_bytes = parse_memory_mb_to_bytes(&proc.memory).unwrap_or_else(|| {
            eprintln!(
                "Failed to parse memory for process {}: {}",
                proc.pid, proc.memory
            );
            0
        });

        *device_memory_usage.entry(proc.npu.clone()).or_insert(0) += memory_bytes;
    }

    device_memory_usage
}

fn create_gpu_info_from_cli(
    device: &FuriosaSmiInfoJson,
    status: Option<&FuriosaSmiStatusJson>,
    device_memory_usage: &HashMap<String, u64>,
    time: &str,
    hostname: &str,
) -> Option<GpuInfo> {
    let mut detail = HashMap::new();

    // Add device details
    crate::extract_struct_fields!(detail, device, {
        "serial_number" => device_sn,
        "firmware_version" => firmware,
        "pert_version" => pert,
        "governor" => governor,
        "pci_bdf" => pci_bdf,
        "pci_dev" => pci_dev
    });
    detail.insert("architecture".to_string(), device.arch.to_uppercase());
    detail.insert("core_count".to_string(), "8".to_string());
    detail.insert("pe_count".to_string(), "64K".to_string());
    detail.insert("memory_bandwidth".to_string(), "1.63TB/s".to_string());
    detail.insert("on_chip_sram".to_string(), "256MB".to_string());

    // Add unified AI acceleration library labels
    detail.insert("lib_name".to_string(), "PERT".to_string());
    detail.insert("lib_version".to_string(), device.pert.clone());

    let temperature = parse_temperature(&device.temperature).unwrap_or_else(|| {
        eprintln!("Failed to parse temperature: {}", device.temperature);
        0
    });
    let power = parse_power(&device.power).unwrap_or_else(|| {
        eprintln!("Failed to parse power: {}", device.power);
        0.0
    });
    let frequency = parse_frequency_mhz(&device.core_clock).unwrap_or_else(|| {
        eprintln!("Failed to parse frequency: {}", device.core_clock);
        0
    });

    let utilization = status
        .and_then(|s| {
            s.pe_utilizations
                .iter()
                .map(|pe| pe.utilization)
                .max_by(|a, b| {
                    // Safe comparison handling NaN values
                    match (a.is_nan(), b.is_nan()) {
                        (true, true) => std::cmp::Ordering::Equal,
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        (false, false) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
                    }
                })
        })
        .unwrap_or(0.0);

    let device_name = format!("npu{}", device.index);
    let used_memory = device_memory_usage.get(&device_name).copied().unwrap_or(0);

    Some(GpuInfo {
        uuid: device.device_uuid.clone(),
        time: time.to_string(),
        name: format!("Furiosa {}", device.arch.to_uppercase()),
        device_type: "NPU".to_string(),
        host_id: hostname.to_string(),
        hostname: hostname.to_string(),
        instance: hostname.to_string(),
        utilization,
        ane_utilization: 0.0,
        dla_utilization: None,
        temperature,
        used_memory,
        total_memory: FURIOSA_HBM3_MEMORY_BYTES,
        frequency,
        power_consumption: power,
        gpu_core_count: None,
        detail,
    })
}

#[cfg(all(target_os = "linux", feature = "furiosa-smi-rs"))]
fn create_gpu_info_from_device_2025(
    info: &furiosa_smi_rs::DeviceInfo,
    utilization: &furiosa_smi_rs::CoreUtilization,
    temperature: &furiosa_smi_rs::DeviceTemperature,
    power: &f64,
    governor: &furiosa_smi_rs::GovernorProfile,
    core_freq: &furiosa_smi_rs::CoreFrequency,
    time: &str,
    hostname: &str,
) -> Option<GpuInfo> {
    let mut detail = HashMap::new();

    // Add device details from DeviceInfo using 2025.3.0 API methods
    detail.insert("serial_number".to_string(), info.serial());
    detail.insert(
        "firmware_version".to_string(),
        info.firmware_version().to_string(),
    );
    detail.insert("architecture".to_string(), format!("{:?}", info.arch()));
    detail.insert("core_count".to_string(), info.core_num().to_string());
    detail.insert("bdf".to_string(), info.bdf());
    detail.insert("numa_node".to_string(), info.numa_node().to_string());

    // Add performance details
    detail.insert("governor".to_string(), format!("{:?}", governor));
    detail.insert(
        "frequency".to_string(),
        format!("{}MHz", core_freq.0), // CoreFrequency is a tuple struct
    );

    // Add unified AI acceleration library labels using PERT version
    detail.insert("lib_name".to_string(), "PERT".to_string());
    detail.insert("lib_version".to_string(), info.pert_version().to_string());

    // Calculate average PE utilization from core utilization
    let avg_util = if !utilization.pe_utilizations.is_empty() {
        let sum: f64 = utilization
            .pe_utilizations
            .iter()
            .map(|pe| pe.utilization as f64)
            .sum();
        sum / utilization.pe_utilizations.len() as f64
    } else {
        0.0
    };

    // TODO: Get memory info - not directly available in 2025.3.0 API
    let (used_memory, total_memory) = (0u64, FURIOSA_HBM3_MEMORY_BYTES);

    Some(GpuInfo {
        uuid: info.uuid(),
        time: time.to_string(),
        name: format!("Furiosa {:?}", info.arch()),
        device_type: "NPU".to_string(),
        host_id: hostname.to_string(),
        hostname: hostname.to_string(),
        instance: hostname.to_string(),
        utilization: avg_util,
        ane_utilization: 0.0,
        dla_utilization: None,
        temperature: temperature.0 as u32, // DeviceTemperature is a tuple struct
        used_memory,
        total_memory,
        frequency: core_freq.0,
        power_consumption: *power,
        gpu_core_count: Some(info.core_num()),
        detail,
    })
}

fn create_process_info_from_ps(proc: &FuriosaPsOutputJson) -> ProcessInfo {
    let device_id = parse_device_id(&proc.npu).unwrap_or_else(|| {
        eprintln!("Failed to parse device ID: {}", proc.npu);
        0
    });
    let used_memory = parse_memory_mb_to_bytes(&proc.memory).unwrap_or_else(|| {
        eprintln!(
            "Failed to parse memory for process {}: {}",
            proc.pid, proc.memory
        );
        0
    });

    ProcessInfo {
        device_id,
        device_uuid: proc.npu.clone(),
        pid: proc.pid,
        process_name: extract_process_name(&proc.cmd),
        used_memory,
        cpu_percent: 0.0,
        memory_percent: 0.0,
        memory_rss: 0,
        memory_vms: 0,
        user: String::new(),
        state: String::new(),
        start_time: String::new(),
        cpu_time: 0,
        command: proc.cmd.clone(),
        ppid: 0,
        threads: 0,
        uses_gpu: true,
        priority: 0,
        nice_value: 0,
        gpu_utilization: 0.0,
    }
}

fn extract_process_name(cmd: &str) -> String {
    cmd.split_whitespace()
        .next()
        .and_then(|path| path.split('/').next_back())
        .unwrap_or("unknown")
        .to_string()
}
