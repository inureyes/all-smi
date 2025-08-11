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

use std::sync::{Arc, Mutex};
use std::time::Duration;

use once_cell::sync::Lazy;

use super::collector::DataCollector;
use super::config::PowerMetricsConfig;
use super::store::MetricsStore;
use crate::device::powermetrics_parser::PowerMetricsData;

/// Global singleton for PowerMetricsManager
static POWERMETRICS_MANAGER: Lazy<Mutex<Option<Arc<PowerMetricsManager>>>> =
    Lazy::new(|| Mutex::new(None));

/// Manages a long-running powermetrics process with in-memory circular buffer
pub struct PowerMetricsManager {
    collector: Mutex<DataCollector>,
}

impl PowerMetricsManager {
    /// Create a new PowerMetricsManager and start the powermetrics process
    fn new(interval_secs: u64) -> Result<Self, Box<dyn std::error::Error>> {
        let config = PowerMetricsConfig::with_interval_secs(interval_secs);
        let store = Arc::new(MetricsStore::new(config.buffer_capacity));
        let mut collector = DataCollector::new(config, store);

        // Start collection
        collector.start()?;

        Ok(Self {
            collector: Mutex::new(collector),
        })
    }

    /// Get the latest powermetrics data from the circular buffer
    fn get_latest_data_internal(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        let collector = self.collector.lock().unwrap();
        collector.get_latest_data()
    }

    /// Get latest data as Result (public API for backward compatibility)
    pub fn get_latest_data_result(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        self.get_latest_data_internal()
    }

    /// Get latest data as Option (for test compatibility)
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn get_latest_data(&self) -> Option<PowerMetricsData> {
        self.get_latest_data_result().ok()
    }

    /// Get process information from the latest powermetrics data
    pub fn get_process_info(&self) -> Vec<(String, u32, f64)> {
        let collector = self.collector.lock().unwrap();
        collector.get_process_info()
    }
}

/// Initialize the global PowerMetrics manager
/// This should be called once at startup for macOS systems
pub fn initialize_powermetrics_manager(
    interval_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut manager_guard = POWERMETRICS_MANAGER.lock().unwrap();
    if manager_guard.is_none() {
        let manager = PowerMetricsManager::new(interval_secs)?;
        *manager_guard = Some(Arc::new(manager));
    }
    Ok(())
}

/// Get the global PowerMetrics manager instance
pub fn get_powermetrics_manager() -> Option<Arc<PowerMetricsManager>> {
    POWERMETRICS_MANAGER.lock().unwrap().clone()
}

/// Shutdown and cleanup the PowerMetrics manager
pub fn shutdown_powermetrics_manager() {
    if let Some(_manager) = get_powermetrics_manager() {
        // Drop all Arc references
        {
            let mut manager_guard = POWERMETRICS_MANAGER.lock().unwrap();
            *manager_guard = None;
        }

        // The manager will be dropped when the last Arc reference is dropped
        // The Drop implementation in DataCollector will handle cleanup
    }
}

/// Public API for getting the latest PowerMetrics data
#[allow(dead_code)]
pub fn get_latest_powermetrics_data() -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
    if let Some(manager) = get_powermetrics_manager() {
        manager.get_latest_data_internal()
    } else {
        Err("PowerMetrics manager not initialized".into())
    }
}

/// Public API for getting process information
#[allow(dead_code)]
pub fn get_powermetrics_process_info() -> Vec<(String, u32, f64)> {
    if let Some(manager) = get_powermetrics_manager() {
        manager.get_process_info()
    } else {
        Vec::new()
    }
}

/// Wait for the initial powermetrics data to be available
#[allow(dead_code)]
pub fn wait_for_initial_powermetrics_data(
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(manager) = get_powermetrics_manager() {
        let collector = manager.collector.lock().unwrap();
        collector.wait_for_initial_data(timeout)
    } else {
        Err("PowerMetrics manager not initialized".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_not_initialized() {
        // Ensure manager is not initialized
        shutdown_powermetrics_manager();

        // Should return error when not initialized
        let result = get_latest_powermetrics_data();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));

        // Process info should return empty when not initialized
        let processes = get_powermetrics_process_info();
        assert!(processes.is_empty());
    }

    #[test]
    fn test_wait_for_initial_data_not_initialized() {
        // Ensure manager is not initialized
        shutdown_powermetrics_manager();

        // Should return error when not initialized
        let result = wait_for_initial_powermetrics_data(Duration::from_millis(100));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }
}
