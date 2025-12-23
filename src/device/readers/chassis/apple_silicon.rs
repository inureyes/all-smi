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

//! Apple Silicon chassis reader
//!
//! Provides chassis-level metrics for Apple Silicon Macs including:
//! - Combined power consumption (CPU + GPU + ANE)
//! - Thermal pressure level
//! - Platform-specific details

use crate::device::powermetrics::get_powermetrics_manager;
use crate::device::{ChassisInfo, ChassisReader};
use crate::utils::get_hostname;
use chrono::Local;
use std::collections::HashMap;

/// Chassis reader for Apple Silicon Macs
pub struct AppleSiliconChassisReader {
    hostname: String,
}

impl Default for AppleSiliconChassisReader {
    fn default() -> Self {
        Self::new()
    }
}

impl AppleSiliconChassisReader {
    pub fn new() -> Self {
        Self {
            hostname: get_hostname(),
        }
    }
}

impl ChassisReader for AppleSiliconChassisReader {
    fn get_chassis_info(&self) -> Option<ChassisInfo> {
        // Try to get data from PowerMetricsManager
        let manager = get_powermetrics_manager()?;
        let data = manager.get_latest_data_result().ok()?;

        // Build detail map with platform-specific information
        let mut detail = HashMap::new();
        detail.insert("platform".to_string(), "Apple Silicon".to_string());

        // Add individual power components to detail
        let cpu_power_watts = data.cpu_power_mw / 1000.0;
        let gpu_power_watts = data.gpu_power_mw / 1000.0;
        let ane_power_watts = data.ane_power_mw / 1000.0;

        detail.insert(
            "cpu_power_watts".to_string(),
            format!("{cpu_power_watts:.2}"),
        );
        detail.insert(
            "gpu_power_watts".to_string(),
            format!("{gpu_power_watts:.2}"),
        );
        detail.insert(
            "ane_power_watts".to_string(),
            format!("{ane_power_watts:.2}"),
        );

        // Add cluster frequency information
        if data.e_cluster_frequency > 0 {
            detail.insert(
                "e_cluster_freq_mhz".to_string(),
                data.e_cluster_frequency.to_string(),
            );
        }
        if data.p_cluster_frequency > 0 {
            detail.insert(
                "p_cluster_freq_mhz".to_string(),
                data.p_cluster_frequency.to_string(),
            );
        }

        // Calculate total power (combined_power_mw includes CPU+GPU+ANE)
        let total_power_watts = if data.combined_power_mw > 0.0 {
            Some(data.combined_power_mw / 1000.0)
        } else {
            // Fallback: sum individual components if combined is not available
            let sum = cpu_power_watts + gpu_power_watts + ane_power_watts;
            if sum > 0.0 {
                Some(sum)
            } else {
                None
            }
        };

        Some(ChassisInfo {
            host_id: self.hostname.clone(),
            hostname: self.hostname.clone(),
            instance: self.hostname.clone(),
            total_power_watts,
            inlet_temperature: None,  // Not available on Apple Silicon
            outlet_temperature: None, // Not available on Apple Silicon
            thermal_pressure: data.thermal_pressure_level,
            fan_speeds: Vec::new(), // Fan control is managed by macOS
            psu_status: Vec::new(), // Not applicable for laptops/desktops
            detail,
            time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apple_silicon_chassis_reader_creation() {
        let reader = AppleSiliconChassisReader::new();
        assert!(!reader.hostname.is_empty());
    }
}
