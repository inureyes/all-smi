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

use crate::device::MemoryInfo;
use crate::ui::text::print_colored_text;
use crate::ui::widgets::{draw_bar_multi, BarSegment};

/// Memory renderer struct implementing the DeviceRenderer trait
#[allow(dead_code)]
pub struct MemoryRenderer;

#[allow(dead_code)]
impl MemoryRenderer {
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

/// Render memory information including total, used, available, and utilization
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
