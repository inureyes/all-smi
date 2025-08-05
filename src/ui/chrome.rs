use std::io::Write;

use crossterm::{cursor, queue, style::Color};

use crate::app_state::AppState;
use crate::ui::text::{display_width, print_colored_text, truncate_to_width};

pub fn print_loading_indicator<W: Write>(stdout: &mut W, cols: u16, rows: u16, frame_counter: u64) {
    // Center the loading message
    let message = "Loading...";
    let x = (cols.saturating_sub(message.len() as u16)) / 2;
    let y = rows / 2;

    queue!(stdout, cursor::MoveTo(x, y)).unwrap();
    print_colored_text(stdout, message, Color::Yellow, None, None);

    // Progress bar parameters
    let bar_width = 40.min(cols as usize - 10); // Ensure it fits on screen
    let bar_x = (cols.saturating_sub(bar_width as u16)) / 2;
    let bar_y = y + 2; // 2 lines below "Loading..."

    // Create animated progress bar
    let animation_speed = 3u64; // Lower = faster (was 8, originally 20)
    let position = ((frame_counter / animation_speed) % (bar_width as u64 * 2)) as usize;

    // Calculate the sliding block position (ping-pong effect)
    let block_size = 3.min(bar_width / 4); // Smaller block (was 5)
    let actual_pos = if position < bar_width {
        position
    } else {
        bar_width * 2 - position - 1
    };

    // Ensure the block doesn't go out of bounds
    let block_start = actual_pos.min(bar_width.saturating_sub(block_size));
    let block_end = (block_start + block_size).min(bar_width);

    // Move to progress bar position
    queue!(stdout, cursor::MoveTo(bar_x, bar_y)).unwrap();

    // Draw the progress bar with thinner characters
    for i in 0..bar_width {
        if i >= block_start && i < block_end {
            print_colored_text(stdout, "━", Color::Cyan, None, None);
        } else {
            print_colored_text(stdout, "─", Color::DarkGrey, None, None);
        }
    }
}

pub fn print_function_keys<W: Write>(
    stdout: &mut W,
    cols: u16,
    rows: u16,
    state: &AppState,
    is_remote: bool,
) {
    // Move to bottom of screen
    queue!(stdout, cursor::MoveTo(0, rows - 1)).unwrap();

    // Get current sorting indicator
    let sort_indicator = match state.sort_criteria {
        crate::app_state::SortCriteria::Default => "Sort:Default",
        crate::app_state::SortCriteria::Pid => "Sort:PID",
        crate::app_state::SortCriteria::User => "Sort:User",
        crate::app_state::SortCriteria::Priority => "Sort:Priority",
        crate::app_state::SortCriteria::Nice => "Sort:Nice",
        crate::app_state::SortCriteria::VirtualMemory => "Sort:VIRT",
        crate::app_state::SortCriteria::ResidentMemory => "Sort:RES",
        crate::app_state::SortCriteria::State => "Sort:State",
        crate::app_state::SortCriteria::CpuPercent => "Sort:CPU%",
        crate::app_state::SortCriteria::MemoryPercent => "Sort:MEM%",
        crate::app_state::SortCriteria::GpuPercent => "Sort:GPU%",
        crate::app_state::SortCriteria::GpuMemoryUsage => "Sort:GPU-Mem",
        crate::app_state::SortCriteria::CpuTime => "Sort:Time",
        crate::app_state::SortCriteria::Command => "Sort:Command",
        crate::app_state::SortCriteria::Utilization => "Sort:Util",
        crate::app_state::SortCriteria::GpuMemory => "Sort:GPU-Mem",
        crate::app_state::SortCriteria::Power => "Sort:Power",
        crate::app_state::SortCriteria::Temperature => "Sort:Temp",
    };

    let function_keys = if is_remote {
        // Remote mode: only GPU sorting
        format!(
            "h:Help q:Exit c:CPU Cores ←→:Tabs ↑↓:Scroll PgUp/PgDn:Page d:Default u:Util g:GPU-Mem [{sort_indicator}]"
        )
    } else {
        // Local mode: both process and GPU sorting
        format!("h:Help q:Exit c:CPU Cores ←→:Tabs ↑↓:Scroll PgUp/PgDn:Page p:PID m:Memory d:Default u:Util g:GPU-Mem [{sort_indicator}]")
    };

    let truncated_keys = if display_width(&function_keys) > cols as usize {
        truncate_to_width(&function_keys, cols as usize)
    } else {
        function_keys
    };

    // Check if there's a notification to display
    let notification_msg = state.notifications.get_current_message().unwrap_or("");
    let notification_len = display_width(notification_msg);

    // Calculate space available for function keys (reserve space for notification)
    let available_space = if notification_len > 0 {
        cols.saturating_sub(notification_len as u16 + 1) // +1 for separator space
    } else {
        cols
    } as usize;

    // Truncate function keys if necessary to make room for notification
    let final_function_keys = if display_width(&truncated_keys) > available_space {
        truncate_to_width(&truncated_keys, available_space)
    } else {
        truncated_keys
    };

    // Print function keys
    print_colored_text(stdout, &final_function_keys, Color::DarkGreen, None, None);

    // Print notification if there is one
    if notification_len > 0 {
        // Add separator
        print_colored_text(stdout, " ", Color::White, None, None);

        // Print notification with appropriate color
        let notification_color =
            if notification_msg.contains("Error") || notification_msg.contains("Failed") {
                Color::Red
            } else if notification_msg.contains("Warning") {
                Color::Yellow
            } else {
                Color::Cyan
            };

        print_colored_text(stdout, notification_msg, notification_color, None, None);
    }

    // Fill remaining space to clear any leftover text
    let used_space = display_width(&final_function_keys)
        + if notification_len > 0 {
            notification_len + 1
        } else {
            0
        };
    let remaining_space = cols as usize - used_space.min(cols as usize);

    if remaining_space > 0 {
        print_colored_text(
            stdout,
            &" ".repeat(remaining_space),
            Color::White,
            None,
            None,
        );
    }
}
