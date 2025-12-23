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

//! Generic chassis reader for non-Apple Silicon platforms
//!
//! This reader aggregates GPU power consumption to provide chassis-level
//! power metrics. It serves as a foundation for future BMC/IPMI integration.

use crate::device::{ChassisInfo, ChassisReader};
use crate::utils::get_hostname;
use chrono::Local;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Generic chassis reader that aggregates device power
#[allow(dead_code)]
pub struct GenericChassisReader {
    hostname: String,
    /// Cached total GPU power (updated externally)
    cached_gpu_power: Arc<RwLock<Option<f64>>>,
}

impl Default for GenericChassisReader {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl GenericChassisReader {
    pub fn new() -> Self {
        Self {
            hostname: get_hostname(),
            cached_gpu_power: Arc::new(RwLock::new(None)),
        }
    }

    /// Update the cached GPU power value
    /// This should be called from the data collection loop with aggregated GPU power
    pub fn update_gpu_power(&self, total_gpu_power_watts: f64) {
        if let Ok(mut power) = self.cached_gpu_power.write() {
            *power = Some(total_gpu_power_watts);
        }
    }

    /// Get the cached GPU power value
    fn get_cached_gpu_power(&self) -> Option<f64> {
        self.cached_gpu_power.read().ok().and_then(|p| *p)
    }
}

impl ChassisReader for GenericChassisReader {
    fn get_chassis_info(&self) -> Option<ChassisInfo> {
        // Build platform detail
        let detail = {
            #[allow(unused_mut)]
            let mut d = HashMap::new();
            #[cfg(target_os = "linux")]
            d.insert("platform".to_string(), "Linux".to_string());
            #[cfg(target_os = "windows")]
            d.insert("platform".to_string(), "Windows".to_string());
            d
        };

        // Get total power from cached GPU power
        // In the future, this can be enhanced with IPMI/BMC data
        let total_power_watts = self.get_cached_gpu_power();

        // Only return chassis info if we have some data
        // For now, we always return at least the hostname info
        // Clone hostname once and use for all identifier fields
        let hostname = self.hostname.clone();
        Some(ChassisInfo {
            host_id: hostname.clone(),
            hostname: hostname.clone(),
            instance: hostname,
            total_power_watts,
            inlet_temperature: None,  // Future: IPMI integration
            outlet_temperature: None, // Future: IPMI integration
            thermal_pressure: None,   // Not applicable for non-Apple platforms
            fan_speeds: Vec::new(),   // Future: IPMI integration
            psu_status: Vec::new(),   // Future: IPMI integration
            detail,
            time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generic_chassis_reader_creation() {
        let reader = GenericChassisReader::new();
        assert!(!reader.hostname.is_empty());
    }

    #[test]
    fn test_update_gpu_power() {
        let reader = GenericChassisReader::new();
        reader.update_gpu_power(350.5);

        let chassis_info = reader.get_chassis_info();
        assert!(chassis_info.is_some());

        let info = chassis_info.unwrap();
        assert_eq!(info.total_power_watts, Some(350.5));
    }

    #[test]
    fn test_chassis_info_without_gpu_power() {
        let reader = GenericChassisReader::new();
        let chassis_info = reader.get_chassis_info();

        assert!(chassis_info.is_some());
        let info = chassis_info.unwrap();
        assert!(info.total_power_watts.is_none());
        assert!(!info.hostname.is_empty());
    }
}
