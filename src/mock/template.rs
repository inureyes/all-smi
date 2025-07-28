//! Template building and rendering for Prometheus metrics

use crate::mock::constants::*;
use crate::mock::metrics::{CpuMetrics, GpuMetrics, MemoryMetrics, PlatformType};

/// Build static response template with placeholders (called once during init)
pub fn build_response_template(
    instance_name: &str,
    gpu_name: &str,
    gpus: &[GpuMetrics],
    cpu: &CpuMetrics,
    memory: &MemoryMetrics,
    platform: &PlatformType,
) -> String {
    let mut template = String::with_capacity(16384); // Pre-allocate 16KB

    // GPU Metrics headers
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
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                gpu_name, instance_name, gpu.uuid, i
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

    // ANE utilization metrics (Apple Silicon only)
    if let PlatformType::Apple = platform {
        template.push_str("# HELP all_smi_ane_utilization ANE utilization in mW\n");
        template.push_str("# TYPE all_smi_ane_utilization gauge\n");

        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                gpu_name, instance_name, gpu.uuid, i
            );
            let placeholder = format!("{{{{ANE_{i}}}}}");
            template.push_str(&format!(
                "all_smi_ane_utilization{{{labels}}} {placeholder}\n"
            ));
        }

        // ANE power in watts
        template.push_str("# HELP all_smi_ane_power_watts ANE power consumption in watts\n");
        template.push_str("# TYPE all_smi_ane_power_watts gauge\n");

        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                gpu_name, instance_name, gpu.uuid, i
            );
            let placeholder = format!("{{{{ANE_WATTS_{i}}}}}");
            template.push_str(&format!(
                "all_smi_ane_power_watts{{{labels}}} {placeholder}\n"
            ));
        }

        // Thermal pressure info metric
        template.push_str("# HELP all_smi_thermal_pressure_info Thermal pressure level\n");
        template.push_str("# TYPE all_smi_thermal_pressure_info info\n");

        for (i, gpu) in gpus.iter().enumerate() {
            if let Some(ref level) = gpu.thermal_pressure_level {
                let labels = format!(
                    "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", level=\"{}\"",
                    gpu_name, instance_name, gpu.uuid, i, level
                );
                template.push_str(&format!("all_smi_thermal_pressure_info{{{labels}}} 1\n"));
            }
        }
    }

    // GPU vendor info metrics (NVIDIA, Jetson, Tenstorrent, Rebellions, and Furiosa)
    if matches!(
        platform,
        PlatformType::Nvidia
            | PlatformType::Jetson
            | PlatformType::Tenstorrent
            | PlatformType::Rebellions
            | PlatformType::Furiosa
    ) {
        template.push_str("# HELP all_smi_gpu_info GPU vendor-specific information\n");
        template.push_str("# TYPE all_smi_gpu_info info\n");

        for (i, gpu) in gpus.iter().enumerate() {
            let mut labels = vec![
                format!("gpu=\"{}\"", gpu_name),
                format!("instance=\"{}\"", instance_name),
                format!("uuid=\"{}\"", gpu.uuid),
                format!("index=\"{}\"", i),
                format!("type=\"GPU\""), // Default to GPU, can be customized in the future
            ];

            // Add CUDA-specific labels based on platform
            match platform {
                PlatformType::Nvidia => {
                    // Determine architecture and capabilities based on GPU name
                    let (architecture, compute_capability, cuda_version, pcie_gen) =
                        if gpu_name.contains("H200") || gpu_name.contains("H100") {
                            ("Hopper", "9.0", "12.6", "5")
                        } else if gpu_name.contains("A100") || gpu_name.contains("A6000") {
                            ("Ampere", "8.0", "12.4", "4")
                        } else if gpu_name.contains("RTX 4090") || gpu_name.contains("RTX 4080") {
                            ("Ada Lovelace", "8.9", "12.3", "4")
                        } else if gpu_name.contains("V100") {
                            ("Volta", "7.0", "12.2", "3")
                        } else if gpu_name.contains("T4") {
                            ("Turing", "7.5", "12.1", "3")
                        } else {
                            ("Ampere", "8.6", "12.4", "4") // Default to A40-like specs
                        };

                    labels.push("driver_version=\"560.35.05\"".to_string());
                    labels.push(format!("cuda_version=\"{cuda_version}\""));
                    labels.push(format!("architecture=\"{architecture}\""));
                    labels.push(format!("compute_capability=\"{compute_capability}\""));
                    labels.push("compute_mode=\"Default\"".to_string());
                    labels.push("persistence_mode=\"Enabled\"".to_string());
                    labels.push("ecc_mode_current=\"Enabled\"".to_string());
                    labels.push("mig_mode_current=\"Disabled\"".to_string());
                    labels.push(format!("pcie_gen_current=\"{pcie_gen}\""));
                    labels.push(format!("pcie_gen_max=\"{pcie_gen}\""));
                    labels.push("pcie_width_current=\"16\"".to_string());
                    labels.push("pcie_width_max=\"16\"".to_string());
                    labels.push("performance_state=\"P0\"".to_string());
                    labels.push("vbios_version=\"96.00.61.00.01\"".to_string());
                }
                PlatformType::Tenstorrent => {
                    // Tenstorrent NPU specific labels
                    labels[4] = "type=\"NPU\"".to_string(); // Override type to NPU

                    // Determine architecture based on NPU name
                    let (architecture, board_type) = if gpu_name.contains("Grayskull") {
                        ("Grayskull", "e75")
                    } else if gpu_name.contains("Wormhole") {
                        ("Wormhole", "n150")
                    } else if gpu_name.contains("Blackhole") {
                        ("Blackhole", "p100")
                    } else {
                        ("Grayskull", "e75") // Default to Grayskull
                    };

                    labels.push("driver_version=\"1.0.0\"".to_string());
                    labels.push(format!("architecture=\"{architecture}\""));
                    labels.push(format!("board_type=\"{board_type}\""));
                    labels.push("firmware=\"2.9.1\"".to_string());
                }
                PlatformType::Jetson => {
                    // Determine Jetson variant based on GPU name
                    let (driver_version, cuda_version, compute_capability) =
                        if gpu_name.contains("Orin") {
                            ("36.2.0", "12.2", "8.7")
                        } else if gpu_name.contains("Xavier") {
                            ("35.4.1", "11.4", "7.2")
                        } else if gpu_name.contains("Nano") {
                            ("32.7.3", "10.2", "5.3")
                        } else {
                            ("35.4.1", "11.4", "7.2") // Default to Xavier
                        };

                    labels.push(format!("driver_version=\"{driver_version}\""));
                    labels.push(format!("cuda_version=\"{cuda_version}\""));
                    labels.push("architecture=\"Tegra\"".to_string());
                    labels.push(format!("compute_capability=\"{compute_capability}\""));
                }
                PlatformType::Rebellions => {
                    // Rebellions NPU specific labels
                    labels[4] = "type=\"NPU\"".to_string(); // Override type to NPU

                    // Determine model based on NPU name
                    let (architecture, board_type) = if gpu_name.contains("ATOM Max") {
                        ("ATOM", "ATOM-Max")
                    } else if gpu_name.contains("ATOM+") {
                        ("ATOM", "ATOM-Plus")
                    } else {
                        ("ATOM", "ATOM")
                    };

                    labels.push("driver_version=\"1.3.73-release\"".to_string());
                    labels.push(format!("architecture=\"{architecture}\""));
                    labels.push(format!("board_type=\"{board_type}\""));
                    labels.push("firmware=\"1.3.73\"".to_string());
                    labels.push("pcie_gen_current=\"4\"".to_string());
                    labels.push("pcie_gen_max=\"4\"".to_string());
                    labels.push("pcie_width_current=\"16\"".to_string());
                    labels.push("pcie_width_max=\"16\"".to_string());
                    labels.push("performance_state=\"P14\"".to_string());
                }
                PlatformType::Furiosa => {
                    // Furiosa NPU specific labels
                    labels[4] = "type=\"NPU\"".to_string(); // Override type to NPU

                    // Always use RNGD architecture for Furiosa
                    labels.push("driver_version=\"2025.3.0\"".to_string());
                    labels.push("architecture=\"RNGD\"".to_string());
                    labels.push("firmware=\"2025.2.0+d3c908a\"".to_string());
                    labels.push("pert=\"2025.2.0+a78ebff\"".to_string());
                    labels.push("governor=\"OnDemand\"".to_string());
                    labels.push("pcie_gen_current=\"3\"".to_string());
                    labels.push("pcie_gen_max=\"3\"".to_string());
                    labels.push("pcie_width_current=\"16\"".to_string());
                    labels.push("pcie_width_max=\"16\"".to_string());
                }
                _ => {}
            }

            template.push_str(&format!("all_smi_gpu_info{{{}}} 1\n", labels.join(", ")));
        }

        // Add numeric metrics for NVIDIA GPUs
        if let PlatformType::Nvidia = platform {
            add_nvidia_numeric_metrics(&mut template, instance_name, gpu_name, gpus);
        }

        // Add Tenstorrent-specific metrics
        if let PlatformType::Tenstorrent = platform {
            add_tenstorrent_metrics(&mut template, instance_name, gpu_name, gpus);
        }

        // Add Rebellions-specific metrics
        if let PlatformType::Rebellions = platform {
            add_rebellions_metrics(&mut template, instance_name, gpu_name, gpus);
        }

        // Add Furiosa-specific metrics
        if let PlatformType::Furiosa = platform {
            add_furiosa_metrics(&mut template, instance_name, gpu_name, gpus);
        }
    }

    // CPU metrics
    add_cpu_metrics(&mut template, instance_name, cpu, platform);

    // Memory metrics
    add_memory_metrics(&mut template, instance_name, memory);

    // Disk metrics
    add_disk_metrics(&mut template, instance_name);

    template
}

