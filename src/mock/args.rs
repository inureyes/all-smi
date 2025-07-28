//! Command-line argument parsing for the mock server

use crate::mock::constants::DEFAULT_GPU_NAME;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about = "High-performance GPU metrics mock server", long_about = None)]
pub struct Args {
    #[arg(long, help = "Port range, e.g., 10001-10010 or 10001")]
    pub port_range: Option<String>,

    #[arg(long, default_value = DEFAULT_GPU_NAME, help = "GPU name")]
    pub gpu_name: String,

    #[arg(
        long,
        default_value = "nvidia",
        help = "Platform type: nvidia, apple, jetson, intel, amd, tenstorrent, rebellions, furiosa"
    )]
    pub platform: String,

    #[arg(
        short,
        long,
        default_value = "hosts.csv",
        help = "Output CSV file name"
    )]
    pub o: String,

    #[arg(
        long,
        default_value_t = 0,
        help = "Number of nodes to simulate random failures (0 = no failures)"
    )]
    pub failure_nodes: u32,

    #[arg(
        long,
        default_value_t = 1,
        help = "Starting index for node naming (e.g., 51 for node-0051)"
    )]
    pub start_index: u32,
}
