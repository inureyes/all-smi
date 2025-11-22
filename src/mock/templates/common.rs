//! Common template functions shared across platforms

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

use crate::mock::metrics::GpuMetrics;
use rand::{rng, Rng};

/// Add basic GPU metrics that are common across all platforms
pub fn add_basic_gpu_metrics(
    template: &mut String,
    gpu_name: &str,
    instance_name: &str,
    gpus: &[GpuMetrics],
) {
    let gpu_metrics = [
        ("all_smi_gpu_utilization", "GPU utilization percentage"),
        ("all_smi_gpu_memory_used_bytes", "GPU memory used in bytes"),
        (
            "all_smi_gpu_memory_total_bytes",
            "GPU memory total in bytes",
        ),
        (
            "all_smi_gpu_temperature_celsius",
            "GPU temperature in celsius",
        ),
        (
            "all_smi_gpu_power_consumption_watts",
            "GPU power consumption in watts",
        ),
        ("all_smi_gpu_frequency_mhz", "GPU frequency in MHz"),
    ];

    for (metric_name, help_text) in gpu_metrics {
        template.push_str(&format!("# HELP {metric_name} {help_text}\n"));
        template.push_str(&format!("# TYPE {metric_name} gauge\n"));

        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{gpu_name}\", instance=\"{instance_name}\", uuid=\"{}\", index=\"{i}\"",
                gpu.uuid
            );

            let placeholder = match metric_name {
                "all_smi_gpu_utilization" => format!("{{{{UTIL_{i}}}}}"),
                "all_smi_gpu_memory_used_bytes" => format!("{{{{MEM_USED_{i}}}}}"),
                "all_smi_gpu_memory_total_bytes" => format!("{{{{MEM_TOTAL_{i}}}}}"),
                "all_smi_gpu_temperature_celsius" => format!("{{{{TEMP_{i}}}}}"),
                "all_smi_gpu_power_consumption_watts" => format!("{{{{POWER_{i}}}}}"),
                "all_smi_gpu_frequency_mhz" => format!("{{{{FREQ_{i}}}}}"),
                _ => "0".to_string(),
            };

            template.push_str(&format!("{metric_name}{{{labels}}} {placeholder}\n"));
        }
    }
}

/// Add system metrics (CPU and memory)
pub fn add_system_metrics(template: &mut String, instance_name: &str) {
    // CPU metrics
    template.push_str("# HELP all_smi_cpu_utilization CPU utilization percentage\n");
    template.push_str("# TYPE all_smi_cpu_utilization gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_utilization{{instance=\"{instance_name}\"}} {{{{CPU_UTIL}}}}\n"
    ));

    template.push_str("# HELP all_smi_cpu_core_count Total number of CPU cores\n");
    template.push_str("# TYPE all_smi_cpu_core_count gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_core_count{{instance=\"{instance_name}\"}} {{{{CPU_CORES}}}}\n"
    ));

    template.push_str("# HELP all_smi_cpu_temperature_celsius CPU temperature in celsius\n");
    template.push_str("# TYPE all_smi_cpu_temperature_celsius gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_temperature_celsius{{instance=\"{instance_name}\"}} {{{{CPU_TEMP}}}}\n"
    ));

    // Memory metrics
    template.push_str("# HELP all_smi_memory_used_bytes System memory used in bytes\n");
    template.push_str("# TYPE all_smi_memory_used_bytes gauge\n");
    template.push_str(&format!(
        "all_smi_memory_used_bytes{{instance=\"{instance_name}\"}} {{{{MEM_USED}}}}\n"
    ));

    template.push_str("# HELP all_smi_memory_total_bytes System memory total in bytes\n");
    template.push_str("# TYPE all_smi_memory_total_bytes gauge\n");
    template.push_str(&format!(
        "all_smi_memory_total_bytes{{instance=\"{instance_name}\"}} {{{{MEM_TOTAL}}}}\n"
    ));
}

/// Render basic GPU metrics
pub fn render_basic_gpu_metrics(mut response: String, gpus: &[GpuMetrics]) -> String {
    for (i, gpu) in gpus.iter().enumerate() {
        response = response
            .replace(
                &format!("{{{{UTIL_{i}}}}}"),
                &format!("{:.2}", gpu.utilization),
            )
            .replace(
                &format!("{{{{MEM_USED_{i}}}}}"),
                &gpu.memory_used_bytes.to_string(),
            )
            .replace(
                &format!("{{{{MEM_TOTAL_{i}}}}}"),
                &gpu.memory_total_bytes.to_string(),
            )
            .replace(
                &format!("{{{{TEMP_{i}}}}}"),
                &gpu.temperature_celsius.to_string(),
            )
            .replace(
                &format!("{{{{POWER_{i}}}}}"),
                &format!("{:.3}", gpu.power_consumption_watts),
            )
            .replace(&format!("{{{{FREQ_{i}}}}}"), &gpu.frequency_mhz.to_string());
    }
    response
}

/// Render system metrics with default values
pub fn render_system_metrics(mut response: String) -> String {
    use rand::{rng, Rng};
    let mut rng = rng();

    response = response
        .replace(
            "{{CPU_UTIL}}",
            &format!("{:.2}", rng.random_range(10.0..90.0)),
        )
        .replace("{{CPU_CORES}}", "128")
        .replace("{{CPU_TEMP}}", &rng.random_range(35..75).to_string())
        .replace(
            "{{MEM_USED}}",
            &rng.random_range(10_000_000_000u64..500_000_000_000u64)
                .to_string(),
        )
        .replace("{{MEM_TOTAL}}", "1099511627776"); // 1TB

    response
}

/// Generate GPU metrics with random values
pub fn generate_gpu_metrics(count: usize, memory_total: u64) -> Vec<GpuMetrics> {
    // Create a single RNG instance outside the loop for better performance
    let mut rng = rng();

    (0..count)
        .map(|_| {
            GpuMetrics {
                uuid: crate::mock::metrics::gpu::generate_uuid_with_rng(&mut rng),
                utilization: rng.random_range(0.0..100.0),
                memory_used_bytes: rng.random_range(1_000_000_000..memory_total),
                memory_total_bytes: memory_total,
                temperature_celsius: rng.random_range(35..75),
                power_consumption_watts: rng.random_range(100.0..450.0),
                frequency_mhz: rng.random_range(1200..1980),
                ane_utilization_watts: 0.0,
                thermal_pressure_level: None,
            }
        })
        .collect()
}

/// Generate empty GPU metrics for template building
pub fn generate_empty_gpu_metrics(count: usize, memory_total: u64) -> Vec<GpuMetrics> {
    (0..count)
        .map(|i| GpuMetrics {
            uuid: format!("GPU-{i:08x}"),
            utilization: 0.0,
            memory_used_bytes: 0,
            memory_total_bytes: memory_total,
            temperature_celsius: 0,
            power_consumption_watts: 0.0,
            frequency_mhz: 0,
            ane_utilization_watts: 0.0,
            thermal_pressure_level: None,
        })
        .collect()
}
