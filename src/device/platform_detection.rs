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

    // Check if furiosactl can list devices
    if let Ok(output) = Command::new("furiosactl").args(["list"]).output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Check if output contains actual device entries
            return output_str.lines().count() > 1; // More than just header
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
                // Look for Tenstorrent devices by vendor ID 1e52
                if output_str.contains("1e52") || output_str.contains("Tenstorrent") {
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
