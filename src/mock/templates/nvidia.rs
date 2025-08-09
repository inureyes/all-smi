//! NVIDIA GPU mock template generator

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

use crate::mock::constants::*;
use crate::mock::metrics::{CpuMetrics, GpuMetrics, MemoryMetrics, PlatformType};
use all_smi::traits::mock_generator::{
    MockConfig, MockData, MockError, MockGenerator, MockPlatform, MockResult,
};
use std::collections::HashMap;

/// NVIDIA GPU mock generator
pub struct NvidiaMockGenerator {
    gpu_name: String,
    instance_name: String,
}

impl NvidiaMockGenerator {
    pub fn new(gpu_name: Option<String>, instance_name: String) -> Self {
        Self {
            gpu_name: gpu_name.unwrap_or_else(|| "NVIDIA H100 80GB HBM3".to_string()),
            instance_name,
        }
    }

    /// Build NVIDIA-specific template
    pub fn build_nvidia_template(
        &self,
        gpus: &[GpuMetrics],
        cpu: &CpuMetrics,
        memory: &MemoryMetrics,
    ) -> String {
        let mut template = String::with_capacity(4096);

        // Basic GPU metrics
        self.add_gpu_metrics(&mut template, gpus);

        // NVIDIA-specific: P-state metrics
        self.add_pstate_metrics(&mut template, gpus);

        // NVIDIA-specific: Process metrics
        self.add_process_metrics(&mut template, gpus);

        // NVIDIA-specific: Driver metrics
        self.add_driver_metrics(&mut template);

        // CPU and memory metrics
        self.add_system_metrics(&mut template, cpu, memory);

        template
    }

