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
        if let Ok(output) = Command::new("nvidia-smi").args(["-L"]).output() {
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
    if let Ok(output) = Command::new("lspci").output() {
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
    // First check if device files exist
    if std::path::Path::new("/dev/npu0").exists() {
        return true;
    }

    // Use lspci to check for Furiosa devices
    if let Ok(output) = Command::new("lspci").output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Look for Furiosa devices (case insensitive)
            for line in output_str.lines() {
                if line.to_lowercase().contains("furiosa") {
                    return true;
                }
            }
        }
    }

    // On macOS, use system_profiler
    if std::env::consts::OS == "macos" {
        if let Ok(output) = Command::new("system_profiler")
            .arg("SPPCIDataType")
            .output()
        {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                if output_str.contains("Furiosa") || output_str.contains("FuriosaAI") {
                    return true;
                }
            }
        }
    }

    // Check if furiosa-smi can list devices
    if let Ok(output) = Command::new("furiosa-smi")
        .args(["status", "--format", "json"])
        .output()
    {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Check if output contains actual device entries
            if output_str.contains("\"index\"") && output_str.contains("\"device\"") {
                return true;
            }
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