fn add_nvidia_numeric_metrics(
    template: &mut String,
    instance_name: &str,
    gpu_name: &str,
    gpus: &[GpuMetrics],
) {
    // Determine PCIe gen based on GPU model
    let pcie_gen = if gpu_name.contains("H200") || gpu_name.contains("H100") {
        5
    } else if gpu_name.contains("A100") || gpu_name.contains("A6000") || gpu_name.contains("RTX 40")
    {
        4
    } else {
        3
    };

    // Determine clock speeds and power limits based on GPU model
    let (max_graphics_clock, max_memory_clock, power_limit) =
        if gpu_name.contains("H200") || gpu_name.contains("H100") {
            (1980, 2619, 700)
        } else if gpu_name.contains("A100") {
            (1410, 1593, 400)
        } else if gpu_name.contains("RTX 4090") {
            (2520, 1313, 450)
        } else if gpu_name.contains("V100") {
            (1380, 877, 300)
        } else if gpu_name.contains("T4") {
            (1590, 1250, 70)
        } else {
            (1770, 1500, 300) // Default
        };

    // PCIe metrics
    template.push_str("# HELP all_smi_gpu_pcie_gen_current Current PCIe generation\n");
    template.push_str("# TYPE all_smi_gpu_pcie_gen_current gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_pcie_gen_current{{{labels}}} {pcie_gen}\n"
        ));
    }

    template.push_str("# HELP all_smi_gpu_pcie_width_current Current PCIe link width\n");
    template.push_str("# TYPE all_smi_gpu_pcie_width_current gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!("all_smi_gpu_pcie_width_current{{{labels}}} 16\n"));
    }

    // Max clocks
    template.push_str("# HELP all_smi_gpu_clock_graphics_max_mhz Maximum graphics clock in MHz\n");
    template.push_str("# TYPE all_smi_gpu_clock_graphics_max_mhz gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_clock_graphics_max_mhz{{{labels}}} {max_graphics_clock}\n"
        ));
    }

    template.push_str("# HELP all_smi_gpu_clock_memory_max_mhz Maximum memory clock in MHz\n");
    template.push_str("# TYPE all_smi_gpu_clock_memory_max_mhz gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_clock_memory_max_mhz{{{labels}}} {max_memory_clock}\n"
        ));
    }

    // Power limits
    template
        .push_str("# HELP all_smi_gpu_power_limit_current_watts Current power limit in watts\n");
    template.push_str("# TYPE all_smi_gpu_power_limit_current_watts gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_power_limit_current_watts{{{labels}}} {power_limit}\n"
        ));
    }

    template.push_str("# HELP all_smi_gpu_power_limit_max_watts Maximum power limit in watts\n");
    template.push_str("# TYPE all_smi_gpu_power_limit_max_watts gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_power_limit_max_watts{{{labels}}} {power_limit}\n"
        ));
    }

    // Performance state (dynamic based on utilization)
    template
        .push_str("# HELP all_smi_gpu_performance_state GPU performance state (P0=0, P1=1, ...)\n");
    template.push_str("# TYPE all_smi_gpu_performance_state gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        let placeholder = format!("{{{{PSTATE_{i}}}}}");
        template.push_str(&format!(
            "all_smi_gpu_performance_state{{{labels}}} {placeholder}\n"
        ));
    }
}

