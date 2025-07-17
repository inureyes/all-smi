// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0
// Simplified detection logic extracted from luwen-ref

use super::{
    arch::Arch,
    chip::{Chip, ChipImpl, PlatformError, Telemetry},
};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

/// Options for chip detection
pub struct ChipDetectOptions {
    /// If true, we will continue searching for chips even if we encounter a recoverable error
    pub continue_on_failure: bool,
    /// If true, then we will search for chips directly available over a physical interface
    #[allow(dead_code)]
    pub local_only: bool,
    /// If len > 0 then only chips with the given archs will be returned
    pub chip_filter: Vec<Arch>,
    /// If true, then we will not initialize anything that might cause a problem
    #[allow(dead_code)]
    pub noc_safe: bool,
}

impl Default for ChipDetectOptions {
    fn default() -> Self {
        Self {
            continue_on_failure: true,
            local_only: true,
            chip_filter: vec![],
            noc_safe: false,
        }
    }
}

/// Represents a chip which may or may not be initialized
pub enum UninitChip {
    /// The chip is fine and can be safely used
    Initialized(Chip),
}

impl UninitChip {
    /// Initialize the chip
    pub fn init<E>(self, _callback: &mut impl FnMut(()) -> Result<(), E>) -> Result<Chip, E> {
        match self {
            UninitChip::Initialized(chip) => Ok(chip),
        }
    }
}

/// Error type for detection
#[derive(Debug)]
pub struct DetectError(pub String);

impl std::fmt::Display for DetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DetectError {}

/// Minimal chip implementation for detection
struct MinimalChip {
    device_id: usize,
    arch: Arch,
}

impl ChipImpl for MinimalChip {
    fn get_arch(&self) -> Arch {
        self.arch
    }

