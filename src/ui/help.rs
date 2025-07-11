use crate::app_state::AppState;
use crossterm::style::{Color, Stylize};

/// Generate a full-screen, colorful help interface with three sections:
/// 1. Top: Title section with ASCII logo
/// 2. Middle: Keyboard shortcuts cheat sheet  
/// 3. Bottom: Terminal usage options
pub fn generate_help_popup_content(
    cols: u16,
    rows: u16,
    state: &AppState,
    is_remote: bool,
) -> String {
    let width = cols as usize;
    let height = rows as usize;

    let mut content = String::new();

    // Create full-screen help with window border
    for row in 0..height {
        let line = match row {
            0 => format!("╔{}╗", "═".repeat(width.saturating_sub(2))),
            r if r == height - 1 => format!("╚{}╝", "═".repeat(width.saturating_sub(2))),
            _ => {
                let inner_content = get_row_content(row, width, height, state, is_remote);
                // Ensure content exactly fills the space between borders
                let padded_content = pad_content_to_width(inner_content, width.saturating_sub(2));
                format!("║{padded_content}║")
            }
        };

        content.push_str(&line);
        if row < height - 1 {
            content.push('\n');
        }
    }

    content
}

fn pad_content_to_width(content: String, target_width: usize) -> String {
    // Calculate display width correctly for Unicode characters
    let display_width = calculate_display_width(&content);

    if display_width >= target_width {
        // If content is too long, just return as-is since our content should fit
        content
    } else {
        // Pad with spaces to reach target width
        format!("{}{}", content, " ".repeat(target_width - display_width))
    }
}

fn calculate_display_width(text: &str) -> usize {
    // First strip ANSI escape codes
    let clean_text = strip_ansi_codes(text);

    // Count display width of each character
    let mut width = 0;
    for ch in clean_text.chars() {
        width += char_display_width(ch);
    }
    width
}

fn char_display_width(ch: char) -> usize {
    match ch {
        // Box drawing characters (ALL-SMI logo)
        '█' | '╔' | '╗' | '╚' | '╝' | '║' | '═' | '╭' | '╮' | '╰' | '╯' | '│' | '─' | '┌' | '┐'
        | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' => 1,

        // Arrow characters
        '←' | '→' | '↑' | '↓' => 1,

        // Regular ASCII characters
        c if c.is_ascii() => 1,

        // Most other Unicode characters (default to width 1 for terminal safety)
        _ => 1,
    }
}

fn get_row_content(
    row: usize,
    width: usize,
    height: usize,
    state: &AppState,
    is_remote: bool,
) -> String {
    let content_width = width.saturating_sub(2); // Account for border

    // Define section boundaries
    let title_start = 2;
    let title_end = 12;
    let shortcuts_start = 14;
    let shortcuts_end = height.saturating_sub(16); // Reserve space for terminal section
    let terminal_start = shortcuts_end + 1;

    if row >= title_start && row <= title_end {
        // TOP SECTION: Title and Logo
        render_title_section(row - title_start, content_width)
    } else if row >= shortcuts_start && row < shortcuts_end {
        // MIDDLE SECTION: Keyboard Shortcuts
        render_shortcuts_section(row - shortcuts_start, content_width, state, is_remote)
    } else if row >= terminal_start && row < height - 1 {
        // BOTTOM SECTION: Terminal Usage
        render_terminal_section(row - terminal_start, content_width)
    } else {
        // Empty spacer lines
        " ".repeat(content_width)
    }
}

fn render_title_section(line_idx: usize, width: usize) -> String {
    let title_lines = [
        "",
        "    █████╗ ██╗     ██╗          ███████╗███╗   ███╗██╗",
        "   ██╔══██╗██║     ██║          ██╔════╝████╗ ████║██║",
        "   ███████║██║     ██║    █████╗███████╗██╔████╔██║██║",
        "   ██╔══██║██║     ██║    ╚════╝╚════██║██║╚██╔╝██║██║",
        "   ██║  ██║███████╗███████╗     ███████║██║ ╚═╝ ██║██║",
        "   ╚═╝  ╚═╝╚══════╝╚══════╝     ╚══════╝╚═╝     ╚═╝╚═╝",
        "",
        "GPU Monitoring and Management Tool",
        "",
    ];

    if line_idx < title_lines.len() {
        center_text_colored(title_lines[line_idx], width, Color::Cyan)
    } else {
        " ".repeat(width)
    }
}

