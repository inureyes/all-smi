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

use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use sysinfo::Disks;
use tokio::sync::Mutex;

use crate::app_state::AppState;
use crate::cli::ViewArgs;
use crate::common::config::{AppConfig, EnvConfig};
use crate::device::{
    get_cpu_readers, get_gpu_readers, get_memory_readers, get_nvml_status_message,
    get_tenstorrent_status_message,
    platform_detection::{has_nvidia, has_tenstorrent},
    CpuInfo, GpuInfo, MemoryInfo, ProcessInfo,
};
use crate::network::NetworkClient;
use crate::storage::info::StorageInfo;
use crate::utils::{filter_docker_aware_disks, get_hostname};

/// Extract hostname from URL, handling both simple hostnames and full URLs
fn extract_hostname_from_url(url: &str) -> String {
    // Handle full URLs like "http://remote1:9090"
    if url.starts_with("http://") || url.starts_with("https://") {
        if let Some(start) = url.find("://") {
            let after_protocol = &url[start + 3..];
            if let Some(end) = after_protocol.find('/') {
                after_protocol[..end].to_string()
            } else {
                after_protocol.to_string()
            }
        } else {
            url.to_string()
        }
    } else {
        // Handle simple hostname:port format
        url.to_string()
    }
}

/// Extract the full host:port combination as unique identifier
fn extract_host_identifier(url: &str) -> String {
    extract_hostname_from_url(url)
}

pub struct DataCollector {
    app_state: Arc<Mutex<AppState>>,
}

impl DataCollector {
    pub fn new(app_state: Arc<Mutex<AppState>>) -> Self {
        Self { app_state }
    }

