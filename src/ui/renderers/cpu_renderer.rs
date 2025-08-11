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

use crate::device::{CoreUtilization, CpuInfo};
use crate::ui::text::print_colored_text;
use crate::ui::widgets::draw_bar;

use super::widgets::gauges::get_utilization_block;

/// CPU renderer struct implementing the DeviceRenderer trait
#[allow(dead_code)]
pub struct CpuRenderer;

#[allow(dead_code)]
impl CpuRenderer {
    pub fn new() -> Self {
        Self
    }
}

/// Helper function to format hostname with scrolling
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

/// Render fancy CPU visualization with utilization
fn render_cpu_visualization<W: Write>(
    stdout: &mut W,
    per_core_utilization: &[CoreUtilization],
    cpuset: Option<&str>,
    width: usize,
    is_container: bool,
) {
    if per_core_utilization.is_empty() {
        return;
    }

    let total_cpus = per_core_utilization.len();

    // Use full width minus padding (5 chars on each side)
    let box_width = width.saturating_sub(10);

    // Create a visual representation
    print_colored_text(stdout, "     ", Color::White, None, None);

    // Draw top border
    let title = if is_container {
        "Container CPUs"
    } else {
        "CPU Cores"
    };
    // Calculate the exact length: "╭─" + " " + title + " " + dashes + "╮"
    // We want total length to be box_width + 2 (for the corners)
    let title_with_spaces_len = 1 + title.len() + 1; // " " + title + " "

    print_colored_text(stdout, "╭─", Color::Cyan, None, None);
    print_colored_text(stdout, " ", Color::White, None, None);
    print_colored_text(stdout, title, Color::Cyan, None, None);
    print_colored_text(stdout, " ", Color::White, None, None);

    // Fill the rest with dashes, accounting for the closing corner
    let remaining_dashes = box_width.saturating_sub(title_with_spaces_len + 1); // +1 for "─" after "╭"
    for _ in 0..remaining_dashes {
        print_colored_text(stdout, "─", Color::Cyan, None, None);
    }
    print_colored_text(stdout, "╮", Color::Cyan, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Draw CPU visualization line
    print_colored_text(stdout, "     ", Color::White, None, None);
    print_colored_text(stdout, "│ ", Color::Cyan, None, None);

    // Calculate actual content for proper padding
    // For containers with cpuset, we show all monitored cores since they're already filtered
    // The per_core_utilization only contains the cores visible to the container
    let cores_to_show: Vec<&CoreUtilization> = per_core_utilization.iter().collect();

    let display_count = cores_to_show.len();

    // Determine grouping based on number of cores
    let group_size = if display_count <= 32 {
        4
    } else if display_count <= 64 {
        8
    } else {
        16
    };

    let content_str = {
        let mut content = String::new();
        let mut idx = 0;
        for core in &cores_to_show {
            // Get utilization block - just the character for length calculation
            let (block, _) = get_utilization_block(core.utilization);
            content.push_str(block);

            // Add grouping spaces for readability
            idx += 1;
            if (idx % group_size == 0) && (idx < display_count) {
                content.push(' ');
            }
        }

        // Add summary
        if is_container && cpuset.is_some() {
            let summary = format!("  ({display_count} allocated)");
            content.push_str(&summary);
        } else {
            // For bare metal or container without cpuset info, show average utilization
            let avg_util = per_core_utilization
                .iter()
                .map(|c| c.utilization)
                .sum::<f64>()
                / total_cpus as f64;
            let summary = format!("  ({total_cpus} cores, {avg_util:.1}% avg)");
            content.push_str(&summary);
        }
        content
    };

    // Print the actual content with colors
    let mut idx = 0;
    for core in &cores_to_show {
        // Show utilization block with color
        let (block, color) = get_utilization_block(core.utilization);
        print_colored_text(stdout, block, color, None, None);

        // Add grouping spaces for readability
        idx += 1;
        if (idx % group_size == 0) && (idx < display_count) {
            print_colored_text(stdout, " ", Color::White, None, None);
        }
    }

    // Add summary
    if is_container && cpuset.is_some() {
        let summary = format!("  ({display_count} allocated)");
        print_colored_text(stdout, &summary, Color::Yellow, None, None);
    } else {
        let avg_util = per_core_utilization
            .iter()
            .map(|c| c.utilization)
            .sum::<f64>()
            / total_cpus as f64;
        let summary = format!("  ({total_cpus} cores, {avg_util:.1}% avg)");
        print_colored_text(stdout, &summary, Color::Yellow, None, None);
    }

    // Add padding to align with box width
    // Account for the "│ " at the start (2 chars) and " │" at the end (2 chars)
    let content_display_len = content_str.chars().count();
    let inner_width = box_width.saturating_sub(2); // Subtract 2 for "│ " and " │"
    let padding_needed = inner_width.saturating_sub(content_display_len);

    for _ in 0..padding_needed {
        print_colored_text(stdout, " ", Color::White, None, None);
    }

    print_colored_text(stdout, " │", Color::Cyan, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Draw bottom border
    print_colored_text(stdout, "     ", Color::White, None, None);
    print_colored_text(stdout, "╰", Color::Cyan, None, None);
    for _ in 0..box_width {
        print_colored_text(stdout, "─", Color::Cyan, None, None);
    }
    print_colored_text(stdout, "╯", Color::Cyan, None, None);
    queue!(stdout, Print("\r\n")).unwrap();
}

/// Render CPU information including model, cores, frequency, and utilization
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
        // Show CPU visualization for both container and bare metal
        #[cfg(target_os = "linux")]
        {
            let is_container = std::path::Path::new("/.dockerenv").exists()
                || std::path::Path::new("/proc/self/cgroup").exists();

            // Check if we have cpuset information for containers
            let cpuset = if is_container {
                // Try cgroup v2 first, then cgroup v1
                std::fs::read_to_string("/sys/fs/cgroup/cpuset.cpus.effective")
                    .or_else(|_| std::fs::read_to_string("/sys/fs/cgroup/cpuset.cpus"))
                    .or_else(|_| std::fs::read_to_string("/sys/fs/cgroup/cpuset/cpuset.cpus"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            };

            // Render CPU visualization with utilization
            render_cpu_visualization(
                stdout,
                &info.per_core_utilization,
                cpuset.as_deref(),
                width,
                is_container,
            );
        }

        // For non-Linux systems (macOS, etc), show CPU visualization as bare metal
        #[cfg(not(target_os = "linux"))]
        {
            render_cpu_visualization(stdout, &info.per_core_utilization, None, width, false);
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

        // Display E-cores first (matches natural order from powermetrics)
        let mut cores_displayed = 0;
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

        // Display P-cores after E-cores
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
