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

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Execute a command with a timeout.
/// Returns Ok(Output) if the command completes within the timeout,
/// Err if timeout occurs or command fails to start.
///
/// IMPORTANT: This function properly kills the child process on timeout
/// to prevent process accumulation.
pub fn run_command_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> io::Result<Output> {
    // Spawn the child process
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let start = Instant::now();
    let poll_interval = Duration::from_millis(10);

    // Poll for completion with timeout
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process completed - read output
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();

                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_end(&mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_end(&mut stderr);
                }

                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                // Process still running - check timeout
                if start.elapsed() >= timeout {
                    // Timeout! Kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie process
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("Command '{command}' timed out after {timeout:?}"),
                    ));
                }
                // Sleep briefly before polling again
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                // Error checking process status
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        }
    }
}

/// Execute a command with a short timeout suitable for container environments.
/// Default timeout is 1 second for fast failure in containers.
pub fn run_command_fast_fail(command: &str, args: &[&str]) -> io::Result<Output> {
    // Check if we're in a container environment
    let timeout = if is_container_environment() {
        Duration::from_millis(500) // Very short timeout in containers
    } else {
        Duration::from_secs(2) // Normal timeout for bare metal
    };

    run_command_with_timeout(command, args, timeout)
}

/// Detect if we're running in a container environment
fn is_container_environment() -> bool {
    // Check for common container indicators
    std::path::Path::new("/.dockerenv").exists()
        || std::path::Path::new("/run/.containerenv").exists()
        || std::env::var("KUBERNETES_SERVICE_HOST").is_ok()
        || std::env::var("CONTAINER_RUNTIME").is_ok()
        || check_cgroup_container()
}

/// Check cgroup for container indicators
fn check_cgroup_container() -> bool {
    if let Ok(contents) = std::fs::read_to_string("/proc/self/cgroup") {
        contents.contains("/docker/")
            || contents.contains("/lxc/")
            || contents.contains("/kubepods/")
            || contents.contains("/containerd/")
    } else {
        false
    }
}