    pub async fn run_local_mode(&self, args: ViewArgs) {
        let mut profiler = crate::utils::StartupProfiler::new();
        profiler.checkpoint("Starting local mode data collection");
        let gpu_readers = get_gpu_readers();
        profiler.checkpoint("GPU readers initialized");
        let cpu_readers = get_cpu_readers();
        profiler.checkpoint("CPU readers initialized");
        let memory_readers = get_memory_readers();
        profiler.checkpoint("Memory readers initialized");

        let mut first_iteration = true;

        loop {
            let all_gpu_info: Vec<GpuInfo> = gpu_readers
                .iter()
                .flat_map(|reader| reader.get_gpu_info())
                .collect();
            if first_iteration {
                profiler.checkpoint("GPU info collected");
            }

            let all_cpu_info: Vec<CpuInfo> = cpu_readers
                .iter()
                .flat_map(|reader| reader.get_cpu_info())
                .collect();
            if first_iteration {
                profiler.checkpoint("CPU info collected");
            }

            let all_memory_info: Vec<MemoryInfo> = memory_readers
                .iter()
                .flat_map(|reader| reader.get_memory_info())
                .collect();
            if first_iteration {
                profiler.checkpoint("Memory info collected");
            }

            // Collect processes from GPU readers if available
            let mut all_processes: Vec<ProcessInfo> = gpu_readers
                .iter()
                .flat_map(|reader| reader.get_process_info())
                .collect();

            // If no GPU readers available, collect all system processes
            if gpu_readers.is_empty() {
                use crate::device::process_list::get_all_processes;
                use std::collections::HashSet;
                use sysinfo::{ProcessesToUpdate, System};

                // Use System::new() instead of new_all() for faster initialization
                let mut system = System::new();
                // Only refresh what we need: processes and memory
                system.refresh_processes(ProcessesToUpdate::All, true);
                system.refresh_memory();
                if first_iteration {
                    profiler.checkpoint("System processes refreshed (lightweight)");
                }
                let empty_gpu_pids = HashSet::new();
                all_processes = get_all_processes(&system, &empty_gpu_pids);
            }

            // Collect local storage information
            let all_storage_info = self.collect_local_storage_info().await;
            if first_iteration {
                profiler.checkpoint("Storage info collected");
            }

            self.update_local_state(
                all_gpu_info,
                all_cpu_info,
                all_memory_info,
                all_processes,
                all_storage_info,
            )
            .await;

            // Only log first iteration
            if first_iteration {
                profiler.checkpoint("First data collection complete");
                profiler.finish();
                first_iteration = false;
            }

            // Use adaptive interval for local mode
            let interval = args
                .interval
                .unwrap_or_else(|| EnvConfig::adaptive_interval(1));
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    }

    pub async fn run_remote_mode(
        &self,
        args: ViewArgs,
        mut hosts: Vec<String>,
        hostfile: Option<String>,
    ) {
        // Strip protocol prefix from command line hosts
        hosts = hosts
            .into_iter()
            .map(|host| {
                if let Some(stripped) = host.strip_prefix("http://") {
                    stripped.to_string()
                } else if let Some(stripped) = host.strip_prefix("https://") {
                    stripped.to_string()
                } else {
                    host
                }
            })
            .collect();

        // Load hosts from file if specified
        if let Some(file_path) = hostfile {
            hosts = self.load_hosts_from_file(hosts, file_path).await;
        }

        let client = self.create_http_client();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(
            EnvConfig::max_concurrent_connections(hosts.len()),
        ));
        let re = Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap();

        loop {
            let (
                all_gpu_info,
                all_cpu_info,
                all_memory_info,
                all_storage_info,
                connection_statuses,
            ) = self
                .fetch_remote_data(&hosts, &client, &semaphore, &re)
                .await;

            self.update_remote_state(
                all_gpu_info,
                all_cpu_info,
                all_memory_info,
                all_storage_info,
                connection_statuses,
                &hosts,
            )
            .await;

            // Use adaptive interval for remote mode based on node count
            let interval = args
                .interval
                .unwrap_or_else(|| EnvConfig::adaptive_interval(hosts.len()));
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    }

    async fn collect_local_storage_info(&self) -> Vec<StorageInfo> {
        let mut all_storage_info = Vec::new();
        let disks = Disks::new_with_refreshed_list();
        let hostname = get_hostname();

        // Use Docker-aware filtering
        let mut filtered_disks = filter_docker_aware_disks(&disks);

        // Sort disks by mount point for consistent ordering
        filtered_disks.sort_by(|a, b| {
            a.mount_point()
                .to_string_lossy()
                .cmp(&b.mount_point().to_string_lossy())
        });

        for (index, disk) in filtered_disks.iter().enumerate() {
            let mount_point_str = disk.mount_point().to_string_lossy();
            all_storage_info.push(StorageInfo {
                mount_point: mount_point_str.to_string(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                host_id: hostname.clone(), // For local mode, host_id is just the hostname
                hostname: hostname.clone(), // DNS hostname
                index: index as u32,
            });
        }

        all_storage_info
    }

    async fn load_hosts_from_file(&self, mut hosts: Vec<String>, file_path: String) -> Vec<String> {
        if let Ok(content) = fs::read_to_string(&file_path) {
            let file_hosts: Vec<String> = content
                .lines()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter(|s| !s.starts_with('#'))
                .map(|s| {
                    // Strip protocol prefix if present
                    if let Some(stripped) = s.strip_prefix("http://") {
                        stripped.to_string()
                    } else if let Some(stripped) = s.strip_prefix("https://") {
                        stripped.to_string()
                    } else {
                        s.to_string()
                    }
                })
                .collect();
            hosts.extend(file_hosts);
        }
        hosts
    }

    fn create_http_client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(200)
            .tcp_keepalive(Duration::from_secs(30))
            .http2_keep_alive_interval(Duration::from_secs(30))
            .build()
            .unwrap()
    }

    async fn update_local_state(
        &self,
        all_gpu_info: Vec<GpuInfo>,
        all_cpu_info: Vec<CpuInfo>,
        all_memory_info: Vec<MemoryInfo>,
        all_processes: Vec<ProcessInfo>,
        all_storage_info: Vec<StorageInfo>,
    ) {
        let mut state = self.app_state.lock().await;

        // Update GPU info with UUID matching
        if state.gpu_info.is_empty() {
            state.gpu_info = all_gpu_info;
        } else {
            for new_info in all_gpu_info {
                if let Some(old_info) = state
                    .gpu_info
                    .iter_mut()
                    .find(|info| info.uuid == new_info.uuid)
                {
                    *old_info = new_info;
                }
            }
        }

        state.cpu_info = all_cpu_info;
        state.memory_info = all_memory_info;

        // Sort processes based on current criteria
        let mut sorted_processes = all_processes;
        sorted_processes.sort_by(|a, b| {
            state
                .sort_criteria
                .sort_processes(a, b, state.sort_direction)
        });
        state.process_info = sorted_processes;

        state.storage_info = all_storage_info;

        // Update notifications (remove expired ones)
        state.notifications.update();

        // Only check NVML status if we're trying to monitor NVIDIA devices
        if has_nvidia() {
            if let Some(nvml_message) = get_nvml_status_message() {
                if !state.nvml_notification_shown {
                    if let Err(e) = state.notifications.warning(nvml_message) {
                        eprintln!("Failed to show NVML notification: {e}");
                    }
                    state.nvml_notification_shown = true;
                }
            }
        }

        // Only check Tenstorrent status if we're trying to monitor Tenstorrent devices
        if has_tenstorrent() {
            if let Some(tt_message) = get_tenstorrent_status_message() {
                if !state.tenstorrent_notification_shown {
                    if let Err(e) = state.notifications.warning(tt_message) {
                        eprintln!("Failed to show Tenstorrent notification: {e}");
                    }
                    state.tenstorrent_notification_shown = true;
                }
            }
        }

        // Rebellions error notifications are now handled by the reader itself

        // Update utilization history
        self.update_utilization_history(&mut state);

        // Update tabs
        self.update_tabs(&mut state);

        // Always clear loading state in local mode after first iteration
        state.loading = false;
    }

    async fn update_remote_state(
        &self,
        all_gpu_info: Vec<GpuInfo>,
        all_cpu_info: Vec<CpuInfo>,
        all_memory_info: Vec<MemoryInfo>,
        all_storage_info: Vec<StorageInfo>,
        connection_statuses: Vec<crate::app_state::ConnectionStatus>,
        hosts: &[String],
    ) {
        // Deduplicate storage info by instance and mount_point
        let mut deduplicated_storage: HashMap<String, StorageInfo> = HashMap::new();
        for storage in all_storage_info {
            let dedup_key = format!("{}:{}", storage.hostname, storage.mount_point);
            deduplicated_storage.insert(dedup_key, storage);
        }
        let mut final_storage_info: Vec<StorageInfo> = deduplicated_storage.into_values().collect();

        // Sort by hostname first, then by mount point for consistent ordering
        final_storage_info.sort_by(|a, b| match a.hostname.cmp(&b.hostname) {
            std::cmp::Ordering::Equal => a.mount_point.cmp(&b.mount_point),
            other => other,
        });

        let mut state = self.app_state.lock().await;

        // Only update GPU info if we have valid data (not empty and has memory info)
        if !all_gpu_info.is_empty() && all_gpu_info.iter().any(|gpu| gpu.total_memory > 0) {
            state.gpu_info = all_gpu_info;
        } else if state.gpu_info.is_empty() {
            // If we don't have any existing GPU info and the new data is invalid,
            // still update to show something (but history won't be updated due to the check)
            state.gpu_info = all_gpu_info;
        }

        state.cpu_info = all_cpu_info;
        state.memory_info = all_memory_info;
        state.storage_info = final_storage_info;

        // Update connection status and maintain known hosts
        self.update_connection_status(&mut state, connection_statuses, hosts);

        // Update utilization history
        self.update_utilization_history(&mut state);

        // Update tabs from all device hostnames (including disconnected ones)
        self.update_remote_tabs(&mut state);

        state.process_info = Vec::new(); // No process info in remote mode
        state.loading = false;
    }

    fn update_connection_status(
        &self,
        state: &mut AppState,
        connection_statuses: Vec<crate::app_state::ConnectionStatus>,
        hosts: &[String],
    ) {
        // Initialize known hosts if not already set
        if state.known_hosts.is_empty() {
            state.known_hosts = hosts.iter().map(|h| extract_host_identifier(h)).collect();
        }

        // Clear the reverse lookup map before rebuilding it
        state.hostname_to_host_id.clear();

        // Update connection status for each received status
        for mut status in connection_statuses {
            // Preserve actual_hostname from previous successful connection if current doesn't have it
            if status.actual_hostname.is_none() {
                if let Some(existing_status) = state.connection_status.get(&status.host_id) {
                    if let Some(existing_hostname) = &existing_status.actual_hostname {
                        status.actual_hostname = Some(existing_hostname.clone());
                    }
                }
            }

            // Update the reverse lookup map if we have an actual hostname
            if let Some(actual_hostname) = &status.actual_hostname {
                state
                    .hostname_to_host_id
                    .insert(actual_hostname.clone(), status.host_id.clone());
            }

            state
                .connection_status
                .insert(status.host_id.clone(), status);
        }

        // For hosts that didn't return a status (e.g., Ok(None) or Err cases),
        // mark them as failed if we don't have recent status
        for host in hosts {
            let host_id = extract_host_identifier(host);
            state
                .connection_status
                .entry(host_id.clone())
                .or_insert_with(|| {
                    let mut status = crate::app_state::ConnectionStatus::new(host_id, host.clone());
                    status.mark_failure("No response received".to_string());
                    status
                });
        }
    }

    fn update_utilization_history(&self, state: &mut AppState) {
        // Always collect CPU statistics if available
        if !state.cpu_info.is_empty() {
            let avg_cpu_utilization = state
                .cpu_info
                .iter()
                .map(|cpu| cpu.utilization)
                .sum::<f64>()
                / state.cpu_info.len() as f64;

            let avg_system_memory_usage = if !state.memory_info.is_empty() {
                state
                    .memory_info
                    .iter()
                    .map(|mem| {
                        if mem.total_bytes > 0 {
                            (mem.used_bytes as f64 / mem.total_bytes as f64) * 100.0
                        } else {
                            0.0
                        }
                    })
                    .sum::<f64>()
                    / state.memory_info.len() as f64
            } else {
                0.0
            };

            let cpu_temps: Vec<f64> = state
                .cpu_info
                .iter()
                .filter_map(|cpu| cpu.temperature.map(|t| t as f64))
                .collect();
            let avg_cpu_temperature = if !cpu_temps.is_empty() {
                cpu_temps.iter().sum::<f64>() / cpu_temps.len() as f64
            } else {
                0.0
            };

            state.cpu_utilization_history.push_back(avg_cpu_utilization);
            state
                .system_memory_history
                .push_back(avg_system_memory_usage);
            state.cpu_temperature_history.push_back(avg_cpu_temperature);

            // Keep only last N entries
            if state.cpu_utilization_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.cpu_utilization_history.pop_front();
            }
            if state.system_memory_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.system_memory_history.pop_front();
            }
            if state.cpu_temperature_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.cpu_temperature_history.pop_front();
            }
        }

        // Update history if we have GPU data OR if we're on Apple Silicon (which has 0 total_memory)
        let has_gpu_data = !state.gpu_info.is_empty();
        let is_apple_silicon = state.gpu_info.iter().any(|gpu| {
            gpu.detail
                .get("Architecture")
                .map(|arch| arch == "Apple Silicon")
                .unwrap_or(false)
        });

        if has_gpu_data
            && (state.gpu_info.iter().any(|gpu| gpu.total_memory > 0) || is_apple_silicon)
        {
            let avg_utilization = state
                .gpu_info
                .iter()
                .map(|gpu| gpu.utilization)
                .sum::<f64>()
                / state.gpu_info.len() as f64;

            let avg_memory = state
                .gpu_info
                .iter()
                .map(|gpu| {
                    if gpu.total_memory > 0 {
                        (gpu.used_memory as f64 / gpu.total_memory as f64) * 100.0
                    } else {
                        0.0
                    }
                })
                .sum::<f64>()
                / state.gpu_info.len() as f64;

            let avg_temperature = state
                .gpu_info
                .iter()
                .map(|gpu| gpu.temperature as f64)
                .sum::<f64>()
                / state.gpu_info.len() as f64;

            state.utilization_history.push_back(avg_utilization);
            state.memory_history.push_back(avg_memory);
            state.temperature_history.push_back(avg_temperature);

            // Keep only last N entries as configured
            if state.utilization_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.utilization_history.pop_front();
            }
            if state.memory_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.memory_history.pop_front();
            }
            if state.temperature_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.temperature_history.pop_front();
            }
        } else if !state.cpu_info.is_empty() {
            // Fallback to CPU-based statistics when no GPU is available
            let avg_cpu_utilization = state
                .cpu_info
                .iter()
                .map(|cpu| cpu.utilization)
                .sum::<f64>()
                / state.cpu_info.len() as f64;

            let avg_memory_usage = if !state.memory_info.is_empty() {
                state
                    .memory_info
                    .iter()
                    .map(|mem| {
                        if mem.total_bytes > 0 {
                            (mem.used_bytes as f64 / mem.total_bytes as f64) * 100.0
                        } else {
                            0.0
                        }
                    })
                    .sum::<f64>()
                    / state.memory_info.len() as f64
            } else {
                0.0
            };

            // Use CPU temperature if available, otherwise use a placeholder
            let cpu_temps: Vec<f64> = state
                .cpu_info
                .iter()
                .filter_map(|cpu| cpu.temperature.map(|t| t as f64))
                .collect();
            let avg_temperature = if !cpu_temps.is_empty() {
                cpu_temps.iter().sum::<f64>() / cpu_temps.len() as f64
            } else {
                0.0
            };

            state.utilization_history.push_back(avg_cpu_utilization);
            state.memory_history.push_back(avg_memory_usage);
            state.temperature_history.push_back(avg_temperature);

            // Keep only last N entries as configured
            if state.utilization_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.utilization_history.pop_front();
            }
            if state.memory_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.memory_history.pop_front();
            }
            if state.temperature_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.temperature_history.pop_front();
            }
        }
    }

    fn update_tabs(&self, state: &mut AppState) {
        let mut host_ids: Vec<String> = state
            .gpu_info
            .iter()
            .map(|info| info.host_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // If no GPU info available, use the local hostname
        if host_ids.is_empty() {
            host_ids.push(get_hostname());
        }

        host_ids.sort();

        // Always create "All" tab for consistent UI behavior
        let mut tabs = vec!["All".to_string()];
        tabs.extend(host_ids);

        state.tabs = tabs;
    }

    fn update_remote_tabs(&self, state: &mut AppState) {
        // Always create "All" tab for consistent UI behavior
        let mut tabs = vec!["All".to_string()];
        tabs.extend(state.known_hosts.clone());

        state.tabs = tabs;
    }

    async fn fetch_remote_data(
        &self,
        hosts: &[String],
        _client: &reqwest::Client,
        semaphore: &Arc<tokio::sync::Semaphore>,
        re: &Regex,
    ) -> (
        Vec<GpuInfo>,
        Vec<CpuInfo>,
        Vec<MemoryInfo>,
        Vec<StorageInfo>,
        Vec<crate::app_state::ConnectionStatus>,
    ) {
        let network_client = NetworkClient::new();
        network_client.fetch_remote_data(hosts, semaphore, re).await
    }
}
