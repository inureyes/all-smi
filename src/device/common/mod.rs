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

// Common utilities for device modules: command execution, error handling, and JSON parsing.

pub mod command_executor;
pub mod error_handling;
pub mod json_parser;

/* Re-exports for convenience (keep minimal to avoid unused-imports clippy errors) */
pub use command_executor::execute_command_default;
pub use error_handling::{DeviceError, DeviceResult};
pub use json_parser::parse_csv_line;
