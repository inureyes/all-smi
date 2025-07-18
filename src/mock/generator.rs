//! Metric generation utilities for creating realistic initial values

use crate::mock::constants::{DISK_SIZE_12TB, DISK_SIZE_1TB, DISK_SIZE_4TB, NUM_GPUS};
use crate::mock::metrics::gpu::generate_uuid;
use crate::mock::metrics::{CpuMetrics, GpuMetrics, MemoryMetrics, PlatformType};
use rand::{rng, Rng};

/// Extract GPU memory size from GPU name (e.g., "NVIDIA H200 141GB HBM3" -> 141)
pub fn extract_gpu_memory_gb(gpu_name: &str) -> u64 {
    if let Some(gb_pos) = gpu_name.find("GB") {
        let before_gb = &gpu_name[..gb_pos];
        if let Some(space_pos) = before_gb.rfind(' ') {
            if let Ok(gb) = before_gb[space_pos + 1..].parse::<u64>() {
                return gb;
            }
        }
        // Try to find number right before GB
        let mut end = gb_pos;
        while end > 0 && gpu_name.chars().nth(end - 1).unwrap().is_ascii_digit() {
            end -= 1;
        }
        if let Ok(gb) = gpu_name[end..gb_pos].parse::<u64>() {
            return gb;
        }
    }
    // Default to 24GB if can't parse
    24
}

/// Generate initial GPU metrics for all GPUs
pub fn generate_gpus(gpu_name: &str, platform: &PlatformType) -> Vec<GpuMetrics> {
    let gpu_memory_gb = extract_gpu_memory_gb(gpu_name);
    let memory_total_bytes = gpu_memory_gb * 1024 * 1024 * 1024;
    let mut rng = rng();

    (0..NUM_GPUS)
        .map(|_| {
            let utilization = rng.random_range(10.0..90.0);
            let memory_used_bytes =
                rng.random_range(memory_total_bytes / 10..memory_total_bytes * 9 / 10);
            let memory_usage_percent =
                (memory_used_bytes as f32 / memory_total_bytes as f32) * 100.0;

            // Calculate realistic initial power consumption
            let base_power = rng.random_range(80.0..120.0);
            let util_power_contribution = utilization * rng.random_range(4.0..6.0);
            let memory_power_contribution = memory_usage_percent * rng.random_range(1.0..2.0);
            let gpu_bias = rng.random_range(-30.0..30.0);
            let power_consumption_watts =
                (base_power + util_power_contribution + memory_power_contribution + gpu_bias)
                    .clamp(80.0, 700.0);

            // Calculate realistic initial temperature
            let base_temp = 45.0;
            let util_temp_contribution = utilization * 0.25;
            let power_temp_contribution = (power_consumption_watts - 200.0) * 0.05;
            let temperature_celsius = (base_temp + util_temp_contribution + power_temp_contribution)
                .clamp(35.0, 85.0) as u32;

            // Calculate realistic initial frequency
            let base_freq = 1200.0;
            let util_freq_contribution = utilization * 6.0;
            let frequency_mhz = (base_freq + util_freq_contribution).clamp(1000.0, 1980.0) as u32;

            // ANE utilization only for Apple Silicon
            let ane_utilization_watts = if *platform == PlatformType::Apple {
                rng.random_range(0.0..2.5) // ANE power consumption 0-2.5W
            } else {
                0.0
            };

            // Thermal pressure level for Apple Silicon
            let thermal_pressure_level = if *platform == PlatformType::Apple {
                let levels = ["Nominal", "Fair", "Serious", "Critical"];
                // Most of the time it should be Nominal
                let weights = [0.85, 0.10, 0.04, 0.01];
                let rand = rng.random::<f32>();
                let mut cumulative = 0.0;
                let mut selected = "Nominal";
                for (level, weight) in levels.iter().zip(weights.iter()) {
                    cumulative += weight;
                    if rand <= cumulative {
                        selected = level;
                        break;
                    }
                }
                Some(selected.to_string())
            } else {
                None
            };

            GpuMetrics {
                uuid: generate_uuid(),
                utilization,
                memory_used_bytes,
                memory_total_bytes,
                temperature_celsius,
                power_consumption_watts,
                frequency_mhz,
                ane_utilization_watts,
                thermal_pressure_level,
            }
        })
        .collect()
}