fn add_furiosa_metrics(
    template: &mut String,
    instance_name: &str,
    gpu_name: &str,
    gpus: &[GpuMetrics],
) {
    // NPU firmware info
    template.push_str("# HELP all_smi_furiosa_firmware_info Furiosa NPU firmware version\n");
    template.push_str("# TYPE all_smi_furiosa_firmware_info info\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", firmware=\"2025.2.0+d3c908a\", pert=\"2025.2.0+a78ebff\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!("all_smi_furiosa_firmware_info{{{labels}}} 1\n"));
    }

    // Core status info (8 cores per NPU)
    template.push_str("# HELP all_smi_furiosa_core_status Furiosa NPU core status\n");
    template.push_str("# TYPE all_smi_furiosa_core_status gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        for core_idx in 0..8 {
            let labels = format!(
                "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", core=\"{}\"",
                gpu_name, instance_name, gpu.uuid, i, core_idx
            );
            // 1 = available, 0 = unavailable
            template.push_str(&format!("all_smi_furiosa_core_status{{{labels}}} 1\n"));
        }
    }

    // PE (Processing Element) utilization per core
    template.push_str("# HELP all_smi_furiosa_pe_utilization PE utilization percentage per core\n");
    template.push_str("# TYPE all_smi_furiosa_pe_utilization gauge\n");
    for (i, _gpu) in gpus.iter().enumerate() {
        for core_idx in 0..8 {
            let labels = format!(
                "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", pe_core=\"{}\"",
                gpu_name, instance_name, gpus[i].uuid, i, core_idx
            );
            let placeholder = format!("{{{{PE_UTIL_{i}_{core_idx}}}}}");
            template.push_str(&format!(
                "all_smi_furiosa_pe_utilization{{{labels}}} {placeholder}\n"
            ));
        }
    }

    // Device liveness status
    template.push_str("# HELP all_smi_furiosa_liveness Device liveness status\n");
    template.push_str("# TYPE all_smi_furiosa_liveness info\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", liveness=\"alive\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!("all_smi_furiosa_liveness{{{labels}}} 1\n"));
    }

    // Governor mode
    template.push_str("# HELP all_smi_furiosa_governor_info Power governor mode\n");
    template.push_str("# TYPE all_smi_furiosa_governor_info info\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", governor=\"OnDemand\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!("all_smi_furiosa_governor_info{{{labels}}} 1\n"));
    }
}

