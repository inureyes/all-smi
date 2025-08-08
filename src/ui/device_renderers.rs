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

use std::io::Write;

use crossterm::{queue, style::Color, style::Print};

use crate::device::{CpuInfo, GpuInfo, MemoryInfo};
use crate::storage::info::StorageInfo;
use crate::ui::text::{print_colored_text, truncate_to_width};
use crate::ui::widgets::{draw_bar, draw_bar_multi, BarSegment};

/// Formats a hostname for display with scrolling animation if it exceeds 9 characters
/// Always returns a string with exactly 9 characters (padded with spaces if needed)
fn format_hostname_with_scroll(hostname: &str, scroll_offset: usize) -> String {
    if hostname.len() > 9 {
        let scroll_len = hostname.len() + 3;
        let start_pos = scroll_offset % scroll_len;
        let extended_hostname = format!("{hostname}   {hostname}");
        extended_hostname
            .chars()
            .skip(start_pos)
            .take(9)
            .collect::<String>()
    } else {
        // Always return 9 characters, left-aligned with space padding
        format!("{hostname:<9}")
    }
}

pub fn print_gpu_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &GpuInfo,
    width: usize,
    device_name_scroll_offset: usize,
    hostname_scroll_offset: usize,
) {
    // Format device name with scrolling if needed
    let device_name = if info.name.len() > 15 {
        let scroll_len = info.name.len() + 3;
        let start_pos = device_name_scroll_offset % scroll_len;
        let extended_name = format!("{}   {}", info.name, info.name);
        let visible_name = extended_name
            .chars()
            .skip(start_pos)
            .take(15)
            .collect::<String>();
        visible_name
    } else {
        format!("{:<15}", info.name)
    };

    // Format hostname with scrolling if needed
    let hostname_display = format_hostname_with_scroll(&info.hostname, hostname_scroll_offset);

    // Calculate values
    let memory_gb = info.used_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_memory_gb = info.total_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let memory_percent = if info.total_memory > 0 {
        (info.used_memory as f64 / info.total_memory as f64) * 100.0
    } else {
        0.0
    };

    // Print info line: <device_type> <name> @ <hostname> Util:4.0% Mem:25.2/128GB Temp:0°C Pwr:0.0W
    print_colored_text(
        stdout,
        &format!("{:<5}", info.device_type),
        Color::Cyan,
        None,
        None,
    );
    print_colored_text(stdout, &device_name, Color::White, None, None);
    print_colored_text(stdout, " @ ", Color::DarkGreen, None, None);
    print_colored_text(stdout, &hostname_display, Color::White, None, None);
    print_colored_text(stdout, " Util:", Color::Yellow, None, None);
    let util_display = if info.utilization < 0.0 {
        format!("{:>6}", "N/A")
    } else {
        format!("{:>5.1}%", info.utilization)
    };
    print_colored_text(stdout, &util_display, Color::White, None, None);
    print_colored_text(stdout, " VRAM:", Color::Blue, None, None);
    let vram_display = if info.detail.get("metrics_available") == Some(&"false".to_string()) {
        format!("{:>11}", "N/A")
    } else {
        format!("{:>11}", format!("{memory_gb:.1}/{total_memory_gb:.0}GB"))
    };
    print_colored_text(stdout, &vram_display, Color::White, None, None);
    print_colored_text(stdout, " Temp:", Color::Magenta, None, None);

    // For Apple Silicon, display thermal pressure level instead of numeric temperature
    let temp_display = if info.name.contains("Apple") || info.name.contains("Metal") {
        if let Some(thermal_level) = info.detail.get("Thermal Pressure") {
            format!("{thermal_level:>7}")
        } else {
            format!("{:>7}", "Unknown")
        }
    } else if info.detail.get("metrics_available") == Some(&"false".to_string()) {
        format!("{:>7}", "N/A")
    } else {
        format!("{:>4}°C", info.temperature)
    };

    print_colored_text(stdout, &temp_display, Color::White, None, None);

    // Display GPU frequency
    if info.frequency > 0 {
        print_colored_text(stdout, " Freq:", Color::Magenta, None, None);
        if info.frequency >= 1000 {
            print_colored_text(
                stdout,
                &format!("{:.2}GHz", info.frequency as f64 / 1000.0),
                Color::White,
                None,
                None,
            );
        } else {
            print_colored_text(
                stdout,
                &format!("{}MHz", info.frequency),
                Color::White,
                None,
                None,
            );
        }
    }

    print_colored_text(stdout, " Pwr:", Color::Red, None, None);

    // Check if power_limit_max is available and display as current/max
    // For Apple Silicon, info.power_consumption contains GPU power only
    let is_apple_silicon = info.name.contains("Apple") || info.name.contains("Metal");
    let power_display = if info.power_consumption < 0.0 {
        "N/A".to_string()
    } else if is_apple_silicon {
        // Apple Silicon GPU uses very little power, show 2 decimal places
        // Use fixed width formatting to prevent trailing characters
        format!("{:5.2}W", info.power_consumption)
    } else if let Some(power_max_str) = info.detail.get("power_limit_max") {
        if let Ok(power_max) = power_max_str.parse::<f64>() {
            format!("{:.0}/{power_max:.0}W", info.power_consumption)
        } else {
            format!("{:.0}W", info.power_consumption)
        }
    } else {
        format!("{:.0}W", info.power_consumption)
    };

    // Dynamically adjust width based on content, with minimum of 8 chars
    let display_width = power_display.len().max(8);
    print_colored_text(
        stdout,
        &format!("{power_display:>display_width$}"),
        Color::White,
        None,
        None,
    );

    // Display CUDA version and Driver version if available
    if let Some(cuda_version) = info.detail.get("cuda_version") {
        print_colored_text(stdout, " CUDA:", Color::Green, None, None);
        print_colored_text(stdout, cuda_version, Color::White, None, None);
    }

    if let Some(driver_version) = info.detail.get("driver_version") {
        print_colored_text(stdout, " Driver:", Color::Green, None, None);
        print_colored_text(stdout, driver_version, Color::White, None, None);
    }

    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side and 2 space separation
    let available_width = width.saturating_sub(10); // 5 padding each side
    let is_apple_silicon = info.name.contains("Apple") || info.name.contains("Metal");
    let num_gauges = if is_apple_silicon { 3 } else { 2 }; // Util, Mem, (ANE for Apple Silicon only)
    let gauge_width = (available_width - (num_gauges - 1) * 2) / num_gauges; // 2 spaces between gauges

    // Calculate actual space used and dynamic right padding
    let total_gauge_width = gauge_width * num_gauges + (num_gauges - 1) * 2;
    let left_padding = 5;
    let right_padding = width - left_padding - total_gauge_width;

    // Print gauges on one line with proper spacing
    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

    // Util gauge
    draw_bar(
        stdout,
        "Util",
        info.utilization,
        100.0,
        gauge_width,
        Some(format!("{:.1}%", info.utilization)),
    );
    print_colored_text(stdout, "  ", Color::White, None, None); // 2 space separator

    // Memory gauge
    draw_bar(
        stdout,
        "Mem",
        memory_percent,
        100.0,
        gauge_width,
        Some(format!("{memory_gb:.1}GB")),
    );

    // ANE gauge only for Apple Silicon (in Watts)
    if is_apple_silicon {
        print_colored_text(stdout, "  ", Color::White, None, None); // 2 space separator

        // Determine max ANE power based on die count (Ultra = 2 dies = 12W, others = 6W)
        let is_ultra = info.name.contains("Ultra");
        let max_ane_power = if is_ultra { 12.0 } else { 6.0 };

        // Convert mW to W and cap at max
        let ane_power_w = (info.ane_utilization / 1000.0).min(max_ane_power);
        let ane_percent = (ane_power_w / max_ane_power) * 100.0;

        draw_bar(
            stdout,
            "ANE",
            ane_percent,
            100.0,
            gauge_width,
            Some(format!("{ane_power_w:.1}W")),
        );
    }

    print_colored_text(stdout, &" ".repeat(right_padding), Color::White, None, None); // dynamic right padding
    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_cpu_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &CpuInfo,
    width: usize,
    show_per_core: bool,
    cpu_name_scroll_offset: usize,
    hostname_scroll_offset: usize,
) {
    // Format CPU name with scrolling if needed (same as GPU: 15 chars)
    let cpu_name = if info.cpu_model.len() > 15 {
        let scroll_len = info.cpu_model.len() + 3;
        let start_pos = cpu_name_scroll_offset % scroll_len;
        let extended_name = format!("{0}   {0}", info.cpu_model);
        let visible_name = extended_name
            .chars()
            .skip(start_pos)
            .take(15)
            .collect::<String>();
        visible_name
    } else {
        format!("{:<15}", info.cpu_model)
    };

    // Format hostname with scrolling if needed (same as GPU: 9 chars)
    let hostname_display = format_hostname_with_scroll(&info.hostname, hostname_scroll_offset);

    // Print CPU info line
    print_colored_text(stdout, "CPU  ", Color::Cyan, None, None);
    print_colored_text(stdout, &cpu_name, Color::White, None, None);
    print_colored_text(stdout, " @ ", Color::DarkGreen, None, None);
    print_colored_text(stdout, &hostname_display, Color::White, None, None);
    print_colored_text(stdout, " Arch:", Color::Yellow, None, None);
    print_colored_text(stdout, &info.architecture, Color::White, None, None);
    print_colored_text(stdout, " Sockets:", Color::Yellow, None, None);
    print_colored_text(
        stdout,
        &format!("{:>2}", info.socket_count),
        Color::White,
        None,
        None,
    );
    // Show P-Core/E-Core counts for Apple Silicon, regular core count for others
    if let Some(apple_info) = &info.apple_silicon_info {
        print_colored_text(stdout, " Cores:", Color::Green, None, None);
        print_colored_text(
            stdout,
            &format!("{:>2}P+", apple_info.p_core_count),
            Color::White,
            None,
            None,
        );
        print_colored_text(
            stdout,
            &format!("{:>2}E", apple_info.e_core_count),
            Color::White,
            None,
            None,
        );
    } else {
        print_colored_text(stdout, " Cores:", Color::Green, None, None);
        print_colored_text(
            stdout,
            &format!("{:>2}", info.total_cores),
            Color::White,
            None,
            None,
        );
    }

    // Display frequency - P+E format for Apple Silicon, regular for others
    print_colored_text(stdout, " Freq:", Color::Magenta, None, None);
    if let Some(apple_info) = &info.apple_silicon_info {
        if let (Some(p_freq), Some(e_freq)) = (
            apple_info.p_cluster_frequency_mhz,
            apple_info.e_cluster_frequency_mhz,
        ) {
            // Format as P+E
            let freq_display = if p_freq >= 1000 && e_freq >= 1000 {
                format!(
                    "{:.2}+{:.2}GHz",
                    p_freq as f64 / 1000.0,
                    e_freq as f64 / 1000.0
                )
            } else if p_freq >= 1000 {
                format!("{:.2}GHz+{e_freq}MHz", p_freq as f64 / 1000.0)
            } else if e_freq >= 1000 {
                format!("{p_freq}MHz+{:.2}GHz", e_freq as f64 / 1000.0)
            } else {
                format!("{p_freq}+{e_freq}MHz")
            };
            print_colored_text(
                stdout,
                &format!("{freq_display:>13}"),
                Color::White,
                None,
                None,
            );
        } else {
            // Fallback to max frequency if cluster frequencies not available
            let freq_ghz = info.max_frequency_mhz as f64 / 1000.0;
            print_colored_text(
                stdout,
                &format!("{freq_ghz:>6.1}GHz"),
                Color::White,
                None,
                None,
            );
        }
    } else {
        // Regular frequency display for non-Apple Silicon
        let freq_ghz = info.max_frequency_mhz as f64 / 1000.0;
        print_colored_text(
            stdout,
            &format!("{freq_ghz:>6.1}GHz"),
            Color::White,
            None,
            None,
        );
    }
    // Display CPU temperature if available (not on macOS)
    if let Some(temp) = info.temperature {
        print_colored_text(stdout, " Temp:", Color::Magenta, None, None);
        print_colored_text(stdout, &format!("{temp:>3}°C"), Color::White, None, None);
    }

    // Display cache based on platform type
    if let Some(apple_info) = &info.apple_silicon_info {
        if let (Some(p_cache), Some(e_cache)) =
            (apple_info.p_core_l2_cache_mb, apple_info.e_core_l2_cache_mb)
        {
            // Apple Silicon: Display L2 cache as P+E format
            print_colored_text(stdout, " L2 Cache:", Color::Red, None, None);
            print_colored_text(
                stdout,
                &format!("{p_cache}MB+{e_cache}MB"),
                Color::White,
                None,
                None,
            );
        } else if info.cache_size_mb > 0 {
            // Fallback to total cache
            print_colored_text(stdout, " L2 Cache:", Color::Red, None, None);
            print_colored_text(
                stdout,
                &format!("{:>5}MB", info.cache_size_mb),
                Color::White,
                None,
                None,
            );
        }
    } else if info.cache_size_mb > 0 {
        // Non-Apple Silicon: display L3 cache (Intel Mac and Linux)
        print_colored_text(stdout, " L3 Cache:", Color::Red, None, None);
        print_colored_text(
            stdout,
            &format!("{:>5}MB", info.cache_size_mb),
            Color::White,
            None,
            None,
        );
    }

    // Display CPU power if available
    if let Some(power) = info.power_consumption {
        print_colored_text(stdout, " Pwr:", Color::Red, None, None);
        print_colored_text(stdout, &format!("{power:>4.0}W"), Color::White, None, None);
    }

    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side and 2 space separation
    let available_width = width.saturating_sub(10); // 5 padding each side

    if let Some(apple_info) = &info.apple_silicon_info {
        // Apple Silicon: Two gauges for P-Core and E-Core
        let num_gauges = 2;
        let gauge_width = (available_width - 2) / 2; // 2 spaces between gauges

        // Calculate actual space used and dynamic right padding
        let total_gauge_width = gauge_width * num_gauges + (num_gauges - 1) * 2;
        let left_padding = 5;
        let right_padding = width - left_padding - total_gauge_width;

        print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

        // P-Core gauge
        draw_bar(
            stdout,
            "P-CPU",
            apple_info.p_core_utilization,
            100.0,
            gauge_width,
            None,
        );
        print_colored_text(stdout, "  ", Color::White, None, None); // 2 space separator

        // E-Core gauge
        draw_bar(
            stdout,
            "E-CPU",
            apple_info.e_core_utilization,
            100.0,
            gauge_width,
            None,
        );

        print_colored_text(stdout, &" ".repeat(right_padding), Color::White, None, None);
    // dynamic right padding
    } else {
        // Other CPUs: Single CPU utilization gauge
        let gauge_width = available_width;

        // Calculate actual space used and dynamic right padding
        let total_gauge_width = gauge_width;
        let left_padding = 5;
        let right_padding = width - left_padding - total_gauge_width;

        print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

        // CPU gauge
        draw_bar(stdout, "CPU", info.utilization, 100.0, gauge_width, None);

        print_colored_text(stdout, &" ".repeat(right_padding), Color::White, None, None);
        // dynamic right padding
    }

    queue!(stdout, Print("\r\n")).unwrap();

    // Display per-core utilization if available and enabled
    if show_per_core && !info.per_core_utilization.is_empty() {
        // Add a note about which physical cores are being monitored in containers
        #[cfg(target_os = "linux")]
        if std::path::Path::new("/.dockerenv").exists()
            || std::path::Path::new("/proc/self/cgroup").exists()
        {
            // Check if we have cpuset information
            if let Ok(cpuset) = std::fs::read_to_string("/sys/fs/cgroup/cpuset.cpus")
                .or_else(|_| std::fs::read_to_string("/sys/fs/cgroup/cpuset.cpus.effective"))
                .or_else(|_| std::fs::read_to_string("/sys/fs/cgroup/cpuset/cpuset.cpus"))
            {
                let cpuset = cpuset.trim();
                if !cpuset.is_empty() && !cpuset.is_empty() {
                    print_colored_text(stdout, "     ", Color::White, None, None);
                    print_colored_text(stdout, "Container CPUs: ", Color::DarkGrey, None, None);
                    print_colored_text(stdout, cpuset, Color::DarkGrey, None, None);
                    queue!(stdout, Print("\r\n")).unwrap();
                }
            }
        }

        let total_cores = info.per_core_utilization.len();
        let cores_per_line = if total_cores <= 16 { 4 } else { 8 };

        // Group cores by type for simpler labeling
        let mut p_cores = Vec::new();
        let mut e_cores = Vec::new();
        let mut standard_cores = Vec::new();

        for core in &info.per_core_utilization {
            match core.core_type {
                crate::device::CoreType::Performance => p_cores.push(core),
                crate::device::CoreType::Efficiency => e_cores.push(core),
                crate::device::CoreType::Standard => standard_cores.push(core),
            }
        }

        // Calculate the width for each core bar
        let available_width = width.saturating_sub(10); // 5 padding each side
        let spacing_between_cores = 2;
        let core_bar_width =
            (available_width - (cores_per_line - 1) * spacing_between_cores) / cores_per_line;

        // Display P-cores
        let mut cores_displayed = 0;
        for (i, core) in p_cores.iter().enumerate() {
            if cores_displayed % cores_per_line == 0 && cores_displayed > 0 {
                queue!(stdout, Print("\r\n")).unwrap();
            }

            if cores_displayed % cores_per_line == 0 {
                print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding
            }

            let label = format!("P{}", i + 1);
            draw_bar(
                stdout,
                &label,
                core.utilization,
                100.0,
                core_bar_width,
                None,
            );

            cores_displayed += 1;
            if cores_displayed % cores_per_line != 0 && cores_displayed < total_cores {
                print_colored_text(stdout, "  ", Color::White, None, None); // spacing between cores
            }
        }

        // Display E-cores
        for (i, core) in e_cores.iter().enumerate() {
            if cores_displayed % cores_per_line == 0 && cores_displayed > 0 {
                queue!(stdout, Print("\r\n")).unwrap();
            }

            if cores_displayed % cores_per_line == 0 {
                print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding
            }

            let label = format!("E{}", i + 1);
            draw_bar(
                stdout,
                &label,
                core.utilization,
                100.0,
                core_bar_width,
                None,
            );

            cores_displayed += 1;
            if cores_displayed % cores_per_line != 0 && cores_displayed < total_cores {
                print_colored_text(stdout, "  ", Color::White, None, None); // spacing between cores
            }
        }

        // Display standard cores (for systems without P/E distinction)
        for (i, core) in standard_cores.iter().enumerate() {
            if cores_displayed % cores_per_line == 0 && cores_displayed > 0 {
                queue!(stdout, Print("\r\n")).unwrap();
            }

            if cores_displayed % cores_per_line == 0 {
                print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding
            }

            let label = format!("C{}", i + 1);
            draw_bar(
                stdout,
                &label,
                core.utilization,
                100.0,
                core_bar_width,
                None,
            );

            cores_displayed += 1;
            if cores_displayed % cores_per_line != 0 && cores_displayed < total_cores {
                print_colored_text(stdout, "  ", Color::White, None, None); // spacing between cores
            }
        }

        // Add right padding for the last line if needed
        if cores_displayed % cores_per_line != 0 {
            let remaining_cores = cores_per_line - (cores_displayed % cores_per_line);
            let remaining_width =
                remaining_cores * core_bar_width + (remaining_cores - 1) * spacing_between_cores;
            print_colored_text(
                stdout,
                &" ".repeat(remaining_width + spacing_between_cores),
                Color::White,
                None,
                None,
            );
        }

        // Add final right padding
        let total_line_width =
            cores_per_line * core_bar_width + (cores_per_line - 1) * spacing_between_cores;
        let right_padding = width - 5 - total_line_width;
        print_colored_text(stdout, &" ".repeat(right_padding), Color::White, None, None);

        queue!(stdout, Print("\r\n")).unwrap();
    }
}

