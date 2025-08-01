use crate::device::{
    furiosa, nvidia, nvidia_jetson,
    platform_detection::{
        get_os_type, has_furiosa, has_nvidia, has_rebellions, has_tenstorrent, is_jetson,
    },
    rebellions, tenstorrent,
    traits::{CpuReader, GpuReader, MemoryReader},
};

#[cfg(target_os = "macos")]
use crate::device::{apple_silicon, cpu_macos, memory_macos, platform_detection::is_apple_silicon};

#[cfg(target_os = "linux")]
use crate::device::{cpu_linux, memory_linux};

pub fn get_gpu_readers() -> Vec<Box<dyn GpuReader>> {
    let mut readers: Vec<Box<dyn GpuReader>> = Vec::new();
    let os_type = get_os_type();

    match os_type {
        "linux" => {
            // Only create NVIDIA reader if we actually have NVIDIA GPUs
            if is_jetson() && has_nvidia() {
                readers.push(Box::new(nvidia_jetson::NvidiaJetsonGpuReader {}));
            } else if has_nvidia() && !is_jetson() {
                readers.push(Box::new(nvidia::NvidiaGpuReader {}));
            }

            // Check for Furiosa NPU support
            if has_furiosa() {
                readers.push(Box::new(furiosa::FuriosaReader::new()));
            }

            // Check for Tenstorrent NPU support
            if has_tenstorrent() {
                readers.push(Box::new(tenstorrent::TenstorrentReader::new()));
            }

            // Check for Rebellions NPU support
            if has_rebellions() {
                readers.push(Box::new(rebellions::RebellionsReader::new()));
            }
        }
        "macos" =>
        {
            #[cfg(target_os = "macos")]
            if is_apple_silicon() {
                readers.push(Box::new(apple_silicon::AppleSiliconGpuReader::new()));
            }
        }
        _ => println!("Unsupported OS type: {os_type}"),
    }

    readers
}

pub fn get_cpu_readers() -> Vec<Box<dyn CpuReader>> {
    let mut readers: Vec<Box<dyn CpuReader>> = Vec::new();
    let os_type = get_os_type();

    match os_type {
        "linux" => {
            #[cfg(target_os = "linux")]
            readers.push(Box::new(cpu_linux::LinuxCpuReader::new()));
        }
        "macos" => {
            #[cfg(target_os = "macos")]
            readers.push(Box::new(cpu_macos::MacOsCpuReader::new()));
        }
        _ => println!("CPU monitoring not supported for OS type: {os_type}"),
    }

    readers
}

pub fn get_memory_readers() -> Vec<Box<dyn MemoryReader>> {
    let mut readers: Vec<Box<dyn MemoryReader>> = Vec::new();
    let os_type = get_os_type();

    match os_type {
        "linux" => {
            #[cfg(target_os = "linux")]
            readers.push(Box::new(memory_linux::LinuxMemoryReader::new()));
        }
        "macos" => {
            #[cfg(target_os = "macos")]
            readers.push(Box::new(memory_macos::MacOsMemoryReader::new()));
        }
        _ => println!("Memory monitoring not supported for OS type: {os_type}"),
    }

    readers
}
