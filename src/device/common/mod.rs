// Common utilities for device modules: command execution, error handling, and JSON parsing.

pub mod command_executor;
pub mod error_handling;
pub mod json_parser;

/* Re-exports for convenience (keep minimal to avoid unused-imports clippy errors) */
pub use command_executor::execute_command_default;
pub use error_handling::{DeviceError, DeviceResult};
pub use json_parser::parse_csv_line;
