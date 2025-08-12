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

use crate::device::common::execute_command_default;
use crate::device::types::{GpuInfo, ProcessInfo};
use crate::device::GpuReader;
use crate::utils::get_hostname;
use chrono::Local;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// JSON structures for Rebellions device information
#[derive(Debug, Deserialize)]
struct RblnPciInfo {
    #[allow(dead_code)]
    dev: String,
    bus_id: String,
    numa_node: String,
    link_speed: String,
    link_width: String,
}

#[derive(Debug, Deserialize)]
struct RblnMemoryInfo {
    used: String,
    total: String,
}

#[derive(Debug, Deserialize)]
struct RblnDevice {
    #[allow(dead_code)]
    npu: String,
    name: String,
    sid: String,
    uuid: String,
    device: String,
    status: String,
    fw_ver: String,
    pci: RblnPciInfo,
    temperature: String,
    card_power: String,
    pstate: String,
    memory: RblnMemoryInfo,
    util: String,
    board_info: String,
    #[allow(dead_code)]
    location: u32,
}

#[derive(Debug, Deserialize)]
struct RblnResponse {
    #[serde(rename = "KMD_version")]
    kmd_version: String,
    devices: Vec<RblnDevice>,
    #[serde(default)]
    contexts: Vec<RblnContext>,
}

#[derive(Debug, Deserialize)]
struct RblnContext {
    #[allow(dead_code)]
    ctx_id: String,
    npu: String,
    pid: u32,
    cmd: String,
    memory: String,
}

/// Cache for rebellions command path
static RBLN_COMMAND_CACHE: Lazy<Arc<Mutex<Option<(String, PathBuf)>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

pub struct RebellionsNpuReader;

impl Default for RebellionsNpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl RebellionsNpuReader {
    pub fn new() -> Self {
        Self {}
    }

    /// Determine which command to use (rbln-stat or rbln-smi)
    fn get_rebellions_command() -> Option<(String, PathBuf)> {
        // Check cache first
        if let Ok(cache) = RBLN_COMMAND_CACHE.lock() {
            if let Some(ref cached) = *cache {
                return Some(cached.clone());
            }
        }

        // Check specific paths first
        const PATHS: &[&str] = &[
            "/usr/local/bin/rbln-stat",
            "/usr/bin/rbln-stat",
            "/usr/local/bin/rbln-smi",
            "/usr/bin/rbln-smi",
        ];

        for path in PATHS {
            if Path::new(path).exists() {
                let cmd_name = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("rbln-stat")
                    .to_string();
                let result = (cmd_name, PathBuf::from(path));

                // Cache the result
                if let Ok(mut cache) = RBLN_COMMAND_CACHE.lock() {
                    *cache = Some(result.clone());
                }

                return Some(result);
            }
        }

        // Check if commands are available in PATH
        for cmd in &["rbln-stat", "rbln-smi"] {
            if execute_command_default("which", &[cmd])
                .map(|output| output.stdout.contains(cmd))
                .unwrap_or(false)
            {
                let result = (cmd.to_string(), PathBuf::from(cmd));

                // Cache the result
                if let Ok(mut cache) = RBLN_COMMAND_CACHE.lock() {
                    *cache = Some(result.clone());
                }

                return Some(result);
            }
        }

        None
    }

    /// Get NPU info using rbln-stat or rbln-smi
    fn get_npu_info_internal(&self) -> Vec<GpuInfo> {
        let (command, path) = match Self::get_rebellions_command() {
            Some(cmd) => cmd,
            None => return Vec::new(),
        };

        let output = match execute_command_default(path.to_str().unwrap(), &["--json"]) {
            Ok(output) => output,
            Err(_) => return Vec::new(),
        };

        let response: RblnResponse = match serde_json::from_str(&output.stdout) {
            Ok(resp) => resp,
            Err(_) => return Vec::new(),
        };

        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let hostname = get_hostname();

        response
            .devices
            .into_iter()
            .filter_map(|device| {
                create_gpu_info_from_device(device, &response.kmd_version, &time, &hostname)
            })
            .collect()
    }

