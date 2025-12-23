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

//! Chassis metrics exporter for Prometheus
//!
//! Exports node-level metrics including:
//! - Total power consumption (CPU+GPU+ANE)
//! - Thermal pressure (Apple Silicon)
//! - Individual power components (CPU, GPU, ANE)

use super::{MetricBuilder, MetricExporter};
use crate::device::ChassisInfo;

/// Exporter for chassis-level metrics
pub struct ChassisMetricExporter<'a> {
    chassis_info: &'a [ChassisInfo],
}

impl<'a> ChassisMetricExporter<'a> {
    pub fn new(chassis_info: &'a [ChassisInfo]) -> Self {
        Self { chassis_info }
    }
}

impl<'a> MetricExporter for ChassisMetricExporter<'a> {
    fn export_metrics(&self) -> String {
        let mut builder = MetricBuilder::new();

        if self.chassis_info.is_empty() {
            return builder.build();
        }

        // Export chassis power metrics
        builder
            .help(
                "all_smi_chassis_power_watts",
                "Total chassis power consumption in watts (CPU+GPU+ANE)",
            )
            .type_("all_smi_chassis_power_watts", "gauge");

        for chassis in self.chassis_info {
            if let Some(power) = chassis.total_power_watts {
                builder.metric(
                    "all_smi_chassis_power_watts",
                    &[
                        ("hostname", &chassis.hostname),
                        ("instance", &chassis.instance),
                    ],
                    format!("{power:.2}"),
                );
            }
        }

        // Export thermal pressure metric (Apple Silicon)
        let has_thermal_pressure = self
            .chassis_info
            .iter()
            .any(|c| c.thermal_pressure.is_some());
        if has_thermal_pressure {
            builder
                .help(
                    "all_smi_chassis_thermal_pressure_info",
                    "Thermal pressure level (Apple Silicon)",
                )
                .type_("all_smi_chassis_thermal_pressure_info", "gauge");

            for chassis in self.chassis_info {
                if let Some(ref pressure) = chassis.thermal_pressure {
                    builder.metric(
                        "all_smi_chassis_thermal_pressure_info",
                        &[
                            ("hostname", &chassis.hostname),
                            ("instance", &chassis.instance),
                            ("level", pressure),
                        ],
                        "1",
                    );
                }
            }
        }

        // Export individual power components if available
        let has_cpu_power = self
            .chassis_info
            .iter()
            .any(|c| c.detail.contains_key("cpu_power_watts"));
        if has_cpu_power {
            builder
                .help(
                    "all_smi_chassis_cpu_power_watts",
                    "CPU power consumption in watts",
                )
                .type_("all_smi_chassis_cpu_power_watts", "gauge");

            for chassis in self.chassis_info {
                if let Some(power_str) = chassis.detail.get("cpu_power_watts") {
                    if let Ok(power) = power_str.parse::<f64>() {
                        builder.metric(
                            "all_smi_chassis_cpu_power_watts",
                            &[
                                ("hostname", &chassis.hostname),
                                ("instance", &chassis.instance),
                            ],
                            format!("{power:.2}"),
                        );
                    }
                }
            }
        }

        let has_gpu_power = self
            .chassis_info
            .iter()
            .any(|c| c.detail.contains_key("gpu_power_watts"));
        if has_gpu_power {
            builder
                .help(
                    "all_smi_chassis_gpu_power_watts",
                    "GPU power consumption in watts",
                )
                .type_("all_smi_chassis_gpu_power_watts", "gauge");

            for chassis in self.chassis_info {
                if let Some(power_str) = chassis.detail.get("gpu_power_watts") {
                    if let Ok(power) = power_str.parse::<f64>() {
                        builder.metric(
                            "all_smi_chassis_gpu_power_watts",
                            &[
                                ("hostname", &chassis.hostname),
                                ("instance", &chassis.instance),
                            ],
                            format!("{power:.2}"),
                        );
                    }
                }
            }
        }

        let has_ane_power = self
            .chassis_info
            .iter()
            .any(|c| c.detail.contains_key("ane_power_watts"));
        if has_ane_power {
            builder
                .help(
                    "all_smi_chassis_ane_power_watts",
                    "ANE (Apple Neural Engine) power consumption in watts",
                )
                .type_("all_smi_chassis_ane_power_watts", "gauge");

            for chassis in self.chassis_info {
                if let Some(power_str) = chassis.detail.get("ane_power_watts") {
                    if let Ok(power) = power_str.parse::<f64>() {
                        builder.metric(
                            "all_smi_chassis_ane_power_watts",
                            &[
                                ("hostname", &chassis.hostname),
                                ("instance", &chassis.instance),
                            ],
                            format!("{power:.2}"),
                        );
                    }
                }
            }
        }

        // Export inlet/outlet temperature if available
        let has_inlet_temp = self
            .chassis_info
            .iter()
            .any(|c| c.inlet_temperature.is_some());
        if has_inlet_temp {
            builder
                .help(
                    "all_smi_chassis_inlet_temperature_celsius",
                    "Chassis inlet temperature in Celsius",
                )
                .type_("all_smi_chassis_inlet_temperature_celsius", "gauge");

            for chassis in self.chassis_info {
                if let Some(temp) = chassis.inlet_temperature {
                    builder.metric(
                        "all_smi_chassis_inlet_temperature_celsius",
                        &[
                            ("hostname", &chassis.hostname),
                            ("instance", &chassis.instance),
                        ],
                        format!("{temp:.1}"),
                    );
                }
            }
        }

        let has_outlet_temp = self
            .chassis_info
            .iter()
            .any(|c| c.outlet_temperature.is_some());
        if has_outlet_temp {
            builder
                .help(
                    "all_smi_chassis_outlet_temperature_celsius",
                    "Chassis outlet temperature in Celsius",
                )
                .type_("all_smi_chassis_outlet_temperature_celsius", "gauge");

            for chassis in self.chassis_info {
                if let Some(temp) = chassis.outlet_temperature {
                    builder.metric(
                        "all_smi_chassis_outlet_temperature_celsius",
                        &[
                            ("hostname", &chassis.hostname),
                            ("instance", &chassis.instance),
                        ],
                        format!("{temp:.1}"),
                    );
                }
            }
        }

        // Export fan speed metrics if available
        if self.chassis_info.iter().any(|c| !c.fan_speeds.is_empty()) {
            builder
                .help("all_smi_chassis_fan_speed_rpm", "Fan speed in RPM")
                .type_("all_smi_chassis_fan_speed_rpm", "gauge");

            for chassis in self.chassis_info {
                for fan in &chassis.fan_speeds {
                    builder.metric(
                        "all_smi_chassis_fan_speed_rpm",
                        &[
                            ("hostname", &chassis.hostname),
                            ("instance", &chassis.instance),
                            ("fan_id", &fan.id.to_string()),
                            ("fan_name", &fan.name),
                        ],
                        fan.speed_rpm.to_string(),
                    );
                }
            }
        }

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_chassis_info() {
        let exporter = ChassisMetricExporter::new(&[]);
        let metrics = exporter.export_metrics();
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_chassis_power_metric() {
        let chassis = ChassisInfo {
            hostname: "test-host".to_string(),
            instance: "test-instance".to_string(),
            total_power_watts: Some(45.5),
            ..Default::default()
        };

        let chassis_vec = vec![chassis];
        let exporter = ChassisMetricExporter::new(&chassis_vec);
        let metrics = exporter.export_metrics();

        assert!(metrics.contains("all_smi_chassis_power_watts"));
        assert!(metrics.contains("hostname=\"test-host\""));
        assert!(metrics.contains("45.50"));
    }

    #[test]
    fn test_thermal_pressure_metric() {
        let chassis = ChassisInfo {
            hostname: "mac-host".to_string(),
            instance: "mac-instance".to_string(),
            thermal_pressure: Some("Nominal".to_string()),
            ..Default::default()
        };

        let chassis_vec = vec![chassis];
        let exporter = ChassisMetricExporter::new(&chassis_vec);
        let metrics = exporter.export_metrics();

        assert!(metrics.contains("all_smi_chassis_thermal_pressure_info"));
        assert!(metrics.contains("level=\"Nominal\""));
    }
}
