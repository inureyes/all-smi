use std::collections::HashMap;
use std::io::Write;

use crossterm::{queue, style::Color, style::Print};

use crate::app_state::AppState;
use crate::common::config::ThemeConfig;
use crate::ui::text::{format_ram_value, print_colored_text};

pub fn draw_system_view<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    let box_width = (cols as usize).min(80);

    // Calculate cluster statistics
    let total_nodes = state.tabs.len().saturating_sub(1); // Exclude "All" tab
    let total_gpus = state.gpu_info.len();
    let total_memory_gb = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.total_memory)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);
    let total_power_watts = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.power_consumption)
        .sum::<f64>();

    // Calculate total CPU cores
    let total_cpu_cores = state
        .cpu_info
        .iter()
        .map(|cpu| {
            if let Some(apple_info) = &cpu.apple_silicon_info {
                apple_info.p_core_count + apple_info.e_core_count
            } else {
                cpu.total_cores
            }
        })
        .sum::<u32>();

    // Calculate total system memory
    let total_system_memory_gb = state
        .memory_info
        .iter()
        .map(|memory| memory.total_bytes)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);

    let used_system_memory_gb = state
        .memory_info
        .iter()
        .map(|memory| memory.used_bytes)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);

    // Calculate averages
    let avg_utilization = if total_gpus > 0 {
        state
            .gpu_info
            .iter()
            .map(|gpu| gpu.utilization)
            .sum::<f64>()
            / total_gpus as f64
    } else {
        0.0
    };

    let avg_temperature = if total_gpus > 0 {
        state
            .gpu_info
            .iter()
            .map(|gpu| gpu.temperature as f64)
            .sum::<f64>()
            / total_gpus as f64
    } else {
        0.0
    };

    // Calculate temperature standard deviation
    let temp_std_dev = if total_gpus > 1 {
        let temp_variance = state
            .gpu_info
            .iter()
            .map(|gpu| {
                let diff = gpu.temperature as f64 - avg_temperature;
                diff * diff
            })
            .sum::<f64>()
            / (total_gpus - 1) as f64;
        temp_variance.sqrt()
    } else {
        0.0
    };

    let avg_power = if total_gpus > 0 {
        total_power_watts / total_gpus as f64
    } else {
        0.0
    };

    // Calculate used GPU memory in GB
    let used_gpu_memory_gb = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.used_memory)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);

    // First row: | Nodes | Total RAM | GPU Cores | Total GPU RAM | Avg. Temp | Total Power |
    print_dashboard_row(
        stdout,
        &[
            ("Nodes", format!("{total_nodes}"), Color::Yellow),
            (
                "Total RAM",
                format_ram_value(total_system_memory_gb),
                Color::Green,
            ),
            ("GPU Cores", format!("{total_gpus}"), Color::Cyan),
            ("Total VRAM", format_ram_value(total_memory_gb), Color::Blue),
            (
                "Avg. Temp",
                format!("{avg_temperature:.0}°C"),
                Color::Magenta,
            ),
            (
                "Total Power",
                format!("{:.1}kW", total_power_watts / 1000.0),
                Color::Red,
            ),
        ],
        box_width,
    );

    // Second row: | CPU Cores | Used RAM | Avg. GPU Util | Used GPU RAM | Temp. Stdev | Avg. Power |
    print_dashboard_row(
        stdout,
        &[
            ("CPU Cores", format!("{total_cpu_cores}"), Color::Cyan),
            (
                "Used RAM",
                format_ram_value(used_system_memory_gb),
                Color::Green,
            ),
            ("GPU Util", format!("{avg_utilization:.1}%"), Color::Blue),
            (
                "Used VRAM",
                format_ram_value(used_gpu_memory_gb),
                Color::Blue,
            ),
            (
                "Temp. Stdev",
                format!("±{temp_std_dev:.1}°C"),
                Color::Magenta,
            ),
            ("Avg. Power", format!("{avg_power:.1}W"), Color::Red),
        ],
        box_width,
    );
}

pub fn draw_dashboard_items<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    // Print separator
    let separator = "─".repeat(cols as usize);
    print_colored_text(stdout, &separator, Color::DarkGrey, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Node utilization history box
    draw_utilization_history(stdout, state, cols);
    queue!(stdout, Print("\r\n")).unwrap();
}