fn add_cpu_metrics(
    template: &mut String,
    instance_name: &str,
    cpu: &CpuMetrics,
    platform: &PlatformType,
) {
    let cpu_labels = format!(
        "cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"0\"",
        cpu.model, instance_name, instance_name
    );

    // Basic CPU metrics
    template.push_str("# HELP all_smi_cpu_utilization CPU utilization percentage\n");
    template.push_str("# TYPE all_smi_cpu_utilization gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_utilization{{{cpu_labels}}} {PLACEHOLDER_CPU_UTIL}\n"
    ));

    template.push_str("# HELP all_smi_cpu_socket_count Number of CPU sockets\n");
    template.push_str("# TYPE all_smi_cpu_socket_count gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_socket_count{{{cpu_labels}}} {}\n",
        cpu.socket_count
    ));

    template.push_str("# HELP all_smi_cpu_core_count Total number of CPU cores\n");
    template.push_str("# TYPE all_smi_cpu_core_count gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_core_count{{{cpu_labels}}} {}\n",
        cpu.core_count
    ));

    template.push_str("# HELP all_smi_cpu_thread_count Total number of CPU threads\n");
    template.push_str("# TYPE all_smi_cpu_thread_count gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_thread_count{{{cpu_labels}}} {}\n",
        cpu.thread_count
    ));

    template.push_str("# HELP all_smi_cpu_frequency_mhz CPU frequency in MHz\n");
    template.push_str("# TYPE all_smi_cpu_frequency_mhz gauge\n");
    template.push_str(&format!(
        "all_smi_cpu_frequency_mhz{{{cpu_labels}}} {}\n",
        cpu.frequency_mhz
    ));

    // Per-core CPU utilization metrics
    if !cpu.per_core_utilization.is_empty() {
        template
            .push_str("# HELP all_smi_cpu_core_utilization Per-core CPU utilization percentage\n");
        template.push_str("# TYPE all_smi_cpu_core_utilization gauge\n");

        for (core_id, _) in cpu.per_core_utilization.iter().enumerate() {
            let core_type =
                if let (Some(p_count), Some(_e_count)) = (cpu.p_core_count, cpu.e_core_count) {
                    if core_id < p_count as usize {
                        "P"
                    } else {
                        "E"
                    }
                } else {
                    "C"
                };

            let core_labels = format!(
                "cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", core_id=\"{}\", core_type=\"{}\"",
                cpu.model, instance_name, instance_name, core_id, core_type
            );

            template.push_str(&format!(
                "all_smi_cpu_core_utilization{{{core_labels}}} {{PLACEHOLDER_CPU_CORE_{core_id}_UTIL}}\n"
            ));
        }
    }

    // Optional CPU metrics (temperature and power)
    if cpu.temperature_celsius.is_some() {
        template.push_str("# HELP all_smi_cpu_temperature_celsius CPU temperature in celsius\n");
        template.push_str("# TYPE all_smi_cpu_temperature_celsius gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_temperature_celsius{{{cpu_labels}}} {PLACEHOLDER_CPU_TEMP}\n"
        ));
    }

    if cpu.power_consumption_watts.is_some() {
        template.push_str(
            "# HELP all_smi_cpu_power_consumption_watts CPU power consumption in watts\n",
        );
        template.push_str("# TYPE all_smi_cpu_power_consumption_watts gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_power_consumption_watts{{{cpu_labels}}} {PLACEHOLDER_CPU_POWER}\n"
        ));
    }

    // Per-socket metrics for multi-socket systems
    if cpu.socket_count > 1 {
        for (socket_id, _) in cpu.socket_utilizations.iter().enumerate() {
            let socket_labels = format!(
                "cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", cpu_index=\"0\", socket_id=\"{}\"",
                cpu.model, instance_name, instance_name, socket_id
            );

            template.push_str(
                "# HELP all_smi_cpu_socket_utilization Per-socket CPU utilization percentage\n",
            );
            template.push_str("# TYPE all_smi_cpu_socket_utilization gauge\n");
            let placeholder = if socket_id == 0 {
                PLACEHOLDER_CPU_SOCKET0_UTIL
            } else {
                PLACEHOLDER_CPU_SOCKET1_UTIL
            };
            template.push_str(&format!(
                "all_smi_cpu_socket_utilization{{{socket_labels}}} {placeholder}\n"
            ));
        }
    }

    // Apple Silicon specific metrics
    if let PlatformType::Apple = platform {
        if let (Some(p_count), Some(e_count), Some(gpu_count)) =
            (cpu.p_core_count, cpu.e_core_count, cpu.gpu_core_count)
        {
            template.push_str("# HELP all_smi_cpu_p_core_count Apple Silicon P-core count\n");
            template.push_str("# TYPE all_smi_cpu_p_core_count gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_p_core_count{{{cpu_labels}}} {p_count}\n"
            ));

            template.push_str("# HELP all_smi_cpu_e_core_count Apple Silicon E-core count\n");
            template.push_str("# TYPE all_smi_cpu_e_core_count gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_e_core_count{{{cpu_labels}}} {e_count}\n"
            ));

            template.push_str("# HELP all_smi_cpu_gpu_core_count Apple Silicon GPU core count\n");
            template.push_str("# TYPE all_smi_cpu_gpu_core_count gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_gpu_core_count{{{cpu_labels}}} {gpu_count}\n"
            ));

            template.push_str("# HELP all_smi_cpu_p_core_utilization Apple Silicon P-core utilization percentage\n");
            template.push_str("# TYPE all_smi_cpu_p_core_utilization gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_p_core_utilization{{{cpu_labels}}} {PLACEHOLDER_CPU_P_CORE_UTIL}\n"
            ));

            template.push_str("# HELP all_smi_cpu_e_core_utilization Apple Silicon E-core utilization percentage\n");
            template.push_str("# TYPE all_smi_cpu_e_core_utilization gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_e_core_utilization{{{cpu_labels}}} {PLACEHOLDER_CPU_E_CORE_UTIL}\n"
            ));
        }

        // Add P/E cluster frequencies for Apple Silicon
        if let (Some(p_freq), Some(e_freq)) =
            (cpu.p_cluster_frequency_mhz, cpu.e_cluster_frequency_mhz)
        {
            template.push_str("# HELP all_smi_cpu_p_cluster_frequency_mhz Apple Silicon P-cluster frequency in MHz\n");
            template.push_str("# TYPE all_smi_cpu_p_cluster_frequency_mhz gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_p_cluster_frequency_mhz{{{cpu_labels}}} {p_freq}\n"
            ));

            template.push_str("# HELP all_smi_cpu_e_cluster_frequency_mhz Apple Silicon E-cluster frequency in MHz\n");
            template.push_str("# TYPE all_smi_cpu_e_cluster_frequency_mhz gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_e_cluster_frequency_mhz{{{cpu_labels}}} {e_freq}\n"
            ));
        }
    }
}

