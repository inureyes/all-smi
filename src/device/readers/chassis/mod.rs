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

//! Chassis/Node-level monitoring module
//!
//! This module provides readers for chassis-level metrics including:
//! - Total power consumption (CPU+GPU+ANE)
//! - Thermal data (inlet/outlet temperature, thermal pressure)
//! - Cooling information (fan speeds)
//! - PSU status

// Apple Silicon chassis reader using powermetrics (requires sudo)
// Only compiled when native-macos feature is NOT enabled
#[cfg(all(target_os = "macos", feature = "powermetrics"))]
mod apple_silicon;

// Native Apple Silicon chassis reader using IOReport/SMC (no sudo required)
#[cfg(all(target_os = "macos", not(feature = "powermetrics")))]
mod apple_silicon_native;

mod generic;

#[cfg(all(target_os = "macos", feature = "powermetrics"))]
pub use apple_silicon::AppleSiliconChassisReader;

#[cfg(all(target_os = "macos", not(feature = "powermetrics")))]
pub use apple_silicon_native::AppleSiliconNativeChassisReader;

#[allow(unused_imports)]
pub use generic::GenericChassisReader;

use crate::device::ChassisReader;

/// Create a platform-appropriate chassis reader
pub fn create_chassis_reader() -> Box<dyn ChassisReader> {
    // On macOS with native APIs (no sudo required)
    #[cfg(all(target_os = "macos", not(feature = "powermetrics")))]
    {
        Box::new(AppleSiliconNativeChassisReader::new())
    }

    // On macOS with powermetrics (requires sudo)
    #[cfg(all(target_os = "macos", feature = "powermetrics"))]
    {
        Box::new(AppleSiliconChassisReader::new())
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On other platforms, use generic reader that aggregates GPU power
        Box::new(GenericChassisReader::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_chassis_reader() {
        let reader = create_chassis_reader();
        // Just verify we can create a reader without panicking
        let _ = reader.get_chassis_info();
    }
}
