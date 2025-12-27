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

//! Intel WMI temperature source.
//!
//! Queries the root/Intel WMI namespace for thermal zone information.
//! This is available on some Intel systems with proper chipset drivers.

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

/// WMI structure for Intel thermal zone information.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct IntelThermalZone {
    /// Current temperature in Celsius or Kelvin (varies by implementation)
    current_temperature: Option<u32>,
    /// Some Intel implementations use Temperature instead
    #[serde(default)]
    temperature: Option<u32>,
}

/// Cached Intel WMI connection state.
struct IntelWmiState {
    connection: WMIConnection,
}

/// Intel WMI temperature source.
pub struct IntelWmiSource {
    /// Cached connection state
    state: RwLock<Option<IntelWmiState>>,
    /// Whether we've already tried to connect
    connect_attempted: OnceCell<bool>,
}

impl Default for IntelWmiSource {
    fn default() -> Self {
        Self::new()
    }
}

impl IntelWmiSource {
    /// Create a new Intel WMI source.
    pub fn new() -> Self {
        Self {
            state: RwLock::new(None),
            connect_attempted: OnceCell::new(),
        }
    }

    /// Attempt to connect to the Intel WMI namespace.
    fn try_connect(&self) -> bool {
        *self.connect_attempted.get_or_init(|| {
            match WMIConnection::with_namespace_path("root\\Intel") {
                Ok(conn) => {
                    *write_lock(&self.state) = Some(IntelWmiState { connection: conn });
                    true
                }
                Err(_) => false,
            }
        })
    }

    /// Get temperature from Intel WMI.
    ///
    /// # Returns
    /// * `TemperatureResult::Success(temp)` - Temperature in Celsius
    /// * `TemperatureResult::NotFound` - Intel WMI namespace not available
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

        // Try different Intel thermal zone classes
        // Intel implementations vary - try common class names
        let queries = [
            "SELECT CurrentTemperature, Temperature FROM ThermalZoneInformation",
            "SELECT CurrentTemperature, Temperature FROM Intel_ThermalZone",
        ];

        for query in queries {
            let results: Result<Vec<IntelThermalZone>, _> = state.connection.raw_query(query);

            match results {
                Ok(zones) if !zones.is_empty() => {
                    for zone in zones {
                        // Try CurrentTemperature first, then Temperature
                        let temp_value = zone.current_temperature.or(zone.temperature);

                        if let Some(temp) = temp_value {
                            // Intel may report in Celsius directly or in tenths of Kelvin
                            let celsius = if temp > 200 {
                                // Likely tenths of Kelvin
                                (temp as f64 / 10.0) - 273.15
                            } else {
                                // Already in Celsius
                                temp as f64
                            };

                            if celsius > 0.0 && celsius < 150.0 {
                                // Use round() for more accurate conversion
                                return TemperatureResult::Success(celsius.round() as u32);
                            }
                        }
                    }
                }
                Ok(_) => continue, // Empty result, try next query
                Err(e) if is_wmi_not_found_error(&e) => continue,
                Err(_) => continue,
            }
        }

        // If we connected but no queries worked, the namespace exists but no thermal data
        TemperatureResult::NotFound
    }
}