fn add_memory_metrics(template: &mut String, instance_name: &str, memory: &MemoryMetrics) {
    let memory_labels =
        format!("instance=\"{instance_name}\", hostname=\"{instance_name}\", index=\"0\"");

    template.push_str("# HELP all_smi_memory_total_bytes Total system memory in bytes\n");
    template.push_str("# TYPE all_smi_memory_total_bytes gauge\n");
    template.push_str(&format!(
        "all_smi_memory_total_bytes{{{memory_labels}}} {}\n",
        memory.total_bytes
    ));

    template.push_str("# HELP all_smi_memory_used_bytes Used system memory in bytes\n");
    template.push_str("# TYPE all_smi_memory_used_bytes gauge\n");
    template.push_str(&format!(
        "all_smi_memory_used_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_USED}\n"
    ));

    template.push_str("# HELP all_smi_memory_available_bytes Available system memory in bytes\n");
    template.push_str("# TYPE all_smi_memory_available_bytes gauge\n");
    template.push_str(&format!(
        "all_smi_memory_available_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_AVAILABLE}\n"
    ));

    template.push_str("# HELP all_smi_memory_free_bytes Free system memory in bytes\n");
    template.push_str("# TYPE all_smi_memory_free_bytes gauge\n");
    template.push_str(&format!(
        "all_smi_memory_free_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_FREE}\n"
    ));

    template.push_str("# HELP all_smi_memory_utilization Memory utilization percentage\n");
    template.push_str("# TYPE all_smi_memory_utilization gauge\n");
    template.push_str(&format!(
        "all_smi_memory_utilization{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_UTIL}\n"
    ));

    // Swap metrics if available
    if memory.swap_total_bytes > 0 {
        template.push_str("# HELP all_smi_swap_total_bytes Total swap space in bytes\n");
        template.push_str("# TYPE all_smi_swap_total_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_swap_total_bytes{{{memory_labels}}} {}\n",
            memory.swap_total_bytes
        ));

        template.push_str("# HELP all_smi_swap_used_bytes Used swap space in bytes\n");
        template.push_str("# TYPE all_smi_swap_used_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_swap_used_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_SWAP_USED}\n"
        ));

        template.push_str("# HELP all_smi_swap_free_bytes Free swap space in bytes\n");
        template.push_str("# TYPE all_smi_swap_free_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_swap_free_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_SWAP_FREE}\n"
        ));
    }

    // Linux-specific metrics
    if memory.buffers_bytes > 0 {
        template.push_str("# HELP all_smi_memory_buffers_bytes Memory used for buffers in bytes\n");
        template.push_str("# TYPE all_smi_memory_buffers_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_buffers_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_BUFFERS}\n"
        ));
    }

    if memory.cached_bytes > 0 {
        template.push_str("# HELP all_smi_memory_cached_bytes Memory used for cache in bytes\n");
        template.push_str("# TYPE all_smi_memory_cached_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_cached_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_CACHED}\n"
        ));
    }
}

