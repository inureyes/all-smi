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

mod api;
mod app_state;
mod cli;
mod common;
mod device;
mod metrics;
mod network;
mod storage;
mod ui;
mod utils;
mod view;

use api::run_api_mode;
use clap::Parser;
use cli::{Cli, Commands, LocalArgs};
use tokio::signal;
use utils::{ensure_sudo_permissions, ensure_sudo_permissions_with_fallback, RuntimeEnvironment};

#[cfg(target_os = "macos")]
use device::is_apple_silicon;
#[cfg(target_os = "macos")]
use device::powermetrics_manager::{
    initialize_powermetrics_manager, shutdown_powermetrics_manager,
};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
static POWERMETRICS_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() {
    // Set up panic handler for cleanup
    #[cfg(target_os = "macos")]
    setup_panic_handler();

    let cli = Cli::parse();

    // Set up signal handler for clean shutdown
    tokio::spawn(async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        #[cfg(target_os = "macos")]
        if POWERMETRICS_INITIALIZED.load(Ordering::Relaxed) {
            shutdown_powermetrics_manager();
        }
        std::process::exit(0);
    });

    // Also handle SIGTERM on Unix systems
    #[cfg(unix)]
    tokio::spawn(async {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to listen for SIGTERM");
        sigterm.recv().await;
        #[cfg(target_os = "macos")]
        if POWERMETRICS_INITIALIZED.load(Ordering::Relaxed) {
            shutdown_powermetrics_manager();
        }
        std::process::exit(0);
    });

    match cli.command {
        Some(Commands::Api(args)) => {
            ensure_sudo_permissions();

            // Initialize PowerMetricsManager after getting sudo
            #[cfg(target_os = "macos")]
            if is_apple_silicon() {
                if let Err(e) = initialize_powermetrics_manager(args.interval) {
                    eprintln!("Warning: Failed to initialize PowerMetricsManager: {e}");
                } else {
                    POWERMETRICS_INITIALIZED.store(true, Ordering::Relaxed);
                }
            }

            run_api_mode(&args).await;
        }
        Some(Commands::Local(args)) => {
            ensure_sudo_permissions();

            // Initialize PowerMetricsManager after getting sudo
            #[cfg(target_os = "macos")]
            if is_apple_silicon() {
                // Use specified interval or default to 1 second for local mode
                let interval = args.interval.unwrap_or(1);
                if let Err(e) = initialize_powermetrics_manager(interval) {
                    eprintln!("Warning: Failed to initialize PowerMetricsManager: {e}");
                } else {
                    POWERMETRICS_INITIALIZED.store(true, Ordering::Relaxed);
                }
            }

            view::run_local_mode(&args).await;
        }
        Some(Commands::View(mut args)) => {
            // Remote mode - no sudo required

            // Check if we're in Backend.AI environment and no hosts/hostfile provided
            if args.hosts.is_none() && args.hostfile.is_none() {
                let runtime_env = RuntimeEnvironment::detect();

                if let Some(backend_ai_hosts) = runtime_env.get_backend_ai_hosts() {
                    eprintln!("Detected Backend.AI environment");
                    eprintln!("Auto-discovered cluster hosts from BACKENDAI_CLUSTER_HOSTS:");
                    for host in &backend_ai_hosts {
                        eprintln!("  - {host}");
                    }
                    args.hosts = Some(backend_ai_hosts);
                } else {
                    eprintln!("Error: Remote view mode requires --hosts or --hostfile");
                    eprintln!(
                        "Usage: all-smi view --hosts <URL>... or all-smi view --hostfile <FILE>"
                    );
                    if runtime_env.is_backend_ai() {
                        eprintln!("\nBackend.AI environment detected but BACKENDAI_CLUSTER_HOSTS is not set.");
                        eprintln!("Set the environment variable with comma-separated host names:");
                        eprintln!("  export BACKENDAI_CLUSTER_HOSTS=\"host1,host2\"");
                    }
                    eprintln!("\nFor local monitoring, use: all-smi local");
                    std::process::exit(1);
                }
            }
            view::run_view_mode(&args).await;
        }
        None => {
            // Default to local mode when no command is specified
            let has_sudo = ensure_sudo_permissions_with_fallback();
            if has_sudo {
                // Initialize PowerMetricsManager after getting sudo
                #[cfg(target_os = "macos")]
                if is_apple_silicon() {
                    // Default to 1 second for local mode
                    if let Err(e) = initialize_powermetrics_manager(1) {
                        eprintln!("Warning: Failed to initialize PowerMetricsManager: {e}");
                    } else {
                        POWERMETRICS_INITIALIZED.store(true, Ordering::Relaxed);
                    }
                }

                view::run_local_mode(&LocalArgs { interval: None }).await;
            }
            // If user declined sudo and chose remote monitoring,
            // they were given instructions and the function exits
        }
    }

    // Cleanup PowerMetricsManager on exit
    #[cfg(target_os = "macos")]
    if POWERMETRICS_INITIALIZED.load(Ordering::Relaxed) {
        shutdown_powermetrics_manager();
    }
}

// Set up a panic handler to ensure cleanup
#[cfg(target_os = "macos")]
fn setup_panic_handler() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Clean up PowerMetricsManager before panicking
        if POWERMETRICS_INITIALIZED.load(Ordering::Relaxed) {
            device::powermetrics_manager::shutdown_powermetrics_manager();
        }
        default_panic(panic_info);
    }));
}
