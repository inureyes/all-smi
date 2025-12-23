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

use axum::{routing::get, Router};
use std::time::Duration;
use sysinfo::Disks;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::api::handlers::{metrics_handler, SharedState};
use crate::app_state::AppState;
use crate::cli::ApiArgs;
use crate::device::{get_cpu_readers, get_gpu_readers, get_memory_readers};
use crate::storage::info::StorageInfo;
use crate::utils::{filter_docker_aware_disks, get_hostname};

pub async fn run_api_mode(args: &ApiArgs) {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "all_smi=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("Starting API mode...");
    let state = SharedState::new(RwLock::new(AppState::new()));
    let state_clone = state.clone();
    let processes = args.processes;
    let interval = args.interval;

    tokio::spawn(async move {
        let gpu_readers = get_gpu_readers();
        let cpu_readers = get_cpu_readers();
        let memory_readers = get_memory_readers();
        loop {
            let all_gpu_info = gpu_readers
                .iter()
                .flat_map(|reader| reader.get_gpu_info())
                .collect();

            let all_cpu_info = cpu_readers
                .iter()
                .flat_map(|reader| reader.get_cpu_info())
                .collect();

            let all_memory_info = memory_readers
                .iter()
                .flat_map(|reader| reader.get_memory_info())
                .collect();

            let all_processes = if processes {
                gpu_readers
                    .iter()
                    .flat_map(|reader| reader.get_process_info())
                    .collect()
            } else {
                Vec::new()
            };

            // Collect disk/storage info (cached in state to avoid per-request collection)
            let storage_info = collect_storage_info();

            let mut state = state_clone.write().await;
            state.gpu_info = all_gpu_info;
            state.cpu_info = all_cpu_info;
            state.memory_info = all_memory_info;
            state.process_info = all_processes;
            state.storage_info = storage_info;
            if state.loading {
                state.loading = false;
            }

            drop(state);
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind(&format!("0.0.0.0:{}", args.port))
        .await
        .unwrap();
    tracing::info!("API server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

/// Collect storage/disk information
/// This is called in the background task and cached in AppState
fn collect_storage_info() -> Vec<StorageInfo> {
    let mut storage_info = Vec::new();
    let disks = Disks::new_with_refreshed_list();
    let hostname = get_hostname();

    let mut filtered_disks = filter_docker_aware_disks(&disks);
    filtered_disks.sort_by(|a, b| {
        a.mount_point()
            .to_string_lossy()
            .cmp(&b.mount_point().to_string_lossy())
    });

    for (index, disk) in filtered_disks.iter().enumerate() {
        let mount_point_str = disk.mount_point().to_string_lossy();
        storage_info.push(StorageInfo {
            mount_point: mount_point_str.to_string(),
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
            host_id: hostname.clone(),
            hostname: hostname.clone(),
            index: index as u32,
        });
    }

    storage_info
}
