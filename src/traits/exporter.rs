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

use std::collections::HashMap;

// Device info types will be defined by implementing modules

/// Result type for export operations
pub type ExporterResult<T> = Result<T, ExporterError>;

/// Errors that can occur during metrics export
#[derive(Debug)]
pub enum ExporterError {
    SerializationError(String),
    FormatError(String),
    UnsupportedFormat(String),
    Io(std::io::Error),
    Other(String),
}

impl std::fmt::Display for ExporterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SerializationError(msg) => write!(f, "Failed to serialize metrics: {msg}"),
            Self::FormatError(msg) => write!(f, "Invalid metric format: {msg}"),
            Self::UnsupportedFormat(msg) => write!(f, "Unsupported export format: {msg}"),
            Self::Io(err) => write!(f, "IO error: {err}"),
            Self::Other(msg) => write!(f, "Other error: {msg}"),
        }
    }
}

impl std::error::Error for ExporterError {}

impl From<std::io::Error> for ExporterError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Supported metric export formats
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFormat {
    /// Prometheus text format
    Prometheus,
    /// JSON format
    Json,
    /// OpenMetrics format
    OpenMetrics,
    /// StatsD format
    StatsD,
    /// InfluxDB line protocol
    InfluxDB,
    /// Custom format
    Custom(String),
}

/// Metric metadata
#[derive(Debug, Clone)]
pub struct MetricMetadata {
    pub name: String,
    pub help: Option<String>,
    pub unit: Option<String>,
    pub metric_type: MetricType,
    pub labels: HashMap<String, String>,
}

/// Metric types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
    Untyped,
}

/// A single metric value
#[derive(Debug, Clone)]
pub struct MetricValue {
    pub value: f64,
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub labels: HashMap<String, String>,
}

/// A collection of metrics
#[derive(Debug, Clone, Default)]
pub struct MetricCollection {
    pub metrics: HashMap<String, Vec<MetricValue>>,
    pub metadata: HashMap<String, MetricMetadata>,
}

/// Trait for exporting metrics in various formats
pub trait MetricsExporter: Send + Sync {
    /// Export metrics in the specified format
    fn export(&self, format: ExportFormat) -> ExporterResult<String>;

    /// Export metrics to Prometheus format (default implementation)
    fn export_prometheus(&self) -> ExporterResult<String> {
        self.export(ExportFormat::Prometheus)
    }

    /// Export metrics to JSON format
    fn export_json(&self) -> ExporterResult<String> {
        self.export(ExportFormat::Json)
    }

    /// Get all available metrics
    fn get_metrics(&self) -> MetricCollection;

    /// Get supported export formats
    fn supported_formats(&self) -> Vec<ExportFormat> {
        vec![ExportFormat::Prometheus, ExportFormat::Json]
    }

    /// Validate metrics before export
    fn validate_metrics(&self) -> ExporterResult<()> {
        Ok(())
    }
}

/// Trait for GPU metrics exporters
pub trait GpuMetricsExporter<T>: MetricsExporter {
    /// Export GPU-specific metrics
    fn export_gpu_metrics(&self, gpu: &T, index: usize) -> MetricCollection;

    /// Export GPU utilization metrics
    fn export_gpu_utilization(&self, gpu: &T, index: usize) -> MetricCollection;

    /// Export GPU memory metrics
    fn export_gpu_memory(&self, gpu: &T, index: usize) -> MetricCollection;

    /// Export GPU temperature metrics
    fn export_gpu_temperature(&self, gpu: &T, index: usize) -> MetricCollection;

    /// Export GPU power metrics
    fn export_gpu_power(&self, gpu: &T, index: usize) -> MetricCollection;

    /// Export GPU process metrics
    fn export_gpu_processes(&self, gpu: &T, index: usize) -> MetricCollection;
}

/// Trait for CPU metrics exporters
pub trait CpuMetricsExporter<T>: MetricsExporter {
    /// Export CPU-specific metrics
    fn export_cpu_metrics(&self, cpu: &T) -> MetricCollection;

    /// Export CPU utilization metrics
    fn export_cpu_utilization(&self, cpu: &T) -> MetricCollection;

