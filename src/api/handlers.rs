use axum::extract::State;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::app_state::AppState;

use super::metrics::{
    cpu::CpuMetricExporter, disk::DiskMetricExporter, gpu::GpuMetricExporter,
    memory::MemoryMetricExporter, npu::NpuMetricExporter, process::ProcessMetricExporter,
    MetricExporter,
};

pub type SharedState = Arc<RwLock<AppState>>;

pub async fn metrics_handler(State(state): State<SharedState>) -> String {
    let state = state.read().await;
    let mut all_metrics = String::new();

    // Export GPU/NPU metrics
    if !state.gpu_info.is_empty() {
        // Export GPU/NPU metrics together since the exporters handle filtering
        let gpu_exporter = GpuMetricExporter::new(&state.gpu_info);
        all_metrics.push_str(&gpu_exporter.export_metrics());

        let npu_exporter = NpuMetricExporter::new(&state.gpu_info);
        all_metrics.push_str(&npu_exporter.export_metrics());
    }

    // Export process metrics
    if !state.process_info.is_empty() {
        let process_exporter = ProcessMetricExporter::new(&state.process_info);
        all_metrics.push_str(&process_exporter.export_metrics());
    }

    // Export CPU metrics
    if !state.cpu_info.is_empty() {
        let cpu_exporter = CpuMetricExporter::new(&state.cpu_info);
        all_metrics.push_str(&cpu_exporter.export_metrics());
    }

    // Export memory metrics
    if !state.memory_info.is_empty() {
        let memory_exporter = MemoryMetricExporter::new(&state.memory_info);
        all_metrics.push_str(&memory_exporter.export_metrics());
    }

    // Export disk metrics
    // Use instance name from first GPU if available, otherwise use hostname
    let instance = state.gpu_info.first().map(|info| info.instance.clone());
    let disk_exporter = DiskMetricExporter::new(instance);
    all_metrics.push_str(&disk_exporter.export_metrics());

    all_metrics
}