fn render_shortcuts_section(
    line_idx: usize,
    width: usize,
    state: &AppState,
    is_remote: bool,
) -> String {
    let mut shortcuts_lines = vec![
        ("", "KEYBOARD SHORTCUTS & NAVIGATION", "title"),
        ("", "", "separator"),
        ("", "", ""),
        ("Navigation Keys:", "", "header"),
        (
            "  ← →",
            "Switch tabs (remote) / Scroll process list (local)",
            "shortcut",
        ),
        ("  ↑ ↓", "Scroll up/down in lists", "shortcut"),
        ("  PgUp PgDn", "Page up/down navigation", "shortcut"),
        ("  Home End", "Jump to top/bottom", "shortcut"),
        ("", "", ""),
        ("Display Control:", "", "header"),
        ("  h / 1", "Toggle this help screen", "shortcut"),
        ("  q", "Exit application", "shortcut"),
        ("  ESC", "Close help or exit", "shortcut"),
        ("", "", ""),
        ("Data Sorting:", "", "header"),
        ("  d", "Sort by default (hostname+index)", "shortcut"),
        ("  u", "Sort by GPU utilization", "shortcut"),
        ("  g", "Sort by GPU memory usage", "shortcut"),
    ];

    // Add mode-specific shortcuts
    if !is_remote {
        shortcuts_lines.extend(vec![
            ("  p", "Sort processes by PID", "shortcut"),
            ("  m", "Sort processes by memory", "shortcut"),
        ]);
    }

    shortcuts_lines.extend(vec![
        ("", "", ""),
        ("Process View Columns:", "", "header"),
        ("  PID", "Process ID", "legend"),
        ("  USER", "Process owner", "legend"),
        ("  PRI", "Priority (0-139, lower is higher)", "legend"),
        ("  NI", "Nice value (-20 to 19)", "legend"),
        ("  VIRT", "Virtual memory size", "legend"),
        ("  RES", "Resident memory size", "legend"),
        ("  S", "Process state (R/S/D/Z/T)", "legend"),
        ("  CPU%", "CPU utilization", "legend"),
        ("  MEM%", "Memory utilization", "legend"),
        ("  GPU%", "GPU utilization (if available)", "legend"),
        ("  VRAM", "GPU memory usage", "legend"),
        ("  TIME+", "Total CPU time used", "legend"),
        ("  Command", "Command line (← → to scroll)", "legend"),
        ("", "", ""),
        ("Process Color Legend:", "", "header"),
        ("  Your processes", "White text", "legend"),
        ("  Root/unknown", "Dark grey text", "legend"),
        ("  High usage", "Red/Yellow based on CPU/Memory %", "legend"),
        (
            "  GPU processes",
            "Green/Cyan based on system load",
            "legend",
        ),
        ("", "", ""),
    ]);

    // Add current sort status
    let sort_status = get_current_sort_status(&state.sort_criteria);
    shortcuts_lines.push(("Current sort:", &sort_status, "status"));

    if line_idx < shortcuts_lines.len() {
        let (key, desc, style) = &shortcuts_lines[line_idx];
        format_shortcut_line(key, desc, style, width)
    } else {
        " ".repeat(width)
    }
}

fn render_terminal_section(line_idx: usize, width: usize) -> String {
    let terminal_lines = vec![
        ("", "TERMINAL USAGE OPTIONS", "title"),
        ("", "", "separator"),
        ("", "", ""),
        ("Local Monitoring:", "", "header"),
        (
            "  sudo all-smi view",
            "Monitor local GPUs (requires sudo on macOS)",
            "command",
        ),
        ("", "", ""),
        ("Remote Monitoring:", "", "header"),
        (
            "  all-smi view --hosts http://node1:9090",
            "Monitor specific remote hosts",
            "command",
        ),
        (
            "  all-smi view --hostfile hosts.csv",
            "Monitor hosts from CSV file",
            "command",
        ),
        ("", "", ""),
        ("API Server Mode:", "", "header"),
        (
            "  all-smi api --port 9090",
            "Run as Prometheus metrics server",
            "command",
        ),
        (
            "  curl http://localhost:9090/metrics",
            "Fetch Prometheus metrics via HTTP",
            "command",
        ),
    ];

    if line_idx < terminal_lines.len() {
        let (cmd, desc, style) = &terminal_lines[line_idx];
        format_terminal_line(cmd, desc, style, width)
    } else {
        " ".repeat(width)
    }
}