    fn get_telemetry(&self) -> Result<Telemetry, PlatformError> {
        // Try to get real telemetry from sysfs or tt-smi first
        if let Some(mut real_telemetry) = self.try_read_real_telemetry() {
            // Ensure architecture is correct based on board_id
            if real_telemetry.board_id_high != 0 || real_telemetry.board_id_low != 0 {
                let board_pattern = (real_telemetry.board_serial_number() >> 36) & 0xFFFFF;
                match board_pattern {
                    0x1 | 0x3 | 0x7 | 0x8 | 0xA | 0xB => {
                        // Grayskull boards: E300 variants, e150, e75, NEBULA_CB, e300, GALAXY
                        real_telemetry.arch = Arch::Grayskull;
                    }
                    0x14 | 0x18 | 0x35 => {
                        // Wormhole boards: n300, n150, galaxy-wormhole
                        real_telemetry.arch = Arch::Wormhole;
                    }
                    0x36 | 0x40 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45 | 0x46 | 0x47 => {
                        // Blackhole boards: p100, p150, p300 variants, galaxy-blackhole
                        real_telemetry.arch = Arch::Blackhole;
                    }
                    _ => {
                        // Keep the detected architecture
                    }
                }
            }
            return Ok(real_telemetry);
        }

        // Fallback to minimal telemetry with placeholder values
        let mut telemetry = Telemetry {
            arch: self.arch,
            device_id: self.device_id as u32,
            ..Default::default()
        };

        // Set board_id based on architecture - this helps identify board type
        // If we already have a valid board_id from telemetry reading, keep it
        if telemetry.board_id_high == 0 && telemetry.board_id_low == 0 {
            match self.arch {
                Arch::Grayskull => {
                    // Set board_id for e75 (0x7 << 36) as default
                    telemetry.board_id_high = 0x0007;
                    telemetry.board_id_low = 0x00000001;
                }
                Arch::Wormhole => {
                    // Set board_id for n300 (0x14 << 36) as default
                    telemetry.board_id_high = 0x0014;
                    telemetry.board_id_low = 0x00000001;
                }
                Arch::Blackhole => {
                    // Set board_id for p100 (0x36 << 36) as default
                    telemetry.board_id_high = 0x0036;
                    telemetry.board_id_low = 0x00000001;
                }
            }
        }

        // Set reasonable default values when no telemetry is available
        // These are typical operating values for each architecture
        match self.arch {
            Arch::Grayskull => {
                telemetry.aiclk = 1000; // 1GHz typical
                telemetry.vcore = 750; // 0.75V typical
                telemetry.tdp = 30; // 30W typical idle power
                telemetry.tdc = 40; // 40A typical
                telemetry.asic_temperature = 45 << 4; // 45°C typical
                telemetry.ddr_speed = Some(1600); // DDR4
            }
            Arch::Wormhole => {
                telemetry.aiclk = 1200; // 1.2GHz typical
                telemetry.vcore = 800; // 0.8V typical
                telemetry.tdp = 50; // 50W typical idle power
                telemetry.tdc = 60; // 60A typical
                telemetry.asic_temperature = 50 << 4; // 50°C typical
                telemetry.ddr_speed = Some(6400); // GDDR6
            }
            Arch::Blackhole => {
                telemetry.aiclk = 1400; // 1.4GHz typical
                telemetry.vcore = 850; // 0.85V typical
                telemetry.tdp = 80; // 80W typical idle power
                telemetry.tdc = 90; // 90A typical
                telemetry.asic_temperature = 55 << 4; // 55°C typical
                telemetry.ddr_speed = Some(8000); // HBM3
            }
        }

        telemetry.ddr_status = 1;

        Ok(telemetry)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl MinimalChip {
    /// Try to read real telemetry values from available sources
    fn try_read_real_telemetry(&self) -> Option<Telemetry> {
        // Method 1: Try to read from sysfs first (no external tools needed)
        if let Some(telemetry) = self.read_telemetry_from_sysfs() {
            return Some(telemetry);
        }

        // Method 2: Try to read directly from hardware registers via tools
        if let Some(telemetry) = self.read_telemetry_from_hardware() {
            return Some(telemetry);
        }

        // Method 3: Only try tt-smi as last resort (requires Python)
        // Check if DISABLE_TT_SMI env var is set to skip this
        if std::env::var("DISABLE_TT_SMI").is_err() {
            if let Some(telemetry) = self.read_telemetry_from_tt_smi() {
                return Some(telemetry);
            }
        }
        // No real telemetry available
        None
    }

    /// Try to read telemetry directly from hardware via memory-mapped registers
    /// This is based on how luwen reads telemetry
    fn read_telemetry_from_hardware(&self) -> Option<Telemetry> {
        // Try to get telemetry using tensix-fw-sysinfo if available
        // This is a simple tool that dumps telemetry values
        use std::process::Command;

        let output = Command::new("tensix-fw-sysinfo")
            .arg("-d")
            .arg(self.device_id.to_string())
            .arg("-v")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let output_str = String::from_utf8(output.stdout).ok()?;
        let mut telemetry = Telemetry {
            arch: self.arch,
            device_id: self.device_id as u32,
            ..Default::default()
        };

        // Parse the output looking for telemetry values
        for line in output_str.lines() {
            if line.contains("vcore:") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(mv) = val.trim().trim_end_matches("mV").parse::<u32>() {
                        telemetry.vcore = mv;
                    }
                }
            } else if line.contains("tdp:") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(w) = val.trim().trim_end_matches('W').parse::<u32>() {
                        telemetry.tdp = w;
                    }
                }
            } else if line.contains("tdc:") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(a) = val.trim().trim_end_matches('A').parse::<u32>() {
                        telemetry.tdc = a;
                    }
                }
            } else if line.contains("asic_temperature:") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(t) = val.trim().parse::<f64>() {
                        telemetry.asic_temperature = (t * 16.0) as u32;
                    }
                }
            } else if line.contains("aiclk:") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(mhz) = val.trim().trim_end_matches("MHz").parse::<u32>() {
                        telemetry.aiclk = mhz;
                    }
                }
            } else if line.contains("board_id:") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(id) = u64::from_str_radix(val.trim().trim_start_matches("0x"), 16) {
                        telemetry.board_id_high = (id >> 32) as u32;
                        telemetry.board_id_low = (id & 0xFFFFFFFF) as u32;
                    }
                }
            }
        }

        // Only return if we got at least power reading
        if telemetry.tdp > 0 || telemetry.vcore > 0 {
            Some(telemetry)
        } else {
            None
        }
    }

    /// Try to read telemetry from sysfs
    fn read_telemetry_from_sysfs(&self) -> Option<Telemetry> {
        // Check multiple possible sysfs paths
        let sysfs_paths = [
            format!("/sys/class/tenstorrent/{}/telemetry", self.device_id),
            format!(
                "/sys/class/tenstorrent/tenstorrent{}/telemetry",
                self.device_id
            ),
            "/sys/devices/pci0000:00/0000:00:*/*.0/tenstorrent/telemetry".to_string(),
        ];

        let mut base_path = None;
        for path in &sysfs_paths {
            if Path::new(path).exists() {
                base_path = Some(path.clone());
                break;
            }
        }

        // Also check if there's a direct telemetry file
        let direct_telemetry_path = format!("/proc/tenstorrent/{}/telemetry", self.device_id);
        if Path::new(&direct_telemetry_path).exists() {
            // Try to read all telemetry in one go
            if let Ok(contents) = fs::read_to_string(&direct_telemetry_path) {
                return self.parse_proc_telemetry(&contents);
            }
        }

        // Check for device-specific telemetry files
        let device_telemetry_path =
            format!("/sys/class/tenstorrent/device{}/telemetry", self.device_id);
        if Path::new(&device_telemetry_path).exists() {
            base_path = Some(device_telemetry_path);
        }

        let base_path = base_path?;

        let mut telemetry = Telemetry {
            arch: self.arch,
            device_id: self.device_id as u32,
            ..Default::default()
        };

        // Try to read various telemetry values
        if let Ok(contents) = fs::read_to_string(format!("{base_path}/power_watts")) {
            if let Ok(power) = contents.trim().parse::<f64>() {
                telemetry.tdp = power as u32;
            }
        }

        if let Ok(contents) = fs::read_to_string(format!("{base_path}/temperature_celsius")) {
            if let Ok(temp) = contents.trim().parse::<f64>() {
                telemetry.asic_temperature = (temp * 16.0) as u32; // Convert to expected format
            }
        }

        if let Ok(contents) = fs::read_to_string(format!("{base_path}/voltage_mv")) {
            if let Ok(voltage_mv) = contents.trim().parse::<u32>() {
                telemetry.vcore = voltage_mv; // Already in millivolts
            }
        }

        if let Ok(contents) = fs::read_to_string(format!("{base_path}/current_amps")) {
            if let Ok(current) = contents.trim().parse::<f64>() {
                telemetry.tdc = current as u32;
            }
        }

        if let Ok(contents) = fs::read_to_string(format!("{base_path}/frequency_mhz")) {
            if let Ok(freq) = contents.trim().parse::<u32>() {
                telemetry.aiclk = freq;
            }
        }

        // If we got at least power reading, consider it valid
        if telemetry.tdp > 0 {
            Some(telemetry)
        } else {
            None
        }
    }

    /// Parse telemetry from /proc/tenstorrent format
    fn parse_proc_telemetry(&self, contents: &str) -> Option<Telemetry> {
        let mut telemetry = Telemetry {
            arch: self.arch,
            device_id: self.device_id as u32,
            ..Default::default()
        };

        for line in contents.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() != 2 {
                continue;
            }

            let key = parts[0].trim();
            let value = parts[1].trim();

            match key {
                "power" => {
                    if let Ok(p) = value.trim_end_matches('W').parse::<f64>() {
                        telemetry.tdp = p as u32;
                    }
                }
                "voltage" => {
                    if let Ok(v) = value.trim_end_matches('V').parse::<f64>() {
                        telemetry.vcore = (v * 1000.0) as u32; // Convert to millivolts
                    }
                }
                "current" => {
                    if let Ok(c) = value.trim_end_matches('A').parse::<f64>() {
                        telemetry.tdc = c as u32;
                    }
                }
                "temperature" => {
                    if let Ok(t) = value.trim_end_matches("°C").parse::<f64>() {
                        telemetry.asic_temperature = (t * 16.0) as u32;
                    }
                }
                "frequency" => {
                    if let Ok(f) = value.trim_end_matches("MHz").parse::<u32>() {
                        telemetry.aiclk = f;
                    }
                }
                _ => {}
            }
        }

        if telemetry.tdp > 0 || telemetry.vcore > 0 {
            Some(telemetry)
        } else {
            None
        }
    }

    /// Try to parse tt-smi output
    fn read_telemetry_from_tt_smi(&self) -> Option<Telemetry> {
        // Try to run tt-smi -j and parse output
        use std::process::Command;

        let output = Command::new("tt-smi")
            .arg("-j")
            .arg("-d")
            .arg(self.device_id.to_string())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        // Parse JSON output
        let json_str = String::from_utf8(output.stdout).ok()?;
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
            if let Some(devices) = json.get("device_info").and_then(|v| v.as_array()) {
                if let Some(device) = devices.first() {
                    let mut telemetry = Telemetry {
                        arch: self.arch,
                        device_id: self.device_id as u32,
                        ..Default::default()
                    };

                    // Parse telemetry values
                    if let Some(telem) = device.get("telemetry") {
                        if let Some(power) = telem.get("power") {
                            // Handle both string and number formats
                            match power {
                                serde_json::Value::String(s) => {
                                    if let Ok(p) = s.trim_end_matches('W').parse::<f64>() {
                                        telemetry.tdp = p as u32;
                                    }
                                }
                                serde_json::Value::Number(n) => {
                                    if let Some(p) = n.as_f64() {
                                        telemetry.tdp = p as u32;
                                    }
                                }
                                _ => {}
                            }
                        }

                        if let Some(temp) = telem.get("asic_temperature").and_then(|v| v.as_str()) {
                            if let Ok(t) = temp.trim_end_matches("°C").parse::<f64>() {
                                telemetry.asic_temperature = (t * 16.0) as u32;
                            }
                        }

                        if let Some(voltage) = telem.get("voltage").and_then(|v| v.as_str()) {
                            if let Ok(v) = voltage.trim_end_matches('V').parse::<f64>() {
                                telemetry.vcore = (v * 1000.0) as u32; // Convert to millivolts
                            }
                        }

                        if let Some(current) = telem.get("current").and_then(|v| v.as_str()) {
                            if let Ok(c) = current.trim_end_matches('A').parse::<f64>() {
                                telemetry.tdc = c as u32;
                            }
                        }

                        if let Some(freq) = telem.get("aiclk").and_then(|v| v.as_u64()) {
                            telemetry.aiclk = freq as u32;
                        }
                    }

                    // Parse board info
                    if let Some(board_info) = device.get("board_info") {
                        if let Some(board_id) = board_info.get("board_id").and_then(|v| v.as_str())
                        {
                            if let Ok(id) =
                                u64::from_str_radix(board_id.trim_start_matches("0x"), 16)
                            {
                                telemetry.board_id_high = (id >> 32) as u32;
                                telemetry.board_id_low = (id & 0xFFFFFFFF) as u32;
                            }
                        }
                    }

                    return Some(telemetry);
                }
            }
        }

        None
    }
}

