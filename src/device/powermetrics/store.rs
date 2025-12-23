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
use std::sync::{Arc, Mutex};

use crate::device::powermetrics_parser::{parse_powermetrics_output, PowerMetricsData};

/// Stores powermetrics data in a circular buffer
pub struct MetricsStore {
    /// Circular buffer storing complete powermetrics sections
    data_buffer: Arc<Mutex<VecDeque<String>>>,
    /// Cache of the last parsed data
    last_data: Arc<Mutex<Option<PowerMetricsData>>>,
}

impl MetricsStore {
    /// Create a new MetricsStore with the specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            data_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            last_data: Arc::new(Mutex::new(None)),
        }
    }

    /// Add a new section to the buffer (used in tests)
    #[cfg(test)]
    pub fn add_section(&self, section: String, capacity: usize) {
        let mut buffer = self.data_buffer.lock().unwrap();
        if buffer.len() >= capacity {
            buffer.pop_front(); // Remove oldest
        }
        buffer.push_back(section);
    }

    /// Get the buffer for direct access (used by collector)
    pub fn get_buffer(&self) -> Arc<Mutex<VecDeque<String>>> {
        self.data_buffer.clone()
    }

    /// Get the latest powermetrics data from the circular buffer
    pub fn get_latest_data(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
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
    #[allow(dead_code)]
    pub fn get_process_info(&self) -> Vec<(String, u32, f64)> {
        let mut processes = Vec::new();

        // Get the most recent complete section from the buffer
        let latest_section = {
            let buffer = self.data_buffer.lock().unwrap();
            buffer.back().cloned()
        };

        if let Some(section) = latest_section {
            let lines: Vec<&str> = section.lines().collect();
            let mut in_tasks_section = false;
            let mut in_gpu_section = false;

            for (i, line) in lines.iter().enumerate() {
                // Look for the start of the tasks section
                if line.contains("ALL_TASKS") {
                    in_tasks_section = true;
                    continue;
                }

                // Check if we've entered the GPU Power section
                if line.contains("GPU Power") {
                    in_gpu_section = true;
                    in_tasks_section = false;
                    continue;
                }

                // Look for process info in tasks section
                if in_tasks_section && !in_gpu_section {
                    // Skip lines that don't look like process info
                    if !line.contains("pid ") {
                        continue;
                    }

                    // Parse process info from lines like:
                    // pid 12345  name  ProcessName
                    if let Some(pid_pos) = line.find("pid ") {
                        let after_pid = &line[pid_pos + 4..];
                        let parts: Vec<&str> = after_pid.split_whitespace().collect();
                        if parts.len() >= 3 && parts[1] == "name" {
                            if let Ok(pid) = parts[0].parse::<u32>() {
                                let name = parts[2..].join(" ");
                                // Default GPU usage to 0.0
                                processes.push((name, pid, 0.0));
                            }
                        }
                    }
                }

                // Look for GPU usage in GPU Power section
                if in_gpu_section {
                    // Look for lines with process GPU usage
                    // Format varies but typically includes pid and percentage
                    if line.contains("pid ") && (line.contains('%') || line.contains("GPU")) {
                        // Try to extract pid and GPU usage
                        if let Some(pid_pos) = line.find("pid ") {
                            let after_pid = &line[pid_pos + 4..];
                            let parts: Vec<&str> = after_pid.split_whitespace().collect();
                            if !parts.is_empty() {
                                if let Ok(pid) = parts[0].parse::<u32>() {
                                    // Look for percentage in remaining parts
                                    for part in &parts[1..] {
                                        if part.ends_with('%') {
                                            let percent_str = part.trim_end_matches('%');
                                            if let Ok(gpu_usage) = percent_str.parse::<f64>() {
                                                // Update existing process or add new one
                                                if let Some(proc) =
                                                    processes.iter_mut().find(|(_, p, _)| *p == pid)
                                                {
                                                    proc.2 = gpu_usage;
                                                } else {
                                                    // If we have the next line with the name, get it
                                                    if i + 1 < lines.len() {
                                                        let next_line = lines[i + 1];
                                                        if next_line.contains("name ") {
                                                            if let Some(name_pos) =
                                                                next_line.find("name ")
                                                            {
                                                                let name = next_line
                                                                    [name_pos + 5..]
                                                                    .trim();
                                                                processes.push((
                                                                    name.to_string(),
                                                                    pid,
                                                                    gpu_usage,
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by GPU usage (descending)
        processes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Limit to processes with actual GPU usage
        processes.retain(|(_, _, gpu_usage)| *gpu_usage > 0.0);

        processes
    }

    /// Clear all stored data
    pub fn clear(&self) {
        let mut buffer = self.data_buffer.lock().unwrap();
        buffer.clear();

        let mut last_data = self.last_data.lock().unwrap();
        *last_data = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_overflow_protection() {
        let capacity = 5;
        let store = MetricsStore::new(capacity);

        // Add more sections than capacity
        for i in 0..10 {
            store.add_section(format!("Section {i}"), capacity);
        }

        // Verify buffer size is maintained at limit
        let buffer = store.data_buffer.lock().unwrap();
        assert_eq!(buffer.len(), capacity);
        assert!(buffer.back().unwrap().contains("Section 9"));
        assert!(buffer.front().unwrap().contains("Section 5"));
    }

    #[test]
    fn test_concurrent_buffer_access() {
        use std::thread;
        use std::time::Duration;

        let capacity = 100;
        let store = Arc::new(MetricsStore::new(capacity));
        let mut handles = vec![];

        // Spawn multiple threads accessing the buffer
        for i in 0..5 {
            let store_clone = store.clone();
            let handle = thread::spawn(move || {
                for j in 0..20 {
                    store_clone.add_section(format!("Thread {i} - Item {j}"), capacity);
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
        let buffer = store.data_buffer.lock().unwrap();
        assert_eq!(buffer.len(), capacity); // 5 threads * 20 items
    }
}
