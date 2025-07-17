use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use std::process::Command;

pub struct FuriosaReader;

impl FuriosaReader {
    pub fn new() -> Self {
        FuriosaReader
    }

    fn parse_furiosactl_output(&self, _output: &str) -> Vec<GpuInfo> {
        // TODO: Parse furiosactl output to extract NPU information
        // This will be implemented once we know the exact output format
        vec![]
    }

    fn get_furiosa_processes(&self) -> Vec<ProcessInfo> {
        // TODO: Get processes using Furiosa NPUs
        vec![]
    }
}

impl GpuReader for FuriosaReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        // Run furiosactl to get NPU information
        match Command::new("furiosactl").arg("info").output() {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                self.parse_furiosactl_output(&output_str)
            }
            Err(_) => vec![],
        }
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.get_furiosa_processes()
    }
}
