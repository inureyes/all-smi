//! Template engine for coordinating platform-specific mock generators

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

use crate::mock::constants::{PLACEHOLDER_DISK_AVAIL, PLACEHOLDER_DISK_TOTAL};
use crate::mock::metrics::{CpuMetrics, GpuMetrics, MemoryMetrics, PlatformType};
use crate::mock::templates::{
    apple_silicon::AppleSiliconMockGenerator, furiosa::FuriosaMockGenerator,
    jetson::JetsonMockGenerator, nvidia::NvidiaMockGenerator, rebellions::RebellionsMockGenerator,
    tenstorrent::TenstorrentMockGenerator,
};
use all_smi::traits::mock_generator::{MockConfig, MockGenerator, MockPlatform};

/// Build response template based on platform type (backward compatibility wrapper)
pub fn build_response_template(
    instance_name: &str,
    gpu_name: &str,
    gpus: &[GpuMetrics],
    cpu: &CpuMetrics,
    memory: &MemoryMetrics,
    platform: &PlatformType,
) -> String {
    // Convert PlatformType to MockConfig
    let config = MockConfig {
        platform: platform_type_to_mock_platform(platform),
        device_count: gpus.len(),
        node_name: instance_name.to_string(),
        gpu_name: Some(gpu_name.to_string()),
        ..MockConfig::default()
    };

    // Create appropriate generator
    let generator = create_generator(platform, gpu_name.to_string(), instance_name.to_string());

    // Build template using the generator
    match platform {
        PlatformType::Nvidia => {
            let gen =
                NvidiaMockGenerator::new(Some(gpu_name.to_string()), instance_name.to_string());
            let mut template = gen.build_nvidia_template(gpus, cpu, memory);

            // Add disk metrics
            crate::mock::templates::disk::add_disk_metrics(&mut template, instance_name);
            template
        }
        PlatformType::Apple => {
            let gen = AppleSiliconMockGenerator::new(
                Some(gpu_name.to_string()),
                instance_name.to_string(),
            );
            let mut template = gen.build_apple_template(gpus, cpu, memory);

            // Add disk metrics
            crate::mock::templates::disk::add_disk_metrics(&mut template, instance_name);
            template
        }
        PlatformType::Jetson => {
            let gen =
                JetsonMockGenerator::new(Some(gpu_name.to_string()), instance_name.to_string());
            let mut template = gen.build_jetson_template(gpus);

            // Add disk metrics
            crate::mock::templates::disk::add_disk_metrics(&mut template, instance_name);
            template
        }
        PlatformType::Tenstorrent => {
            let gen = TenstorrentMockGenerator::new(
                Some(gpu_name.to_string()),
                instance_name.to_string(),
            );
            let mut template = gen.build_tenstorrent_template(gpus);

            // Add disk metrics
            crate::mock::templates::disk::add_disk_metrics(&mut template, instance_name);
            template
        }
        PlatformType::Rebellions => {
            let gen =
                RebellionsMockGenerator::new(Some(gpu_name.to_string()), instance_name.to_string());
            let mut template = gen.build_rebellions_template(gpus);

            // Add disk metrics
            crate::mock::templates::disk::add_disk_metrics(&mut template, instance_name);
            template
        }
        PlatformType::Furiosa => {
            let gen =
                FuriosaMockGenerator::new(Some(gpu_name.to_string()), instance_name.to_string());
            let mut template = gen.build_furiosa_template(gpus);

            // Add disk metrics
            crate::mock::templates::disk::add_disk_metrics(&mut template, instance_name);
            template
        }
        _ => {
            // Default to NVIDIA for unsupported platforms
            let gen =
                NvidiaMockGenerator::new(Some(gpu_name.to_string()), instance_name.to_string());
            let mut template = gen.build_nvidia_template(gpus, cpu, memory);

            // Add disk metrics
            crate::mock::templates::disk::add_disk_metrics(&mut template, instance_name);
            template
        }
    }
}