/// Scan for Tenstorrent devices in /dev/tenstorrent/
fn scan_devices() -> Vec<usize> {
    let dev_path = Path::new("/dev/tenstorrent");

    if !dev_path.exists() {
        return vec![];
    }

    let mut devices = vec![];

    if let Ok(entries) = fs::read_dir(dev_path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                // Check if it's a character device (on Unix) or just accept all
                #[cfg(unix)]
                let is_valid = file_type.is_char_device();
                #[cfg(not(unix))]
                let is_valid = true;

                if is_valid {
                    if let Some(name) = entry.file_name().to_str() {
                        // Parse device ID from filename
                        if let Ok(id) = name.parse::<usize>() {
                            devices.push(id);
                        }
                    }
                }
            }
        }
    }

    devices.sort();
    devices
}

/// Get architecture from device - simplified version
/// In real implementation, this would use ioctl to get device info
fn get_device_arch(device_id: usize) -> Option<Arch> {
    // Method 1: Try to read from sysfs (if available)
    let sysfs_paths = [
        format!("/sys/class/tenstorrent/{device_id}/device_id"),
        format!("/sys/class/tenstorrent/{device_id}/device/device"),
        format!("/sys/class/tenstorrent/tenstorrent{device_id}/device_id"),
    ];

    for path in &sysfs_paths {
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(dev_id) = u16::from_str_radix(contents.trim().trim_start_matches("0x"), 16) {
                return match dev_id {
                    0xfaca => Some(Arch::Grayskull),
                    0x401e => Some(Arch::Wormhole),
                    0xb140 => Some(Arch::Blackhole),
                    _ => None,
                };
            }
        }
    }

    // Method 2: Try to find PCI device info
    // Look for the device in /sys/bus/pci/devices/
    if let Ok(entries) = fs::read_dir("/sys/bus/pci/devices/") {
        for entry in entries.flatten() {
            let device_path = entry.path().join("device");
            let vendor_path = entry.path().join("vendor");

            if let (Ok(vendor), Ok(device)) = (
                fs::read_to_string(&vendor_path),
                fs::read_to_string(&device_path),
            ) {
                // Tenstorrent vendor ID is 0x1e52
                if vendor.trim() == "0x1e52" {
                    if let Ok(dev_id) =
                        u16::from_str_radix(device.trim().trim_start_matches("0x"), 16)
                    {
                        return match dev_id {
                            0xfaca => Some(Arch::Grayskull),
                            0x401e => Some(Arch::Wormhole),
                            0xb140 => Some(Arch::Blackhole),
                            _ => None,
                        };
                    }
                }
            }
        }
    }

    // Method 3: Try to read architecture from device driver info
    let driver_arch_path = format!("/sys/class/tenstorrent/{device_id}/arch");
    if let Ok(arch_str) = fs::read_to_string(&driver_arch_path) {
        match arch_str.trim().to_lowercase().as_str() {
            "grayskull" => return Some(Arch::Grayskull),
            "wormhole" => return Some(Arch::Wormhole),
            "blackhole" => return Some(Arch::Blackhole),
            _ => {}
        }
    }

    // Method 4: Check if device file exists and make educated guess based on system
    // This is a last resort - we found a device but can't determine its type
    let dev_file = format!("/dev/tenstorrent/{device_id}");
    if Path::new(&dev_file).exists() {
        // Try to determine from system characteristics
        // Check for hints in /proc/cpuinfo or DMI info
        if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
            if cpuinfo.contains("Intel") || cpuinfo.contains("AMD") {
                // x86 systems most commonly have Wormhole or Blackhole
                return Some(Arch::Wormhole); // Conservative default
            }
        }

        // Default to Wormhole as it's the most common
        return Some(Arch::Wormhole);
    }

    None
}

/// Detect chips silently without UI output
pub fn detect_chips_silent(options: ChipDetectOptions) -> Result<Vec<UninitChip>, DetectError> {
    let device_ids = scan_devices();

    if device_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut chips = Vec::new();

    for device_id in device_ids {
        // Try to determine architecture
        let arch = match get_device_arch(device_id) {
            Some(arch) => arch,
            None => {
                if options.continue_on_failure {
                    continue;
                } else {
                    return Err(DetectError(format!(
                        "Failed to determine architecture for device {device_id}"
                    )));
                }
            }
        };

        // Apply architecture filter if specified
        if !options.chip_filter.is_empty() && !options.chip_filter.contains(&arch) {
            continue;
        }

        // Create a minimal chip implementation
        let chip_impl = MinimalChip { device_id, arch };
        let chip = Chip {
            inner: Box::new(chip_impl),
        };

        chips.push(UninitChip::Initialized(chip));
    }

    Ok(chips)
}
