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

use std::sync::Arc;

use super::config::PowerMetricsConfig;
use super::process::ProcessManager;
use super::store::MetricsStore;
use crate::device::powermetrics_parser::PowerMetricsData;

/// Collects powermetrics data in the background
pub struct DataCollector {
    process_manager: ProcessManager,
    store: Arc<MetricsStore>,
}

impl DataCollector {
    /// Create a new DataCollector
    pub fn new(config: PowerMetricsConfig, store: Arc<MetricsStore>) -> Self {
        let process_manager = ProcessManager::new(config, store.clone());

        Self {
            process_manager,
            store,
        }
    }

    /// Start collecting data
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.process_manager.start()
    }

    /// Stop collecting data
    pub fn stop(&mut self) {
        self.process_manager.shutdown();
        self.store.clear();
    }

    /// Get the latest powermetrics data
    pub fn get_latest_data(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        self.store.get_latest_data()
    }

    /// Get process information
    pub fn get_process_info(&self) -> Vec<(String, u32, f64)> {
        self.store.get_process_info()
    }

    /// Check if collection is running (test use only)
    #[cfg(test)]
    pub(super) fn is_running(&self) -> bool {
        self.process_manager.is_running()
    }

    /// Wait for initial data to be available (test use only)
    #[cfg(test)]
    pub(super) fn wait_for_initial_data(
        &self,
        timeout: std::time::Duration,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if self.get_latest_data().is_ok() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        Err("Timeout waiting for initial powermetrics data".into())
    }
}

impl Drop for DataCollector {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::AppConfig;

    #[test]
    fn test_collector_creation() {
        let config = PowerMetricsConfig::default();
        let store = Arc::new(MetricsStore::new(AppConfig::POWERMETRICS_BUFFER_CAPACITY));
        let collector = DataCollector::new(config, store);

        // Verify collector is created but not running
        assert!(!collector.is_running());
    }

    #[test]
    fn test_wait_for_initial_data_timeout() {
        let config = PowerMetricsConfig::default();
        let store = Arc::new(MetricsStore::new(AppConfig::POWERMETRICS_BUFFER_CAPACITY));
        let collector = DataCollector::new(config, store);

        // Should timeout since we haven't started collection
        let result = collector.wait_for_initial_data(std::time::Duration::from_millis(100));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Timeout"));
    }
}
