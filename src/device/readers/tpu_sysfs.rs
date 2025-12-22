// Copyright 2025 Lablup Inc.
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

//! Sysfs-based TPU monitoring module.
//! 
//! This module reads TPU information directly from the Linux sysfs interface,
//! providing a lightweight and dependency-free way to get basic TPU metrics
//! like presence, temperature, and chip version.

use std::fs;
use std::path::{Path, PathBuf};
use crate::device::common::constants::google_tpu::GOOGLE_VENDOR_ID;

/// Struct to hold raw sysfs data
#[derive(Debug, Clone)]
pub struct SysfsTpuInfo {
    pub index: u32,
    #[allow(dead_code)]
    pub path: PathBuf,
    #[allow(dead_code)]
    pub vendor_id: String,
    pub device_id: String,
    pub temperature: Option<f64>,
}

/// Scans /sys/class/accel and /sys/bus/pci/devices for Google TPU devices
pub fn scan_sysfs_tpus() -> Vec<SysfsTpuInfo> {
    let mut devices = Vec::new();
    
    // 1. Try /sys/class/accel (Standard driver)
    let accel_path = Path::new("/sys/class/accel");
    if accel_path.exists() {
        if let Ok(entries) = fs::read_dir(accel_path) {
            let mut accel_entries: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .collect();
            
            accel_entries.sort();

            for (idx, path) in accel_entries.iter().enumerate() {
                if let Some(info) = parse_accel_device(path, idx as u32) {
                    devices.push(info);
                }
            }
        }
    }

    // 2. If no accel devices found, try scanning PCI bus directly (VFIO/Passthrough)
    if devices.is_empty() {
        let pci_path = Path::new("/sys/bus/pci/devices");
        if pci_path.exists() {
            if let Ok(entries) = fs::read_dir(pci_path) {
                let mut pci_entries: Vec<_> = entries
                    .flatten()
                    .map(|e| e.path())
                    .collect();
                pci_entries.sort();

                let mut index = 0;
                for path in pci_entries {
                    if let Some(info) = parse_pci_device(&path, index) {
                        devices.push(info);
                        index += 1;
                    }
                }
            }
        }
    }

    devices
}

fn parse_pci_device(path: &Path, index: u32) -> Option<SysfsTpuInfo> {
    // Check Vendor ID
    let vendor_path = path.join("vendor");
    let vendor = read_sysfs_string(&vendor_path)?;
    
    // Normalize vendor string (handle 0x prefix)
    let vendor_norm = vendor.trim().to_lowercase();
    if !vendor_norm.ends_with("1ae0") { // 0x1ae0 or 1ae0
        return None;
    }

    // Get Device ID
    let device_id_path = path.join("device");
    let device_id = read_sysfs_string(&device_id_path).unwrap_or_else(|| "unknown".to_string());

    // Get Temperature (Likely not available for VFIO devices via sysfs)
    let temperature = read_temperature(path);

    Some(SysfsTpuInfo {
        index,
        path: path.to_path_buf(),
        vendor_id: vendor,
        device_id,
        temperature,
    })
}

fn parse_accel_device(path: &Path, index: u32) -> Option<SysfsTpuInfo> {
    let device_dir = path.join("device");
    
    // Check Vendor ID
    let vendor_path = device_dir.join("vendor");
    let vendor = read_sysfs_string(&vendor_path)?;
    
    if vendor != GOOGLE_VENDOR_ID {
        return None;
    }

    // Get Device ID
    let device_id_path = device_dir.join("device");
    let device_id = read_sysfs_string(&device_id_path).unwrap_or_else(|| "unknown".to_string());

    // Get Temperature
    let temperature = read_temperature(&device_dir);

    Some(SysfsTpuInfo {
        index,
        path: path.to_path_buf(),
        vendor_id: vendor,
        device_id,
        temperature,
    })
}

fn read_temperature(device_dir: &Path) -> Option<f64> {
    // Try standard hwmon path: device/hwmon/hwmonX/temp1_input
    let hwmon_dir = device_dir.join("hwmon");
    if let Ok(entries) = fs::read_dir(hwmon_dir) {
        for entry in entries.flatten() {
            let temp_input = entry.path().join("temp1_input");
            if temp_input.exists() {
                if let Some(val) = read_sysfs_int(&temp_input) {
                    return Some(val as f64 / 1000.0);
                }
            }
        }
    }
    
    // Sometimes TPUs might be registered in thermal zones
    // This is less common for PCIe TPUs but possible for some embedded/SoC variations
    None
}

fn read_sysfs_string(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn read_sysfs_int(path: &Path) -> Option<i64> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
}
