use std::process::Command;

pub fn has_nvidia() -> bool {
    // Check if nvidia-smi can actually list GPUs
    if let Ok(output) = Command::new("nvidia-smi").args(["-L"]).output() {
        // Check if the command succeeded and has GPU output
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // nvidia-smi -L outputs lines like "GPU 0: NVIDIA GeForce..."
            return output_str.contains("GPU");
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

    // If not, check if tt-smi can actually list devices
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

pub fn get_os_type() -> &'static str {
    std::env::consts::OS
}
