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

use std::io::{self, Write};
use std::process::Command;
use sysinfo::System;

pub fn get_hostname() -> String {
    System::host_name().unwrap_or_else(|| "unknown".to_string())
}

/// Check if the current process already has sudo privileges
pub fn has_sudo_privileges() -> bool {
    Command::new("sudo")
        .arg("-n") // Non-interactive mode
        .arg("-v") // Validate sudo timestamp
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[allow(dead_code)] // Used in runner_old.rs (backup file)
pub fn calculate_adaptive_interval(node_count: usize) -> u64 {
    // Adaptive interval based on node count to prevent overwhelming the network
    // For 1-10 nodes: 2 seconds
    // For 11-50 nodes: 3 seconds
    // For 51-100 nodes: 4 seconds
    // For 101-200 nodes: 5 seconds
    // For 201+ nodes: 6 seconds
    match node_count {
        0..=10 => 2,
        11..=50 => 3,
        51..=100 => 4,
        101..=200 => 5,
        _ => 6,
    }
}

#[allow(dead_code)] // Used conditionally based on platform and feature flags
pub fn ensure_sudo_permissions() {
    if cfg!(target_os = "macos") {
        // Force flush any pending output before showing our messages
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        request_sudo_with_explanation(SudoPlatform::MacOS, false);
    } else if cfg!(target_os = "linux") {
        // On Linux, check if we have AMD GPUs that require sudo (glibc only)
        #[cfg(all(target_os = "linux", not(target_env = "musl")))]
        {
            use crate::device::platform_detection::has_amd;

            if has_amd() {
                // AMD GPUs require sudo to access /dev/dri devices
                // Check if running as root
                if unsafe { libc::geteuid() } != 0 {
                    request_sudo_with_explanation(SudoPlatform::Linux, false);
                }
            }
        }
    } else {
        // For other systems, we might need different handling
        eprintln!("Note: This platform may not require sudo for hardware monitoring.");
    }
}

pub fn ensure_sudo_permissions_for_api() -> bool {
    #[cfg(target_os = "windows")]
    {
        // Windows does not use sudo/root model
        println!("✅ Running on Windows, no sudo required.");
        true
    }

    #[cfg(unix)]
    {
        // When using native macOS APIs, no sudo is required
        #[cfg(all(target_os = "macos", not(feature = "powermetrics")))]
        {
            // Native macOS APIs (IOReport, SMC) don't require sudo
            true
        }

        #[cfg(not(all(target_os = "macos", not(feature = "powermetrics"))))]
        {
            // Check if we are already running as root
            if std::env::var("USER").unwrap_or_default() == "root"
                || unsafe { libc::geteuid() } == 0
            {
                println!("✅ Running as root, no sudo required.");
                return true;
            }

            // Check if we already have sudo privileges cached
            if has_sudo_privileges() {
                println!("✅ Sudo privileges already available.");
                return true;
            }

            // Sudo not available - warn but continue (for API mode)
            println!("⚠️  Warning: Running without sudo privileges.");
            println!("   Some hardware metrics may not be available.");
            println!("   For full functionality, run with: sudo all-smi api --port <port>");
            false
        }
    }

    #[cfg(not(any(target_os = "windows", unix)))]
    {
        // For other platforms, assume no sudo required
        println!("✅ Running on unsupported platform, no sudo required.");
        true
    }
}

#[allow(dead_code)] // Used conditionally based on platform and feature flags
pub fn ensure_sudo_permissions_with_fallback() -> bool {
    if cfg!(target_os = "macos") {
        request_sudo_with_explanation(SudoPlatform::MacOS, true)
    } else if cfg!(target_os = "linux") {
        // On Linux, check if we have AMD GPUs that require sudo (glibc only)
        #[cfg(all(target_os = "linux", not(target_env = "musl")))]
        {
            use crate::device::platform_detection::has_amd;

            if has_amd() {
                // AMD GPUs require sudo - check if running as root
                if unsafe { libc::geteuid() } != 0 {
                    request_sudo_with_explanation(SudoPlatform::Linux, false);
                }
                // If we're here, either:
                // 1. We were already root (geteuid() == 0)
                // 2. sudo request succeeded (otherwise process would have exited)
                // In both cases, we can proceed
                return true;
            }
        }
        // For musl builds or when AMD GPU is not detected, no sudo needed
        true
    } else {
        true
    }
}

/// Platform-specific sudo messages
#[derive(Copy, Clone)]
#[allow(dead_code)] // Variants used conditionally based on platform
enum SudoPlatform {
    MacOS,
    Linux,
}

/// Get platform-specific sudo explanation messages
#[allow(dead_code)] // Used conditionally based on platform and feature flags
fn get_sudo_messages(
    platform: SudoPlatform,
) -> (
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
) {
    match platform {
        SudoPlatform::MacOS => (
            // Required reasons
            "   • Access to hardware metrics requires the 'powermetrics' command\n   • powermetrics needs elevated privileges to read low-level system data\n   • This includes GPU utilization, power consumption, and thermal information",
            // Security info
            "   • all-smi only reads system metrics - it does not modify your system\n   • The sudo access is used exclusively for running 'powermetrics'\n   • No data is transmitted externally without your explicit configuration",
            // Monitored items
            "   • GPU: Utilization, memory usage, temperature, power consumption\n   • CPU: Core utilization and performance metrics\n   • Memory: System RAM usage and allocation\n   • Storage: Disk usage and performance",
            // Alternative (None for macOS)
            None,
            // Additional troubleshooting (None for macOS)
            None,
        ),
        SudoPlatform::Linux => (
            // Required reasons
            "   • Access to AMD GPU devices requires read/write permissions on /dev/dri\n   • These devices are typically only accessible by root or video/render group\n   • This includes GPU utilization, memory usage, temperature, and power data",
            // Security info
            "   • all-smi only reads GPU metrics - it does not modify your system\n   • The sudo access is used exclusively for accessing AMD GPU devices\n   • No data is transmitted externally without your explicit configuration",
            // Monitored items
            "   • AMD GPU: Utilization, VRAM usage, temperature, power, clock speeds\n   • CPU: Core utilization and performance metrics\n   • Memory: System RAM usage and allocation\n   • Storage: Disk usage and performance",
            // Alternative
            Some("💡 Alternative: Add your user to the 'video' and 'render' groups:\n   sudo usermod -a -G video,render $USER\n   (requires logout/login to take effect)"),
            // Additional troubleshooting
            Some("   Alternative: Add your user to video/render groups:\n   → sudo usermod -a -G video,render $USER"),
        ),
    }
}

/// Unified function to request sudo with platform-specific explanations
#[allow(dead_code)] // Used conditionally based on platform and feature flags
fn request_sudo_with_explanation(platform: SudoPlatform, return_bool: bool) -> bool {
    // Check if we already have sudo privileges
    if has_sudo_privileges() {
        // On Linux, having sudo timestamp is not enough - process must run as root
        #[cfg(target_os = "linux")]
        {
            if matches!(platform, SudoPlatform::Linux) && unsafe { libc::geteuid() } != 0 {
                println!();
                println!("⚠️  Sudo timestamp is valid, but the process is not running as root.");
                println!();
                println!("AMD GPU monitoring requires the program itself to run with sudo:");
                println!("   → sudo all-smi");
                println!();
                println!("(Unlike macOS, Linux requires root privileges for /dev/dri access)");
                println!();
                std::process::exit(1);
            }
        }

        println!();
        println!("✅ Administrator privileges already available.");
        println!("   Starting system monitoring...");
        println!();
        // Add a small delay for non-fallback mode so user can see the message
        if !return_bool {
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        return true; // Always return true if sudo is available
    }

    let (required_reasons, security_info, monitored_items, alternative, additional_troubleshooting) =
        get_sudo_messages(platform);

    // Always show the explanation first, regardless of sudo status
    println!();
    println!("🔧 all-smi: System Monitoring Interface");
    println!("============================================");
    println!();
    println!("This application monitors GPU, CPU, and memory usage on your system.");
    println!();
    println!("🔒 Administrator privileges are required because:");
    println!("{required_reasons}");
    println!();
    println!("🛡️  Security Information:");
    println!("{security_info}");
    println!();
    println!("📋 What will be monitored:");
    println!("{monitored_items}");
    println!();

    // Show alternative if available (Linux only)
    if let Some(alt) = alternative {
        println!("{alt}");
        println!();
    }

    // Give user a choice to continue
    print!("To proceed, you need to enter your sudo password.");
    println!();
    println!("🔑 Requesting administrator privileges...");
    println!("   (You may be prompted for your password)");
    println!();

    // Flush output to ensure all messages are displayed before sudo prompt
    io::stdout().flush().unwrap();

    // Attempt to get sudo privileges
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .expect("Failed to execute sudo command");

    if !status.success() {
        println!("❌ Failed to acquire administrator privileges.");
        println!();
        println!("💡 Troubleshooting:");
        println!("   • Make sure you entered the correct password");
        println!(
            "   • Ensure your user account has {}",
            if matches!(platform, SudoPlatform::MacOS) {
                "administrator privileges"
            } else {
                "sudo privileges"
            }
        );
        println!("   • Try running 'sudo -v' manually to test sudo access");
        println!();

        // Show additional troubleshooting if available (Linux only)
        if let Some(additional) = additional_troubleshooting {
            println!("{additional}");
            println!();
        }

        println!("   For remote monitoring without sudo, use:");
        println!("   → all-smi view --hosts <url1> <url2>");
        println!();
        std::process::exit(1);
    }

    println!("✅ Administrator privileges granted successfully.");
    println!("   Starting system monitoring...");
    println!();

    true // Always return true if we reach this point (sudo was successful)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_adaptive_interval() {
        assert_eq!(calculate_adaptive_interval(0), 2);
        assert_eq!(calculate_adaptive_interval(1), 2);
        assert_eq!(calculate_adaptive_interval(5), 2);
        assert_eq!(calculate_adaptive_interval(10), 2);
        assert_eq!(calculate_adaptive_interval(11), 3);
        assert_eq!(calculate_adaptive_interval(25), 3);
        assert_eq!(calculate_adaptive_interval(50), 3);
        assert_eq!(calculate_adaptive_interval(51), 4);
        assert_eq!(calculate_adaptive_interval(75), 4);
        assert_eq!(calculate_adaptive_interval(100), 4);
        assert_eq!(calculate_adaptive_interval(101), 5);
        assert_eq!(calculate_adaptive_interval(150), 5);
        assert_eq!(calculate_adaptive_interval(200), 5);
        assert_eq!(calculate_adaptive_interval(201), 6);
        assert_eq!(calculate_adaptive_interval(500), 6);
        assert_eq!(calculate_adaptive_interval(1000), 6);
    }

    #[test]
    fn test_get_hostname() {
        let hostname = get_hostname();
        assert!(!hostname.is_empty(), "Hostname should not be empty");
        assert!(
            !hostname.contains('\n'),
            "Hostname should not contain newlines"
        );
        assert!(
            !hostname.contains('\r'),
            "Hostname should not contain carriage returns"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore] // Run with: sudo cargo test -- --ignored
    fn test_ensure_sudo_permissions_macos() {
        ensure_sudo_permissions();
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_ensure_sudo_permissions_non_macos() {
        ensure_sudo_permissions();
    }

    #[test]
    #[cfg_attr(target_os = "macos", ignore)] // Run with: sudo cargo test -- --ignored
    fn test_ensure_sudo_permissions_with_fallback_returns_bool() {
        let _result = ensure_sudo_permissions_with_fallback();
        // Function should execute without panicking and return a boolean
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_has_sudo_privileges_on_macos() {
        // This test just checks if the function runs without error
        // It doesn't require sudo itself
        let _result = has_sudo_privileges();
        // Function should execute without panicking and return a boolean
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_has_sudo_privileges_on_non_macos() {
        let result = has_sudo_privileges();
        // Result is always a boolean, so just verify it completes
        let _ = result;
    }
}
