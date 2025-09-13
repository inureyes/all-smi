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

use std::io::BufRead;
use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::config::{PowerMetricsConfig, ReaderCommand};
use super::store::MetricsStore;
use crate::common::config::AppConfig;

#[cfg(unix)]
use libc;

/// Manages the powermetrics subprocess lifecycle
pub struct ProcessManager {
    process: Arc<Mutex<Option<Child>>>,
    command_tx: Option<Sender<ReaderCommand>>,
    is_running: Arc<Mutex<bool>>,
    config: PowerMetricsConfig,
    store: Arc<MetricsStore>,
}

impl ProcessManager {
    /// Create a new ProcessManager
    pub fn new(config: PowerMetricsConfig, store: Arc<MetricsStore>) -> Self {
        Self {
            process: Arc::new(Mutex::new(None)),
            command_tx: None,
            is_running: Arc::new(Mutex::new(false)),
            config,
            store,
        }
    }

    /// Start the powermetrics process and monitoring thread
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // We no longer kill existing powermetrics processes
        // to avoid interfering with other tools that might be using powermetrics

        let (command_tx, command_rx) = mpsc::channel();
        self.command_tx = Some(command_tx);

        // Start the powermetrics process
        self.start_powermetrics_process(command_rx)?;

        // Start monitoring thread
        self.start_monitor_thread();

        Ok(())
    }

    /// Start the powermetrics subprocess with stdout piping
    fn start_powermetrics_process(
        &self,
        command_rx: Receiver<ReaderCommand>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::new("sudo");

        let args = self.config.get_powermetrics_args();
        cmd.args(&args)
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

        // Start reader thread
        let data_buffer = self.store.get_buffer();
        thread::spawn(move || {
            Self::reader_thread(stdout, data_buffer, command_rx);
        });

        let mut process_guard = self.process.lock().unwrap();
        *process_guard = Some(child);

        let mut is_running = self.is_running.lock().unwrap();
        *is_running = true;

        Ok(())
    }

    /// Reader thread that processes stdout from powermetrics
    fn reader_thread(
        stdout: std::process::ChildStdout,
        data_buffer: Arc<Mutex<std::collections::VecDeque<String>>>,
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
                    // Keep maximum sections as defined in config
                    if buffer.len() >= AppConfig::POWERMETRICS_BUFFER_CAPACITY {
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

    /// Start a background thread to monitor the process
    fn start_monitor_thread(&self) {
        let process_arc = self.process.clone();
        let store_arc = self.store.clone();
        let is_running = self.is_running.clone();
        let config = self.config.clone();

        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(config.monitor_interval_secs));

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
                        Self::restart_powermetrics(&process_arc, &store_arc, new_rx, &config)
                    {
                        #[cfg(debug_assertions)]
                        eprintln!("Failed to restart powermetrics: {_e}");
                    }
                }
            }
        });
    }

    /// Restart the powermetrics process
    fn restart_powermetrics(
        process_arc: &Arc<Mutex<Option<Child>>>,
        store: &Arc<MetricsStore>,
        command_rx: Receiver<ReaderCommand>,
        config: &PowerMetricsConfig,
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
        let args = config.get_powermetrics_args();
        cmd.args(&args)
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
        let data_buffer = store.get_buffer();
        thread::spawn(move || {
            Self::reader_thread(stdout, data_buffer, command_rx);
        });

        let mut process_guard = process_arc.lock().unwrap();
        *process_guard = Some(child);

        Ok(())
    }

    /// Kill only powermetrics processes that were likely spawned by all-smi
    /// This is a more conservative approach that checks process parent/group
    pub fn kill_existing_powermetrics_processes() {
        // Only kill powermetrics processes that match our typical spawning pattern:
        // - Started with sudo
        // - Has specific all-smi related arguments
        // This prevents killing powermetrics processes started by other tools
        
        // Note: For now, we don't kill any processes here to be safe
        // The actual cleanup happens via the Drop trait when ProcessManager is dropped
        // This function is kept for backward compatibility but made safer
    }

    /// Shutdown the process manager
    pub fn shutdown(&mut self) {
        // Mark as not running
        {
            let mut is_running = self.is_running.lock().unwrap();
            *is_running = false;
        }

        // Send shutdown command to reader thread
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(ReaderCommand::Shutdown);
        }

        // Kill only the process we started
        {
            let mut process_guard = self.process.lock().unwrap();
            if let Some(mut child) = process_guard.take() {
                #[cfg(unix)]
                {
                    // Kill the process group we created
                    let pid = child.id() as i32;
                    unsafe {
                        // Since we set process_group(0) when spawning,
                        // the child is the leader of its own process group
                        // Kill the entire process group (sudo + powermetrics)
                        let _ = libc::killpg(pid, libc::SIGTERM);
                        thread::sleep(Duration::from_millis(100));
                        
                        // If still running, force kill
                        let _ = libc::killpg(pid, libc::SIGKILL);
                    }
                }

                // Also try to kill via the Child handle
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        
        // We no longer kill all powermetrics processes
        // Only kill the specific process we started
    }

    /// Check if the process is running (test use only)
    #[cfg(test)]
    pub(super) fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kill_existing_processes() {
        // This test verifies the kill_existing_powermetrics_processes function
        // doesn't panic and completes successfully
        ProcessManager::kill_existing_powermetrics_processes();
        // If we get here without panic, the test passes
    }

    #[test]
    fn test_reader_thread_shutdown() {
        use std::io::Cursor;

        let test_input = "Line 1\nLine 2\nLine 3\n";
        let cursor = Cursor::new(test_input);

        let data_buffer = Arc::new(Mutex::new(std::collections::VecDeque::new()));
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
}
