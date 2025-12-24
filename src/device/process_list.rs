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

use crate::device::types::ProcessInfo;
use std::collections::{HashMap, HashSet};
use sysinfo::{ProcessStatus, System};

/// Get all system processes with GPU usage information
pub fn get_all_processes(system: &System, gpu_pids: &HashSet<u32>) -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();

        let uses_gpu = gpu_pids.contains(&pid_u32);

        // Get process priority and nice values
        let (priority, nice_value) = get_process_priority_nice(pid_u32);

        // Get process information
        let process_info = ProcessInfo {
            device_id: 0, // Will be set by GPU-specific code if uses_gpu
            device_uuid: if uses_gpu {
                "GPU".to_string()
            } else {
                String::new()
            },
            pid: pid_u32,
            process_name: process.name().to_string_lossy().to_string(),
            used_memory: 0, // GPU memory, will be set by GPU-specific code
            cpu_percent: process.cpu_usage() as f64,
            memory_percent: (process.memory() as f64 / system.total_memory() as f64) * 100.0,
            memory_rss: process.memory(),         // Already in bytes
            memory_vms: process.virtual_memory(), // Already in bytes
            user: get_process_user(process),
            state: convert_process_state(process.status()),
            start_time: format!("{}", process.start_time()),
            cpu_time: process.run_time(),
            command: get_process_command(process),
            ppid: process.parent().map(|p| p.as_u32()).unwrap_or(0),
            threads: 1, // sysinfo doesn't provide thread count directly
            uses_gpu,
            priority,
            nice_value,
            gpu_utilization: 0.0, // Will be set by GPU-specific code
        };

        processes.push(process_info);
    }

    // Sort by PID for consistent ordering
    processes.sort_by_key(|p| p.pid);
    processes
}

/// Update a process cache in place, reusing existing ProcessInfo objects where possible.
/// This reduces memory allocation overhead compared to creating new objects each cycle.
/// Returns a Vec of ProcessInfo cloned from the cache for the current snapshot.
pub fn update_process_cache(
    system: &System,
    gpu_pids: &HashSet<u32>,
    cache: &mut HashMap<u32, ProcessInfo>,
) -> Vec<ProcessInfo> {
    // Track which PIDs are still alive this cycle
    let mut seen_pids: HashSet<u32> = HashSet::with_capacity(system.processes().len());
    let total_memory = system.total_memory();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        seen_pids.insert(pid_u32);

        let uses_gpu = gpu_pids.contains(&pid_u32);

        if let Some(cached) = cache.get_mut(&pid_u32) {
            // Update existing entry - only update dynamic fields to reduce allocations
            cached.cpu_percent = process.cpu_usage() as f64;
            cached.memory_percent = (process.memory() as f64 / total_memory as f64) * 100.0;
            cached.memory_rss = process.memory();
            cached.memory_vms = process.virtual_memory();
            cached.state = convert_process_state(process.status());
            cached.cpu_time = process.run_time();
            // Update GPU status (may change if process starts/stops using GPU)
            cached.uses_gpu = uses_gpu;
            if uses_gpu && cached.device_uuid.is_empty() {
                cached.device_uuid = "GPU".to_string();
            }
            // Note: Static fields like process_name, user, command, start_time, ppid are kept unchanged
            // They don't change during process lifetime
        } else {
            // New process - create full ProcessInfo entry
            let (priority, nice_value) = get_process_priority_nice(pid_u32);
            let process_info = ProcessInfo {
                device_id: 0,
                device_uuid: if uses_gpu {
                    "GPU".to_string()
                } else {
                    String::new()
                },
                pid: pid_u32,
                process_name: process.name().to_string_lossy().to_string(),
                used_memory: 0,
                cpu_percent: process.cpu_usage() as f64,
                memory_percent: (process.memory() as f64 / total_memory as f64) * 100.0,
                memory_rss: process.memory(),
                memory_vms: process.virtual_memory(),
                user: get_process_user(process),
                state: convert_process_state(process.status()),
                start_time: format!("{}", process.start_time()),
                cpu_time: process.run_time(),
                command: get_process_command(process),
                ppid: process.parent().map(|p| p.as_u32()).unwrap_or(0),
                threads: 1,
                uses_gpu,
                priority,
                nice_value,
                gpu_utilization: 0.0,
            };
            cache.insert(pid_u32, process_info);
        }
    }

    // Remove stale entries (processes that no longer exist)
    cache.retain(|pid, _| seen_pids.contains(pid));

    // Return a clone of the cached data as a Vec
    let mut processes: Vec<ProcessInfo> = cache.values().cloned().collect();
    processes.sort_by_key(|p| p.pid);
    processes
}

