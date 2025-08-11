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

//! PowerMetrics module for managing macOS powermetrics process and data collection
//!
//! This module provides a modular architecture for:
//! - Process lifecycle management
//! - Data storage with circular buffering
//! - Background data collection
//! - Configuration management

mod collector;
mod config;
mod manager;
mod process;
mod store;

// Re-export public types and functions
pub use manager::{
    get_powermetrics_manager, has_powermetrics_data, initialize_powermetrics_manager,
    shutdown_powermetrics_manager,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_api_availability() {
        // This test just verifies that the public API is available
        // and can be referenced without errors

        // Functions should be accessible
        let _ = get_powermetrics_manager();
    }
}
