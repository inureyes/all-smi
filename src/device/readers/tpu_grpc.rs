// Copyright 2025 Lablup Inc.
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

//! gRPC client for TPU runtime metrics.
//!
//! This module provides a native Rust gRPC client to collect TPU metrics
//! directly from the libtpu runtime metrics server at localhost:8431.
//!
//! The gRPC server is only available when a TPU workload (JAX/TensorFlow)
//! is actively running.

#![allow(unused)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use once_cell::sync::OnceCell;
use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, warn};

use super::tpu_info_runner;

// Include the generated protobuf code
pub mod tpu_proto {
    tonic::include_proto!("tpu.monitoring.runtime");
}

use tpu_proto::runtime_metric_service_client::RuntimeMetricServiceClient;
use tpu_proto::MetricRequest;

/// Default gRPC server address for libtpu metrics
const DEFAULT_GRPC_ADDR: &str = "http://localhost:8431";

/// Connection timeout for gRPC
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

/// Request timeout for gRPC calls
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Interval for background gRPC reconnection attempts
const GRPC_RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// Track whether gRPC was available on last check
static GRPC_WAS_AVAILABLE: OnceCell<AtomicBool> = OnceCell::new();

/// Metric names defined in libtpu
pub mod metric_names {
    pub const TOTAL_MEMORY: &str = "tpu.runtime.hbm.memory.total.bytes";
    pub const MEMORY_USAGE: &str = "tpu.runtime.hbm.memory.usage.bytes";
    pub const DUTY_CYCLE_PCT: &str = "tpu.runtime.tensorcore.dutycycle.percent";
}

/// TPU usage metrics from gRPC
#[derive(Debug, Clone, Default)]
pub struct TpuUsageMetrics {
    /// Device ID (chip index)
    pub device_id: i64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Total memory in bytes
    pub total_memory: u64,
    /// Duty cycle percentage (0-100)
    pub duty_cycle_pct: f64,
}

/// Cached gRPC channel
static GRPC_CHANNEL: OnceCell<Arc<Mutex<Option<Channel>>>> = OnceCell::new();

/// Get or create a cached gRPC channel
async fn get_channel() -> Option<Channel> {
    let channel_holder = GRPC_CHANNEL.get_or_init(|| Arc::new(Mutex::new(None)));
    let mut guard = channel_holder.lock().await;

    // Return cached channel if available
    if let Some(ref channel) = *guard {
        return Some(channel.clone());
    }

    // Try to create a new channel
    match create_channel().await {
        Ok(channel) => {
            *guard = Some(channel.clone());
            Some(channel)
        }
        Err(e) => {
            debug!("Failed to create gRPC channel: {}", e);
            None
        }
    }
}

/// Create a new gRPC channel to the TPU metrics server
async fn create_channel() -> Result<Channel, tonic::transport::Error> {
    Endpoint::from_static(DEFAULT_GRPC_ADDR)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .connect()
        .await
}

/// Clear cached channel (call when connection fails)
async fn clear_channel() {
    if let Some(channel_holder) = GRPC_CHANNEL.get() {
        let mut guard = channel_holder.lock().await;
        *guard = None;
    }
}

/// Fetch a single metric from the gRPC server
async fn fetch_metric(
    client: &mut RuntimeMetricServiceClient<Channel>,
    metric_name: &str,
) -> Option<Vec<(i64, MetricValue)>> {
    let request = tonic::Request::new(MetricRequest {
        metric_name: metric_name.to_string(),
        skip_node_aggregation: false,
    });

    match client.get_runtime_metric(request).await {
        Ok(response) => {
            let metric = response.into_inner().metric?;
            let mut results = Vec::new();

            for m in metric.metrics {
                // Extract device ID from attribute
                let device_id = m.attribute
                    .as_ref()
                    .and_then(|attr| attr.value.as_ref())
                    .and_then(|v| match v.attr.as_ref()? {
                        tpu_proto::attr_value::Attr::IntAttr(i) => Some(*i),
                        _ => None,
                    })
                    .unwrap_or(0);

                // Extract gauge value
                if let Some(tpu_proto::metric::Measure::Gauge(gauge)) = m.measure {
                    let value = match gauge.value {
                        Some(tpu_proto::gauge::Value::AsInt(i)) => MetricValue::Int(i),
                        Some(tpu_proto::gauge::Value::AsDouble(d)) => MetricValue::Double(d),
                        _ => continue,
                    };
                    results.push((device_id, value));
                }
            }

            // Sort by device ID
            results.sort_by_key(|(id, _)| *id);
            Some(results)
        }
        Err(e) => {
            debug!("Failed to fetch metric '{}': {}", metric_name, e);
            None
        }
    }
}

