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

use crate::common::config::AppConfig;

/// Configuration for the PowerMetrics system
#[derive(Debug, Clone)]
pub struct PowerMetricsConfig {
    /// Interval in milliseconds for powermetrics collection
    pub interval_ms: u64,
    /// Buffer capacity (number of samples to store)
    pub buffer_capacity: usize,
    /// Nice value for the powermetrics process (lower priority)
    pub nice_value: i32,
    /// Samplers to use for powermetrics
    pub samplers: Vec<String>,
    /// Whether to show process GPU information
    pub show_process_gpu: bool,
    /// Monitoring thread sleep duration in seconds
    pub monitor_interval_secs: u64,
}

impl Default for PowerMetricsConfig {
    fn default() -> Self {
        Self {
            interval_ms: AppConfig::POWERMETRICS_DEFAULT_INTERVAL_MS,
            buffer_capacity: AppConfig::POWERMETRICS_BUFFER_CAPACITY,
            nice_value: 10,
            samplers: vec![
                "cpu_power".to_string(),
                "gpu_power".to_string(),
                "ane_power".to_string(),
                "thermal".to_string(),
                "tasks".to_string(),
            ],
            show_process_gpu: true,
            monitor_interval_secs: 5,
        }
    }
}

impl PowerMetricsConfig {
    /// Create a new configuration with the specified interval in seconds
    pub fn with_interval_secs(interval_secs: u64) -> Self {
        Self {
            interval_ms: interval_secs * 1000,
            ..Default::default()
        }
    }

    /// Get the command-line arguments for powermetrics
    pub fn get_powermetrics_args(&self) -> Vec<String> {
        let mut args = vec![
            "nice".to_string(),
            "-n".to_string(),
            self.nice_value.to_string(),
            "powermetrics".to_string(),
            "--samplers".to_string(),
            self.samplers.join(","),
        ];

        if self.show_process_gpu {
            args.push("--show-process-gpu".to_string());
        }

        args.extend_from_slice(&["-i".to_string(), self.interval_ms.to_string()]);

        args
    }
}

/// Command types for the reader thread
#[derive(Debug)]
pub enum ReaderCommand {
    Shutdown,
}
