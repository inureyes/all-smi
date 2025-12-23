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
#[macro_use]
mod parsing;
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
use utils::{
    ensure_sudo_permissions, ensure_sudo_permissions_for_api,
    ensure_sudo_permissions_with_fallback, RuntimeEnvironment,
};

#[cfg(target_os = "macos")]
use device::is_apple_silicon;

// Use native macOS APIs when the feature is enabled (no sudo required)
#[cfg(all(target_os = "macos", feature = "native-macos"))]
use device::macos_native::{
    get_native_metrics_manager, initialize_native_metrics_manager, shutdown_native_metrics_manager,
};

// Use powermetrics when native-macos feature is NOT enabled (requires sudo)
#[cfg(all(target_os = "macos", not(feature = "native-macos")))]
use device::powermetrics::{initialize_powermetrics_manager, shutdown_powermetrics_manager};

#[cfg(target_os = "linux")]
use device::hlsmi::{initialize_hlsmi_manager, shutdown_hlsmi_manager};
#[cfg(target_os = "linux")]
use device::platform_detection::has_gaudi;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::atomic::AtomicBool;
#[cfg(all(target_os = "macos", not(feature = "native-macos")))]
use std::sync::atomic::Ordering;

#[cfg(all(target_os = "macos", not(feature = "native-macos")))]
static POWERMETRICS_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[cfg(all(target_os = "macos", feature = "native-macos"))]
static NATIVE_METRICS_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "linux")]
static HLSMI_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // Set up panic handler for cleanup
    #[cfg(target_os = "macos")]
    setup_panic_handler();

    let cli = Cli::parse();

    // Set up signal handler for clean shutdown
    tokio::spawn(async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        #[cfg(all(target_os = "macos", feature = "native-macos"))]
        {
            // Cleanup native metrics manager on signal
            shutdown_native_metrics_manager();
        }
        #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
        {
            // Cleanup powermetrics on signal
            shutdown_powermetrics_manager();
        }
        #[cfg(target_os = "linux")]
        {
            // Always cleanup hlsmi on signal
            shutdown_hlsmi_manager();
        }
        std::process::exit(0);
    });

    // Also handle SIGTERM on Unix systems
    #[cfg(unix)]
    tokio::spawn(async {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to listen for SIGTERM");
        sigterm.recv().await;
        #[cfg(all(target_os = "macos", feature = "native-macos"))]
        {
            // Cleanup native metrics manager on signal
            shutdown_native_metrics_manager();
        }
        #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
        {
            // Cleanup powermetrics on signal
            shutdown_powermetrics_manager();
        }
        #[cfg(target_os = "linux")]
        {
            // Always cleanup hlsmi on signal
            shutdown_hlsmi_manager();
        }
        std::process::exit(0);
    });

    match cli.command {
        Some(Commands::Api(args)) => {
            // When using native macOS APIs, no sudo is needed
            #[cfg(all(target_os = "macos", feature = "native-macos"))]
            let _ = ensure_sudo_permissions_for_api(); // Just for any other checks

            #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
            let has_sudo = ensure_sudo_permissions_for_api();

            #[cfg(not(target_os = "macos"))]
            let _has_sudo = ensure_sudo_permissions_for_api();

            // Initialize native metrics manager (no sudo required)
            #[cfg(all(target_os = "macos", feature = "native-macos"))]
            if is_apple_silicon() {
                if let Err(e) = initialize_native_metrics_manager(args.interval as u64 * 1000) {
                    eprintln!("Warning: Failed to initialize native metrics manager: {e}");
                } else {
                    use std::sync::atomic::Ordering;
                    NATIVE_METRICS_INITIALIZED.store(true, Ordering::Relaxed);
                }
            }

            // Initialize PowerMetricsManager if we have sudo (legacy mode)
            #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
            if has_sudo && is_apple_silicon() {
                if let Err(e) = initialize_powermetrics_manager(args.interval) {
                    eprintln!("Warning: Failed to initialize PowerMetricsManager: {e}");
                } else {
                    POWERMETRICS_INITIALIZED.store(true, Ordering::Relaxed);
                }
            }

            // Initialize hlsmi manager for Intel Gaudi on Linux
            #[cfg(target_os = "linux")]
            if has_gaudi() {
                if let Err(e) = initialize_hlsmi_manager(args.interval) {
                    eprintln!("Warning: Failed to initialize hlsmi manager: {e}");
                } else {
                    use std::sync::atomic::Ordering;
                    HLSMI_INITIALIZED.store(true, Ordering::Relaxed);
                }
            }

            run_api_mode(&args).await;
        }
        Some(Commands::Local(args)) => {
            // Only require sudo when NOT using native macOS APIs
            #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
            ensure_sudo_permissions();

            #[cfg(not(target_os = "macos"))]
            ensure_sudo_permissions();

            // Initialize native metrics manager (no sudo required)
            #[cfg(all(target_os = "macos", feature = "native-macos"))]
            if is_apple_silicon() {
                let interval = args.interval.unwrap_or(2);
                if let Err(e) = initialize_native_metrics_manager(interval as u64 * 1000) {
                    eprintln!("Warning: Failed to initialize native metrics manager: {e}");
                } else {
                    use std::sync::atomic::Ordering;
                    NATIVE_METRICS_INITIALIZED.store(true, Ordering::Relaxed);
                }
            }

            // Initialize PowerMetricsManager in background after getting sudo (legacy mode)
            #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
            if is_apple_silicon() {
                // Use specified interval or default to 2 seconds for local mode
                let interval = args.interval.unwrap_or(2);
                // Spawn initialization in background to avoid blocking startup
                std::thread::spawn(move || {
                    if let Err(e) = initialize_powermetrics_manager(interval) {
                        eprintln!("Warning: Failed to initialize PowerMetricsManager: {e}");
                    } else {
                        POWERMETRICS_INITIALIZED.store(true, Ordering::Relaxed);
                    }
                });
            }

            // Initialize hlsmi manager for Intel Gaudi on Linux
            #[cfg(target_os = "linux")]
            if has_gaudi() {
                let interval = args.interval.unwrap_or(2);
                std::thread::spawn(move || {
                    if let Err(e) = initialize_hlsmi_manager(interval) {
                        eprintln!("Warning: Failed to initialize hlsmi manager: {e}");
                    } else {
                        use std::sync::atomic::Ordering;
                        HLSMI_INITIALIZED.store(true, Ordering::Relaxed);
                    }
                });
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

            // Cleanup after view mode exits
            #[cfg(all(target_os = "macos", feature = "native-macos"))]
            {
                // Cleanup native metrics manager
                shutdown_native_metrics_manager();
            }
            #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
            {
                // Cleanup powermetrics
                shutdown_powermetrics_manager();
            }
            #[cfg(target_os = "linux")]
            {
                // Always try to shutdown hlsmi, even if not fully initialized
                shutdown_hlsmi_manager();
            }
        }
        None => {
            // Default to local mode when no command is specified
            // With native macOS feature, sudo is not needed
            #[cfg(all(target_os = "macos", feature = "native-macos"))]
            let has_sudo = true; // Always proceed, no sudo needed

            #[cfg(not(all(target_os = "macos", feature = "native-macos")))]
            let has_sudo = ensure_sudo_permissions_with_fallback();

            if has_sudo {
                // Initialize native metrics manager (no sudo required)
                #[cfg(all(target_os = "macos", feature = "native-macos"))]
                if is_apple_silicon() {
                    if let Err(e) = initialize_native_metrics_manager(2000) {
                        eprintln!("Warning: Failed to initialize native metrics manager: {e}");
                    } else {
                        use std::sync::atomic::Ordering;
                        NATIVE_METRICS_INITIALIZED.store(true, Ordering::Relaxed);
                    }
                }

                // Initialize PowerMetricsManager in background after getting sudo (legacy mode)
                #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
                if is_apple_silicon() {
                    // Default to 2 seconds for local mode
                    std::thread::spawn(|| {
                        if let Err(e) = initialize_powermetrics_manager(2) {
                            eprintln!("Warning: Failed to initialize PowerMetricsManager: {e}");
                        } else {
                            POWERMETRICS_INITIALIZED.store(true, Ordering::Relaxed);
                        }
                    });
                }

                // Initialize hlsmi manager for Intel Gaudi on Linux
                #[cfg(target_os = "linux")]
                if has_gaudi() {
                    std::thread::spawn(|| {
                        if let Err(e) = initialize_hlsmi_manager(2) {
                            eprintln!("Warning: Failed to initialize hlsmi manager: {e}");
                        } else {
                            use std::sync::atomic::Ordering;
                            HLSMI_INITIALIZED.store(true, Ordering::Relaxed);
                        }
                    });
                }

                view::run_local_mode(&LocalArgs { interval: None }).await;

                // Cleanup after local mode exits
                #[cfg(all(target_os = "macos", feature = "native-macos"))]
                {
                    // Cleanup native metrics manager
                    shutdown_native_metrics_manager();
                }
                #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
                {
                    // Cleanup powermetrics
                    shutdown_powermetrics_manager();
                }
                #[cfg(target_os = "linux")]
                {
                    // Always try to shutdown hlsmi, even if not fully initialized
                    shutdown_hlsmi_manager();
                }
            }
            // If user declined sudo and chose remote monitoring,
            // they were given instructions and the function exits
        }
    }

    // Final cleanup - ensure all managers are terminated
    #[cfg(all(target_os = "macos", feature = "native-macos"))]
    {
        shutdown_native_metrics_manager();
    }
    #[cfg(all(target_os = "macos", not(feature = "native-macos")))]
    {
        shutdown_powermetrics_manager();
    }
    #[cfg(target_os = "linux")]
    {
        shutdown_hlsmi_manager();
    }
}

// Set up a panic handler to ensure cleanup
#[cfg(all(target_os = "macos", feature = "native-macos"))]
fn setup_panic_handler() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Cleanup native metrics manager before panicking
        device::macos_native::shutdown_native_metrics_manager();
        default_panic(panic_info);
    }));
}

#[cfg(all(target_os = "macos", not(feature = "native-macos")))]
fn setup_panic_handler() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Always clean up PowerMetrics processes before panicking
        device::powermetrics::shutdown_powermetrics_manager();
        default_panic(panic_info);
    }));
}
