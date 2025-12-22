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

// Re-export status functions for UI
pub use readers::nvidia::get_nvml_status_message;
#[cfg(target_os = "linux")]
pub use readers::tenstorrent::get_tenstorrent_status_message;
#[cfg(target_os = "linux")]
pub use readers::google_tpu::get_tpu_status_message;

// CPU reader modules
#[cfg(target_os = "linux")]
pub mod cpu_linux;
#[cfg(target_os = "macos")]
pub mod cpu_macos;
#[cfg(target_os = "windows")]
pub mod cpu_windows;

// Container resource support
#[cfg(target_os = "linux")]
pub mod container_info;

// Memory reader modules
#[cfg(target_os = "linux")]
pub mod memory_linux;
#[cfg(target_os = "macos")]
pub mod memory_macos;
#[cfg(target_os = "windows")]
pub mod memory_windows;

// Powermetrics parser for Apple Silicon
#[cfg(target_os = "macos")]
pub mod powermetrics;
#[cfg(target_os = "macos")]
pub mod powermetrics_parser;

// hl-smi manager for Intel Gaudi
#[cfg(target_os = "linux")]
pub mod hlsmi;

/* Refactored modules */
pub mod common;
pub mod container_utils;
pub mod platform_detection;
pub mod process_list;
pub mod process_utils;
pub mod reader_factory;
pub mod readers;
pub mod traits;
pub mod types;

// Re-export commonly used items
pub use platform_detection::*;
pub use reader_factory::*;
pub use traits::*;
pub use types::*;
