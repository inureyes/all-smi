use std::sync::Arc;

use tokio::sync::Mutex;

use crate::app_state::AppState;
use crate::cli::ViewArgs;
use crate::view::{
    data_collector::DataCollector, terminal_manager::TerminalManager, ui_loop::UiLoop,
};

pub async fn run_view_mode(args: &ViewArgs) {
    // Initialize application state
    let mut initial_state = AppState::new();

    // Set mode based on CLI arguments
    initial_state.is_local_mode = args.hosts.is_none() && args.hostfile.is_none();

    let app_state = Arc::new(Mutex::new(initial_state));

    // Initialize terminal
    let _terminal_manager = match TerminalManager::new() {
        Ok(manager) => manager,
        Err(e) => {
            eprintln!("Failed to initialize terminal: {e}");
            return;
        }
    };

    // Start data collection in background
    let data_collector = DataCollector::new(Arc::clone(&app_state));
    let args_clone = args.clone();
    tokio::spawn(async move {
        let hosts = args_clone.hosts.clone().unwrap_or_default();
        let hostfile = args_clone.hostfile.clone();

        if hosts.is_empty() && hostfile.is_none() {
            // Local mode
            data_collector.run_local_mode(args_clone).await;
        } else {
            // Remote mode
            data_collector
                .run_remote_mode(args_clone, hosts, hostfile)
                .await;
        }
    });

    // Run UI loop
    let mut ui_loop = match UiLoop::new(app_state) {
        Ok(ui_loop) => ui_loop,
        Err(e) => {
            eprintln!("Failed to initialize UI: {e}");
            return;
        }
    };

    if let Err(e) = ui_loop.run(args).await {
        eprintln!("UI loop error: {e}");
    }

    // Terminal cleanup is handled by TerminalManager's Drop trait
}
