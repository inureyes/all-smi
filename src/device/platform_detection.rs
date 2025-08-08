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

use crate::utils::run_command_fast_fail;
use std::process::Command;

pub fn has_nvidia() -> bool {
    // On macOS, use system_profiler to check for NVIDIA devices
    if std::env::consts::OS == "macos" {
        // First check system_profiler for NVIDIA PCI devices
        if let Ok(output) = Command::new("system_profiler")
            .arg("SPPCIDataType")
            .output()
        {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Look for NVIDIA in the output - could be in Type field or device name
                if output_str.contains("NVIDIA") {
                    return true;
                }
            }
        }

        // Fallback to nvidia-smi check
        if let Ok(output) = run_command_fast_fail("nvidia-smi", &["-L"]) {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // nvidia-smi -L outputs lines like "GPU 0: NVIDIA GeForce..."
                return output_str
                    .lines()
                    .any(|line| line.trim().starts_with("GPU"));
            }
        }
        return false;
    }

    // On Linux, first try lspci to check for NVIDIA VGA/3D controllers
    if let Ok(output) = run_command_fast_fail("lspci", &[]) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Look for NVIDIA VGA or 3D controllers
            for line in output_str.lines() {
                if (line.contains("VGA") || line.contains("3D")) && line.contains("NVIDIA") {
                    return true;
                }
            }
        }
    }

    // Fallback: Check if nvidia-smi can actually list GPUs
    if let Ok(output) = Command::new("nvidia-smi").args(["-L"]).output() {
        // Check both exit status and output content
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // nvidia-smi -L outputs lines like "GPU 0: NVIDIA GeForce..."
            // Make sure we have actual GPU lines, not just an empty output
            let has_gpu = output_str.lines().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("GPU") && trimmed.contains(":")
            });
            if has_gpu {
                return true;
            }
        }

        // Also check stderr for "No devices were found" message
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        if stderr_str.contains("No devices were found")
            || stderr_str.contains("Failed to initialize NVML")
        {
            return false;
        }
    }
    false
}

pub fn is_jetson() -> bool {
    if let Ok(compatible) = std::fs::read_to_string("/proc/device-tree/compatible") {
        return compatible.contains("tegra");
    }
    false
}

pub fn is_apple_silicon() -> bool {
    // Only check on macOS
    if std::env::consts::OS != "macos" {
        return false;
    }

    let output = Command::new("uname")
        .arg("-m")
        .output()
        .expect("Failed to execute uname command");

    let architecture = String::from_utf8_lossy(&output.stdout);
    architecture.trim() == "arm64"
}

pub fn has_furiosa() -> bool {
    // Check if devices are visible under the /sys/class/rngd_mgmt directory
    let rngd_mgmt_path = std::path::Path::new("/sys/class/rngd_mgmt");
    if !rngd_mgmt_path.exists() {
        return false;
    }

    // Check if /sys/class/rngd_mgmt/rngd!npu0mgmt exists
    let npu0_mgmt_path = rngd_mgmt_path.join("rngd!npu0mgmt");
    if !npu0_mgmt_path.exists() {
        return false;
    }

    // Check if the content of platform_type is FuriosaAI
    let platform_type_path = npu0_mgmt_path.join("platform_type");
    if let Ok(platform_type) = std::fs::read_to_string(platform_type_path) {
        if platform_type.trim() == "FuriosaAI" {
            return true;
        }
    }

    false
}

pub fn has_tenstorrent() -> bool {
    // First check if device directory exists
    if std::path::Path::new("/dev/tenstorrent").exists() {
        return true;
    }

    // On macOS, use system_profiler
    if std::env::consts::OS == "macos" {
        if let Ok(output) = Command::new("system_profiler")
            .arg("SPPCIDataType")
            .output()
        {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                if output_str.contains("Tenstorrent") {
                    return true;
                }
            }
        }
    } else {
        // On Linux, try lspci to check for Tenstorrent devices
        if let Ok(output) = Command::new("lspci").output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Look for Tenstorrent devices
                if output_str.contains("Tenstorrent") {
                    return true;
                }
            }
        }
    }

    // Last resort: check if tt-smi can actually list devices
    if let Ok(output) = Command::new("tt-smi")
        .args(["-s", "--snapshot_no_tty"])
        .output()
    {
        if output.status.success() {
            // Check if output contains device_info
            let output_str = String::from_utf8_lossy(&output.stdout);
            return output_str.contains("device_info");
        }
    }

    false
}

