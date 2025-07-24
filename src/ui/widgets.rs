use std::io::Write;

use crossterm::style::Color;

use crate::common::config::ThemeConfig;
use crate::ui::text::print_colored_text;

pub struct BarSegment {
    pub value: f64,
    pub color: Color,
    pub label: Option<String>,
}

impl BarSegment {
    pub fn new(value: f64, color: Color) -> Self {
        Self {
            value,
            color,
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

pub fn draw_bar<W: Write>(
    stdout: &mut W,
    label: &str,
    value: f64,
    max_value: f64,
    width: usize,
    show_text: Option<String>,
) {
    // Format label to exactly 5 characters for consistent alignment
    let formatted_label = if label.len() > 5 {
        // Trim to 5 characters if too long
        label[..5].to_string()
    } else {
        // Pad with spaces if too short
        format!("{label:<5}")
    };
    let available_bar_width = width.saturating_sub(9); // 9 for "LABEL: [" and "] " (5 + 4)

    // Calculate the filled portion
    let fill_ratio = (value / max_value).min(1.0);
    let filled_width = (available_bar_width as f64 * fill_ratio) as usize;

    // Choose color based on usage using ThemeConfig
    let color = ThemeConfig::progress_bar_color(fill_ratio);

    // Prepare text to display inside the bar with fixed width
    let display_text = if let Some(text) = show_text {
        // Ensure consistent width for value text (8 characters)
        if text.len() > 8 {
            text[..8].to_string()
        } else {
            format!("{text:>8}") // Right-align in 8-character field
        }
    } else {
        format!("{:>7.1}%", fill_ratio * 100.0) // Right-align percentage in 8-character field
    };

    // Print label
    print_colored_text(stdout, &formatted_label, Color::White, None, None);
    print_colored_text(stdout, ": [", Color::White, None, None);

    // Calculate positioning for right-aligned text
    let text_len = display_text.len();
    let text_pos = available_bar_width.saturating_sub(text_len);

    // Print the bar with embedded text using filled vertical lines
    for i in 0..available_bar_width {
        if i >= text_pos && i < text_pos + text_len {
            // Print text character
            let char_index = i - text_pos;
            if let Some(ch) = display_text.chars().nth(char_index) {
                // Always use white for text to ensure readability
                print_colored_text(stdout, &ch.to_string(), Color::White, None, None);
            }
        } else if i < filled_width {
            // Print filled area with shorter vertical lines in load color
            print_colored_text(stdout, "▬", color, None, None);
        } else {
            // Print empty line segments
            print_colored_text(stdout, "─", Color::DarkGrey, None, None);
        }
    }

    print_colored_text(stdout, "]", Color::White, None, None);
}

pub fn draw_bar_multi<W: Write>(
    stdout: &mut W,
    label: &str,
    segments: &[BarSegment],
    max_value: f64,
    width: usize,
    show_text: Option<String>,
) {
    // Format label to exactly 5 characters for consistent alignment
    let formatted_label = if label.len() > 5 {
        label[..5].to_string()
    } else {
        format!("{label:<5}")
    };
    let available_bar_width = width.saturating_sub(9); // 9 for "LABEL: [" and "] " (5 + 4)

    // Calculate total value
    let total_value: f64 = segments.iter().map(|s| s.value).sum();
    let total_ratio = (total_value / max_value).min(1.0);

    // Prepare text to display inside the bar
    let display_text = if let Some(text) = show_text {
        // Ensure consistent width for value text (8 characters)
        if text.len() > 8 {
            text[..8].to_string()
        } else {
            format!("{text:>8}")
        }
    } else {
        format!("{:>7.1}%", total_ratio * 100.0)
    };

    // Print label
    print_colored_text(stdout, &formatted_label, Color::White, None, None);
    print_colored_text(stdout, ": [", Color::White, None, None);

    // Calculate positioning for right-aligned text
    let text_len = display_text.len();
    let text_pos = available_bar_width.saturating_sub(text_len);

    // Calculate segment positions
    let mut segment_positions = Vec::new();
    let mut current_pos = 0;

    for segment in segments {
        let segment_ratio = segment.value / max_value;
        let segment_width = (available_bar_width as f64 * segment_ratio).round() as usize;
        segment_positions.push((current_pos, current_pos + segment_width, segment.color));
        current_pos += segment_width;
    }

    // Ensure we don't exceed the total filled width
    let total_filled_width = (available_bar_width as f64 * total_ratio).round() as usize;
    if current_pos > total_filled_width {
        // Adjust the last segment to fit
        if let Some(last) = segment_positions.last_mut() {
            last.1 = total_filled_width;
        }
    }

    // Print the bar with segments
    for i in 0..available_bar_width {
        if i >= text_pos && i < text_pos + text_len {
            // Print text character
            let char_index = i - text_pos;
            if let Some(ch) = display_text.chars().nth(char_index) {
                print_colored_text(stdout, &ch.to_string(), Color::White, None, None);
            }
        } else {
            // Find which segment this position belongs to
            let mut printed = false;
            for &(start, end, color) in &segment_positions {
                if i >= start && i < end {
                    print_colored_text(stdout, "▬", color, None, None);
                    printed = true;
                    break;
                }
            }

            if !printed {
                // Print empty line segments
                print_colored_text(stdout, "─", Color::DarkGrey, None, None);
            }
        }
    }

    print_colored_text(stdout, "]", Color::White, None, None);
}

// Helper functions for common use cases
impl BarSegment {
    // CPU usage helpers (reserved for future use)
    #[allow(dead_code)]
    pub fn cpu_low_priority(value: f64) -> Self {
        // nice
        Self::new(value, Color::Blue).with_label("low")
    }

    #[allow(dead_code)]
    pub fn cpu_normal(value: f64) -> Self {
        // user
        Self::new(value, Color::Green).with_label("normal")
    }

    #[allow(dead_code)]
    pub fn cpu_kernel(value: f64) -> Self {
        // system
        Self::new(value, Color::Red).with_label("kernel")
    }

    #[allow(dead_code)]
    pub fn cpu_virtualized(value: f64) -> Self {
        // steal + guest
        Self::new(value, Color::DarkBlue).with_label("virtual")
    }

    // Memory usage helpers
    pub fn memory_used(value: f64) -> Self {
        Self::new(value, Color::Green).with_label("used")
    }

    pub fn memory_buffers(value: f64) -> Self {
        Self::new(value, Color::Blue).with_label("buffers")
    }

    pub fn memory_cache(value: f64) -> Self {
        Self::new(value, Color::Yellow).with_label("cache")
    }
}
