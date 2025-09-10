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

use crate::api::metrics::MetricBuilder;
use crate::device::GpuInfo;

/// Trait for NPU vendor-specific metric exporters
/// This trait defines the interface that all NPU vendor implementations must follow
pub trait NpuExporter: Send + Sync {
    /// Check if this exporter can handle the given NPU device
    fn can_handle(&self, info: &GpuInfo) -> bool;

    /// Export vendor-specific metrics for a single NPU device
    fn export_vendor_metrics(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
        index_str: &str,
    );

    /// Get the vendor name for identification purposes
    fn vendor_name(&self) -> &'static str;
}

/// Common interface for exporting metrics that all NPU exporters should implement
pub trait CommonNpuMetrics {
    /// Export generic NPU metrics that are common across vendors
    fn export_generic_npu_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize);

    /// Export generic NPU metrics with pre-allocated index string (optimization)
    fn export_generic_npu_metrics_str(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index_str: &str,
    );

    /// Export basic device information metrics
    fn export_device_info(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize);

    /// Export firmware version information
    fn export_firmware_info(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize);

    /// Export temperature metrics if available
    fn export_temperature_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize);

    /// Export power-related metrics if available
    fn export_power_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize);
}
