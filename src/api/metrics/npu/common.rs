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

use super::exporter_trait::CommonNpuMetrics;
use crate::api::metrics::MetricBuilder;
use crate::device::GpuInfo;

/// Common NPU metrics implementation
/// Contains shared functionality and patterns used across all NPU vendors
pub struct CommonNpuExporter;

impl CommonNpuExporter {
    pub fn new() -> Self {
        Self
    }

    /// Helper function to parse hex register values commonly found in NPU metrics
    /// Safely handles overflow by using checked parsing and reasonable bounds
    pub fn parse_hex_register(value: &str) -> Option<f64> {
        let trimmed = value.trim_start_matches("0x").trim();
        
        // Validate input: max 8 hex chars for u32 to prevent overflow
        if trimmed.len() > 8 || trimmed.is_empty() {
            return None;
        }
        
        // Validate hex characters
        if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        
        // Use checked parsing to prevent panic on overflow
        u32::from_str_radix(trimmed, 16)
            .ok()
            .map(|reg_val| reg_val as f64)
    }

    /// Helper function to safely parse numeric values from device details
    /// Rejects NaN, infinity, and malformed values
    pub fn parse_numeric_value(value: &str) -> Option<f64> {
        value.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }

    /// Export status metrics with predefined status values
    pub fn export_status_metric(
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
        metric_name: &str,
        metric_help: &str,
        status_key: &str,
        normal_status: &str,
    ) {
        if let Some(status) = info.detail.get(status_key) {
            let status_value = if status == normal_status { 1.0 } else { 0.0 };
            let status_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("status", status.as_str()),
            ];
            builder
                .help(metric_name, metric_help)
                .type_(metric_name, "gauge")
                .metric(metric_name, &status_labels, status_value);
        }
    }
}

impl Default for CommonNpuExporter {
    fn default() -> Self {
        Self::new()
    }
}

impl CommonNpuMetrics for CommonNpuExporter {
    fn export_generic_npu_metrics(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        if info.device_type != "NPU" {
            return;
        }

        // Generic NPU firmware version
        if let Some(firmware) = info.detail.get("firmware") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("firmware", firmware.as_str()),
            ];
            builder
                .help("all_smi_npu_firmware_info", "NPU firmware version")
                .type_("all_smi_npu_firmware_info", "gauge")
                .metric("all_smi_npu_firmware_info", &fw_labels, 1);
        }
    }

    fn export_device_info(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize) {
        if info.device_type != "NPU" {
            return;
        }

        // Export basic device information
        let device_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
            ("device_type", info.device_type.as_str()),
        ];

        builder
            .help("all_smi_npu_device_info", "NPU device information")
            .type_("all_smi_npu_device_info", "gauge")
            .metric("all_smi_npu_device_info", &device_labels, 1);
    }

    fn export_firmware_info(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize) {
        if info.device_type != "NPU" {
            return;
        }

        // Generic firmware version export (this is called by the generic method above)
        self.export_generic_npu_metrics(builder, info, index);
    }

    fn export_temperature_metrics(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        if info.device_type != "NPU" {
            return;
        }

        let base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // Generic temperature metric if available
        if let Some(temp_str) = info.detail.get("temperature") {
            if let Some(temp) = Self::parse_numeric_value(temp_str) {
                builder
                    .help(
                        "all_smi_npu_temperature_celsius",
                        "NPU temperature in celsius",
                    )
                    .type_("all_smi_npu_temperature_celsius", "gauge")
                    .metric("all_smi_npu_temperature_celsius", &base_labels, temp);
            }
        }
    }

    fn export_power_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize) {
        if info.device_type != "NPU" {
            return;
        }

        let base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // Generic power metric if available
        if let Some(power_str) = info.detail.get("power") {
            if let Some(power) = Self::parse_numeric_value(power_str) {
                builder
                    .help("all_smi_npu_power_watts", "NPU power consumption in watts")
                    .type_("all_smi_npu_power_watts", "gauge")
                    .metric("all_smi_npu_power_watts", &base_labels, power);
            }
        }

        // Generic power draw (common field name)
        if let Some(power_str) = info.detail.get("power_draw") {
            if let Some(power) = Self::parse_numeric_value(power_str) {
                builder
                    .help("all_smi_npu_power_draw_watts", "NPU power draw in watts")
                    .type_("all_smi_npu_power_draw_watts", "gauge")
                    .metric("all_smi_npu_power_draw_watts", &base_labels, power);
            }
        }
    }
}
