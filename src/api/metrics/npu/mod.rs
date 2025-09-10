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

pub mod common;
pub mod exporter_trait;
pub mod furiosa;
pub mod rebellions;
pub mod tenstorrent;

use crate::api::metrics::{MetricBuilder, MetricExporter};
use crate::device::GpuInfo;
use exporter_trait::{CommonNpuMetrics, NpuExporter};
use std::sync::OnceLock;

/// Static pool of vendor exporters to avoid repeated allocations
static EXPORTER_POOL: OnceLock<Vec<Box<dyn NpuExporter + Send + Sync>>> = OnceLock::new();

/// Main NPU metric exporter that coordinates between different vendor-specific exporters
pub struct NpuMetricExporter<'a> {
    pub npu_info: &'a [GpuInfo],
    common: common::CommonNpuExporter,
}

impl<'a> NpuMetricExporter<'a> {
    pub fn new(npu_info: &'a [GpuInfo]) -> Self {
        // Initialize the exporter pool once
        EXPORTER_POOL.get_or_init(|| {
            vec![
                Box::new(tenstorrent::TenstorrentExporter::new()),
                Box::new(rebellions::RebellionsExporter::new()),
                Box::new(furiosa::FuriosaExporter::new()),
            ]
        });

        Self {
            npu_info,
            common: common::CommonNpuExporter::new(),
        }
    }

    /// Find the appropriate exporter for a given NPU device
    /// Optimized with early pattern matching to avoid linear search
    fn find_exporter(&self, info: &GpuInfo) -> Option<&(dyn NpuExporter + Send + Sync)> {
        EXPORTER_POOL.get().and_then(|exporters| {
            // Fast path: match common vendor patterns first
            let name = &info.name;
            
            // Direct index access for known vendors (most common first)
            if name.contains("Tenstorrent") {
                return Some(exporters[0].as_ref());
            } else if name.contains("Rebellions") {
                return Some(exporters[1].as_ref());
            } else if name.contains("Furiosa") || name.contains("RNGD") || name.contains("Warboy") {
                return Some(exporters[2].as_ref());
            }
            
            // Fallback to dynamic check for unknown patterns
            exporters
                .iter()
                .find(|exporter| exporter.can_handle(info))
                .map(|b| b.as_ref())
        })
    }

    /// Export generic NPU metrics that are common across all vendors
    fn export_generic_npu_metrics(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        if info.device_type != "NPU" {
            return;
        }

        // Always export common metrics first
        self.common.export_generic_npu_metrics(builder, info, index);
    }

    /// Export vendor-specific metrics using the appropriate exporter
    fn export_vendor_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize, index_str: &str) {
        if let Some(exporter) = self.find_exporter(info) {
            exporter.export_vendor_metrics(builder, info, index, index_str);
        }
    }

    /// Export all NPU metrics for a single device
    fn export_device_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize) {
        // Pre-allocate index string once per device
        let index_str = index.to_string();
        
        // Export generic metrics first
        self.export_generic_npu_metrics_with_str(builder, info, &index_str);

        // Then export vendor-specific metrics
        self.export_vendor_metrics(builder, info, index, &index_str);
    }
    
    /// Export generic NPU metrics with pre-allocated index string
    fn export_generic_npu_metrics_with_str(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index_str: &str,
    ) {
        if info.device_type != "NPU" {
            return;
        }

        // Always export common metrics first
        self.common.export_generic_npu_metrics_str(builder, info, index_str);
    }
}

impl<'a> MetricExporter for NpuMetricExporter<'a> {
    fn export_metrics(&self) -> String {
        let mut builder = MetricBuilder::new();

        for (i, info) in self.npu_info.iter().enumerate() {
            self.export_device_metrics(&mut builder, info, i);
        }

        builder.build()
    }
}