    fn add_gpu_metrics(&self, template: &mut String, gpus: &[GpuMetrics]) {
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
                    "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{i}\"",
                    self.gpu_name, self.instance_name, gpu.uuid
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

    fn add_pstate_metrics(&self, template: &mut String, gpus: &[GpuMetrics]) {
        template.push_str("# HELP all_smi_gpu_pstate GPU performance state\n");
        template.push_str("# TYPE all_smi_gpu_pstate gauge\n");

        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{i}\"",
                self.gpu_name, self.instance_name, gpu.uuid
            );
            template.push_str(&format!(
                "all_smi_gpu_pstate{{{labels}}} {{{{PSTATE_{i}}}}}\n"
            ));
        }
    }

    fn add_process_metrics(&self, template: &mut String, gpus: &[GpuMetrics]) {
        // Process count
        template.push_str("# HELP all_smi_gpu_process_count Number of processes running on GPU\n");
        template.push_str("# TYPE all_smi_gpu_process_count gauge\n");

        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{i}\"",
                self.gpu_name, self.instance_name, gpu.uuid
            );
            template.push_str(&format!(
                "all_smi_gpu_process_count{{{labels}}} {{{{PROC_COUNT_{i}}}}}\n"
            ));
        }

        // Process utilization
        template.push_str("# HELP all_smi_gpu_process_utilization Process GPU utilization\n");
        template.push_str("# TYPE all_smi_gpu_process_utilization gauge\n");

        // Add placeholders for multiple processes per GPU
        for (gpu_idx, gpu) in gpus.iter().enumerate() {
            for proc_idx in 0..MAX_PROCESSES_PER_GPU {
                let labels = format!(
                    "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{gpu_idx}\", \
                     pid=\"{{{{PID_{gpu_idx}_{proc_idx}}}}}\", name=\"{{{{PROC_NAME_{gpu_idx}_{proc_idx}}}}}\"",
                    self.gpu_name, self.instance_name, gpu.uuid
                );
                template.push_str(&format!(
                    "all_smi_gpu_process_utilization{{{labels}}} {{{{PROC_UTIL_{gpu_idx}_{proc_idx}}}}}\n"
                ));
            }
        }

        // Process memory usage
        template.push_str("# HELP all_smi_gpu_process_memory_bytes Process GPU memory usage\n");
        template.push_str("# TYPE all_smi_gpu_process_memory_bytes gauge\n");

        for (gpu_idx, gpu) in gpus.iter().enumerate() {
            for proc_idx in 0..MAX_PROCESSES_PER_GPU {
                let labels = format!(
                    "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{gpu_idx}\", \
                     pid=\"{{{{PID_{gpu_idx}_{proc_idx}}}}}\", name=\"{{{{PROC_NAME_{gpu_idx}_{proc_idx}}}}}\"",
                    self.gpu_name, self.instance_name, gpu.uuid
                );
                template.push_str(&format!(
                    "all_smi_gpu_process_memory_bytes{{{labels}}} {{{{PROC_MEM_{gpu_idx}_{proc_idx}}}}}\n"
                ));
            }
        }
    }

    fn add_driver_metrics(&self, template: &mut String) {
        // NVIDIA driver version
        template.push_str("# HELP all_smi_nvidia_driver_version NVIDIA driver version\n");
        template.push_str("# TYPE all_smi_nvidia_driver_version gauge\n");
        template.push_str(&format!(
            "all_smi_nvidia_driver_version{{instance=\"{}\"}} 1\n",
            self.instance_name
        ));
    }

    fn add_system_metrics(&self, template: &mut String, cpu: &CpuMetrics, memory: &MemoryMetrics) {
        // CPU metrics
        template.push_str("# HELP all_smi_cpu_utilization CPU utilization percentage\n");
        template.push_str("# TYPE all_smi_cpu_utilization gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_utilization{{instance=\"{}\"}} {{{{CPU_UTIL}}}}\n",
            self.instance_name
        ));

        template.push_str("# HELP all_smi_cpu_cores Number of CPU cores\n");
        template.push_str("# TYPE all_smi_cpu_cores gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_cores{{instance=\"{}\"}} {}\n",
            self.instance_name, cpu.core_count
        ));

        // Memory metrics
        template.push_str("# HELP all_smi_memory_used_bytes System memory used in bytes\n");
        template.push_str("# TYPE all_smi_memory_used_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_used_bytes{{instance=\"{}\"}} {{{{MEM_USED}}}}\n",
            self.instance_name
        ));

        template.push_str("# HELP all_smi_memory_total_bytes System memory total in bytes\n");
        template.push_str("# TYPE all_smi_memory_total_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_total_bytes{{instance=\"{}\"}} {}\n",
            self.instance_name, memory.total_bytes
        ));
    }

    /// Render dynamic values for NVIDIA GPUs
    pub fn render_nvidia_response(
        &self,
        template: &str,
        gpus: &[GpuMetrics],
        cpu: &CpuMetrics,
        memory: &MemoryMetrics,
    ) -> String {
        let mut response = template.to_string();

        // Replace GPU metrics
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

            // Replace P-state based on utilization
            let pstate = if gpu.utilization > 80.0 {
                0 // P0 - Maximum performance
            } else if gpu.utilization > 50.0 {
                2 // P2 - Balanced
            } else if gpu.utilization > 20.0 {
                5 // P5 - Auto
            } else if gpu.utilization > 0.0 {
                8 // P8 - Adaptive
            } else {
                12 // P12 - Idle
            };
            response = response.replace(&format!("{{{{PSTATE_{i}}}}}"), &pstate.to_string());

            // Process metrics (simplified for now - no actual processes)
            response = response.replace(&format!("{{{{PROC_COUNT_{i}}}}}"), "0");

            for proc_idx in 0..MAX_PROCESSES_PER_GPU {
                response = response
                    .replace(&format!("{{{{PID_{i}_{proc_idx}}}}}"), "0")
                    .replace(&format!("{{{{PROC_NAME_{i}_{proc_idx}}}}}"), "none")
                    .replace(&format!("{{{{PROC_UTIL_{i}_{proc_idx}}}}}"), "0")
                    .replace(&format!("{{{{PROC_MEM_{i}_{proc_idx}}}}}"), "0");
            }
        }

        // Replace CPU and memory metrics
        response = response
            .replace("{{CPU_UTIL}}", &format!("{:.2}", cpu.utilization))
            .replace("{{MEM_USED}}", &memory.used_bytes.to_string());

        response
    }
}