/// Metric value type
#[derive(Debug, Clone)]
enum MetricValue {
    Int(i64),
    Double(f64),
}

impl MetricValue {
    fn as_u64(&self) -> u64 {
        match self {
            MetricValue::Int(i) => (*i).max(0) as u64,
            MetricValue::Double(d) => (*d).max(0.0) as u64,
        }
    }

    fn as_f64(&self) -> f64 {
        match self {
            MetricValue::Int(i) => *i as f64,
            MetricValue::Double(d) => *d,
        }
    }
}

/// Update gRPC availability status and notify tpu_info_runner
fn update_grpc_status(available: bool) {
    let was_available = GRPC_WAS_AVAILABLE
        .get_or_init(|| AtomicBool::new(false));

    let prev = was_available.swap(available, Ordering::Relaxed);

    // Notify tpu_info_runner of status change
    if prev != available {
        tpu_info_runner::get_runner().set_grpc_available(available);
        if available {
            debug!("gRPC server became available - switching to native metrics");
        } else {
            debug!("gRPC server unavailable - falling back to CLI polling");
        }
    }
}

/// Fetch all TPU usage metrics via gRPC
///
/// Returns None if the gRPC server is not available (no workload running)
pub async fn get_tpu_metrics_grpc() -> Option<Vec<TpuUsageMetrics>> {
    let channel = match get_channel().await {
        Some(ch) => ch,
        None => {
            update_grpc_status(false);
            return None;
        }
    };

    let mut client = RuntimeMetricServiceClient::new(channel);

    // Fetch all three metrics
    let totals = match fetch_metric(&mut client, metric_names::TOTAL_MEMORY).await {
        Some(t) => t,
        None => {
            update_grpc_status(false);
            clear_channel().await;
            return None;
        }
    };

    let usages = match fetch_metric(&mut client, metric_names::MEMORY_USAGE).await {
        Some(u) => u,
        None => {
            update_grpc_status(false);
            clear_channel().await;
            return None;
        }
    };

    let duty_cycles = fetch_metric(&mut client, metric_names::DUTY_CYCLE_PCT)
        .await
        .unwrap_or_default();

    // Verify we have matching data
    if totals.len() != usages.len() {
        warn!(
            "Metric count mismatch: totals={}, usages={}",
            totals.len(),
            usages.len()
        );
        update_grpc_status(false);
        clear_channel().await;
        return None;
    }

    // Build result vector
    let mut results = Vec::with_capacity(totals.len());

    for ((device_id, total), (_, usage)) in totals.iter().zip(usages.iter()) {
        // Find matching duty cycle (may have different count due to per-chip vs per-core)
        let duty_cycle = duty_cycles
            .iter()
            .find(|(id, _)| *id == *device_id)
            .map(|(_, v)| v.as_f64())
            .unwrap_or(0.0);

        results.push(TpuUsageMetrics {
            device_id: *device_id,
            memory_usage: usage.as_u64(),
            total_memory: total.as_u64(),
            duty_cycle_pct: duty_cycle.clamp(0.0, 100.0),
        });
    }

    if results.is_empty() {
        update_grpc_status(false);
        None
    } else {
        // Success! gRPC is working
        update_grpc_status(true);
        Some(results)
    }
}

/// Check if the gRPC metrics server is available
pub async fn is_grpc_server_available() -> bool {
    get_channel().await.is_some()
}

/// Synchronous wrapper for get_tpu_metrics_grpc
/// Uses the tokio runtime to run the async function
pub fn get_tpu_metrics_grpc_sync() -> Option<Vec<TpuUsageMetrics>> {
    // Try to get the current tokio runtime handle
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // We're in an async context, use block_in_place
        tokio::task::block_in_place(|| handle.block_on(get_tpu_metrics_grpc()))
    } else {
        // No runtime available, create a temporary one
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        rt.block_on(get_tpu_metrics_grpc())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_grpc_connection() {
        // This test will pass if no TPU workload is running (expected to fail gracefully)
        let result = get_tpu_metrics_grpc().await;
        println!("gRPC metrics result: {:?}", result);
    }
}
