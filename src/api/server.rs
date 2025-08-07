use axum::{routing::get, Router};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::api::handlers::{metrics_handler, SharedState};
use crate::app_state::AppState;
use crate::cli::ApiArgs;
use crate::device::{get_cpu_readers, get_gpu_readers, get_memory_readers};

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

            let mut state = state_clone.write().await;
            state.gpu_info = all_gpu_info;
            state.cpu_info = all_cpu_info;
            state.memory_info = all_memory_info;
            state.process_info = all_processes;
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