impl MockGenerator for NvidiaMockGenerator {
    fn generate(&self, config: &MockConfig) -> MockResult<MockData> {
        self.validate_config(config)?;

        // Generate initial GPU metrics
        let gpus: Vec<GpuMetrics> = (0..config.device_count)
            .map(|_| {
                use rand::{rng, Rng};
                let mut rng = rng();

                GpuMetrics {
                    uuid: crate::mock::metrics::gpu::generate_uuid(),
                    utilization: rng.random_range(0.0..100.0),
                    memory_used_bytes: rng.random_range(1_000_000_000..80_000_000_000),
                    memory_total_bytes: 85_899_345_920, // 80GB
                    temperature_celsius: rng.random_range(35..75),
                    power_consumption_watts: rng.random_range(100.0..450.0),
                    frequency_mhz: rng.random_range(1200..1980),
                    ane_utilization_watts: 0.0,
                    thermal_pressure_level: None,
                }
            })
            .collect();

        // Generate CPU and memory metrics
        use rand::{rng, Rng};
        let mut rng = rng();
        let cpu = CpuMetrics {
            model: "Intel Xeon Platinum".to_string(),
            utilization: rng.random_range(10.0..90.0),
            socket_count: 2,
            core_count: 128,
            thread_count: 256,
            frequency_mhz: 2400,
            temperature_celsius: Some(65),
            power_consumption_watts: Some(250.0),
            socket_utilizations: vec![rng.random_range(10.0..90.0), rng.random_range(10.0..90.0)],
            p_core_count: None,
            e_core_count: None,
            gpu_core_count: None,
            p_core_utilization: None,
            e_core_utilization: None,
            p_cluster_frequency_mhz: None,
            e_cluster_frequency_mhz: None,
            per_core_utilization: vec![],
        };

        let memory = MemoryMetrics {
            total_bytes: 1099511627776, // 1TB
            used_bytes: rng.random_range(10_000_000_000..500_000_000_000),
            available_bytes: rng.random_range(100_000_000_000..600_000_000_000),
            free_bytes: rng.random_range(50_000_000_000..400_000_000_000),
            cached_bytes: rng.random_range(10_000_000_000..100_000_000_000),
            buffers_bytes: rng.random_range(1_000_000_000..10_000_000_000),
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            swap_free_bytes: 0,
            utilization: rng.random_range(10.0..90.0),
        };

        // Build and render template
        let template = self.build_nvidia_template(&gpus, &cpu, &memory);
        let response = self.render_nvidia_response(&template, &gpus, &cpu, &memory);

        Ok(MockData {
            response,
            content_type: "text/plain; version=0.0.4".to_string(),
            timestamp: chrono::Utc::now(),
            platform: MockPlatform::Nvidia,
        })
    }

    fn generate_template(&self, config: &MockConfig) -> MockResult<String> {
        self.validate_config(config)?;

        // Generate sample metrics for template
        let gpus: Vec<GpuMetrics> = (0..config.device_count)
            .map(|i| GpuMetrics {
                uuid: format!("GPU-{:08x}", i as u32),
                utilization: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 85_899_345_920,
                temperature_celsius: 0,
                power_consumption_watts: 0.0,
                frequency_mhz: 0,
                ane_utilization_watts: 0.0,
                thermal_pressure_level: None,
            })
            .collect();

        let cpu = CpuMetrics {
            model: "Intel Xeon Platinum".to_string(),
            utilization: 0.0,
            socket_count: 2,
            core_count: 128,
            thread_count: 256,
            frequency_mhz: 2400,
            temperature_celsius: Some(65),
            power_consumption_watts: Some(250.0),
            socket_utilizations: vec![0.0, 0.0],
            p_core_count: None,
            e_core_count: None,
            gpu_core_count: None,
            p_core_utilization: None,
            e_core_utilization: None,
            p_cluster_frequency_mhz: None,
            e_cluster_frequency_mhz: None,
            per_core_utilization: vec![],
        };

        let memory = MemoryMetrics {
            total_bytes: 1099511627776,
            used_bytes: 0,
            available_bytes: 1099511627776,
            free_bytes: 1099511627776,
            cached_bytes: 0,
            buffers_bytes: 0,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            swap_free_bytes: 0,
            utilization: 0.0,
        };

        Ok(self.build_nvidia_template(&gpus, &cpu, &memory))
    }

    fn render(&self, template: &str, config: &MockConfig) -> MockResult<String> {
        self.validate_config(config)?;

        // This would use actual dynamic values in production
        Ok(template.to_string())
    }

    fn platform(&self) -> MockPlatform {
        MockPlatform::Nvidia
    }
}

// Constants
const MAX_PROCESSES_PER_GPU: usize = 10;
