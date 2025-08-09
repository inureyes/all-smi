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

use tokio::sync::Mutex;

use crate::app_state::AppState;
use crate::cli::{LocalArgs, ViewArgs};
use crate::view::{
    data_collector::DataCollector, terminal_manager::TerminalManager, ui_loop::UiLoop,
};

pub async fn run_local_mode(args: &LocalArgs) {
    let mut startup_profiler = crate::utils::StartupProfiler::new();
    startup_profiler.checkpoint("Starting run_local_mode");

    // Initialize application state for local mode
    let mut initial_state = AppState::new();
    initial_state.is_local_mode = true;
    let app_state = Arc::new(Mutex::new(initial_state));
    startup_profiler.checkpoint("AppState initialized");

    // Initialize terminal
    let _terminal_manager = match TerminalManager::new() {
        Ok(manager) => manager,
        Err(e) => {
            eprintln!("Failed to initialize terminal: {e}");
            return;
        }
    };
    startup_profiler.checkpoint("Terminal initialized");

    // Start data collection in background
    let data_collector = DataCollector::new(Arc::clone(&app_state));
    let view_args = ViewArgs {
        hosts: None,
        hostfile: None,
        interval: args.interval,
    };
    tokio::spawn(async move {
        data_collector.run_local_mode(view_args).await;
    });
    startup_profiler.checkpoint("Data collector spawned");

    // Run UI loop
    let mut ui_loop = match UiLoop::new(app_state) {
        Ok(ui_loop) => ui_loop,
        Err(e) => {
            eprintln!("Failed to initialize UI: {e}");
            return;
        }
    };
    startup_profiler.checkpoint("UI loop initialized");
    startup_profiler.finish();

    // Create ViewArgs again for UI loop
    let view_args = ViewArgs {
        hosts: None,
        hostfile: None,
        interval: args.interval,
    };
    if let Err(e) = ui_loop.run(&view_args).await {
        eprintln!("UI loop error: {e}");
    }

    // Terminal cleanup is handled by TerminalManager's Drop trait
}

pub async fn run_view_mode(args: &ViewArgs) {
    // Initialize application state for remote mode
    let mut initial_state = AppState::new();
    initial_state.is_local_mode = false;
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

        // Remote mode
        data_collector
            .run_remote_mode(args_clone, hosts, hostfile)
            .await;
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
