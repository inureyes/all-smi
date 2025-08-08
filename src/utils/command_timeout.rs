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

use std::io;
use std::process::{Command, Output};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Execute a command with a timeout.
/// Returns Ok(Output) if the command completes within the timeout,
/// Err if timeout occurs or command fails to start.
pub fn run_command_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> io::Result<Output> {
    let (tx, rx) = mpsc::channel();

    // Clone the command and args for the thread
    let command = command.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    // Spawn a thread to run the command
    thread::spawn(move || {
        let output = Command::new(command).args(args).output();
        let _ = tx.send(output);
    });

    // Wait for the result with timeout
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("Command timed out after {timeout:?}"),
        )),
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
