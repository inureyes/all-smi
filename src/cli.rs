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

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run in API mode, exposing metrics in Prometheus format.
    Api(ApiArgs),
    /// Run in local mode, monitoring local GPUs/NPUs. (default)
    Local(LocalArgs),
    /// Run in remote view mode, monitoring remote nodes via API endpoints.
    View(ViewArgs),
}

#[derive(Parser)]
pub struct ApiArgs {
    /// The port to listen on for the API server. Use 0 to disable TCP listener.
    #[arg(short, long, default_value_t = 9090)]
    pub port: u16,
    /// The interval in seconds at which to update the GPU information.
    #[arg(short, long, default_value_t = 3)]
    pub interval: u64,
    /// Include the process list in the API output.
    #[arg(long)]
    pub processes: bool,
    /// Unix domain socket path for local IPC (Unix only).
    /// When specified without a value, uses platform default:
    /// - Linux: /var/run/all-smi.sock (fallback to /tmp/all-smi.sock if no permission)
    /// - macOS: /tmp/all-smi.sock
    #[cfg(unix)]
    #[arg(short, long, num_args = 0..=1, default_missing_value = "")]
    pub socket: Option<String>,
}

#[derive(Parser, Clone)]
pub struct LocalArgs {
    /// The interval in seconds at which to update the GPU information.
    #[arg(short, long)]
    pub interval: Option<u64>,
}

#[derive(Parser, Clone)]
pub struct ViewArgs {
    /// A list of host addresses to connect to for remote monitoring.
    #[arg(long, num_args = 1..)]
    pub hosts: Option<Vec<String>>,
    /// A file containing a list of host addresses to connect to for remote monitoring.
    #[arg(long)]
    pub hostfile: Option<String>,
    /// The interval in seconds at which to update the GPU information. If not specified, uses adaptive interval based on node count.
    #[arg(short, long)]
    pub interval: Option<u64>,
}
