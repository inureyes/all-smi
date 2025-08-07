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

use crate::device::ProcessInfo;
use crate::ui::text::{print_colored_text, truncate_to_width};

#[allow(clippy::too_many_arguments)]
pub fn print_process_info<W: Write>(
    stdout: &mut W,
    processes: &[ProcessInfo],
    selected_index: usize,
    start_index: usize,
    half_rows: u16,
    cols: u16,
    horizontal_scroll_offset: usize,
    current_user: &str,
    sort_criteria: &crate::app_state::SortCriteria,
    sort_direction: &crate::app_state::SortDirection,
) {
    queue!(stdout, Print("\r\nProcesses:\r\n")).unwrap();

    let width = cols as usize;

    // Fixed column widths based on actual data sizes
    // PID: 7 (up to 9999999), USER: 12, PRI: 3, NI: 3, VIRT: 6, RES: 6, S: 1,
    // CPU%: 5, MEM%: 5, GPU%: 5, VRAM: 7, TIME+: 8, Command: remaining
    let fixed_widths = [7, 12, 3, 3, 6, 6, 1, 5, 5, 5, 7, 8];
    let num_gaps = fixed_widths.len(); // Gaps between columns (not after last column)
    let fixed_total: usize = fixed_widths.iter().sum::<usize>() + num_gaps;

    // Give remaining space to command column, ensure at least 20 chars
    let _command_w = if width > fixed_total + 20 {
        width - fixed_total
    } else {
        20
    };

    let (pid_w, user_w, pri_w, ni_w, virt_w, res_w, s_w, cpu_w, mem_w, gpu_w, gpu_mem_w, time_w) = (
        fixed_widths[0],  // PID: 7
        fixed_widths[1],  // USER: 12
        fixed_widths[2],  // PRI: 3
        fixed_widths[3],  // NI: 3
        fixed_widths[4],  // VIRT: 6
        fixed_widths[5],  // RES: 6
        fixed_widths[6],  // S: 1
        fixed_widths[7],  // CPU%: 5
        fixed_widths[8],  // MEM%: 5
        fixed_widths[9],  // GPU%: 5
        fixed_widths[10], // VRAM: 7
        fixed_widths[11], // TIME+: 8
    );

    // Helper function to add sort arrow
    let get_sort_arrow = |criteria: crate::app_state::SortCriteria| -> &'static str {
        if sort_criteria == &criteria {
            match sort_direction {
                crate::app_state::SortDirection::Ascending => "↑",
                crate::app_state::SortDirection::Descending => "↓",
            }
        } else {
            ""
        }
    };

    // Build header format string with proper alignment and sort arrows
    #[allow(clippy::format_in_format_args)]
    let header_format = format!(
        "{:>pid_w$} {:<user_w$} {:>pri_w$} {:>ni_w$} {:>virt_w$} {:>res_w$} {:<s_w$} {:>cpu_w$} {:>mem_w$} {:>gpu_w$} {:>gpu_mem_w$} {:>time_w$} {}",
        format!("PID{}", get_sort_arrow(crate::app_state::SortCriteria::Pid)),
        format!("USER{}", get_sort_arrow(crate::app_state::SortCriteria::User)),
        format!("PRI{}", get_sort_arrow(crate::app_state::SortCriteria::Priority)),
        format!("NI{}", get_sort_arrow(crate::app_state::SortCriteria::Nice)),
        format!("VIRT{}", get_sort_arrow(crate::app_state::SortCriteria::VirtualMemory)),
        format!("RES{}", get_sort_arrow(crate::app_state::SortCriteria::ResidentMemory)),
        format!("S{}", get_sort_arrow(crate::app_state::SortCriteria::State)),
        format!("CPU%{}", get_sort_arrow(crate::app_state::SortCriteria::CpuPercent)),
        format!("MEM%{}", get_sort_arrow(crate::app_state::SortCriteria::MemoryPercent)),
        format!("GPU%{}", get_sort_arrow(crate::app_state::SortCriteria::GpuPercent)),
        format!("VRAM{}", get_sort_arrow(crate::app_state::SortCriteria::GpuMemoryUsage)),
        format!("TIME+{}", get_sort_arrow(crate::app_state::SortCriteria::CpuTime)),
        format!("Command{}", get_sort_arrow(crate::app_state::SortCriteria::Command)),
        pid_w = pid_w,
        user_w = user_w,
        pri_w = pri_w,
        ni_w = ni_w,
        virt_w = virt_w,
        res_w = res_w,
        s_w = s_w,
        cpu_w = cpu_w,
        mem_w = mem_w,
        gpu_w = gpu_w,
        gpu_mem_w = gpu_mem_w,
        time_w = time_w,
    );

    // Apply horizontal scrolling
    let visible_header = if horizontal_scroll_offset < header_format.len() {
        let scrolled = &header_format[horizontal_scroll_offset..];
        // Pad the header to full width to clear any previous content
        format!(
            "{:<width$}",
            truncate_to_width(scrolled, width),
            width = width
        )
    } else {
        // Clear the entire line when scrolled past the content
        " ".repeat(width)
    };

    print_colored_text(stdout, &visible_header, Color::White, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Print separator line
    let separator = "─".repeat(width.min(120));
    print_colored_text(stdout, &separator, Color::DarkGrey, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate how many processes we can display
    let available_rows = half_rows.saturating_sub(3) as usize; // Reserve 3 rows for header and separator
    let end_index = (start_index + available_rows).min(processes.len());

    // Print process information
    for i in start_index..end_index {
        if let Some(process) = processes.get(i) {
            let is_selected = i == selected_index;

            // Format process information with proper truncation
            let pid = format!("{}", process.pid);
            let user = truncate_to_width(&process.user, user_w);
            let priority = format!("{}", process.priority);
            let nice = format!("{:+}", process.nice_value); // Show + for positive values

            // Format memory sizes
            let virt = format_memory_size(process.memory_vms);
            let res = format_memory_size(process.memory_rss);

            let state = truncate_to_width(&process.state, s_w);
            let cpu_percent = format!("{:.1}", process.cpu_percent);
            let mem_percent = format!("{:.1}", process.memory_percent);

            // Format GPU utilization
            let gpu_percent = if process.uses_gpu && process.gpu_utilization > 0.0 {
                format!("{:.1}", process.gpu_utilization)
            } else if process.uses_gpu {
                "-".to_string()
            } else {
                "".to_string()
            };

            // Format GPU memory usage
            let gpu_mem = if process.used_memory > 0 {
                let gpu_mem_mb = process.used_memory as f64 / (1024.0 * 1024.0);
                if gpu_mem_mb >= 1024.0 {
                    format!("{:.1}G", gpu_mem_mb / 1024.0)
                } else {
                    format!("{gpu_mem_mb:.0}M")
                }
            } else if process.uses_gpu {
                "-".to_string()
            } else {
                "".to_string()
            };

            // Format CPU time
            let time_plus = format_cpu_time(process.cpu_time);

            let command = process.command.clone();

            // Build the row with proper formatting and padding
            let row_format = format!(
                "{:>pid_w$} {:<user_w$} {:>pri_w$} {:>ni_w$} {:>virt_w$} {:>res_w$} {:<s_w$} {:>cpu_w$} {:>mem_w$} {:>gpu_w$} {:>gpu_mem_w$} {:>time_w$} {}",
                pid,
                truncate_to_width(&user, user_w),
                priority,
                nice,
                virt,
                res,
                state,
                cpu_percent,
                mem_percent,
                gpu_percent,
                gpu_mem,
                time_plus,
                command,
                pid_w = pid_w,
                user_w = user_w,
                pri_w = pri_w,
                ni_w = ni_w,
                virt_w = virt_w,
                res_w = res_w,
                s_w = s_w,
                cpu_w = cpu_w,
                mem_w = mem_w,
                gpu_w = gpu_w,
                gpu_mem_w = gpu_mem_w,
                time_w = time_w,
            );

            // Apply horizontal scrolling
            let visible_row = if horizontal_scroll_offset < row_format.len() {
                let scrolled = &row_format[horizontal_scroll_offset..];
                // Pad the row to full width to clear any previous content
                format!(
                    "{:<width$}",
                    truncate_to_width(scrolled, width),
                    width = width
                )
            } else {
                // Clear the entire line when scrolled past the content
                " ".repeat(width)
            };

            // Print with selection highlight or individual column colors
            if is_selected {
                print_colored_text(stdout, &visible_row, Color::Black, Some(Color::White), None);
            } else {
                // We need to print each column separately with its own color
                // So we'll reconstruct the visible parts column by column
                print_process_row_colored(
                    stdout,
                    process,
                    current_user,
                    &pid,
                    &user,
                    &priority,
                    &nice,
                    &virt,
                    &res,
                    &state,
                    &cpu_percent,
                    &mem_percent,
                    &gpu_percent,
                    &gpu_mem,
                    &time_plus,
                    &command,
                    horizontal_scroll_offset,
                    width,
                    &fixed_widths,
                );
            }

            queue!(stdout, Print("\r\n")).unwrap();
        }
    }

    // Show navigation info if there are more processes
    if processes.len() > available_rows {
        let nav_info = format!(
            "Showing {}-{} of {} processes (Use ↑↓ to navigate, PgUp/PgDn for pages)",
            start_index + 1,
            end_index,
            processes.len()
        );
        // Pad the line to full width to clear any previous content
        let padded_nav_info = format!("{nav_info:<width$}");
        print_colored_text(stdout, &padded_nav_info, Color::DarkGrey, None, None);
        queue!(stdout, Print("\r\n")).unwrap();
    }

    // Show process statistics
    if !processes.is_empty() {
        let total_gpu_mem: u64 = processes.iter().map(|p| p.used_memory).sum();
        let gpu_mem_gb = total_gpu_mem as f64 / (1024.0 * 1024.0 * 1024.0);

        let active_processes = processes.iter().filter(|p| p.cpu_percent > 0.1).count();
        let gpu_processes = processes.iter().filter(|p| p.used_memory > 0).count();

        let stats = format!(
            "Active: {active_processes} | GPU: {gpu_processes} | Total GPU Memory: {gpu_mem_gb:.1}GB"
        );
        // Pad the line to full width to clear any previous content
        let padded_stats = format!("{stats:<width$}");
        print_colored_text(stdout, &padded_stats, Color::Cyan, None, None);
        queue!(stdout, Print("\r\n")).unwrap();
    }

    // Clear any remaining lines from the process section to prevent artifacts
    // This handles the case where processes are removed and old lines remain
    let mut lines_used = 3; // header (2) + separator (1)
    lines_used += end_index.saturating_sub(start_index); // actual process lines
    if processes.len() > available_rows {
        lines_used += 1; // navigation info line
    }
    if !processes.is_empty() {
        lines_used += 1; // stats line
    }

    // Clear any remaining lines up to the full half_rows allocation
    while lines_used < half_rows as usize {
        let clear_line = " ".repeat(width);
        queue!(stdout, Print(&clear_line)).unwrap();
        queue!(stdout, Print("\r\n")).unwrap();
        lines_used += 1;
    }
}

/// Format memory size in human-readable format (e.g., 187T, 123G, 500M, 16K)
fn format_memory_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }

    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let mb = bytes as f64 / (1024.0 * 1024.0);
    let kb = bytes as f64 / 1024.0;

    if gb >= 1000.0 {
        // Only show TB if >= 1000GB
        let tb = gb / 1024.0;
        format!("{tb:.0}T")
    } else if gb >= 1.0 {
        format!("{gb:.0}G")
    } else if mb >= 1.0 {
        format!("{mb:.0}M")
    } else if kb >= 1.0 {
        format!("{kb:.0}K")
    } else {
        format!("{bytes}")
    }
}

