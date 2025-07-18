use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::{FuturesUnordered, StreamExt};
use regex::Regex;

use crate::app_state::ConnectionStatus;
use crate::common::config::{AppConfig, EnvConfig};
use crate::device::{CpuInfo, GpuInfo, MemoryInfo};
use crate::storage::info::StorageInfo;

pub struct NetworkClient {
    client: reqwest::Client,
}

impl NetworkClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(AppConfig::CONNECTION_TIMEOUT_SECS))
            .pool_idle_timeout(Duration::from_secs(AppConfig::POOL_IDLE_TIMEOUT_SECS))
            .pool_max_idle_per_host(AppConfig::POOL_MAX_IDLE_PER_HOST)
            .tcp_keepalive(Duration::from_secs(AppConfig::TCP_KEEPALIVE_SECS))
            .http2_keep_alive_interval(Duration::from_secs(AppConfig::HTTP2_KEEPALIVE_SECS))
            .build()
            .unwrap();

        Self { client }
    }

    pub async fn fetch_remote_data(
        &self,
        hosts: &[String],
        semaphore: &Arc<tokio::sync::Semaphore>,
        re: &Regex,
    ) -> (
        Vec<GpuInfo>,
        Vec<CpuInfo>,
        Vec<MemoryInfo>,
        Vec<StorageInfo>,
        Vec<ConnectionStatus>,
    ) {
        let mut all_gpu_info = Vec::new();
        let mut all_cpu_info = Vec::new();
        let mut all_memory_info = Vec::new();
        let mut all_storage_info = Vec::new();
        let mut connection_statuses = Vec::new();

        // Parallel data collection with concurrency limiting and retries
        let total_hosts = hosts.len();
        let mut fetch_futures = FuturesUnordered::new();

        for (i, host) in hosts.iter().enumerate() {
            let client = self.client.clone();
            let host = host.clone();
            let semaphore = semaphore.clone();

            let future = tokio::spawn(async move {
                // Stagger connection attempts to avoid overwhelming the listen queue
                let stagger_delay = EnvConfig::connection_stagger_delay(i, total_hosts);
                tokio::time::sleep(Duration::from_millis(stagger_delay)).await;

                // Acquire semaphore permit to limit concurrency
                let _permit = semaphore.acquire().await.unwrap();

                let url = if host.starts_with("http://") || host.starts_with("https://") {
                    format!("{host}/metrics")
                } else {
                    format!("http://{host}/metrics")
                };

                // Retry logic with exponential backoff
                for attempt in 1..=AppConfig::RETRY_ATTEMPTS {
                    match client.get(&url).send().await {
                        Ok(response) => {
                            if response.status().is_success() {
                                match response.text().await {
                                    Ok(text) => return Some((host, text, None)),
                                    Err(e) => {
                                        if attempt == 3 {
                                            return Some((
                                                host,
                                                String::new(),
                                                Some(format!("Text parse error: {e}")),
                                            ));
                                        }
                                    }
                                }
                            } else if attempt == 3 {
                                return Some((
                                    host,
                                    String::new(),
                                    Some(format!("HTTP {}", response.status())),
                                ));
                            }
                        }
                        Err(e) => {
                            if attempt == 3 {
                                return Some((
                                    host,
                                    String::new(),
                                    Some(format!("Connection error after {attempt} attempts: {e}")),
                                ));
                            }
                        }
                    }

                    // Exponential backoff
                    tokio::time::sleep(Duration::from_millis(EnvConfig::retry_delay(attempt)))
                        .await;
                }

                Some((
                    host,
                    String::new(),
                    Some("All retry attempts failed".to_string()),
                ))
            });

            fetch_futures.push(future);
        }

        // Process results as they arrive using streaming with overall timeout
        let mut _successful_connections = 0;
        let mut _failed_connections = 0;
        let mut responses_received = 0;

        // Set overall timeout for collecting results (4 seconds)
        let overall_timeout = Duration::from_secs(4);
        let timeout_future = tokio::time::sleep(overall_timeout);
        tokio::pin!(timeout_future);

        loop {
            tokio::select! {
                // Process next result if available
                Some(task_result) = fetch_futures.next() => {
                    responses_received += 1;

                    match task_result {
                        Ok(Some((host, text, error))) => {
                            let host_identifier = host.clone();
                            let mut connection_status =
                                ConnectionStatus::new(host_identifier.clone(), host.clone());

                            if let Some(error_msg) = error {
                                _failed_connections += 1;
                                connection_status.mark_failure(error_msg);
                                connection_statuses.push(connection_status);
                            } else {
                                _successful_connections += 1;
                                connection_status.mark_success();

                                if text.is_empty() {
                                    connection_statuses.push(connection_status);
                                } else {
                                    let parser = super::metrics_parser::MetricsParser::new();
                                    let (gpu_info, cpu_info, memory_info, storage_info) =
                                        parser.parse_metrics(&text, &host, re);

                                    // Extract the instance name from device info if available
                                    let instance_name = if let Some(first_gpu) = gpu_info.first() {
                                        Some(first_gpu.instance.clone())
                                    } else if let Some(first_cpu) = cpu_info.first() {
                                        Some(first_cpu.instance.clone())
                                    } else { memory_info.first().map(|first_memory| first_memory.instance.clone()) };

                                    // Store the instance name as actual_hostname for display purposes
                                    connection_status.actual_hostname = instance_name;
                                    connection_statuses.push(connection_status);

                                    all_gpu_info.extend(gpu_info);
                                    all_cpu_info.extend(cpu_info);
                                    all_memory_info.extend(memory_info);
                                    all_storage_info.extend(storage_info);
                                }
                            }
                        }
                        Ok(None) => {
                            _failed_connections += 1;
                            // We don't have host information for None results, so we can't create a connection status
                        }
                        Err(_) => {
                            _failed_connections += 1;
                            // We don't have host information for Err results, so we can't create a connection status
                        }
                    }

                    // Check if we've received responses from all hosts
                    if responses_received >= total_hosts {
                        break;
                    }
                }
                // Timeout reached - return partial results
                _ = &mut timeout_future => {
                    // Mark remaining hosts as timed out
                    break;
                }
            }
        }

        // Debug logging for connection success rate - commented out to avoid interfering with TUI
        // if failed_connections > 0 {
        //     eprintln!(
        //         "Connection stats: {successful_connections} successful, {failed_connections} failed out of {total_hosts} total"
        //     );
        // }

        (
            all_gpu_info,
            all_cpu_info,
            all_memory_info,
            all_storage_info,
            connection_statuses,
        )
    }
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}