    /// Export CPU frequency metrics
    fn export_cpu_frequency(&self, cpu: &T) -> MetricCollection;

    /// Export CPU temperature metrics
    fn export_cpu_temperature(&self, cpu: &T) -> MetricCollection;

    /// Export per-core metrics
    fn export_cpu_cores(&self, cpu: &T) -> MetricCollection;
}

/// Trait for Memory metrics exporters
pub trait MemoryMetricsExporter<T>: MetricsExporter {
    /// Export memory-specific metrics
    fn export_memory_metrics(&self, memory: &T) -> MetricCollection;

    /// Export memory usage metrics
    fn export_memory_usage(&self, memory: &T) -> MetricCollection;

    /// Export swap metrics
    fn export_swap_metrics(&self, memory: &T) -> MetricCollection;

    /// Export cache metrics
    fn export_cache_metrics(&self, memory: &T) -> MetricCollection;
}

/// Trait for Storage metrics exporters
pub trait StorageMetricsExporter<T>: MetricsExporter {
    /// Export storage-specific metrics
    fn export_storage_metrics(&self, storage: &T, index: usize) -> MetricCollection;

    /// Export storage usage metrics
    fn export_storage_usage(&self, storage: &T, index: usize) -> MetricCollection;

    /// Export storage I/O metrics
    fn export_storage_io(&self, storage: &T, index: usize) -> MetricCollection;
}

/// Trait for composite exporters that handle multiple device types
pub trait CompositeExporter: MetricsExporter {
    type GpuInfo;
    type CpuInfo;
    type MemoryInfo;
    type StorageInfo;

    /// Export all system metrics
    fn export_all(&self) -> ExporterResult<String>;

    /// Export metrics for specific device types
    fn export_gpus(&self, gpus: &[Self::GpuInfo]) -> ExporterResult<String>;
    fn export_cpu(&self, cpu: &Self::CpuInfo) -> ExporterResult<String>;
    fn export_memory(&self, memory: &Self::MemoryInfo) -> ExporterResult<String>;
    fn export_storage(&self, storage: &[Self::StorageInfo]) -> ExporterResult<String>;

    /// Set global labels for all metrics
    fn set_global_labels(&mut self, labels: HashMap<String, String>);

    /// Add a prefix to all metric names
    fn set_metric_prefix(&mut self, prefix: String);
}

/// Builder for creating metric exporters
pub trait ExporterBuilder {
    type Exporter: MetricsExporter;

    /// Build the exporter
    fn build(self) -> ExporterResult<Self::Exporter>;

    /// Set the export format
    fn with_format(self, format: ExportFormat) -> Self;

    /// Add global labels
    fn with_labels(self, labels: HashMap<String, String>) -> Self;

    /// Set metric prefix
    fn with_prefix(self, prefix: String) -> Self;
}

/// Type alias for boxed composite exporter
pub type BoxedCompositeExporter<G, C, M, S> =
    Box<dyn CompositeExporter<GpuInfo = G, CpuInfo = C, MemoryInfo = M, StorageInfo = S>>;

/// Factory for creating exporters
pub trait ExporterFactory {
    type GpuInfo;
    type CpuInfo;
    type MemoryInfo;
    type StorageInfo;

    /// Create an exporter for the given format
    fn create(&self, format: ExportFormat) -> ExporterResult<Box<dyn MetricsExporter>>;

    /// Create a GPU metrics exporter
    fn create_gpu_exporter(
        &self,
        format: ExportFormat,
    ) -> ExporterResult<Box<dyn GpuMetricsExporter<Self::GpuInfo>>>;

    /// Create a CPU metrics exporter
    fn create_cpu_exporter(
        &self,
        format: ExportFormat,
    ) -> ExporterResult<Box<dyn CpuMetricsExporter<Self::CpuInfo>>>;

    /// Create a composite exporter
    #[allow(clippy::type_complexity)]
    fn create_composite(
        &self,
        format: ExportFormat,
    ) -> ExporterResult<
        BoxedCompositeExporter<Self::GpuInfo, Self::CpuInfo, Self::MemoryInfo, Self::StorageInfo>,
    >;
}
