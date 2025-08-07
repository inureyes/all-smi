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

use crossterm::{
    cursor, queue,
    style::Print,
    terminal::{size, ClearType},
};
use std::io::{stdout, Write};

pub struct BufferWriter {
    buffer: String,
    line_count: usize,
}

impl BufferWriter {
    pub fn new() -> Self {
        Self {
            buffer: String::with_capacity(1024 * 1024), // Pre-allocate 1MB
            line_count: 0,
        }
    }

    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }

    pub fn line_count(&self) -> usize {
        self.line_count
    }
}

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = std::str::from_utf8(buf)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8"))?;

        // Count newlines in the new content
        self.line_count += s.matches('\n').count();

        self.buffer.push_str(s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Differential renderer that only updates changed lines to eliminate flickering
pub struct DifferentialRenderer {
    previous_lines: Vec<String>,
    screen_height: usize,
    screen_width: usize,
}

impl DifferentialRenderer {
    pub fn new() -> std::io::Result<Self> {
        let (width, height) = size().unwrap_or((80, 24));
        Ok(Self {
            previous_lines: Vec::new(),
            screen_height: height as usize,
            screen_width: width as usize,
        })
    }

    /// Render content with differential updates - only changed lines are updated
    pub fn render_differential(&mut self, content: &str) -> std::io::Result<()> {
        // Split content into lines - no padding to avoid truncation issues
        let current_lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();

        // Initialize previous_lines on first run
        if self.previous_lines.is_empty() {
            self.previous_lines = vec![String::new(); self.screen_height];
        }

        // Adjust buffer size if screen dimensions changed
        let (width, height) = size().unwrap_or((80, 24));
        if width as usize != self.screen_width || height as usize != self.screen_height {
            self.screen_width = width as usize;
            self.screen_height = height as usize;
            self.previous_lines
                .resize(self.screen_height, String::new());
        }

        // Find changed lines and update only those
        let mut stdout = stdout();
        let max_lines = std::cmp::min(current_lines.len(), self.screen_height);

        for (line_num, current_line) in current_lines.iter().enumerate().take(max_lines) {
            // Check if this line has changed
            if line_num >= self.previous_lines.len()
                || &self.previous_lines[line_num] != current_line
            {
                // Update this line
                queue!(
                    stdout,
                    cursor::MoveTo(0, line_num as u16),
                    Print(current_line)
                )?;
            }
        }

        // Clear any remaining lines if the new content is shorter
        if self.previous_lines.len() > current_lines.len() {
            for line_num in
                current_lines.len()..std::cmp::min(self.previous_lines.len(), self.screen_height)
            {
                if !self.previous_lines[line_num].is_empty() {
                    queue!(
                        stdout,
                        cursor::MoveTo(0, line_num as u16),
                        crossterm::terminal::Clear(ClearType::CurrentLine)
                    )?;
                }
            }
        }

        // Flush all queued updates at once
        stdout.flush()?;

        // Update previous_lines for next comparison
        self.previous_lines.clear();
        self.previous_lines.extend(current_lines);
        self.previous_lines
            .resize(self.screen_height, String::new());

        Ok(())
    }

    /// Force clear the entire screen (use sparingly, e.g., on startup or resize)
    pub fn force_clear(&mut self) -> std::io::Result<()> {
        let mut stdout = stdout();
        queue!(stdout, crossterm::terminal::Clear(ClearType::All))?;
        stdout.flush()?;

        // Reset previous state
        self.previous_lines.clear();
        self.previous_lines
            .resize(self.screen_height, String::new());

        Ok(())
    }
}