/// Print process row with individual column colors
#[allow(clippy::too_many_arguments)]
fn print_process_row_colored<W: Write>(
    stdout: &mut W,
    process: &ProcessInfo,
    current_user: &str,
    pid: &str,
    user: &str,
    priority: &str,
    nice: &str,
    virt: &str,
    res: &str,
    state: &str,
    cpu_percent: &str,
    mem_percent: &str,
    gpu_percent: &str,
    gpu_mem: &str,
    time_plus: &str,
    command: &str,
    horizontal_scroll_offset: usize,
    width: usize,
    fixed_widths: &[usize; 12],
) {
    let values = vec![
        pid,
        user,
        priority,
        nice,
        virt,
        res,
        state,
        cpu_percent,
        mem_percent,
        gpu_percent,
        gpu_mem,
        time_plus,
        command,
    ];

    let mut current_pos = 0;
    let mut output_pos = 0;

    // Determine base colors
    let is_current_user = process.user == current_user;

    // Determine the default text color based on user and resource usage
    let default_color = if process.cpu_percent >= 90.0 || process.memory_percent >= 90.0 {
        Color::Red
    } else if process.cpu_percent >= 80.0 || process.memory_percent >= 80.0 {
        Color::Rgb {
            r: 255,
            g: 100,
            b: 100,
        }
    } else if process.cpu_percent >= 70.0 || process.memory_percent >= 70.0 {
        Color::Yellow
    } else if process.cpu_percent >= 50.0 || process.memory_percent >= 50.0 {
        Color::Rgb {
            r: 255,
            g: 200,
            b: 0,
        }
    } else if process.uses_gpu && (process.cpu_percent >= 30.0 || process.memory_percent >= 30.0) {
        Color::Cyan
    } else if process.uses_gpu {
        Color::Green
    } else if is_current_user {
        Color::White
    } else {
        // Root, unknown, or other users' processes
        Color::DarkGrey
    };

    for (idx, value) in values.iter().enumerate() {
        let col_width = if idx < fixed_widths.len() {
            fixed_widths[idx]
        } else {
            // Command column takes remaining space
            width
                .saturating_sub(current_pos)
                .saturating_sub(horizontal_scroll_offset)
        };

        // Check if this column is visible after scrolling
        let col_start = current_pos;
        let col_end = if idx < fixed_widths.len() {
            current_pos + col_width + 1 // +1 for space
        } else {
            current_pos + value.len() // Command doesn't have fixed width
        };

        if col_end > horizontal_scroll_offset && output_pos < width {
            // Determine color for this column
            let color = match idx {
                4 => {
                    // VIRT column
                    if process.memory_vms == 0 {
                        Color::White
                    } else {
                        Color::Green
                    }
                }
                0 => {
                    // PID - white if non-zero
                    if process.pid > 0 {
                        Color::White
                    } else {
                        default_color
                    }
                }
                2 => {
                    // Priority - white if not default (20)
                    if process.priority != 20 {
                        Color::White
                    } else {
                        default_color
                    }
                }
                3 => {
                    // Nice - white if not 0
                    if process.nice_value != 0 {
                        Color::White
                    } else {
                        Color::DarkGrey
                    }
                }
                5 => {
                    // RES - white if non-zero
                    if process.memory_rss > 0 {
                        Color::White
                    } else {
                        default_color
                    }
                }
                7 => {
                    // CPU% - white if non-zero
                    if process.cpu_percent > 0.0 {
                        Color::White
                    } else {
                        default_color
                    }
                }
                8 => {
                    // MEM% - white if non-zero
                    if process.memory_percent > 0.0 {
                        Color::White
                    } else {
                        default_color
                    }
                }
                9 => {
                    // GPU% - white if non-zero
                    if process.gpu_utilization > 0.0 {
                        Color::White
                    } else {
                        default_color
                    }
                }
                10 => {
                    // GPUMEM - white if non-zero
                    if process.used_memory > 0 {
                        Color::White
                    } else {
                        default_color
                    }
                }
                11 => {
                    // TIME+ - white if not 0:00:00
                    if time_plus != "0:00:00" {
                        Color::White
                    } else {
                        default_color
                    }
                }
                _ => default_color, // USER, State, Command use default color
            };

            // Calculate what part of this column to display
            let skip = horizontal_scroll_offset.saturating_sub(col_start);

            // Format the value with proper alignment
            let formatted = if idx < fixed_widths.len() {
                match idx {
                    0 => format!("{value:>col_width$}"), // PID - right align
                    1 => format!(
                        "{:<width$}",
                        truncate_to_width(value, col_width),
                        width = col_width
                    ), // USER - left align
                    2..=11 => format!("{value:>col_width$}"), // Numbers - right align
                    _ => value.to_string(),
                }
            } else {
                value.to_string() // Command - no padding
            };

            // Print the visible part
            if skip < formatted.len() {
                let visible_part = &formatted[skip..];
                let remaining_width = width.saturating_sub(output_pos);
                let to_print = truncate_to_width(visible_part, remaining_width);
                print_colored_text(stdout, &to_print, color, None, None);
                output_pos += to_print.len();
            }

            // Add space between columns (except after last column)
            if idx < values.len() - 1 && output_pos < width && col_end > horizontal_scroll_offset {
                print_colored_text(stdout, " ", default_color, None, None);
                output_pos += 1;
            }
        }

        current_pos = col_end;
    }

    // Fill the rest of the line with spaces to clear any previous content
    if output_pos < width {
        print_colored_text(
            stdout,
            &" ".repeat(width - output_pos),
            Color::Black,
            None,
            None,
        );
    }
}

/// Format CPU time in TIME+ format (e.g., 12:34.56, 1:23:45)
/// For extremely long-running basic system processes, show as 0:00:00
fn format_cpu_time(seconds: u64) -> String {
    if seconds == 0 {
        return "0:00:00".to_string();
    }

    // If the process has been running for more than 365 days (basic system process)
    // show as 0:00:00 to avoid clutter
    if seconds > 365 * 24 * 3600 {
        return "0:00:00".to_string();
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{}:{:02}:{:02}", minutes / 60, minutes % 60, secs)
    }
}