fn format_shortcut_line(key: &str, desc: &str, style: &str, width: usize) -> String {
    let content = match style {
        "title" => center_text_colored(desc, width, Color::Yellow),
        "separator" => "═".repeat(width).with(Color::DarkGrey).to_string(),
        "header" => format!("  {}", key.green()),
        "shortcut" => {
            if key.is_empty() {
                String::new()
            } else {
                format!("  {:<12} {}", key.white().bold().to_string(), desc.white())
            }
        }
        "legend" => format!("  {:<12} {}", key, desc.white()),
        "status" => format!("  {:<12} {}", key.cyan().to_string(), desc.yellow()),
        _ => String::new(),
    };

    // Ensure content fills exactly the width
    pad_content_to_width(content, width)
}

fn format_terminal_line(cmd: &str, desc: &str, style: &str, width: usize) -> String {
    let content = match style {
        "title" => center_text_colored(desc, width, Color::Magenta),
        "separator" => "═".repeat(width).with(Color::DarkGrey).to_string(),
        "header" => format!("  {}", cmd.green()),
        "command" => {
            if cmd.is_empty() {
                String::new()
            } else {
                let formatted_cmd = format!("  {:<35}", cmd.white().bold().to_string());
                let formatted_seperator = "#".with(Color::DarkGrey).to_string();
                let formatted_desc = desc.blue().to_string();
                format!("{formatted_cmd} {formatted_seperator} {formatted_desc}")
            }
        }
        _ => String::new(),
    };

    // Ensure content fills exactly the width
    pad_content_to_width(content, width)
}

fn center_text_colored(text: &str, width: usize, color: Color) -> String {
    let display_width = calculate_display_width(text);

    if display_width >= width {
        text.to_string()
    } else {
        let total_padding = width - display_width;
        let left_padding = total_padding / 2;
        let right_padding = total_padding - left_padding;

        format!(
            "{}{}{}",
            " ".repeat(left_padding),
            text.with(color),
            " ".repeat(right_padding)
        )
    }
}

fn strip_ansi_codes(text: &str) -> String {
    // Simple ANSI escape sequence removal for length calculation
    let mut result = String::new();
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip ANSI escape sequence
            if chars.next() == Some('[') {
                // Skip until we find the end character (typically 'm')
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn get_current_sort_status(sort_criteria: &crate::app_state::SortCriteria) -> String {
    match sort_criteria {
        crate::app_state::SortCriteria::Default => "Default (hostname+index)",
        crate::app_state::SortCriteria::Pid => "Process PID",
        crate::app_state::SortCriteria::User => "User",
        crate::app_state::SortCriteria::Priority => "Priority",
        crate::app_state::SortCriteria::Nice => "Nice Value",
        crate::app_state::SortCriteria::VirtualMemory => "Virtual Memory",
        crate::app_state::SortCriteria::ResidentMemory => "Resident Memory",
        crate::app_state::SortCriteria::State => "Process State",
        crate::app_state::SortCriteria::CpuPercent => "CPU Usage %",
        crate::app_state::SortCriteria::MemoryPercent => "Memory Usage %",
        crate::app_state::SortCriteria::GpuPercent => "GPU Usage %",
        crate::app_state::SortCriteria::GpuMemoryUsage => "GPU Memory Usage",
        crate::app_state::SortCriteria::CpuTime => "CPU Time",
        crate::app_state::SortCriteria::Command => "Command",
        crate::app_state::SortCriteria::Utilization => "GPU Utilization",
        crate::app_state::SortCriteria::GpuMemory => "GPU Memory",
        crate::app_state::SortCriteria::Power => "Power Consumption",
        crate::app_state::SortCriteria::Temperature => "Temperature",
    }
    .to_string()
}
