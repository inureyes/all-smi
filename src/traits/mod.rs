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

//! Base traits for the all-smi refactoring
//!
//! This module contains foundational traits that will be used to refactor
//! the all-smi codebase into smaller, more maintainable modules while
//! eliminating code duplication.

pub mod collector;
pub mod exporter;
pub mod mock_generator;
pub mod renderer;

// Re-export main traits for convenience
pub use collector::{
    AggregatedCollector, CachedCollector, CollectorBuilder, CollectorError, CollectorFactory,
    CollectorResult, CollectorState, DataCollector, LocalCollector, RemoteCollector,
    StreamingCollector, SystemData,
};

pub use exporter::{
    BoxedCompositeExporter, CompositeExporter, CpuMetricsExporter, ExportFormat, ExporterBuilder,
    ExporterError, ExporterFactory, ExporterResult, GpuMetricsExporter, MemoryMetricsExporter,
    MetricCollection, MetricMetadata, MetricType, MetricValue, MetricsExporter,
    StorageMetricsExporter,
};

pub use mock_generator::{
    DynamicMockGenerator, MockConfig, MockData, MockError, MockGenerator, MockGeneratorBuilder,
    MockGeneratorFactory, MockPlatform, MockProcess, MockResult, ProcessMockGenerator,
    TemplateEngine, ValueGenerator,
};

pub use renderer::{
    CpuRenderer, DeviceRenderer, GpuRenderer, MemoryRenderer, MultiDeviceRenderer, StorageRenderer,
};
