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

// Re-export the draw_bar function from the main widgets module
pub use crate::ui::widgets::draw_bar;

/// Get utilization block character and color based on usage percentage
pub fn get_utilization_block(utilization: f64) -> (&'static str, Color) {
    match utilization {
        u if u >= 90.0 => ("█", Color::Red), // Full block, red for high usage
        u if u >= 80.0 => ("▇", Color::Magenta), // 7/8 block
        u if u >= 70.0 => ("▆", Color::Yellow), // 6/8 block
        u if u >= 60.0 => ("▅", Color::Yellow), // 5/8 block
        u if u >= 50.0 => ("▄", Color::Green), // 4/8 block
        u if u >= 40.0 => ("▃", Color::Green), // 3/8 block
        u if u >= 30.0 => ("▂", Color::Cyan), // 2/8 block
        u if u >= 20.0 => ("▁", Color::Cyan), // 1/8 block
        u if u >= 10.0 => ("▁", Color::Blue), // Low usage
        _ => ("▁", Color::DarkGrey),         // Minimal or no usage (still show lowest bar)
    }
}

/// Helper function to render a simple gauge bar
#[allow(dead_code)]
pub fn render_gauge<W: Write>(
    stdout: &mut W,
    label: &str,
    value: f64,
    max_value: f64,
    width: usize,
    _label_color: Color,
    show_text: Option<String>,
) {
    draw_bar(stdout, label, value, max_value, width, show_text);
}

/// Gauge style constants
#[allow(dead_code)]
pub const GAUGE_HIGH_COLOR: Color = Color::Red;
#[allow(dead_code)]
pub const GAUGE_MEDIUM_COLOR: Color = Color::Yellow;
#[allow(dead_code)]
pub const GAUGE_LOW_COLOR: Color = Color::Green;
#[allow(dead_code)]
pub const GAUGE_MINIMAL_COLOR: Color = Color::Blue;
