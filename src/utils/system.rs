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

pub fn get_hostname() -> String {
    let output = Command::new("hostname")
        .output()
        .expect("Failed to execute hostname command");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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

pub fn ensure_sudo_permissions() {
    if cfg!(target_os = "macos") {
        // Force flush any pending output before showing our messages
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        request_sudo_with_explanation(false);
    } else {
        // For non-macOS systems, we might need different handling
        eprintln!("Note: This platform may not require sudo for hardware monitoring.");
    }
}

pub fn ensure_sudo_permissions_with_fallback() -> bool {
    if cfg!(target_os = "macos") {
        request_sudo_with_explanation(true)
    } else {
        true
    }
}

fn request_sudo_with_explanation(return_bool: bool) -> bool {
    // Check if we already have sudo privileges
    if has_sudo_privileges() {
        println!();
        println!("✅ Administrator privileges already available.");
        println!("   Starting system monitoring...");
        println!();
        if return_bool {
            return true;
        } else {
            // Add a small delay so user can see the message before terminal is cleared
            std::thread::sleep(std::time::Duration::from_millis(300));
            return false; // This return value won't be used when return_bool is false
        }
    }

    // Always show the explanation first, regardless of sudo status
    println!();
    println!("🔧 all-smi: System Monitoring Interface");
    println!("============================================");
    println!();
    println!("This application monitors GPU, CPU, and memory usage on your system.");
    println!();
    println!("🔒 Administrator privileges are required because:");
    println!("   • Access to hardware metrics requires the 'powermetrics' command");
    println!("   • powermetrics needs elevated privileges to read low-level system data");
    println!("   • This includes GPU utilization, power consumption, and thermal information");
    println!();
    println!("🛡️  Security Information:");
    println!("   • all-smi only reads system metrics - it does not modify your system");
    println!("   • The sudo access is used exclusively for running 'powermetrics'");
    println!("   • No data is transmitted externally without your explicit configuration");
    println!();
    println!("📋 What will be monitored:");
    println!("   • GPU: Utilization, memory usage, temperature, power consumption");
    println!("   • CPU: Core utilization and performance metrics");
    println!("   • Memory: System RAM usage and allocation");
    println!("   • Storage: Disk usage and performance");
    println!();

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
        println!("   • Ensure your user account has administrator privileges");
        println!("   • Try running 'sudo -v' manually to test sudo access");
        println!();
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
