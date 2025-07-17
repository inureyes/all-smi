use std::process::Command;

pub fn has_nvidia() -> bool {
    Command::new("nvidia-smi").output().is_ok()
}

pub fn is_jetson() -> bool {
    if let Ok(compatible) = std::fs::read_to_string("/proc/device-tree/compatible") {
        return compatible.contains("tegra");
    }
    false
}

pub fn is_apple_silicon() -> bool {
    let output = Command::new("uname")
        .arg("-m")
        .output()
        .expect("Failed to execute uname command");

    let architecture = String::from_utf8_lossy(&output.stdout);
    architecture.trim() == "arm64"
}

pub fn has_furiosa() -> bool {
    // Check if furiosactl is available
    Command::new("furiosactl").output().is_ok()
}

pub fn has_tenstorrent() -> bool {
    // Check if tt-smi or tensix-stat is available
    Command::new("tt-smi").output().is_ok() || Command::new("tensix-stat").output().is_ok()
}

pub fn get_os_type() -> &'static str {
    std::env::consts::OS
}