fn print_dashboard_row<W: Write>(
    stdout: &mut W,
    items: &[(&str, String, Color)],
    _total_width: usize,
) {
    const ITEM_WIDTH: usize = 15; // Fixed width for each dashboard item

    // Print labels row
    print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    for (label, _, color) in items {
        // Truncate label if too long, ensuring it fits in 15 characters minus padding and separator
        let max_label_len = ITEM_WIDTH.saturating_sub(3);
        let truncated_label = if label.len() > max_label_len {
            &label[..max_label_len]
        } else {
            label
        };
        let formatted_label = format!(" {truncated_label:<max_label_len$}");
        print_colored_text(stdout, &formatted_label, *color, None, None);
        print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    }
    queue!(stdout, Print("\r\n")).unwrap();

    // Print values row
    print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    for (_, value, _) in items {
        // Truncate value if too long, ensuring it fits in 15 characters minus padding and separator
        let max_value_len = ITEM_WIDTH.saturating_sub(3);
        let truncated_value = if value.len() > max_value_len {
            &value[..max_value_len]
        } else {
            value
        };
        let formatted_value = format!(" {truncated_value:<max_value_len$}");
        print_colored_text(stdout, &formatted_value, Color::White, None, None);
        print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    }
    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn draw_utilization_history<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    let box_width = (cols as usize).min(80);

    if state.utilization_history.is_empty() {
        return;
    }

    // Calculate averages for display
    let avg_util =
        state.utilization_history.iter().sum::<f64>() / state.utilization_history.len() as f64;
    let avg_mem = state.memory_history.iter().sum::<f64>() / state.memory_history.len() as f64;
    let avg_temp =
        state.temperature_history.iter().sum::<f64>() / state.temperature_history.len() as f64;

    // Print header
    print_colored_text(stdout, "Live Statistics", Color::Cyan, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Split layout: left half for node view, right half for history gauges
    let left_width = box_width / 2;
    let right_width = box_width - left_width;
    let history_width = right_width.saturating_sub(15); // Leave space for labels

    // Print node view and history gauges side by side
    print_node_view_and_history(
        stdout,
        state,
        NodeViewParams {
            left_width,
            _right_width: right_width,
            history_width,
            avg_util,
            avg_mem,
            avg_temp,
        },
    );
}

struct NodeViewParams {
    left_width: usize,
    _right_width: usize,
    history_width: usize,
    avg_util: f64,
    avg_mem: f64,
    avg_temp: f64,
}

fn print_node_view_and_history<W: Write>(stdout: &mut W, state: &AppState, params: NodeViewParams) {
    // Get nodes (excluding "All" tab)
    let nodes: Vec<&String> = state.tabs.iter().skip(1).collect();

    // Calculate per-node utilization
    let mut node_utils: HashMap<String, f64> = HashMap::new();
    for node in &nodes {
        let node_gpus: Vec<_> = state
            .gpu_info
            .iter()
            .filter(|gpu| &gpu.hostname == *node)
            .collect();
        if !node_gpus.is_empty() {
            let node_util =
                node_gpus.iter().map(|gpu| gpu.utilization).sum::<f64>() / node_gpus.len() as f64;
            node_utils.insert(node.to_string(), node_util);
        }
    }

    // Calculate node grid layout
    let nodes_per_row = params.left_width.saturating_sub(2).max(1);
    let num_rows = if nodes.is_empty() {
        1
    } else {
        ((nodes.len() - 1) / nodes_per_row) + 1
    };
    let num_rows = num_rows.min(3); // Limit to 3 rows max

    // Print each row of the combined view
    for row in 0..3 {
        if row < num_rows {
            // Print node view for this row
            print_node_view_row(
                stdout,
                &nodes,
                &node_utils,
                state.current_tab,
                params.left_width,
                row,
                nodes_per_row,
            );
        } else {
            // Print empty space for this row
            print_colored_text(
                stdout,
                &" ".repeat(params.left_width),
                Color::White,
                None,
                None,
            );
        }

        // Print corresponding history line
        match row {
            0 => {
                print_colored_text(stdout, "GPU Util.", Color::Yellow, None, None);
                // Show as time series in both local and remote modes
                print_history_bar_with_value(
                    stdout,
                    &state.utilization_history,
                    params.history_width,
                    100.0,
                    format!("{:.1}%", params.avg_util),
                );
            }
            1 => {
                print_colored_text(stdout, "GPU Mem. ", Color::Yellow, None, None);
                // Show as time series in both local and remote modes
                print_history_bar_with_value(
                    stdout,
                    &state.memory_history,
                    params.history_width,
                    100.0,
                    format!("{:.1}%", params.avg_mem),
                );
            }
            2 => {
                print_colored_text(stdout, "Temp     ", Color::Yellow, None, None);
                // Show as time series in both local and remote modes
                print_history_bar_with_value(
                    stdout,
                    &state.temperature_history,
                    params.history_width,
                    100.0,
                    format!("{:.0}°C", params.avg_temp),
                );
            }
            _ => {}
        }
        queue!(stdout, Print("\r\n")).unwrap();
    }
}

fn print_node_view_row<W: Write>(
    stdout: &mut W,
    nodes: &[&String],
    node_utils: &HashMap<String, f64>,
    current_tab: usize,
    left_width: usize,
    row: usize,
    nodes_per_row: usize,
) {
    let start_idx = row * nodes_per_row;
    let row_nodes: Vec<&String> = nodes
        .iter()
        .skip(start_idx)
        .take(nodes_per_row)
        .copied()
        .collect();

    let mut row_chars = Vec::new();
    for (col, node) in row_nodes.iter().enumerate() {
        let util = node_utils.get(*node).unwrap_or(&0.0);
        let (char, color) = get_node_char_and_color(*util, current_tab == col + 1 + start_idx);
        row_chars.push((char, color));
    }

    // Calculate actual display width (Unicode chars like ● take 2 display columns)
    let actual_width = row_chars.len(); // Each node character takes 1 position, regardless of Unicode width
    let padding_needed = left_width.saturating_sub(actual_width);

    // Print each character with its color
    for (char, color) in row_chars {
        print_colored_text(stdout, &char.to_string(), color, None, None);
    }

    // Add padding spaces
    if padding_needed > 0 {
        print_colored_text(
            stdout,
            &" ".repeat(padding_needed),
            Color::White,
            None,
            None,
        );
    }
}

fn get_node_char_and_color(utilization: f64, is_selected: bool) -> (char, Color) {
    let base_color = ThemeConfig::utilization_color(utilization);

    let char = if is_selected { '●' } else { '○' };
    (char, base_color)
}

fn print_history_bar_with_value<W: Write>(
    stdout: &mut W,
    history: &std::collections::VecDeque<f64>,
    width: usize,
    max_value: f64,
    value_text: String,
) {
    if history.is_empty() || width == 0 {
        return;
    }

    let available_width = width.saturating_sub(value_text.len() + 1);
    let step = if history.len() > available_width {
        history.len() / available_width
    } else {
        1
    };

    // Sample the history based on available width and reverse for right-to-left display
    let sampled_data: Vec<f64> = history
        .iter()
        .step_by(step.max(1))
        .take(available_width)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    // Print the bar
    for &value in &sampled_data {
        let normalized = (value / max_value).min(1.0);
        let color = ThemeConfig::progress_bar_color(normalized);

        let bar_char = if normalized > 0.875 {
            '⣿' // All 8 dots filled
        } else if normalized > 0.75 {
            '⣾' // 7 dots filled
        } else if normalized > 0.625 {
            '⣶' // 6 dots filled
        } else if normalized > 0.5 {
            '⣦' // 5 dots filled
        } else if normalized > 0.375 {
            '⣤' // 4 dots filled
        } else if normalized > 0.25 {
            '⣠' // 3 dots filled
        } else if normalized > 0.125 {
            '⣀' // 2 dots filled (bottom)
        } else if normalized > 0.0 {
            '⡀' // 1 dot filled (bottom)
        } else {
            '⠀' // Empty Braille character for zero values
        };

        print_colored_text(stdout, &bar_char.to_string(), color, None, None);
    }

    // Print remaining space as empty Braille characters (but limit to reasonable amount)
    let remaining = available_width.saturating_sub(sampled_data.len());
    if remaining > 0 && remaining < 50 {
        // Limit to prevent long horizontal lines
        print_colored_text(stdout, &"⠀".repeat(remaining), Color::DarkGrey, None, None);
    }

    // Print value
    print_colored_text(stdout, " ", Color::White, None, None);
    print_colored_text(stdout, &value_text, Color::White, None, None);
}
