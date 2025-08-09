//! Apple Silicon GPU mock template generator

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
use all_smi::traits::mock_generator::{MockConfig, MockData, MockError, MockGenerator, MockPlatform, MockResult};
use std::collections::HashMap;

/// Apple Silicon GPU mock generator
pub struct AppleSiliconMockGenerator {
    gpu_name: String,
    instance_name: String,
}

impl AppleSiliconMockGenerator {
    pub fn new(gpu_name: Option<String>, instance_name: String) -> Self {
        Self {
            gpu_name: gpu_name.unwrap_or_else(|| "Apple M3 Max".to_string()),
            instance_name,
        }
    }

    /// Build Apple Silicon-specific template
    pub fn build_apple_template(
        &self,
        gpus: &[GpuMetrics],
        cpu: &CpuMetrics,
        memory: &MemoryMetrics,
    ) -> String {
        let mut template = String::with_capacity(3072);
        
        // Basic GPU metrics
        self.add_gpu_metrics(&mut template, gpus);
        
        // Apple-specific: ANE metrics
        self.add_ane_metrics(&mut template, gpus);
        
        // Apple-specific: Thermal pressure metrics
        self.add_thermal_metrics(&mut template, gpus);
        
        // Apple-specific: System metrics with efficiency cores
        self.add_apple_system_metrics(&mut template, cpu, memory);
        
        template
    }

    fn add_gpu_metrics(&self, template: &mut String, gpus: &[GpuMetrics]) {
        let gpu_metrics = [
            ("all_smi_gpu_utilization", "GPU utilization percentage"),
            ("all_smi_gpu_memory_used_bytes", "GPU memory used in bytes"),
            ("all_smi_gpu_memory_total_bytes", "GPU memory total in bytes"),
            ("all_smi_gpu_temperature_celsius", "GPU temperature in celsius"),
            ("all_smi_gpu_power_consumption_watts", "GPU power consumption in watts"),
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

    fn add_ane_metrics(&self, template: &mut String, gpus: &[GpuMetrics]) {
        // ANE utilization in milliwatts
        template.push_str("# HELP all_smi_ane_utilization ANE utilization in mW\n");
        template.push_str("# TYPE all_smi_ane_utilization gauge\n");
        
        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{i}\"",
                self.gpu_name, self.instance_name, gpu.uuid
            );
            template.push_str(&format!(
                "all_smi_ane_utilization{{{labels}}} {{{{ANE_{i}}}}}\n"
            ));
        }

        // ANE power in watts
        template.push_str("# HELP all_smi_ane_power_watts ANE power consumption in watts\n");
        template.push_str("# TYPE all_smi_ane_power_watts gauge\n");
        
        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{i}\"",
                self.gpu_name, self.instance_name, gpu.uuid
            );
            template.push_str(&format!(
                "all_smi_ane_power_watts{{{labels}}} {{{{ANE_WATTS_{i}}}}}\n"
            ));
        }
    }

    fn add_thermal_metrics(&self, template: &mut String, gpus: &[GpuMetrics]) {
        // Thermal pressure level
        template.push_str("# HELP all_smi_thermal_pressure_level Thermal pressure level (nominal/fair/serious/critical)\n");
        template.push_str("# TYPE all_smi_thermal_pressure_level gauge\n");
        
        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{i}\", level=\"{{{{THERMAL_LEVEL_{i}}}}}\"",
                self.gpu_name, self.instance_name, gpu.uuid
            );
            template.push_str(&format!(
                "all_smi_thermal_pressure_level{{{labels}}} {{{{THERMAL_VALUE_{i}}}}}\n"
            ));
        }
    }

    fn add_apple_system_metrics(&self, template: &mut String, cpu: &CpuMetrics, memory: &MemoryMetrics) {
        // CPU metrics with efficiency and performance cores
        template.push_str("# HELP all_smi_cpu_utilization CPU utilization percentage\n");
        template.push_str("# TYPE all_smi_cpu_utilization gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_utilization{{instance=\"{}\", core_type=\"efficiency\"}} {{{{CPU_E_UTIL}}}}\n",
            self.instance_name
        ));
        template.push_str(&format!(
            "all_smi_cpu_utilization{{instance=\"{}\", core_type=\"performance\"}} {{{{CPU_P_UTIL}}}}\n",
            self.instance_name
        ));
        template.push_str(&format!(
            "all_smi_cpu_utilization{{instance=\"{}\"}} {{{{CPU_UTIL}}}}\n",
            self.instance_name
        ));

        // CPU core counts
        template.push_str("# HELP all_smi_cpu_cores Number of CPU cores\n");
        template.push_str("# TYPE all_smi_cpu_cores gauge\n");
        
        // M3 Max has 12 performance + 4 efficiency = 16 total cores
        let p_cores = 12;
        let e_cores = 4;
        template.push_str(&format!(
            "all_smi_cpu_cores{{instance=\"{}\", core_type=\"efficiency\"}} {e_cores}\n",
            self.instance_name
        ));
        template.push_str(&format!(
            "all_smi_cpu_cores{{instance=\"{}\", core_type=\"performance\"}} {p_cores}\n",
            self.instance_name
        ));
        template.push_str(&format!(
            "all_smi_cpu_cores{{instance=\"{}\"}} {}\n",
            self.instance_name, p_cores + e_cores
        ));

        // Memory metrics (unified memory)
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

        // Unified memory pressure
        template.push_str("# HELP all_smi_memory_pressure Memory pressure level\n");
        template.push_str("# TYPE all_smi_memory_pressure gauge\n");
        template.push_str(&format!(
            "all_smi_memory_pressure{{instance=\"{}\"}} {{{{MEM_PRESSURE}}}}\n",
            self.instance_name
        ));
    }

    /// Render dynamic values for Apple Silicon
    pub fn render_apple_response(
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
                .replace(&format!("{{{{UTIL_{i}}}}}"), &format!("{:.2}", gpu.utilization))
                .replace(&format!("{{{{MEM_USED_{i}}}}}"), &gpu.memory_used_bytes.to_string())
                .replace(&format!("{{{{MEM_TOTAL_{i}}}}}"), &gpu.memory_total_bytes.to_string())
                .replace(&format!("{{{{TEMP_{i}}}}}"), &gpu.temperature_celsius.to_string())
                .replace(&format!("{{{{POWER_{i}}}}}"), &format!("{:.3}", gpu.power_consumption_watts))
                .replace(&format!("{{{{FREQ_{i}}}}}"), &gpu.frequency_mhz.to_string());

            // ANE metrics
            response = response
                .replace(&format!("{{{{ANE_{i}}}}}"), &format!("{:.1}", gpu.ane_utilization_watts * 1000.0))
                .replace(&format!("{{{{ANE_WATTS_{i}}}}}"), &format!("{:.3}", gpu.ane_utilization_watts));

            // Thermal pressure
            let (thermal_level, thermal_value) = match gpu.temperature_celsius {
                t if t < 50 => ("nominal", 0),
                t if t < 65 => ("fair", 1),
                t if t < 75 => ("serious", 2),
                _ => ("critical", 3),
            };
            response = response
                .replace(&format!("{{{{THERMAL_LEVEL_{i}}}}}"), thermal_level)
                .replace(&format!("{{{{THERMAL_VALUE_{i}}}}}"), &thermal_value.to_string());
        }

        // Replace CPU metrics with efficiency/performance core split
        let e_util = cpu.utilization * 0.3; // Efficiency cores handle ~30% of load
        let p_util = cpu.utilization * 0.7; // Performance cores handle ~70% of load
        
        response = response
            .replace("{{CPU_E_UTIL}}", &format!("{:.2}", e_util))
            .replace("{{CPU_P_UTIL}}", &format!("{:.2}", p_util))
            .replace("{{CPU_UTIL}}", &format!("{:.2}", cpu.utilization))
            .replace("{{MEM_USED}}", &memory.used_bytes.to_string());

        // Memory pressure (based on usage percentage)
        let mem_pressure = (memory.used_bytes as f64 / memory.total_bytes as f64) * 100.0;
        response = response.replace("{{MEM_PRESSURE}}", &format!("{:.2}", mem_pressure));

        response
    }
}