fn add_disk_metrics(template: &mut String, instance_name: &str) {
    template.push_str("# HELP all_smi_disk_total_bytes Total disk space in bytes\n");
    template.push_str("# TYPE all_smi_disk_total_bytes gauge\n");
    let disk_labels = format!("instance=\"{instance_name}\", mount_point=\"/\", index=\"0\"");
    template.push_str(&format!(
        "all_smi_disk_total_bytes{{{disk_labels}}} {PLACEHOLDER_DISK_TOTAL}\n"
    ));

    template.push_str("# HELP all_smi_disk_available_bytes Available disk space in bytes\n");
    template.push_str("# TYPE all_smi_disk_available_bytes gauge\n");
    template.push_str(&format!(
        "all_smi_disk_available_bytes{{{disk_labels}}} {PLACEHOLDER_DISK_AVAIL}\n"
    ));
}

/// Render the response by replacing placeholders with actual values
pub fn render_response(
    template: &str,
    gpus: &[GpuMetrics],
    cpu: &CpuMetrics,
    memory: &MemoryMetrics,
    disk_available_bytes: u64,
    disk_total_bytes: u64,
    platform: &PlatformType,
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

        // Replace ANE metrics for Apple Silicon
        if let PlatformType::Apple = platform {
            // ANE utilization in mW
            response = response.replace(
                &format!("{{{{ANE_{i}}}}}"),
                &format!("{:.1}", gpu.ane_utilization_watts * 1000.0),
            );
            // ANE power in watts
            response = response.replace(
                &format!("{{{{ANE_WATTS_{i}}}}}"),
                &format!("{:.3}", gpu.ane_utilization_watts),
            );
        }

        // Replace performance state for NVIDIA GPUs (based on utilization)
        if let PlatformType::Nvidia = platform {
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
        }

        // Replace Tenstorrent-specific metrics
        if let PlatformType::Tenstorrent = platform {
            use rand::{rng, Rng};
            let mut rng = rng();

            // Temperature sensors (slight variations from main temp)
            let asic_temp = gpu.temperature_celsius;
            let vreg_temp =
                (asic_temp as f32 + rng.random_range(-5.0..5.0)).clamp(30.0, 90.0) as u32;
            let inlet_temp =
                (asic_temp as f32 - rng.random_range(10.0..20.0)).clamp(20.0, 60.0) as u32;

            response = response
                .replace(&format!("{{{{ASIC_TEMP_{i}}}}}"), &asic_temp.to_string())
                .replace(&format!("{{{{VREG_TEMP_{i}}}}}"), &vreg_temp.to_string())
                .replace(&format!("{{{{INLET_TEMP_{i}}}}}"), &inlet_temp.to_string());

            // Clock frequencies
            let ai_clk = gpu.frequency_mhz; // Use main frequency as AI clock
            let axi_clk = rng.random_range(800..1200);
            let arc_clk = rng.random_range(500..800);

            response = response
                .replace(&format!("{{{{AICLK_{i}}}}}"), &ai_clk.to_string())
                .replace(&format!("{{{{AXICLK_{i}}}}}"), &axi_clk.to_string())
                .replace(&format!("{{{{ARCCLK_{i}}}}}"), &arc_clk.to_string());

            // Voltage and current (derive from power)
            let voltage = rng.random_range(0.85..0.95); // Core voltage in volts
            let current = gpu.power_consumption_watts / voltage; // P = V * I

            response = response
                .replace(&format!("{{{{VOLTAGE_{i}}}}}"), &format!("{voltage:.3}"))
                .replace(&format!("{{{{CURRENT_{i}}}}}"), &format!("{current:.1}"));

            // Heartbeat counter (incrementing)
            let heartbeat = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + i as u64)
                * 10; // Different counter for each NPU

            response = response.replace(&format!("{{{{HEARTBEAT_{i}}}}}"), &heartbeat.to_string());
        }

        // Replace Furiosa-specific metrics
        if let PlatformType::Furiosa = platform {
            use rand::{rng, Rng};
            let mut rng = rng();

            // PE utilization per core (8 cores per NPU)
            for core_idx in 0..8 {
                // Generate different utilization for each core, correlated with overall GPU utilization
                let base_util = gpu.utilization;
                let core_variation = rng.random_range(-15.0..15.0);
                let core_util = (base_util + core_variation).clamp(0.0, 100.0);

                response = response.replace(
                    &format!("{{{{PE_UTIL_{i}_{core_idx}}}}}"),
                    &format!("{core_util:.2}"),
                );
            }
        }
    }

    // Replace CPU metrics
    response = response
        .replace(PLACEHOLDER_CPU_UTIL, &format!("{:.2}", cpu.utilization))
        .replace(
            PLACEHOLDER_CPU_SOCKET0_UTIL,
            &format!(
                "{:.2}",
                cpu.socket_utilizations.first().copied().unwrap_or(0.0)
            ),
        )
        .replace(
            PLACEHOLDER_CPU_SOCKET1_UTIL,
            &format!(
                "{:.2}",
                cpu.socket_utilizations.get(1).copied().unwrap_or(0.0)
            ),
        );

    if let Some(temp) = cpu.temperature_celsius {
        response = response.replace(PLACEHOLDER_CPU_TEMP, &temp.to_string());
    }

    if let Some(power) = cpu.power_consumption_watts {
        response = response.replace(PLACEHOLDER_CPU_POWER, &format!("{power:.3}"));
    }

    // Replace per-core CPU utilization metrics
    for (core_id, util) in cpu.per_core_utilization.iter().enumerate() {
        response = response.replace(
            &format!("{{PLACEHOLDER_CPU_CORE_{core_id}_UTIL}}"),
            &format!("{util:.2}"),
        );
    }

    // Apple Silicon specific replacements
    if let PlatformType::Apple = platform {
        if let (Some(p_util), Some(e_util)) = (cpu.p_core_utilization, cpu.e_core_utilization) {
            response = response
                .replace(PLACEHOLDER_CPU_P_CORE_UTIL, &format!("{p_util:.2}"))
                .replace(PLACEHOLDER_CPU_E_CORE_UTIL, &format!("{e_util:.2}"));
        }
    }

    // Replace memory metrics
    response = response
        .replace(PLACEHOLDER_SYS_MEMORY_USED, &memory.used_bytes.to_string())
        .replace(
            PLACEHOLDER_SYS_MEMORY_AVAILABLE,
            &memory.available_bytes.to_string(),
        )
        .replace(PLACEHOLDER_SYS_MEMORY_FREE, &memory.free_bytes.to_string())
        .replace(
            PLACEHOLDER_SYS_MEMORY_UTIL,
            &format!("{:.2}", memory.utilization),
        );

    // Replace swap metrics if available
    if memory.swap_total_bytes > 0 {
        response = response
            .replace(
                PLACEHOLDER_SYS_SWAP_USED,
                &memory.swap_used_bytes.to_string(),
            )
            .replace(
                PLACEHOLDER_SYS_SWAP_FREE,
                &memory.swap_free_bytes.to_string(),
            );
    }

    // Replace buffer and cache metrics if available
    if memory.buffers_bytes > 0 {
        response = response.replace(
            PLACEHOLDER_SYS_MEMORY_BUFFERS,
            &memory.buffers_bytes.to_string(),
        );
    }

    if memory.cached_bytes > 0 {
        response = response.replace(
            PLACEHOLDER_SYS_MEMORY_CACHED,
            &memory.cached_bytes.to_string(),
        );
    }

    // Replace disk metrics
    response = response
        .replace(PLACEHOLDER_DISK_TOTAL, &disk_total_bytes.to_string())
        .replace(PLACEHOLDER_DISK_AVAIL, &disk_available_bytes.to_string());

    response
}

