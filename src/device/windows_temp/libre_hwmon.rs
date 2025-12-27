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

//! LibreHardwareMonitor WMI temperature source.
//!
//! LibreHardwareMonitor is an open-source application that can read temperatures
//! from various hardware sensors and exposes them via WMI.
//!
//! Reference: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor
//!
//! Note: LibreHardwareMonitor must be running for this source to work.
//! The user should be advised to run LibreHardwareMonitor if they want
//! temperature monitoring on Windows systems without ACPI thermal zones.

use super::{is_wmi_not_found_error, TemperatureResult};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use wmi::WMIConnection;

/// Helper to get read lock, recovering from poisoned state.
fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Helper to get write lock, recovering from poisoned state.
fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// WMI structure for LibreHardwareMonitor sensor.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct LhmSensor {
    /// Sensor name (e.g., "CPU Package", "CPU Core #1")
    name: Option<String>,
    /// Sensor type (e.g., "Temperature", "Voltage", "Clock")
    sensor_type: Option<String>,
    /// Current sensor value
    value: Option<f32>,
    /// Parent hardware identifier
    #[serde(default)]
    parent: Option<String>,
}

/// Cached LibreHardwareMonitor WMI connection state.
struct LhmWmiState {
    connection: WMIConnection,
}

/// LibreHardwareMonitor WMI temperature source.
pub struct LibreHardwareMonitorSource {
    /// Cached connection state
    state: RwLock<Option<LhmWmiState>>,
    /// Whether we've already tried to connect
    connect_attempted: OnceCell<bool>,
}

impl Default for LibreHardwareMonitorSource {
    fn default() -> Self {
        Self::new()
    }
}

impl LibreHardwareMonitorSource {
    /// Create a new LibreHardwareMonitor source.
    pub fn new() -> Self {
        Self {
            state: RwLock::new(None),
            connect_attempted: OnceCell::new(),
        }
    }

    /// Attempt to connect to the LibreHardwareMonitor WMI namespace.
    fn try_connect(&self) -> bool {
        *self.connect_attempted.get_or_init(|| {
            match WMIConnection::with_namespace_path("root\\LibreHardwareMonitor") {
                Ok(conn) => {
                    *write_lock(&self.state) = Some(LhmWmiState { connection: conn });
                    true
                }
                Err(_) => {
                    // Also try OpenHardwareMonitor namespace (older versions)
                    match WMIConnection::with_namespace_path("root\\OpenHardwareMonitor") {
                        Ok(conn) => {
                            *write_lock(&self.state) = Some(LhmWmiState { connection: conn });
                            true
                        }
                        Err(_) => false,
                    }
                }
            }
        })
    }

    /// Get temperature from LibreHardwareMonitor WMI.
    ///
    /// # Returns
    /// * `TemperatureResult::Success(temp)` - Temperature in Celsius
    /// * `TemperatureResult::NotFound` - LibreHardwareMonitor WMI namespace not available
    /// * `TemperatureResult::Error` - Transient error during query
    /// * `TemperatureResult::NoValidReading` - Query succeeded but returned invalid data
    pub fn get_temperature(&self) -> TemperatureResult {
        // Try to connect if not already attempted
        if !self.try_connect() {
            return TemperatureResult::NotFound;
        }

        let state_guard = read_lock(&self.state);
        let state = match state_guard.as_ref() {
            Some(s) => s,
            None => return TemperatureResult::NotFound,
        };

        // Query for CPU temperature sensors
        let query =
            "SELECT Name, SensorType, Value, Parent FROM Sensor WHERE SensorType='Temperature'";

        let results: Result<Vec<LhmSensor>, _> = state.connection.raw_query(query);

        match results {
            Ok(sensors) => {
                if sensors.is_empty() {
                    // LibreHardwareMonitor is running but no temperature sensors found
                    return TemperatureResult::NoValidReading;
                }

                // Priority order for CPU temperature sensors:
                // 1. "CPU Package" - Package temperature (most accurate overall CPU temp)
                // 2. "CPU CCD" - Chiplet temperature (AMD)
                // 3. "CPU Core #0" or similar - Individual core temperature
                // 4. Any CPU-related temperature

                let priority_names = ["CPU Package", "CPU CCD", "CPU Core"];

                for priority in priority_names {
                    for sensor in &sensors {
                        if let (Some(name), Some(value)) = (&sensor.name, sensor.value) {
                            // Check if it's a CPU sensor
                            let is_cpu = sensor
                                .parent
                                .as_ref()
                                .map(|p| p.to_lowercase().contains("cpu"))
                                .unwrap_or(false)
                                || name.to_lowercase().contains("cpu");

                            if is_cpu && name.contains(priority) {
                                let temp = value as f64;
                                if temp > 0.0 && temp < 150.0 {
                                    // Use round() for more accurate conversion
                                    return TemperatureResult::Success(temp.round() as u32);
                                }
                            }
                        }
                    }
                }

                // Fallback: any CPU temperature sensor
                for sensor in &sensors {
                    if let (Some(name), Some(value)) = (&sensor.name, sensor.value) {
                        let is_cpu = sensor
                            .parent
                            .as_ref()
                            .map(|p| p.to_lowercase().contains("cpu"))
                            .unwrap_or(false)
                            || name.to_lowercase().contains("cpu");

                        if is_cpu {
                            let temp = value as f64;
                            if temp > 0.0 && temp < 150.0 {
                                // Use round() for more accurate conversion
                                return TemperatureResult::Success(temp.round() as u32);
                            }
                        }
                    }
                }

                TemperatureResult::NoValidReading
            }
            Err(e) => {
                if is_wmi_not_found_error(&e) {
                    TemperatureResult::NotFound
                } else {
                    // LibreHardwareMonitor might not be running
                    // This could be transient (user might start it later)
                    TemperatureResult::Error
                }
            }
        }
    }
}