/// Render response with dynamic values (backward compatibility wrapper)
pub fn render_response(
    template: &str,
    gpus: &[GpuMetrics],
    cpu: &CpuMetrics,
    memory: &MemoryMetrics,
    disk_available_bytes: u64,
    disk_total_bytes: u64,
    platform: &PlatformType,
) -> String {
    let mut response = match platform {
        PlatformType::Nvidia => {
            let gen = NvidiaMockGenerator::new(None, "".to_string());
            gen.render_nvidia_response(template, gpus, cpu, memory)
        }
        PlatformType::Apple => {
            let gen = AppleSiliconMockGenerator::new(None, "".to_string());
            gen.render_apple_response(template, gpus, cpu, memory)
        }
        PlatformType::Jetson => {
            let gen = JetsonMockGenerator::new(None, "".to_string());
            gen.render_jetson_response(template, gpus)
        }
        PlatformType::Tenstorrent => {
            let gen = TenstorrentMockGenerator::new(None, "".to_string());
            gen.render_tenstorrent_response(template, gpus)
        }
        PlatformType::Rebellions => {
            let gen = RebellionsMockGenerator::new(None, "".to_string());
            gen.render_rebellions_response(template, gpus)
        }
        PlatformType::Furiosa => {
            let gen = FuriosaMockGenerator::new(None, "".to_string());
            gen.render_furiosa_response(template, gpus)
        }
        _ => template.to_string(),
    };

    // Render CPU and memory metrics for platforms that don't handle them
    if response.contains("{{CPU_UTIL}}") {
        response = response
            .replace("{{CPU_UTIL}}", &format!("{:.2}", cpu.utilization))
            .replace("{{CPU_CORES}}", &cpu.core_count.to_string())
            .replace("{{MEM_USED}}", &memory.used_bytes.to_string())
            .replace("{{MEM_TOTAL}}", &memory.total_bytes.to_string());
    }

    // Render disk metrics (common for all platforms)
    response = response
        .replace(PLACEHOLDER_DISK_TOTAL, &disk_total_bytes.to_string())
        .replace(PLACEHOLDER_DISK_AVAIL, &disk_available_bytes.to_string());

    // Calculate and render disk utilization
    let disk_used = disk_total_bytes - disk_available_bytes;
    let disk_util = if disk_total_bytes > 0 {
        (disk_used as f64 / disk_total_bytes as f64) * 100.0
    } else {
        0.0
    };
    response = response.replace("{{DISK_UTIL}}", &format!("{:.2}", disk_util));

    // Default I/O values if not already replaced
    if response.contains("{{DISK_READ}}") {
        use rand::{rng, Rng};
        let mut rng = rng();
        response = response
            .replace(
                "{{DISK_READ}}",
                &rng.random_range(0..100_000_000).to_string(),
            )
            .replace(
                "{{DISK_WRITE}}",
                &rng.random_range(0..50_000_000).to_string(),
            );
    }

    response
}

/// Create a generator for the given platform
fn create_generator(
    platform: &PlatformType,
    gpu_name: String,
    instance_name: String,
) -> Box<dyn MockGenerator> {
    match platform {
        PlatformType::Nvidia => Box::new(NvidiaMockGenerator::new(Some(gpu_name), instance_name)),
        PlatformType::Apple => Box::new(AppleSiliconMockGenerator::new(
            Some(gpu_name),
            instance_name,
        )),
        PlatformType::Jetson => Box::new(JetsonMockGenerator::new(Some(gpu_name), instance_name)),
        PlatformType::Tenstorrent => {
            Box::new(TenstorrentMockGenerator::new(Some(gpu_name), instance_name))
        }
        PlatformType::Rebellions => {
            Box::new(RebellionsMockGenerator::new(Some(gpu_name), instance_name))
        }
        PlatformType::Furiosa => Box::new(FuriosaMockGenerator::new(Some(gpu_name), instance_name)),
        _ => Box::new(NvidiaMockGenerator::new(Some(gpu_name), instance_name)),
    }
}

/// Convert PlatformType to MockPlatform
fn platform_type_to_mock_platform(platform: &PlatformType) -> MockPlatform {
    match platform {
        PlatformType::Nvidia => MockPlatform::Nvidia,
        PlatformType::Apple => MockPlatform::AppleSilicon,
        PlatformType::Jetson => MockPlatform::Jetson,
        PlatformType::Tenstorrent => MockPlatform::Custom("Tenstorrent".to_string()),
        PlatformType::Rebellions => MockPlatform::Custom("Rebellions".to_string()),
        PlatformType::Furiosa => MockPlatform::Custom("Furiosa".to_string()),
        _ => MockPlatform::Nvidia,
    }
}