/// Generate initial CPU metrics based on platform
pub fn generate_cpu_metrics(platform: &PlatformType) -> CpuMetrics {
    let mut rng = rng();

    match platform {
        PlatformType::Apple => {
            // Apple Silicon M1/M2/M3
            let models = [
                "Apple M1",
                "Apple M2",
                "Apple M2 Pro",
                "Apple M2 Max",
                "Apple M3",
            ];
            let model = models[rng.random_range(0..models.len())].to_string();

            let (p_cores, e_cores, gpu_cores) = match model.as_str() {
                "Apple M1" => (4, 4, 8),
                "Apple M2" => (4, 4, 10),
                "Apple M2 Pro" => (8, 4, 19),
                "Apple M2 Max" => (8, 4, 38),
                "Apple M3" => (4, 4, 10),
                _ => (4, 4, 8),
            };

            CpuMetrics {
                model,
                utilization: rng.random_range(15.0..75.0),
                socket_count: 1,
                core_count: p_cores + e_cores,
                thread_count: p_cores + e_cores, // Apple Silicon doesn't use hyperthreading
                frequency_mhz: rng.random_range(3000..3500),
                temperature_celsius: Some(rng.random_range(45..70)),
                power_consumption_watts: Some(rng.random_range(15.0..35.0)),
                socket_utilizations: vec![rng.random_range(15.0..75.0)],
                p_core_count: Some(p_cores),
                e_core_count: Some(e_cores),
                gpu_core_count: Some(gpu_cores),
                p_core_utilization: Some(rng.random_range(10.0..80.0)),
                e_core_utilization: Some(rng.random_range(5.0..40.0)),
                p_cluster_frequency_mhz: Some(rng.random_range(2800..3400)),
                e_cluster_frequency_mhz: Some(rng.random_range(1000..1800)),
            }
        }
        PlatformType::Intel => {
            let models = [
                "Intel Xeon Gold 6248R",
                "Intel Xeon Platinum 8280",
                "Intel Core i9-13900K",
                "Intel Xeon E5-2699 v4",
            ];
            let model = models[rng.random_range(0..models.len())].to_string();

            let socket_count = if model.contains("Xeon") {
                rng.random_range(1..=2)
            } else {
                1
            };
            let cores_per_socket = rng.random_range(8..32);
            let total_cores = socket_count * cores_per_socket;
            let total_threads = total_cores * 2; // Intel hyperthreading

            let socket_utilizations: Vec<f32> = (0..socket_count)
                .map(|_| rng.random_range(20.0..80.0))
                .collect();
            let overall_util =
                socket_utilizations.iter().sum::<f32>() / socket_utilizations.len() as f32;

            CpuMetrics {
                model,
                utilization: overall_util,
                socket_count,
                core_count: total_cores,
                thread_count: total_threads,
                frequency_mhz: rng.random_range(2400..3800),
                temperature_celsius: Some(rng.random_range(55..85)),
                power_consumption_watts: Some(rng.random_range(150.0..400.0)),
                socket_utilizations,
                p_core_count: None,
                e_core_count: None,
                gpu_core_count: None,
                p_core_utilization: None,
                e_core_utilization: None,
                p_cluster_frequency_mhz: None,
                e_cluster_frequency_mhz: None,
            }
        }
        PlatformType::Amd => {
            let models = [
                "AMD EPYC 7742",
                "AMD Ryzen 9 7950X",
                "AMD EPYC 9554",
                "AMD Threadripper PRO 5995WX",
            ];
            let model = models[rng.random_range(0..models.len())].to_string();

            let socket_count = if model.contains("EPYC") {
                rng.random_range(1..=2)
            } else {
                1
            };
            let cores_per_socket = rng.random_range(16..64);
            let total_cores = socket_count * cores_per_socket;
            let total_threads = total_cores * 2; // AMD SMT

            let socket_utilizations: Vec<f32> = (0..socket_count)
                .map(|_| rng.random_range(25.0..85.0))
                .collect();
            let overall_util =
                socket_utilizations.iter().sum::<f32>() / socket_utilizations.len() as f32;

            CpuMetrics {
                model,
                utilization: overall_util,
                socket_count,
                core_count: total_cores,
                thread_count: total_threads,
                frequency_mhz: rng.random_range(2200..4500),
                temperature_celsius: Some(rng.random_range(50..80)),
                power_consumption_watts: Some(rng.random_range(180.0..500.0)),
                socket_utilizations,
                p_core_count: None,
                e_core_count: None,
                gpu_core_count: None,
                p_core_utilization: None,
                e_core_utilization: None,
                p_cluster_frequency_mhz: None,
                e_cluster_frequency_mhz: None,
            }
        }
        PlatformType::Jetson => {
            // NVIDIA Jetson platforms
            let models = [
                "NVIDIA Jetson AGX Orin",
                "NVIDIA Jetson Xavier NX",
                "NVIDIA Jetson Nano",
            ];
            let model = models[rng.random_range(0..models.len())].to_string();

            let (cores, threads) = match model.as_str() {
                "NVIDIA Jetson AGX Orin" => (12, 12),
                "NVIDIA Jetson Xavier NX" => (6, 6),
                "NVIDIA Jetson Nano" => (4, 4),
                _ => (6, 6),
            };

            CpuMetrics {
                model,
                utilization: rng.random_range(20.0..70.0),
                socket_count: 1,
                core_count: cores,
                thread_count: threads,
                frequency_mhz: rng.random_range(1400..2200),
                temperature_celsius: Some(rng.random_range(55..75)),
                power_consumption_watts: Some(rng.random_range(10.0..60.0)),
                socket_utilizations: vec![rng.random_range(20.0..70.0)],
                p_core_count: None,
                e_core_count: None,
                gpu_core_count: None,
                p_core_utilization: None,
                e_core_utilization: None,
                p_cluster_frequency_mhz: None,
                e_cluster_frequency_mhz: None,
            }
        }
        PlatformType::Nvidia => {
            // Default NVIDIA GPU server (Intel/AMD CPU)
            let models = ["Intel Xeon Gold 6248R", "AMD EPYC 7742"];
            let model = models[rng.random_range(0..models.len())].to_string();

            let socket_count = 2;
            let cores_per_socket = rng.random_range(16..32);
            let total_cores = socket_count * cores_per_socket;
            let total_threads = total_cores * 2;

            let socket_utilizations: Vec<f32> = (0..socket_count)
                .map(|_| rng.random_range(30.0..85.0))
                .collect();
            let overall_util =
                socket_utilizations.iter().sum::<f32>() / socket_utilizations.len() as f32;

            CpuMetrics {
                model,
                utilization: overall_util,
                socket_count,
                core_count: total_cores,
                thread_count: total_threads,
                frequency_mhz: rng.random_range(2400..3600),
                temperature_celsius: Some(rng.random_range(60..80)),
                power_consumption_watts: Some(rng.random_range(200.0..450.0)),
                socket_utilizations,
                p_core_count: None,
                e_core_count: None,
                gpu_core_count: None,
                p_core_utilization: None,
                e_core_utilization: None,
                p_cluster_frequency_mhz: None,
                e_cluster_frequency_mhz: None,
            }
        }
        PlatformType::Tenstorrent => {
            // Tenstorrent NPU server (typically AMD CPU)
            let models = ["AMD EPYC 7713P", "AMD EPYC 7763", "AMD EPYC 9754"];
            let model = models[rng.random_range(0..models.len())].to_string();

            let socket_count = 1; // Tenstorrent systems often use single-socket configurations
            let cores_per_socket = rng.random_range(32..64);
            let total_cores = socket_count * cores_per_socket;
            let total_threads = total_cores * 2;

            let socket_utilizations: Vec<f32> = (0..socket_count)
                .map(|_| rng.random_range(25.0..75.0))
                .collect();
            let overall_util =
                socket_utilizations.iter().sum::<f32>() / socket_utilizations.len() as f32;

            CpuMetrics {
                model,
                utilization: overall_util,
                socket_count,
                core_count: total_cores,
                thread_count: total_threads,
                frequency_mhz: rng.random_range(2800..3500),
                temperature_celsius: Some(rng.random_range(55..75)),
                power_consumption_watts: Some(rng.random_range(180.0..280.0)),
                socket_utilizations,
                p_core_count: None,
                e_core_count: None,
                gpu_core_count: None,
                p_core_utilization: None,
                e_core_utilization: None,
                p_cluster_frequency_mhz: None,
                e_cluster_frequency_mhz: None,
            }
        }
    }
}