pub fn print_memory_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &MemoryInfo,
    width: usize,
    hostname_scroll_offset: usize,
) {
    // Convert bytes to GB for display
    let total_gb = info.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = info.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = info.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    // Format hostname with scrolling if needed (same as GPU/CPU: 9 chars)
    let hostname_display = format_hostname_with_scroll(&info.hostname, hostname_scroll_offset);

    // Print Memory info line
    print_colored_text(stdout, "Host Memory         ", Color::Cyan, None, None);
    print_colored_text(stdout, " @ ", Color::DarkGreen, None, None);
    print_colored_text(stdout, &hostname_display, Color::White, None, None);
    print_colored_text(stdout, " Total:", Color::Green, None, None);
    print_colored_text(
        stdout,
        &format!("{total_gb:>6.0}GB"),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Used:", Color::Red, None, None);
    print_colored_text(
        stdout,
        &format!("{used_gb:>6.1}GB"),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Avail:", Color::Green, None, None);
    print_colored_text(
        stdout,
        &format!("{available_gb:>6.1}GB"),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Util:", Color::Magenta, None, None);
    print_colored_text(
        stdout,
        &format!("{:>5.1}%", info.utilization),
        Color::White,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side
    let available_width = width.saturating_sub(10); // 5 padding each side
    let gauge_width = available_width;

    // Calculate actual space used and dynamic right padding
    let total_gauge_width = gauge_width;
    let left_padding = 5;
    let right_padding = width - left_padding - total_gauge_width;

    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

    // Create segments for multi-bar display
    let mut segments = Vec::new();

    // Calculate memory values in bytes
    let actual_used_bytes = info
        .used_bytes
        .saturating_sub(info.buffers_bytes + info.cached_bytes);
    let actual_used_gb = actual_used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let buffers_gb = info.buffers_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let cached_gb = info.cached_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    // Add used memory segment (actual used without buffers/cache)
    if actual_used_bytes > 0 {
        segments.push(BarSegment::memory_used(actual_used_gb));
    }

    // Add buffers segment
    if info.buffers_bytes > 0 {
        segments.push(BarSegment::memory_buffers(buffers_gb));
    }

    // Add cache segment
    if info.cached_bytes > 0 {
        segments.push(BarSegment::memory_cache(cached_gb));
    }

    // Calculate total used memory for display text
    let total_used_gb = actual_used_gb + buffers_gb + cached_gb;
    let display_text = format!("{total_used_gb:.1}GB");

    // Draw the multi-segment bar
    draw_bar_multi(
        stdout,
        "Mem",
        &segments,
        total_gb,
        gauge_width,
        Some(display_text),
    );

    print_colored_text(stdout, &" ".repeat(right_padding), Color::White, None, None);
    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_storage_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &StorageInfo,
    width: usize,
    hostname_scroll_offset: usize,
) {
    // Convert bytes to appropriate units
    let total_gb = info.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = info.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = total_gb - available_gb;

    // Calculate usage percentage
    let usage_percent = if total_gb > 0.0 {
        (used_gb / total_gb) * 100.0
    } else {
        0.0
    };

    // Format size with appropriate units
    let format_size = |gb: f64| -> String {
        if gb >= 1024.0 {
            format!("{:.1}TB", gb / 1024.0)
        } else {
            format!("{gb:.0}GB")
        }
    };

    // Print Disk info line
    print_colored_text(stdout, "Disk ", Color::Cyan, None, None);
    print_colored_text(
        stdout,
        &format!("{:<15}", truncate_to_width(&info.mount_point, 15)),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " @ ", Color::DarkGreen, None, None);
    let hostname_display = format_hostname_with_scroll(&info.hostname, hostname_scroll_offset);
    print_colored_text(stdout, &hostname_display, Color::White, None, None);
    print_colored_text(stdout, " Total:", Color::Green, None, None);
    print_colored_text(
        stdout,
        &format!("{:>8}", format_size(total_gb)),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Used:", Color::Red, None, None);
    print_colored_text(
        stdout,
        &format!("{:>8}", format_size(used_gb)),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Util:", Color::Magenta, None, None);
    print_colored_text(
        stdout,
        &format!("{usage_percent:>5.1}%"),
        Color::White,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side
    let available_width = width.saturating_sub(10); // 5 padding each side

    let gauge_width = available_width;

    // Calculate actual space used and dynamic right padding
    let total_gauge_width = gauge_width;
    let left_padding = 5;
    let right_padding = width - left_padding - total_gauge_width;

    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

    // Just Used gauge (matching the other lists format)
    draw_bar(
        stdout,
        "Used",
        usage_percent,
        100.0,
        gauge_width,
        Some(format_size(used_gb)),
    );

    print_colored_text(stdout, &" ".repeat(right_padding), Color::White, None, None); // dynamic right padding
    queue!(stdout, Print("\r\n")).unwrap();
}