    /// Get process info from rbln-stat/rbln-smi
    fn get_process_info_internal(&self) -> Vec<ProcessInfo> {
        let (_command, path) = match Self::get_rebellions_command() {
            Some(cmd) => cmd,
            None => return Vec::new(),
        };

        let output = match execute_command_default(path.to_str().unwrap(), &["--json"]) {
            Ok(output) => output,
            Err(_) => return Vec::new(),
        };

        let response: RblnResponse = match serde_json::from_str(&output.stdout) {
            Ok(resp) => resp,
            Err(_) => return Vec::new(),
        };

        response
            .contexts
            .into_iter()
            .map(create_process_info_from_context)
            .collect()
    }
}

impl GpuReader for RebellionsNpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        self.get_npu_info_internal()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.get_process_info_internal()
    }
}

// Helper functions

fn create_gpu_info_from_device(
    device: RblnDevice,
    kmd_version: &str,
    time: &str,
    hostname: &str,
) -> Option<GpuInfo> {
    let mut detail = HashMap::new();

    // Add device details
    detail.insert("Serial ID".to_string(), device.sid.clone());
    detail.insert("Device Path".to_string(), device.device.clone());
    detail.insert("Status".to_string(), device.status.clone());
    detail.insert("Firmware Version".to_string(), device.fw_ver.clone());
    detail.insert("KMD Version".to_string(), kmd_version.to_string());
    detail.insert("Board Info".to_string(), device.board_info.clone());

    // PCI details
    detail.insert("PCI Bus ID".to_string(), device.pci.bus_id.clone());
    detail.insert("PCI NUMA Node".to_string(), device.pci.numa_node.clone());
    detail.insert("PCI Link Speed".to_string(), device.pci.link_speed.clone());
    detail.insert("PCI Link Width".to_string(), device.pci.link_width.clone());

    // Performance state
    detail.insert("Performance State".to_string(), device.pstate.clone());

    // Parse metrics
    let temperature = parse_temperature(&device.temperature);
    let power = parse_power(&device.card_power);
    let utilization = parse_utilization(&device.util);
    let (used_memory, total_memory) = parse_memory(&device.memory);

    Some(GpuInfo {
        uuid: device.uuid,
        time: time.to_string(),
        name: device.name,
        device_type: "NPU".to_string(),
        host_id: hostname.to_string(),
        hostname: hostname.to_string(),
        instance: hostname.to_string(),
        utilization,
        ane_utilization: 0.0,
        dla_utilization: None,
        temperature,
        used_memory,
        total_memory,
        frequency: 0, // Rebellions doesn't report frequency
        power_consumption: power,
        gpu_core_count: None,
        detail,
    })
}

fn create_process_info_from_context(ctx: RblnContext) -> ProcessInfo {
    let device_id = ctx
        .npu
        .trim_start_matches("npu")
        .parse::<usize>()
        .unwrap_or(0);
    let memory_mb = ctx
        .memory
        .trim_end_matches("MB")
        .parse::<u64>()
        .unwrap_or(0);

    ProcessInfo {
        device_id,
        device_uuid: ctx.npu,
        pid: ctx.pid,
        process_name: extract_process_name(&ctx.cmd),
        used_memory: memory_mb * 1024 * 1024,
        cpu_percent: 0.0,
        memory_percent: 0.0,
        memory_rss: 0,
        memory_vms: 0,
        user: String::new(),
        state: String::new(),
        start_time: String::new(),
        cpu_time: 0,
        command: ctx.cmd,
        ppid: 0,
        threads: 0,
        uses_gpu: true,
        priority: 0,
        nice_value: 0,
        gpu_utilization: 0.0,
    }
}

fn parse_temperature(temp_str: &str) -> u32 {
    temp_str
        .trim_end_matches('C')
        .split('/')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

fn parse_power(power_str: &str) -> f64 {
    power_str
        .trim_end_matches('W')
        .split('/')
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn parse_utilization(util_str: &str) -> f64 {
    util_str.trim_end_matches('%').parse::<f64>().unwrap_or(0.0)
}

fn parse_memory(mem: &RblnMemoryInfo) -> (u64, u64) {
    let used = mem.used.trim_end_matches("MiB").parse::<u64>().unwrap_or(0) * 1024 * 1024;

    let total = mem
        .total
        .trim_end_matches("MiB")
        .parse::<u64>()
        .unwrap_or(0)
        * 1024
        * 1024;

    (used, total)
}

fn extract_process_name(cmd: &str) -> String {
    cmd.split_whitespace()
        .next()
        .and_then(|path| path.split('/').next_back())
        .unwrap_or("unknown")
        .to_string()
}