/// Generate initial memory metrics
pub fn generate_memory_metrics() -> MemoryMetrics {
    let mut rng = rng();

    // Memory size options: 256GB, 512GB, 1TB, 2TB, 4TB
    let memory_sizes_gb = [256, 512, 1024, 2048, 4096];
    let total_gb = memory_sizes_gb[rng.random_range(0..memory_sizes_gb.len())];
    let total_bytes = total_gb as u64 * 1024 * 1024 * 1024;

    // Start used memory at 40%+ and make it fluctuate
    let base_utilization = rng.random_range(40.0..80.0);
    let utilization = base_utilization as f32;
    let used_bytes = (total_bytes as f64 * utilization as f64 / 100.0) as u64;
    let available_bytes = total_bytes - used_bytes;
    let free_bytes = rng.random_range(available_bytes / 4..available_bytes * 3 / 4);

    // Linux-specific memory breakdown
    let buffers_bytes = rng.random_range(total_bytes / 100..total_bytes / 20); // 1-5% for buffers
    let cached_bytes = rng.random_range(total_bytes / 50..total_bytes / 10); // 2-10% for cache

    // Swap configuration (some nodes have swap, others don't)
    let (swap_total_bytes, swap_used_bytes, swap_free_bytes) = if rng.random_bool(0.7) {
        // 70% chance of having swap
        // Swap size: min(1/8 of total memory, 32GB)
        let max_swap_32gb = 32 * 1024 * 1024 * 1024; // 32GB in bytes
        let max_swap_eighth = total_bytes / 8; // 1/8 of total memory
        let swap_total = std::cmp::min(max_swap_32gb, max_swap_eighth);

        // Swap is only used when memory usage is at 100%
        // Since we start at 40-80% usage, no swap is used initially
        let swap_used = 0;
        let swap_free = swap_total;
        (swap_total, swap_used, swap_free)
    } else {
        (0, 0, 0)
    };

    MemoryMetrics {
        total_bytes,
        used_bytes,
        available_bytes,
        free_bytes,
        buffers_bytes,
        cached_bytes,
        swap_total_bytes,
        swap_used_bytes,
        swap_free_bytes,
        utilization,
    }
}

/// Generate random disk metrics
pub fn generate_disk_metrics() -> (u64, u64) {
    let mut rng = rng();

    // Choose random disk size from options
    let disk_sizes = [DISK_SIZE_1TB, DISK_SIZE_4TB, DISK_SIZE_12TB];
    let disk_total_bytes = disk_sizes[rng.random_range(0..disk_sizes.len())];
    let disk_available_bytes = rng.random_range(disk_total_bytes / 10..disk_total_bytes * 9 / 10);

    (disk_total_bytes, disk_available_bytes)
}
