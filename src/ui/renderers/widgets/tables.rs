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

use crossterm::style::Color;

use crate::ui::text::print_colored_text;

/// A key-value pair for table rendering
#[allow(dead_code)]
pub struct TableRow {
    pub label: String,
    pub value: String,
    pub label_color: Color,
    pub value_color: Color,
}

#[allow(dead_code)]
impl TableRow {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            label_color: Color::Yellow,
            value_color: Color::White,
        }
    }

    pub fn with_colors(mut self, label_color: Color, value_color: Color) -> Self {
        self.label_color = label_color;
        self.value_color = value_color;
        self
    }
}

/// Render a simple info table with key-value pairs
#[allow(dead_code)]
pub fn render_info_table<W: Write>(stdout: &mut W, rows: &[TableRow]) {
    for row in rows {
        print_colored_text(stdout, &row.label, row.label_color, None, None);
        print_colored_text(stdout, &row.value, row.value_color, None, None);
    }
}

/// Render a bordered box with title
#[allow(dead_code)]
pub fn render_bordered_box<W: Write>(stdout: &mut W, title: &str, width: usize, color: Color) {
    // Draw top border
    let title_with_spaces_len = 1 + title.len() + 1; // " " + title + " "

    print_colored_text(stdout, "╭─", color, None, None);
    print_colored_text(stdout, " ", Color::White, None, None);
    print_colored_text(stdout, title, color, None, None);
    print_colored_text(stdout, " ", Color::White, None, None);

    // Fill the rest with dashes
    let remaining_dashes = width.saturating_sub(title_with_spaces_len + 1);
    for _ in 0..remaining_dashes {
        print_colored_text(stdout, "─", color, None, None);
    }
    print_colored_text(stdout, "╮", color, None, None);
}

/// Close a bordered box
#[allow(dead_code)]
pub fn close_bordered_box<W: Write>(stdout: &mut W, width: usize, color: Color) {
    print_colored_text(stdout, "╰", color, None, None);
    for _ in 0..width {
        print_colored_text(stdout, "─", color, None, None);
    }
    print_colored_text(stdout, "╯", color, None, None);
}

/// Table style constants
#[allow(dead_code)]
pub const TABLE_LABEL_COLOR: Color = Color::Yellow;
#[allow(dead_code)]
pub const TABLE_VALUE_COLOR: Color = Color::White;
#[allow(dead_code)]
pub const TABLE_BORDER_COLOR: Color = Color::Cyan;
