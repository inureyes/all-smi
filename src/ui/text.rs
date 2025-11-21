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

use crossterm::{
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

// Helper function to get display width of a single character
pub fn char_display_width(c: char) -> usize {
    match c {
        // Arrow characters that display as 1 character width
        '←' | '→' | '↑' | '↓' => 1,
        // Most other characters display as their char count
        _ => 1,
    }
}

// Helper function to calculate display width of a string, accounting for Unicode characters
pub fn display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

// Helper function to truncate a string to fit within a given display width
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut result = String::new();
    let mut current_width = 0;

    for c in s.chars() {
        let char_width = char_display_width(c);
        if current_width + char_width <= max_width {
            result.push(c);
            current_width += char_width;
        } else {
            break;
        }
    }

    result
}

// Helper function to format RAM values with appropriate units
pub fn format_ram_value(gb_value: f64) -> String {
    if gb_value >= 1024.0 {
        format!("{:.2}TB", gb_value / 1024.0)
    } else if gb_value < 1.0 {
        // For sub-GB values (like 512MB = 0.5GB), show with 1 decimal place
        format!("{gb_value:.1}GB")
    } else {
        format!("{gb_value:.0}GB")
    }
}

pub fn print_colored_text<W: Write>(
    stdout: &mut W,
    text: &str,
    fg_color: Color,
    bg_color: Option<Color>,
    width: Option<usize>,
) {
    let adjusted_text = if let Some(w) = width {
        if text.len() > w {
            text.chars().take(w).collect::<String>()
        } else {
            format!("{text:<w$}")
        }
    } else {
        text.to_string()
    };

    if let Some(bg) = bg_color {
        queue!(
            stdout,
            SetForegroundColor(fg_color),
            SetBackgroundColor(bg),
            Print(adjusted_text),
            ResetColor
        )
        .unwrap();
    } else {
        queue!(
            stdout,
            SetForegroundColor(fg_color),
            Print(adjusted_text),
            ResetColor
        )
        .unwrap();
    }
}