/// Convert sysinfo ProcessStatus to standard Unix state code
fn convert_process_state(status: ProcessStatus) -> String {
    // Convert the status to string and then map to single-letter codes
    let status_str = status.to_string();

    match status_str.as_str() {
        "Run" | "Runnable" | "Running" => "R",            // Running
        "Sleep" | "Sleeping" => "S",                      // Sleeping
        "Idle" => "I",                                    // Idle
        "Stop" | "Stopped" => "T",                        // Stopped (traced)
        "Zombie" => "Z",                                  // Zombie
        "Dead" => "X",                                    // Dead
        "Disk Sleep" | "UninterruptibleDiskSleep" => "D", // Uninterruptible disk sleep
        "Unknown" => "?",                                 // Unknown
        _ => "?",                                         // Any other state
    }
    .to_string()
}

/// Get process user name
fn get_process_user(process: &sysinfo::Process) -> String {
    if let Some(user_id) = process.user_id() {
        // Try to get username from user ID
        #[cfg(unix)]
        {
            use std::ffi::CStr;
            unsafe {
                let passwd = libc::getpwuid(**user_id);
                if !passwd.is_null() {
                    if let Ok(name) = CStr::from_ptr((*passwd).pw_name).to_str() {
                        return name.to_string();
                    }
                }
            }
        }
        user_id.to_string()
    } else {
        "unknown".to_string()
    }
}

/// Get process command line
fn get_process_command(process: &sysinfo::Process) -> String {
    let cmd = process.cmd();
    if cmd.is_empty() {
        format!("[{}]", process.name().to_string_lossy())
    } else {
        // Convert OsStr arguments to String
        cmd.iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Get process priority and nice value
#[allow(unused_variables)]
fn get_process_priority_nice(pid: u32) -> (i32, i32) {
    #[cfg(target_os = "linux")]
    {
        // On Linux, read from /proc/[pid]/stat
        // Format: pid (comm) state ppid ... priority nice ...
        // Priority is field 18 (17 0-indexed), Nice is field 19 (18 0-indexed)
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            // The process name is in parentheses and may contain spaces/parens
            // Find the last ) to properly split the fields
            if let Some(last_paren) = stat.rfind(')') {
                // Everything after the last ) contains the actual stat fields
                let after_name = &stat[last_paren + 1..];
                let fields: Vec<&str> = after_name.split_whitespace().collect();

                // After removing pid and (name), the remaining fields are:
                // 0: state, 1: ppid, 2: pgrp, 3: session, 4: tty_nr, 5: tpgid,
                // 6: flags, 7: minflt, 8: cminflt, 9: majflt, 10: cmajflt,
                // 11: utime, 12: stime, 13: cutime, 14: cstime,
                // 15: priority, 16: nice, ...
                if fields.len() > 16 {
                    let priority = fields
                        .get(15) // priority is at index 15 after the name
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(20);
                    let nice = fields
                        .get(16) // nice is at index 16 after the name
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(0);
                    return (priority, nice);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, use ps command to get priority and nice
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pri,nice"])
            .output()
        {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().collect();
            if lines.len() > 1 {
                let fields: Vec<&str> = lines[1].split_whitespace().collect();
                if fields.len() >= 2 {
                    let priority = fields[0].parse::<i32>().unwrap_or(20);
                    let nice = fields[1].parse::<i32>().unwrap_or(0);
                    return (priority, nice);
                }
            }
        }
    }

    // On Windows, process priority is managed differently than Unix systems.
    // Windows uses priority classes (Idle, Below Normal, Normal, Above Normal, High, Realtime)
    // rather than nice values. The sysinfo crate doesn't expose priority directly.
    // Return default values as Unix-style nice values don't apply to Windows.
    // Default values if unable to retrieve (for Windows or any other OS)
    (20, 0)
}

/// Merge GPU process information with system process list
pub fn merge_gpu_processes(all_processes: &mut [ProcessInfo], gpu_processes: Vec<ProcessInfo>) {
    // Create a map of GPU processes by PID
    let gpu_map: std::collections::HashMap<u32, ProcessInfo> =
        gpu_processes.into_iter().map(|p| (p.pid, p)).collect();

    // Update matching processes with GPU information
    for process in all_processes.iter_mut() {
        if let Some(gpu_process) = gpu_map.get(&process.pid) {
            process.device_id = gpu_process.device_id;
            process.device_uuid = gpu_process.device_uuid.clone();
            process.used_memory = gpu_process.used_memory;
            process.gpu_utilization = gpu_process.gpu_utilization;
            process.uses_gpu = true;
        }
    }
}