pub fn has_rebellions() -> bool {
    // First check if device files exist (rbln0, rbln1, etc.)
    if std::path::Path::new("/dev/rbln0").exists() {
        return true;
    }

    // On macOS, use system_profiler
    if std::env::consts::OS == "macos" {
        if let Ok(output) = Command::new("system_profiler")
            .arg("SPPCIDataType")
            .output()
        {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                if output_str.contains("Rebellions") || output_str.contains("RBLN") {
                    return true;
                }
            }
        }
    } else {
        // On Linux, try lspci to check for Rebellions devices
        if let Ok(output) = Command::new("lspci").output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Look for Rebellions devices - vendor ID 1f3f
                if output_str.contains("1f3f:") || output_str.contains("Rebellions") {
                    return true;
                }
            }
        }
    }

    // Last resort: check if rbln-stat or rbln-smi can actually list devices
    for cmd in &[
        "rbln-stat",
        "/usr/local/bin/rbln-stat",
        "/usr/bin/rbln-stat",
        "rbln-smi",
        "/usr/local/bin/rbln-smi",
        "/usr/bin/rbln-smi",
    ] {
        if let Ok(output) = Command::new(cmd).args(["-j"]).output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Check if output contains device information
                if output_str.contains("\"devices\"") && output_str.contains("\"uuid\"") {
                    return true;
                }
            }
        }
    }

    false
}

pub fn get_os_type() -> &'static str {
    std::env::consts::OS
}

#[allow(dead_code)]
pub fn is_running_in_container() -> bool {
    // Only check on Linux, as containers are Linux-specific
    if std::env::consts::OS != "linux" {
        return false;
    }

    // Check for Docker
    if std::path::Path::new("/.dockerenv").exists() {
        return true;
    }

    // Check for Kubernetes
    if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
        return true;
    }

    // Check /proc/self/cgroup for container runtimes
    if let Ok(cgroup_content) = std::fs::read_to_string("/proc/self/cgroup") {
        let container_patterns = [
            "docker",
            "containerd",
            "crio",
            "podman",
            "garden",
            "lxc",
            "systemd-nspawn",
        ];

        for pattern in &container_patterns {
            if cgroup_content.contains(pattern) {
                return true;
            }
        }
    }

    // Check /proc/1/sched for container hints
    if let Ok(sched_content) = std::fs::read_to_string("/proc/1/sched") {
        if sched_content.lines().next().is_some_and(|line| {
            line.contains("bash") || line.contains("sh") || line.contains("init")
        }) {
            // If PID 1 is a shell or init process that's not systemd/upstart, likely in container
            if !sched_content.contains("systemd") && !sched_content.contains("upstart") {
                return true;
            }
        }
    }

    false
}

#[allow(dead_code)]
pub fn get_container_pid_namespace() -> Option<u32> {
    // Get the PID namespace ID for the current process
    if let Ok(ns_link) = std::fs::read_link("/proc/self/ns/pid") {
        // Convert PathBuf to String
        if let Some(ns_str) = ns_link.to_str() {
            // Extract namespace ID from the link (format: "pid:[4026531836]")
            if let Some(start) = ns_str.find('[') {
                if let Some(end) = ns_str.find(']') {
                    let ns_id_str = &ns_str[start + 1..end];
                    // Parse as u64 first, then convert to u32 if within range
                    if let Ok(ns_id_u64) = ns_id_str.parse::<u64>() {
                        // Namespace IDs can be larger than u32::MAX
                        // For comparison purposes, we'll use the lower 32 bits
                        let ns_id = ns_id_u64 as u32;
                        return Some(ns_id);
                    }
                }
            }
        }
    }
    None
}
