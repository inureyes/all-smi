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

// These types will be provided by implementing modules
// For now, we use generic type parameters to avoid dependencies

/// Trait for rendering device information in the terminal UI
///
/// This trait defines the common interface for rendering different types
/// of device information (GPU, CPU, Memory, Storage) in the terminal.
pub trait DeviceRenderer {
    /// The type of device info this renderer handles
    type DeviceInfo;

    /// Render the device information to the given writer
    ///
    /// # Arguments
    /// * `writer` - The output writer (typically stdout)
    /// * `info` - The device information to render
    /// * `width` - The available terminal width
    /// * `height` - The available terminal height
    /// * `is_remote` - Whether this is remote monitoring mode
    fn render<W: Write>(
        &self,
        writer: &mut W,
        info: &Self::DeviceInfo,
        width: u16,
        height: u16,
        is_remote: bool,
    ) -> std::io::Result<()>;

    /// Render a summary view (used for compact displays)
    ///
    /// # Arguments
    /// * `writer` - The output writer
    /// * `info` - The device information to render
    /// * `width` - The available terminal width
    fn render_summary<W: Write>(
        &self,
        writer: &mut W,
        info: &Self::DeviceInfo,
        width: u16,
    ) -> std::io::Result<()>;

    /// Get the minimum required width for rendering
    fn min_width(&self) -> u16 {
        80
    }

    /// Get the minimum required height for rendering
    fn min_height(&self) -> u16 {
        24
    }
}

/// Trait for renderers that support multiple devices
pub trait MultiDeviceRenderer: DeviceRenderer {
    /// Render multiple devices in a grid or list layout
    ///
    /// # Arguments
    /// * `writer` - The output writer
    /// * `devices` - List of devices to render
    /// * `width` - The available terminal width
    /// * `height` - The available terminal height
    /// * `scroll_offset` - The current scroll offset for pagination
    fn render_multiple<W: Write>(
        &self,
        writer: &mut W,
        devices: &[Self::DeviceInfo],
        width: u16,
        height: u16,
        scroll_offset: usize,
    ) -> std::io::Result<()>;

    /// Calculate the number of devices that can fit in the available space
    fn calculate_visible_count(&self, width: u16, height: u16) -> usize;
}

/// Trait for GPU-specific rendering capabilities
pub trait GpuRenderer<T>: DeviceRenderer<DeviceInfo = T> {
    /// Render GPU utilization graph
    fn render_utilization_graph<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;

    /// Render GPU memory usage
    fn render_memory_usage<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;

    /// Render GPU temperature
    fn render_temperature<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;

    /// Render GPU processes
    fn render_processes<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
        height: u16,
    ) -> std::io::Result<()>;
}

/// Trait for CPU-specific rendering capabilities
pub trait CpuRenderer<T>: DeviceRenderer<DeviceInfo = T> {
    /// Render CPU core visualization
    fn render_core_visualization<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;

    /// Render CPU frequency information
    fn render_frequency<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;

    /// Render CPU temperature
    fn render_temperature<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;
}

/// Trait for Memory-specific rendering capabilities
pub trait MemoryRenderer<T>: DeviceRenderer<DeviceInfo = T> {
    /// Render memory usage bars
    fn render_usage_bars<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;

    /// Render memory statistics table
    fn render_stats_table<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;
}

/// Trait for Storage-specific rendering capabilities
pub trait StorageRenderer<T>: DeviceRenderer<DeviceInfo = T> {
    /// Render storage usage bars
    fn render_usage_bars<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;

    /// Render storage I/O statistics
    fn render_io_stats<W: Write>(
        &self,
        writer: &mut W,
        info: &T,
        width: u16,
    ) -> std::io::Result<()>;
}
