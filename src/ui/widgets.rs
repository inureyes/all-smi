use std::io::Write;

use crossterm::style::Color;

use crate::common::config::ThemeConfig;
use crate::ui::text::print_colored_text;

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