fn add_tenstorrent_metrics(
    template: &mut String,
    instance_name: &str,
    gpu_name: &str,
    gpus: &[GpuMetrics],
) {
    // NPU firmware info
    template.push_str("# HELP all_smi_npu_firmware_info NPU firmware version\n");
    template.push_str("# TYPE all_smi_npu_firmware_info info\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", firmware=\"2.9.1\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!("all_smi_npu_firmware_info{{{labels}}} 1\n"));
    }

    // Tenstorrent board info
    template.push_str("# HELP all_smi_tenstorrent_board_info Tenstorrent board information\n");
    template.push_str("# TYPE all_smi_tenstorrent_board_info info\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let (architecture, board_type) = if gpu_name.contains("Grayskull") {
            ("grayskull", "e75")
        } else if gpu_name.contains("Wormhole") {
            ("wormhole", "n150")
        } else if gpu_name.contains("Blackhole") {
            ("blackhole", "p100")
        } else {
            ("grayskull", "e75")
        };

        let board_id = format!("00000000{:08x}", i + 1);
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", board_type=\"{}\", board_id=\"{}\", architecture=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i, board_type, board_id, architecture
        );
        template.push_str(&format!("all_smi_tenstorrent_board_info{{{labels}}} 1\n"));
    }

    // Firmware versions
    template.push_str("# HELP all_smi_tenstorrent_arc_firmware_info ARC firmware version\n");
    template.push_str("# TYPE all_smi_tenstorrent_arc_firmware_info info\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", version=\"2.9.1\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_tenstorrent_arc_firmware_info{{{labels}}} 1\n"
        ));
    }

    // Additional temperature sensors
    let temp_sensors = [
        (
            "all_smi_tenstorrent_asic_temperature_celsius",
            "ASIC temperature in celsius",
            "{{{{ASIC_TEMP_{}}}}}",
        ),
        (
            "all_smi_tenstorrent_vreg_temperature_celsius",
            "Voltage regulator temperature in celsius",
            "{{{{VREG_TEMP_{}}}}}",
        ),
        (
            "all_smi_tenstorrent_inlet_temperature_celsius",
            "Inlet temperature in celsius",
            "{{{{INLET_TEMP_{}}}}}",
        ),
    ];

    for (metric_name, help_text, placeholder_pattern) in &temp_sensors {
        template.push_str(&format!("# HELP {metric_name} {help_text}\n"));
        template.push_str(&format!("# TYPE {metric_name} gauge\n"));

        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                gpu_name, instance_name, gpu.uuid, i
            );
            let placeholder = placeholder_pattern.replace("{}", &i.to_string());
            template.push_str(&format!("{metric_name}{{{labels}}} {placeholder}\n"));
        }
    }

    // Clock frequencies
    let clock_metrics = [
        (
            "all_smi_tenstorrent_aiclk_mhz",
            "AI clock frequency in MHz",
            "{{{{AICLK_{}}}}}",
        ),
        (
            "all_smi_tenstorrent_axiclk_mhz",
            "AXI clock frequency in MHz",
            "{{{{AXICLK_{}}}}}",
        ),
        (
            "all_smi_tenstorrent_arcclk_mhz",
            "ARC clock frequency in MHz",
            "{{{{ARCCLK_{}}}}}",
        ),
    ];

    for (metric_name, help_text, placeholder_pattern) in &clock_metrics {
        template.push_str(&format!("# HELP {metric_name} {help_text}\n"));
        template.push_str(&format!("# TYPE {metric_name} gauge\n"));

        for (i, gpu) in gpus.iter().enumerate() {
            let labels = format!(
                "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                gpu_name, instance_name, gpu.uuid, i
            );
            let placeholder = placeholder_pattern.replace("{}", &i.to_string());
            template.push_str(&format!("{metric_name}{{{labels}}} {placeholder}\n"));
        }
    }

    // Power metrics
    template.push_str("# HELP all_smi_tenstorrent_voltage_volts Core voltage in volts\n");
    template.push_str("# TYPE all_smi_tenstorrent_voltage_volts gauge\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        let placeholder = format!("{{{{VOLTAGE_{i}}}}}");
        template.push_str(&format!(
            "all_smi_tenstorrent_voltage_volts{{{labels}}} {placeholder}\n"
        ));
    }

    template.push_str("# HELP all_smi_tenstorrent_current_amperes Current in amperes\n");
    template.push_str("# TYPE all_smi_tenstorrent_current_amperes gauge\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        let placeholder = format!("{{{{CURRENT_{i}}}}}");
        template.push_str(&format!(
            "all_smi_tenstorrent_current_amperes{{{labels}}} {placeholder}\n"
        ));
    }

    // Heartbeat counter
    template.push_str("# HELP all_smi_tenstorrent_heartbeat Device heartbeat counter\n");
    template.push_str("# TYPE all_smi_tenstorrent_heartbeat counter\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        let placeholder = format!("{{{{HEARTBEAT_{i}}}}}");
        template.push_str(&format!(
            "all_smi_tenstorrent_heartbeat{{{labels}}} {placeholder}\n"
        ));
    }
}

