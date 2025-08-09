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

#![allow(async_fn_in_trait)]

use std::sync::Arc;
use tokio::sync::Mutex;

// Device info types will be defined by implementing modules

/// Result type for data collection operations
pub type CollectorResult<T> = Result<T, CollectorError>;

/// Errors that can occur during data collection
#[derive(Debug)]
pub enum CollectorError {
    ConnectionError(String),
    CollectionError(String),
    ParseError(String),
    Timeout,
    Io(std::io::Error),
    Other(String),
}

impl std::fmt::Display for CollectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionError(msg) => write!(f, "Failed to connect to remote host: {msg}"),
            Self::CollectionError(msg) => write!(f, "Failed to collect data: {msg}"),
            Self::ParseError(msg) => write!(f, "Failed to parse data: {msg}"),
            Self::Timeout => write!(f, "Timeout while collecting data"),
            Self::Io(err) => write!(f, "IO error: {err}"),
            Self::Other(msg) => write!(f, "Other error: {msg}"),
        }
    }
}

impl std::error::Error for CollectorError {}

impl From<std::io::Error> for CollectorError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Collected system data - generic over device info types
#[derive(Debug, Clone, Default)]
pub struct SystemData<G, C, M, S> {
    pub gpus: Vec<G>,
    pub cpu: Option<C>,
    pub memory: Option<M>,
    pub storage: Vec<S>,
    pub hostname: String,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Trait for collecting system data from various sources
pub trait DataCollector: Send + Sync {
    type GpuInfo;
    type CpuInfo;
    type MemoryInfo;
    type StorageInfo;
    type Data: Default;

    /// Initialize the collector
    async fn initialize(&mut self) -> CollectorResult<()>;

    /// Collect all system data
    async fn collect(&self) -> CollectorResult<Self::Data>;

    /// Collect GPU information
    async fn collect_gpus(&self) -> CollectorResult<Vec<Self::GpuInfo>>;

    /// Collect CPU information
    async fn collect_cpu(&self) -> CollectorResult<Option<Self::CpuInfo>>;

    /// Collect memory information
    async fn collect_memory(&self) -> CollectorResult<Option<Self::MemoryInfo>>;

    /// Collect storage information
    async fn collect_storage(&self) -> CollectorResult<Vec<Self::StorageInfo>>;

    /// Check if the collector is healthy/connected
    async fn is_healthy(&self) -> bool;

    /// Get the collector's identifier (hostname, URL, etc.)
    fn get_identifier(&self) -> String;

    /// Shutdown the collector gracefully
    async fn shutdown(&mut self) -> CollectorResult<()> {
        Ok(())
    }
}

/// Trait for local data collection
pub trait LocalCollector: DataCollector {
    /// Set the collection interval
    fn set_interval(&mut self, interval: std::time::Duration);

    /// Enable/disable specific data collection
    fn set_collect_gpu(&mut self, enabled: bool);
    fn set_collect_cpu(&mut self, enabled: bool);
    fn set_collect_memory(&mut self, enabled: bool);
    fn set_collect_storage(&mut self, enabled: bool);
}

/// Trait for remote data collection
pub trait RemoteCollector: DataCollector {
    /// Connect to a remote host
    async fn connect(&mut self, url: &str) -> CollectorResult<()>;

    /// Disconnect from the remote host
    async fn disconnect(&mut self) -> CollectorResult<()>;

    /// Set connection timeout
    fn set_timeout(&mut self, timeout: std::time::Duration);

    /// Set retry policy
    fn set_retry_policy(&mut self, max_retries: u32, backoff: std::time::Duration);

    /// Get connection status
    async fn is_connected(&self) -> bool;

    /// Get the remote host URL
    fn get_url(&self) -> String;
}

/// Trait for collectors that support caching
pub trait CachedCollector: DataCollector {
    /// Get cached data if available
    async fn get_cached(&self) -> Option<Self::Data>;

    /// Clear the cache
    async fn clear_cache(&mut self);

    /// Set cache TTL
    fn set_cache_ttl(&mut self, ttl: std::time::Duration);

    /// Check if cache is valid
    async fn is_cache_valid(&self) -> bool;
}

/// Trait for aggregating data from multiple collectors
pub trait AggregatedCollector: Send + Sync {
    type Collector: DataCollector;

    /// Add a collector to the aggregator
    async fn add_collector(&mut self, collector_id: String);

    /// Remove a collector by identifier
    async fn remove_collector(&mut self, identifier: &str) -> bool;

    /// Collect data from all collectors
    async fn collect_all<T>(&self) -> Vec<(String, CollectorResult<T>)>;

    /// Collect data from all collectors in parallel
    async fn collect_parallel<T>(&self) -> Vec<(String, CollectorResult<T>)>;

    /// Get the number of collectors
    fn collector_count(&self) -> usize;

    /// Get all collector identifiers
    fn get_identifiers(&self) -> Vec<String>;
}

/// Builder pattern for creating collectors
pub trait CollectorBuilder {
    type Collector: DataCollector;

    /// Build the collector
    fn build(self) -> CollectorResult<Self::Collector>;
}

/// Factory for creating collectors based on configuration
pub trait CollectorFactory {
    type Local: LocalCollector;
    type Remote: RemoteCollector;
    type Aggregated: AggregatedCollector;

    /// Create a local collector
    async fn create_local(&self) -> CollectorResult<Self::Local>;

    /// Create a remote collector for the given URL
    async fn create_remote(&self, url: &str) -> CollectorResult<Self::Remote>;

    /// Create an aggregated collector
    async fn create_aggregated(&self) -> CollectorResult<Self::Aggregated>;
}

/// Trait for collectors that support streaming updates
pub trait StreamingCollector: DataCollector {
    /// Subscribe to data updates
    async fn subscribe(&mut self) -> CollectorResult<tokio::sync::mpsc::Receiver<Self::Data>>;

    /// Start streaming data
    async fn start_streaming(&mut self, interval: std::time::Duration) -> CollectorResult<()>;

    /// Stop streaming data
    async fn stop_streaming(&mut self) -> CollectorResult<()>;

    /// Check if currently streaming
    fn is_streaming(&self) -> bool;
}

/// Shared state for collectors - generic over data type
pub struct CollectorState<T> {
    pub data: Arc<Mutex<T>>,
    pub last_update: Arc<Mutex<Option<chrono::DateTime<chrono::Utc>>>>,
    pub error_count: Arc<Mutex<u32>>,
    pub is_running: Arc<Mutex<bool>>,
}