impl MockGenerator for AppleSiliconMockGenerator {
    fn generate(&self, config: &MockConfig) -> MockResult<MockData> {
        self.validate_config(config)?;
        
        // Generate initial GPU metrics (typically 1 for Apple Silicon)
        let gpus: Vec<GpuMetrics> = (0..config.device_count.min(1))
            .map(|_| {
                use rand::{rng, Rng};
                let mut rng = rng();
                
                GpuMetrics {
                    uuid: format!("APPLE-{:08x}", rng.random::<u32>()),
                    utilization: rng.random_range(0.0..100.0),
                    memory_used_bytes: rng.random_range(1_000_000_000..64_000_000_000),
                    memory_total_bytes: 68_719_476_736, // 64GB unified memory
                    temperature_celsius: rng.random_range(35..75),
                    power_consumption_watts: rng.random_range(5.0..120.0),
                    frequency_mhz: rng.random_range(400..1400),
                    ane_utilization_watts: rng.random_range(0.0..3.0),
                    thermal_pressure_level: Some("nominal".to_string()),
                }
            })
            .collect();

        // Generate CPU and memory metrics
        let cpu = CpuMetrics {
            utilization: rand::rng().random_range(10.0..90.0),
            cores: 16, // M3 Max: 12P + 4E
        };
        
        let memory = MemoryMetrics {
            used_bytes: rand::rng().random_range(10_000_000_000..60_000_000_000),
            total_bytes: 68_719_476_736, // 64GB unified memory
        };

        // Build and render template
        let template = self.build_apple_template(&gpus, &cpu, &memory);
        let response = self.render_apple_response(&template, &gpus, &cpu, &memory);

        Ok(MockData {
            response,
            content_type: "text/plain; version=0.0.4".to_string(),
            timestamp: chrono::Utc::now(),
            platform: MockPlatform::AppleSilicon,
        })
    }

    fn generate_template(&self, config: &MockConfig) -> MockResult<String> {
        self.validate_config(config)?;
        
        // Generate sample metrics for template
        let gpus: Vec<GpuMetrics> = (0..config.device_count.min(1))
            .map(|_| GpuMetrics {
                uuid: format!("APPLE-{:08x}", rand::rng().random::<u32>()),
                utilization: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 68_719_476_736,
                temperature_celsius: 0,
                power_consumption_watts: 0.0,
                frequency_mhz: 0,
                ane_utilization_watts: 0.0,
                thermal_pressure_level: Some("nominal".to_string()),
            })
            .collect();

        let cpu = CpuMetrics {
            utilization: 0.0,
            cores: 16,
        };
        
        let memory = MemoryMetrics {
            used_bytes: 0,
            total_bytes: 68_719_476_736,
        };

        Ok(self.build_apple_template(&gpus, &cpu, &memory))
    }

    fn render(&self, template: &str, config: &MockConfig) -> MockResult<String> {
        self.validate_config(config)?;
        
        // This would use actual dynamic values in production
        Ok(template.to_string())
    }

    fn platform(&self) -> MockPlatform {
        MockPlatform::AppleSilicon
    }
}