fn add_rebellions_metrics(
    template: &mut String,
    instance_name: &str,
    gpu_name: &str,
    gpus: &[GpuMetrics],
) {
    // NPU firmware info
    template.push_str("# HELP all_smi_rebellions_firmware_info Rebellions NPU firmware version\n");
    template.push_str("# TYPE all_smi_rebellions_firmware_info info\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", firmware=\"1.3.73\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!("all_smi_rebellions_firmware_info{{{labels}}} 1\n"));
    }

    // Rebellions device info
    template.push_str("# HELP all_smi_rebellions_device_info Rebellions device information\n");
    template.push_str("# TYPE all_smi_rebellions_device_info info\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let model_type = if gpu_name.contains("ATOM Max") {
            "ATOM-Max"
        } else if gpu_name.contains("ATOM+") {
            "ATOM-Plus"
        } else {
            "ATOM"
        };

        let sid = format!("00000000225091{:02}", 38 + i);
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", model=\"{}\", sid=\"{}\", location=\"5\"",
            gpu_name, instance_name, gpu.uuid, i, model_type, sid
        );
        template.push_str(&format!("all_smi_rebellions_device_info{{{labels}}} 1\n"));
    }

    // KMD (Kernel Mode Driver) version info
    template.push_str("# HELP all_smi_rebellions_kmd_info Rebellions KMD version\n");
    template.push_str("# TYPE all_smi_rebellions_kmd_info info\n");

    let labels = format!("instance=\"{instance_name}\", version=\"1.3.73-release\"");
    template.push_str(&format!("all_smi_rebellions_kmd_info{{{labels}}} 1\n"));

    // Performance state
    template.push_str("# HELP all_smi_rebellions_pstate_info Current performance state\n");
    template.push_str("# TYPE all_smi_rebellions_pstate_info info\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let pstate = "P14"; // Default performance state
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", pstate=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i, pstate
        );
        template.push_str(&format!("all_smi_rebellions_pstate_info{{{labels}}} 1\n"));
    }

    // Device status
    template.push_str("# HELP all_smi_rebellions_status Device operational status\n");
    template.push_str("# TYPE all_smi_rebellions_status gauge\n");

    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "npu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\", status=\"normal\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        // 1 = normal, 0 = error
        template.push_str(&format!("all_smi_rebellions_status{{{labels}}} 1\n"));
    }
}
