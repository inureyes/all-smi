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

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::device::powermetrics_parser::{parse_powermetrics_output, PowerMetricsData};

#[cfg(unix)]
use libc;

/// Manages a long-running powermetrics process with in-memory circular buffer
pub struct PowerMetricsManager {
    process: Arc<Mutex<Option<Child>>>,
    // Circular buffer storing complete powermetrics sections
    data_buffer: Arc<Mutex<VecDeque<String>>>,
    // Channel for sending commands to reader thread
    command_tx: Option<Sender<ReaderCommand>>,
    last_data: Arc<Mutex<Option<PowerMetricsData>>>,
    is_running: Arc<Mutex<bool>>,
    interval_ms: u64, // Interval in milliseconds for powermetrics collection
}

#[derive(Debug)]
enum ReaderCommand {
    Shutdown,
}

impl PowerMetricsManager {
    /// Create a new PowerMetricsManager and start the powermetrics process
    fn new(interval_secs: u64) -> Result<Self, Box<dyn std::error::Error>> {
        // Kill any existing powermetrics processes first
        Self::kill_existing_powermetrics_processes();

        let (command_tx, command_rx) = mpsc::channel();
        let interval_ms = interval_secs * 1000; // Convert seconds to milliseconds

        let manager = Self {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(120))), // 2 minutes of data
            command_tx: Some(command_tx),
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms,
        };

        manager.start_powermetrics(command_rx)?;

        // Start a background thread to monitor the process
        let process_arc = manager.process.clone();
        let data_buffer = manager.data_buffer.clone();
        let is_running = manager.is_running.clone();
        let interval_ms = manager.interval_ms;
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(5));

                let should_restart = {
                    let mut process_guard = process_arc.lock().unwrap();
                    if let Some(ref mut child) = *process_guard {
                        match child.try_wait() {
                            Ok(Some(_)) => {
                                // Process has exited, need to restart
                                #[cfg(debug_assertions)]
                                eprintln!("powermetrics process died, restarting...");
                                true
                            }
                            Ok(None) => false, // Still running
                            Err(_e) => {
                                #[cfg(debug_assertions)]
                                eprintln!("Error checking powermetrics status: {_e}");
                                true
                            }
                        }
                    } else {
                        false
                    }
                };

                if should_restart {
                    if let Ok(running) = is_running.lock() {
                        if !*running {
                            break; // Manager is shutting down
                        }
                    }

                    // Create new channel for the restarted process
                    let (_new_tx, new_rx) = mpsc::channel();

                    // Restart powermetrics
                    if let Err(_e) =
                        Self::restart_powermetrics(&process_arc, &data_buffer, new_rx, interval_ms)
                    {
                        #[cfg(debug_assertions)]
                        eprintln!("Failed to restart powermetrics: {_e}");
                    }
                }
            }
        });

        Ok(manager)
    }

    /// Start the powermetrics subprocess with stdout piping
    fn start_powermetrics(
        &self,
        command_rx: Receiver<ReaderCommand>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::new("sudo");
        cmd.args([
            "nice",
            "-n",
            "10", // Lower priority to reduce system impact
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal,tasks",
            "--show-process-gpu",
            "-i",
            &self.interval_ms.to_string(), // Use configurable interval
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped()) // Capture stdout instead of writing to file
        .stderr(Stdio::null());

        // On Unix, create a new process group so we can kill all children
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;

        // Start reader thread
        let data_buffer = self.data_buffer.clone();
        thread::spawn(move || {
            Self::reader_thread(stdout, data_buffer, command_rx);
        });

        let mut process_guard = self.process.lock().unwrap();
        *process_guard = Some(child);

        let mut is_running = self.is_running.lock().unwrap();
        *is_running = true;

        // Don't wait for initial data - let it collect asynchronously
        // This significantly improves startup time

        Ok(())
    }

    /// Reader thread that processes stdout from powermetrics
    fn reader_thread(
        stdout: std::process::ChildStdout,
        data_buffer: Arc<Mutex<VecDeque<String>>>,
        command_rx: Receiver<ReaderCommand>,
    ) {
        let reader = BufReader::new(stdout);
        let mut current_section = String::new();
        let mut in_section = false;

        for line in reader.lines() {
            // Check for shutdown command
            if let Ok(ReaderCommand::Shutdown) = command_rx.try_recv() {
                break;
            }

            let line = match line {
                Ok(l) => l,
                Err(_) => break, // Pipe broken, process died
            };

            // Detect start of new section
            if line.contains("*** Sampled system activity") {
                // If we have a complete section, store it
                if in_section && !current_section.is_empty() {
                    let mut buffer = data_buffer.lock().unwrap();
                    if buffer.len() >= 120 {
                        buffer.pop_front(); // Remove oldest
                    }
                    buffer.push_back(current_section.clone());
                }
                // Start new section
                current_section.clear();
                in_section = true;
            }

            if in_section {
                current_section.push_str(&line);
                current_section.push('\n');
            }
        }
    }

    /// Restart the powermetrics process
    fn restart_powermetrics(
        process_arc: &Arc<Mutex<Option<Child>>>,
        data_buffer: &Arc<Mutex<VecDeque<String>>>,
        command_rx: Receiver<ReaderCommand>,
        interval_ms: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Kill existing process if any
        {
            let mut process_guard = process_arc.lock().unwrap();
            if let Some(mut child) = process_guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        let mut cmd = Command::new("sudo");
        cmd.args([
            "nice",
            "-n",
            "10",
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal,tasks",
            "--show-process-gpu",
            "-i",
            &interval_ms.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

        // On Unix, create a new process group so we can kill all children
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;

        // Start new reader thread
        let data_buffer_clone = data_buffer.clone();
        thread::spawn(move || {
            Self::reader_thread(stdout, data_buffer_clone, command_rx);
        });

        let mut process_guard = process_arc.lock().unwrap();
        *process_guard = Some(child);

        Ok(())
    }

    /// Get the latest powermetrics data from the circular buffer
    fn get_latest_data_internal(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        // Get the most recent complete section from the buffer
        let latest_section = {
            let buffer = self.data_buffer.lock().unwrap();
            buffer.back().cloned()
        };

        if let Some(section) = latest_section {
            // Parse the data
            if let Ok(data) = parse_powermetrics_output(&section) {
                // Cache the data
                let mut last_data = self.last_data.lock().unwrap();
                *last_data = Some(data.clone());
                return Ok(data);
            }
        }

        // If we can't read fresh data, return cached data if available
        if let Some(cached) = self.last_data.lock().unwrap().clone() {
            return Ok(cached);
        }

        Err("No powermetrics data available".into())
    }

    /// Get process information from the latest powermetrics data
    pub fn get_process_info(&self) -> Vec<(String, u32, f64)> {
        let mut processes = Vec::new();

        // Get the most recent complete section from the buffer
        let latest_section = {
            let buffer = self.data_buffer.lock().unwrap();
            buffer.back().cloned()
        };

        if let Some(section) = latest_section {
            // Find the "*** Running tasks ***" section
            if let Some(tasks_start) = section.find("*** Running tasks ***") {
                let tasks_section = &section[tasks_start..];

                // Find the next section marker or use the rest of the content
                let tasks_end = tasks_section[20..]
                    .find("***")
                    .unwrap_or(tasks_section.len() - 20)
                    + 20;
                let tasks_content = &tasks_section[..tasks_end];

                let mut in_header = false;

                // Parse process information from powermetrics output
                // Format: Name ID CPU ms/s User% Deadlines Deadlines Wakeups Wakeups GPU ms/s
                for line in tasks_content.lines() {
                    let line = line.trim();

                    // Skip empty lines and section markers
                    if line.is_empty() || line.contains("***") {
                        continue;
                    }

                    // Detect and skip header line
                    if line.contains("Name") && line.contains("ID") && line.contains("GPU ms/s") {
                        in_header = true;
                        continue;
                    }

                    // Skip lines until we're past the header
                    if in_header {
                        in_header = false;
                        continue;
                    }

                    // Process data lines - the format has fixed column positions
                    // Name (0-34), ID (35-41), CPU ms/s (42-51), User% (52-58),
                    // Deadlines (59-66, 67-74), Wakeups (75-82, 83-90), GPU ms/s (91-100)

                    // For robustness, we'll use a different approach:
                    // Split by whitespace but handle the known column structure
                    let parts: Vec<&str> = line.split_whitespace().collect();

                    // We need at least 9 parts (name, id, cpu, user%, 2 deadlines, 2 wakeups, gpu)
                    if parts.len() >= 9 {
                        // Try to find the PID - it should be an integer after the process name
                        let mut pid_index = None;
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                // Check if this could be a PID (positive integer)
                                if let Ok(pid) = part.parse::<u32>() {
                                    // Verify it's in a reasonable PID range
                                    if pid > 0 && pid < 100000 {
                                        pid_index = Some(i);
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(idx) = pid_index {
                            // We expect 8 numeric values after PID
                            if parts.len() >= idx + 8 {
                                if let Ok(pid) = parts[idx].parse::<u32>() {
                                    // GPU ms/s is the last value
                                    if let Ok(gpu_ms) = parts[idx + 7].parse::<f64>() {
                                        // Reconstruct process name from parts before PID
                                        let process_name = parts[0..idx].join(" ");

                                        // Include all processes for better visibility
                                        // We'll let the caller decide what to filter
                                        processes.push((process_name, pid, gpu_ms));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by GPU usage (highest first)
        processes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        processes
    }

    /// Get process information with both CPU and GPU metrics
    #[allow(dead_code)]
    pub fn get_process_info_detailed(&self) -> Vec<(String, u32, f64, f64)> {
        let mut processes = Vec::new();

        // Get the most recent complete section from the buffer
        let latest_section = {
            let buffer = self.data_buffer.lock().unwrap();
            buffer.back().cloned()
        };

        if let Some(section) = latest_section {
            // Find the "*** Running tasks ***" section
            if let Some(tasks_start) = section.find("*** Running tasks ***") {
                let tasks_section = &section[tasks_start..];

                let tasks_end = tasks_section[20..]
                    .find("***")
                    .unwrap_or(tasks_section.len() - 20)
                    + 20;
                let tasks_content = &tasks_section[..tasks_end];

                let mut in_header = false;

                for line in tasks_content.lines() {
                    let line = line.trim();

                    if line.is_empty() || line.contains("***") {
                        continue;
                    }

                    if line.contains("Name") && line.contains("ID") && line.contains("GPU ms/s") {
                        in_header = true;
                        continue;
                    }

                    if in_header {
                        in_header = false;
                        continue;
                    }

                    let parts: Vec<&str> = line.split_whitespace().collect();

                    if parts.len() >= 9 {
                        let mut pid_index = None;
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                if let Ok(pid) = part.parse::<u32>() {
                                    if pid > 0 && pid < 100000 {
                                        pid_index = Some(i);
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(idx) = pid_index {
                            if parts.len() >= idx + 8 {
                                if let Ok(pid) = parts[idx].parse::<u32>() {
                                    // CPU ms/s is at idx + 1
                                    if let Ok(cpu_ms) = parts[idx + 1].parse::<f64>() {
                                        // GPU ms/s is at idx + 7
                                        if let Ok(gpu_ms) = parts[idx + 7].parse::<f64>() {
                                            let process_name = parts[0..idx].join(" ");
                                            processes.push((process_name, pid, cpu_ms, gpu_ms));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by GPU usage first, then by CPU usage
        processes.sort_by(|a, b| {
            match b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal) {
                std::cmp::Ordering::Equal => {
                    b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)
                }
                other => other,
            }
        });

        processes
    }

    /// Get latest data as Option (for test compatibility)
    #[cfg(test)]
    pub fn get_latest_data(&self) -> Option<PowerMetricsData> {
        self.get_latest_data_result().ok()
    }

    /// Get latest data as Result
    pub fn get_latest_data_result(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        self.get_latest_data_internal()
    }

    /// Kill any existing powermetrics processes spawned by all-smi
    fn kill_existing_powermetrics_processes() {
        // Use ps auxww to see full command line without truncation
        if let Ok(output) = Command::new("ps").args(["auxww"]).output() {
            let ps_output = String::from_utf8_lossy(&output.stdout);
            let mut parent_pids = Vec::new();
            let mut all_pids = Vec::new();

            for line in ps_output.lines() {
                // Look for powermetrics processes spawned by all-smi
                if line.contains("sudo nice")
                    && line.contains("powermetrics")
                    && line.contains("--samplers")
                    && line.contains("cpu_power")
                {
                    // Extract PID (second column)
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        if let Ok(pid) = parts[1].parse::<u32>() {
                            parent_pids.push(pid);
                            all_pids.push(pid);
                        }
                    }
                }
                // Also look for powermetrics processes that might be orphaned
                else if line.contains("powermetrics")
                    && line.contains("--samplers")
                    && line.contains("cpu_power")
                    && line.contains("gpu_power")
                    && !line.contains("grep")
                {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        if let Ok(pid) = parts[1].parse::<u32>() {
                            all_pids.push(pid);
                        }
                    }
                }
            }

            // Kill parent processes with their process groups first
            #[cfg(unix)]
            {
                for pid in &parent_pids {
                    unsafe {
                        // Kill the entire process group (negative PID) without sudo
                        libc::kill(-(*pid as i32), libc::SIGTERM);
                    }
                }

                // Wait a moment for processes to terminate gracefully
                thread::sleep(Duration::from_millis(200));

                // Force kill any remaining processes individually
                for pid in all_pids {
                    unsafe {
                        libc::kill(pid as i32, libc::SIGKILL);
                    }
                }
            }

            #[cfg(not(unix))]
            {
                // On non-Unix systems, we still need to use sudo
                for pid in &parent_pids {
                    let _ = Command::new("sudo")
                        .args(["kill", "-TERM", &format!("-{pid}")])
                        .output();
                }

                thread::sleep(Duration::from_millis(200));

                for pid in all_pids {
                    let _ = Command::new("sudo")
                        .args(["kill", "-9", &pid.to_string()])
                        .output();
                }
            }
        }
    }
}

impl Drop for PowerMetricsManager {
    fn drop(&mut self) {
        // Debug: PowerMetricsManager Drop called
        // Stop the monitoring flag
        if let Ok(mut is_running) = self.is_running.lock() {
            *is_running = false;
        }

        // Send shutdown command to reader thread
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(ReaderCommand::Shutdown);
        }

        // Kill the powermetrics process
        if let Ok(mut process_guard) = self.process.lock() {
            if let Some(mut child) = process_guard.take() {
                // Try to kill the child process directly first (no sudo needed for our own child)
                let _ = child.kill();
                let _ = child.wait();

                #[cfg(unix)]
                {
                    // If the process group still exists, try to kill it
                    // This shouldn't require sudo since we spawned the process
                    let pid = child.id();
                    unsafe {
                        // Use libc to send signal to process group without sudo
                        libc::kill(-(pid as i32), libc::SIGTERM);
                        thread::sleep(Duration::from_millis(100));
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
            }
        }

        // No temporary files to clean up with in-memory buffer approach
    }
}

// Global singleton instance
use once_cell::sync::Lazy;
static POWERMETRICS_MANAGER: Lazy<Mutex<Option<Arc<PowerMetricsManager>>>> =
    Lazy::new(|| Mutex::new(None));

/// Initialize the global PowerMetricsManager
pub fn initialize_powermetrics_manager(
    interval_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut manager_guard = POWERMETRICS_MANAGER.lock().unwrap();
    if manager_guard.is_none() {
        let manager = PowerMetricsManager::new(interval_secs)?;
        *manager_guard = Some(Arc::new(manager));
    }
    Ok(())
}

/// Get the global PowerMetricsManager instance
pub fn get_powermetrics_manager() -> Option<Arc<PowerMetricsManager>> {
    POWERMETRICS_MANAGER.lock().unwrap().clone()
}

/// Shutdown the global PowerMetricsManager
pub fn shutdown_powermetrics_manager() {
    // Debug: shutdown_powermetrics_manager called

    // Take the manager out to ensure Drop is called
    let manager_arc = {
        let mut manager_guard = POWERMETRICS_MANAGER.lock().unwrap();
        manager_guard.take()
    };

    // If we had a manager, clean it up
    if let Some(manager) = manager_arc {
        // Debug: Shutting down PowerMetricsManager

        // Stop the monitoring flag
        if let Ok(mut is_running) = manager.is_running.lock() {
            *is_running = false;
        }

        // Send shutdown command to reader thread
        if let Some(tx) = &manager.command_tx {
            let _ = tx.send(ReaderCommand::Shutdown);
        }

        // Kill the powermetrics process
        if let Ok(mut process_guard) = manager.process.lock() {
            if let Some(mut child) = process_guard.take() {
                // Try to kill the child process directly first (no sudo needed for our own child)
                let _ = child.kill();
                let _ = child.wait();

                #[cfg(unix)]
                {
                    // If the process group still exists, try to kill it
                    // This shouldn't require sudo since we spawned the process
                    let pid = child.id();
                    unsafe {
                        // Use libc to send signal to process group without sudo
                        libc::kill(-(pid as i32), libc::SIGTERM);
                        thread::sleep(Duration::from_millis(100));
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
            }
        }

        // The Arc will be dropped when this function ends
    }

    // Extra cleanup to catch any orphaned processes
    // Note: We don't call kill_existing_powermetrics_processes() here anymore
    // because it would require sudo. We've already killed our own child process above.
    // PowerMetricsManager::kill_existing_powermetrics_processes();

    // Give Drop a moment to execute
    thread::sleep(Duration::from_millis(100));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_powermetrics_data_cache() {
        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000, // 1 second for testing
        };

        // Test initial state - no data
        assert!(manager.get_latest_data().is_none());

        // Create test data
        let test_data = PowerMetricsData {
            e_cluster_active_residency: 25.5,
            p_cluster_active_residency: 75.5,
            e_cluster_frequency: 1020,
            p_cluster_frequency: 3000,
            cpu_power_mw: 1500.0,
            core_active_residencies: vec![],
            core_frequencies: vec![],
            core_cluster_types: vec![],
            gpu_active_residency: 45.5,
            gpu_frequency: 1200,
            gpu_power_mw: 2500.0,
            ane_power_mw: 100.0,
            combined_power_mw: 4100.0,
            thermal_pressure_level: Some("Nominal".to_string()),
        };

        // Set cached data
        {
            let mut last_data = manager.last_data.lock().unwrap();
            *last_data = Some(test_data.clone());
        }

        // Verify cached data
        let cached = manager.get_latest_data();
        assert!(cached.is_some());
        let cached_data = cached.unwrap();
        assert_eq!(cached_data.gpu_active_residency, 45.5);
        assert_eq!(cached_data.gpu_frequency, 1200);
        assert_eq!(cached_data.e_cluster_active_residency, 25.5);
        assert_eq!(cached_data.p_cluster_active_residency, 75.5);
    }

    #[test]
    fn test_singleton_instance() {
        // Clean up any existing instance
        shutdown_powermetrics_manager();

        // Skip this test if not running as root (required for powermetrics)
        if std::env::var("USER").unwrap_or_default() != "root" {
            eprintln!("Skipping test that requires root privileges");
            return;
        }

        // Initialize the manager with 1 second interval for testing
        let _ = initialize_powermetrics_manager(1);

        // Test that we get the same instance
        let manager1 = get_powermetrics_manager();
        let manager2 = get_powermetrics_manager();

        assert!(manager1.is_some());
        assert!(manager2.is_some());

        // Both should return the same Arc pointer
        assert!(Arc::ptr_eq(&manager1.unwrap(), &manager2.unwrap()));

        // Clean up
        shutdown_powermetrics_manager();
    }

    #[test]
    fn test_get_latest_data_from_buffer() {
        // Create test powermetrics output that matches expected format
        let test_output = r#"*** Sampled system activity (Fri Nov 15 10:00:00 2024 -0800) (1000ms elapsed) ***

*** Processor usage ***
E-Cluster HW active frequency: 1020 MHz
E-Cluster HW active residency: 25.5%

P-Cluster HW active frequency: 3000 MHz  
P-Cluster HW active residency: 75.5%

CPU Power: 1500 mW

*** GPU usage ***
GPU HW active frequency: 1200 MHz
GPU HW active residency: 45.5%
GPU Power: 2500 mW

ANE Power: 100 mW
Combined Power (CPU + GPU + ANE): 4100 mW

*** Sampled system activity"#;

        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000,
        };

        // Add test data to buffer
        {
            let mut buffer = manager.data_buffer.lock().unwrap();
            buffer.push_back(test_output.to_string());
        }

        // Get data from buffer
        let result = manager.get_latest_data_result();

        // Check that data was parsed
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.e_cluster_frequency, 1020);
        assert_eq!(data.e_cluster_active_residency, 25.5);
        assert_eq!(data.p_cluster_frequency, 3000);
        assert_eq!(data.p_cluster_active_residency, 75.5);
    }

    #[test]
    fn test_circular_buffer_limits() {
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(120)));

        // Add more than capacity
        for i in 0..150 {
            let mut buf = buffer.lock().unwrap();
            if buf.len() >= 120 {
                buf.pop_front();
            }
            buf.push_back(format!("Sample {i}"));
        }

        // Should only have last 120 samples
        let buf = buffer.lock().unwrap();
        assert_eq!(buf.len(), 120);
        assert_eq!(buf.front().unwrap(), "Sample 30");
        assert_eq!(buf.back().unwrap(), "Sample 149");
    }

    #[test]
    fn test_reader_thread_line_parsing() {
        use std::io::Cursor;

        // Create test input with mixed complete and incomplete lines
        let test_input =
            "*** Sampled system activity\nLine 1\nLine 2\n*** Sampled system activity\nLine 3\n";
        let cursor = Cursor::new(test_input);

        let data_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let (_tx, _rx) = mpsc::channel::<ReaderCommand>();

        // Simulate reader thread behavior
        let reader = BufReader::new(cursor);
        let mut current_section = String::new();
        let mut in_section = false;

        for line in reader.lines() {
            let line = line.unwrap();

            if line.contains("*** Sampled system activity") {
                if in_section && !current_section.is_empty() {
                    let mut buffer = data_buffer.lock().unwrap();
                    buffer.push_back(current_section.clone());
                }
                current_section.clear();
                in_section = true;
            }

            if in_section {
                current_section.push_str(&line);
                current_section.push('\n');
            }
        }

        // Should have captured one complete section
        let buffer = data_buffer.lock().unwrap();
        assert_eq!(buffer.len(), 1);
        assert!(buffer[0].contains("Line 1"));
        assert!(buffer[0].contains("Line 2"));
    }

    #[test]
    fn test_process_info_extraction_from_buffer() {
        let test_output = r#"*** Sampled system activity (Fri Nov 15 10:00:00 2024 -0800) (1000ms elapsed) ***

*** Running tasks ***

Name                    ID      CPU ms/s  User%  Deadlines  Deadlines  Wakeups  Wakeups  GPU ms/s
                                                  (Called)   (Missed)   (Intr)   (Pkg)           
kernel_task             0       45.23     0.00   0          0          1234     567      0.00
WindowServer            123     12.34     1.23   100        0          200      50       15.67
Code                    456     78.90     5.67   50         0          150      30       8.34
Firefox                 789     34.56     2.34   75         0          125      25       22.45

*** End of tasks ***"#;

        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000,
        };

        // Add test data to buffer
        {
            let mut buffer = manager.data_buffer.lock().unwrap();
            buffer.push_back(test_output.to_string());
        }

        // Get process info
        let processes = manager.get_process_info();

        // Verify process extraction (kernel_task has 0.00 GPU so might be filtered)
        assert!(processes.len() >= 3);

        // Check that processes are sorted by GPU usage (highest first)
        assert_eq!(processes[0].0, "Firefox");
        assert_eq!(processes[0].1, 789);
        assert_eq!(processes[0].2, 22.45);

        assert_eq!(processes[1].0, "WindowServer");
        assert_eq!(processes[1].1, 123);
        assert_eq!(processes[1].2, 15.67);

        assert_eq!(processes[2].0, "Code");
        assert_eq!(processes[2].1, 456);
        assert_eq!(processes[2].2, 8.34);
    }

    #[test]
    fn test_buffer_with_multiple_sections() {
        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000,
        };

        // Add multiple sections to buffer
        let section1 = r#"*** Sampled system activity (Fri Nov 15 10:00:00 2024 -0800) (1000ms elapsed) ***
*** Processor usage ***
E-Cluster HW active frequency: 1000 MHz
E-Cluster HW active residency: 20.0%
CPU Power: 1000 mW
"#;

        let section2 = r#"*** Sampled system activity (Fri Nov 15 10:00:01 2024 -0800) (1000ms elapsed) ***
*** Processor usage ***
E-Cluster HW active frequency: 1020 MHz
E-Cluster HW active residency: 25.5%
P-Cluster HW active frequency: 3000 MHz
P-Cluster HW active residency: 75.5%
CPU Power: 1500 mW
*** GPU usage ***
GPU HW active frequency: 1200 MHz
GPU HW active residency: 45.5%
GPU Power: 2500 mW
ANE Power: 100 mW
Combined Power (CPU + GPU + ANE): 4100 mW
"#;

        {
            let mut buffer = manager.data_buffer.lock().unwrap();
            buffer.push_back(section1.to_string());
            buffer.push_back(section2.to_string());
        }

        // Should get the latest (most recent) data
        let result = manager.get_latest_data_result();
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.e_cluster_frequency, 1020); // From section2, not section1
        assert_eq!(data.gpu_frequency, 1200);
        assert_eq!(data.combined_power_mw, 4100.0);
    }

    #[test]
    fn test_empty_buffer_fallback_to_cache() {
        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000,
        };

        // Create cached data
        let cached_data = PowerMetricsData {
            e_cluster_active_residency: 30.0,
            p_cluster_active_residency: 70.0,
            e_cluster_frequency: 1100,
            p_cluster_frequency: 2900,
            cpu_power_mw: 1600.0,
            core_active_residencies: vec![],
            core_frequencies: vec![],
            core_cluster_types: vec![],
            gpu_active_residency: 50.0,
            gpu_frequency: 1300,
            gpu_power_mw: 2600.0,
            ane_power_mw: 110.0,
            combined_power_mw: 4310.0,
            thermal_pressure_level: Some("Fair".to_string()),
        };

        // Set cached data
        {
            let mut last_data = manager.last_data.lock().unwrap();
            *last_data = Some(cached_data.clone());
        }

        // With empty buffer, should return cached data
        let result = manager.get_latest_data_result();
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.gpu_frequency, 1300);
        assert_eq!(data.combined_power_mw, 4310.0);
    }

    #[test]
    fn test_malformed_section_handling() {
        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000,
        };

        // Add malformed section
        let malformed = "This is not valid powermetrics output";
        {
            let mut buffer = manager.data_buffer.lock().unwrap();
            buffer.push_back(malformed.to_string());
        }

        // Parser returns default data for malformed input, which is valid but has all zeros
        let result = manager.get_latest_data_result();
        assert!(result.is_ok());
        let data = result.unwrap();
        // Verify it's default/empty data
        assert_eq!(data.e_cluster_frequency, 0);
        assert_eq!(data.gpu_frequency, 0);
        assert_eq!(data.cpu_power_mw, 0.0);
    }

    #[test]
    fn test_process_info_detailed() {
        let test_output = r#"*** Sampled system activity (Fri Nov 15 10:00:00 2024 -0800) (1000ms elapsed) ***

*** Running tasks ***

Name                    ID      CPU ms/s  User%  Deadlines  Deadlines  Wakeups  Wakeups  GPU ms/s
                                                  (Called)   (Missed)   (Intr)   (Pkg)           
Safari                  1001    56.78     3.45   80         0          175      35       18.90
Chrome Helper           2002    89.01     6.78   120        5          250      60       25.34
Slack                   3003    23.45     1.89   40         0          100      20       5.67

*** End of tasks ***"#;

        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000,
        };

        // Add test data to buffer
        {
            let mut buffer = manager.data_buffer.lock().unwrap();
            buffer.push_back(test_output.to_string());
        }

        // Get detailed process info
        let processes = manager.get_process_info_detailed();

        // Verify detailed process extraction
        assert_eq!(processes.len(), 3);

        // Check sorting by GPU usage first, then CPU
        assert_eq!(processes[0].0, "Chrome Helper");
        assert_eq!(processes[0].1, 2002);
        assert_eq!(processes[0].2, 89.01); // CPU ms/s
        assert_eq!(processes[0].3, 25.34); // GPU ms/s

        assert_eq!(processes[1].0, "Safari");
        assert_eq!(processes[1].2, 56.78); // CPU ms/s
        assert_eq!(processes[1].3, 18.90); // GPU ms/s
    }

    #[test]
    fn test_reader_thread_shutdown() {
        use std::io::Cursor;
        use std::time::Duration;

        let test_input = "Line 1\nLine 2\nLine 3\n";
        let cursor = Cursor::new(test_input);

        let data_buffer = Arc::new(Mutex::new(VecDeque::new()));
        let (tx, rx) = mpsc::channel::<ReaderCommand>();

        // Spawn a thread simulating the reader
        let buffer_clone = data_buffer.clone();
        let handle = thread::spawn(move || {
            let reader = BufReader::new(cursor);
            for line in reader.lines() {
                // Check for shutdown
                if let Ok(ReaderCommand::Shutdown) = rx.try_recv() {
                    break;
                }

                if let Ok(line) = line {
                    let mut buffer = buffer_clone.lock().unwrap();
                    buffer.push_back(line);
                }

                thread::sleep(Duration::from_millis(10));
            }
        });

        // Let it read some lines
        thread::sleep(Duration::from_millis(50));

        // Send shutdown command
        let _ = tx.send(ReaderCommand::Shutdown);

        // Wait for thread to finish
        let _ = handle.join();

        // The thread completed
        // No assertions needed - test passes if no panic
    }

    #[test]
    fn test_kill_existing_processes() {
        // This test verifies the kill_existing_powermetrics_processes function
        // doesn't panic and completes successfully
        PowerMetricsManager::kill_existing_powermetrics_processes();
        // If we get here without panic, the test passes
    }

    #[test]
    fn test_buffer_overflow_protection() {
        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(120))),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
            interval_ms: 1000,
        };

        // Fill buffer beyond capacity
        {
            let mut buffer = manager.data_buffer.lock().unwrap();
            for i in 0..200 {
                if buffer.len() >= 120 {
                    buffer.pop_front();
                }
                buffer.push_back(format!("Section {i}"));
            }
        }

        // Verify buffer size is maintained at limit
        let buffer = manager.data_buffer.lock().unwrap();
        assert_eq!(buffer.len(), 120);
        assert!(buffer.back().unwrap().contains("Section 199"));
    }

    #[test]
    fn test_concurrent_buffer_access() {
        let buffer = Arc::new(Mutex::new(VecDeque::new()));
        let mut handles = vec![];

        // Spawn multiple threads accessing the buffer
        for i in 0..5 {
            let buffer_clone = buffer.clone();
            let handle = thread::spawn(move || {
                for j in 0..20 {
                    let mut buf = buffer_clone.lock().unwrap();
                    buf.push_back(format!("Thread {i} - Item {j}"));
                    drop(buf); // Explicitly release lock
                    thread::sleep(Duration::from_micros(100));
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all items were added
        let final_buffer = buffer.lock().unwrap();
        assert_eq!(final_buffer.len(), 100); // 5 threads * 20 items
    }
}
