// Integration tests for container-aware functionality
// These tests verify the public API behavior when running in containerized environments

#[cfg(all(test, target_os = "linux"))]
mod container_integration_tests {
    use all_smi::device::cpu_linux::LinuxCpuReader;
    use all_smi::device::memory_linux::LinuxMemoryReader;
    use all_smi::device::{CpuReader, MemoryReader};

    #[test]
    fn test_cpu_reader_in_host_environment() {
        // Test that CPU reader works correctly in non-container environment
        let reader = LinuxCpuReader::new();
        let cpu_infos = reader.get_cpu_info();

        // Basic sanity checks
        assert!(!cpu_infos.is_empty());

        // Calculate total cores
        let total_cores: u32 = cpu_infos.iter().map(|info| info.total_cores).sum();

        // Get overall utilization (average across all CPUs)
        let overall_utilization = if !cpu_infos.is_empty() {
            cpu_infos.iter().map(|info| info.utilization).sum::<f64>() / cpu_infos.len() as f64
        } else {
            0.0
        };

        assert!(total_cores > 0);
        assert!(overall_utilization >= 0.0 && overall_utilization <= 100.0);

        println!("Host CPU info:");
        println!("  Total cores: {}", total_cores);
        println!("  Utilization: {:.2}%", overall_utilization);
    }

    #[test]
    fn test_memory_reader_in_host_environment() {
        // Test that memory reader works correctly in non-container environment
        let reader = LinuxMemoryReader::new();
        let memory_infos = reader.get_memory_info();

        assert!(!memory_infos.is_empty());
        let memory_info = &memory_infos[0];

        // Basic sanity checks
        assert!(memory_info.total_bytes > 0);
        assert!(memory_info.utilization >= 0.0 && memory_info.utilization <= 100.0);

        println!("Host memory info:");
        println!("  Total: {} MB", memory_info.total_bytes / 1024 / 1024);
        println!("  Used: {} MB", memory_info.used_bytes / 1024 / 1024);
        println!("  Utilization: {:.2}%", memory_info.utilization);
    }

    // Note: Testing container-aware functionality requires actually running inside a container
    // The following test will behave differently when run inside vs outside a container
    #[test]
    fn test_container_awareness() {
        let cpu_reader = LinuxCpuReader::new();
        let memory_reader = LinuxMemoryReader::new();

        let cpu_infos = cpu_reader.get_cpu_info();
        let memory_infos = memory_reader.get_memory_info();

        // Just verify that the readers work without panicking
        // The actual values will depend on whether we're in a container
        assert!(!cpu_infos.is_empty());
        assert!(!memory_infos.is_empty());

        let total_cores: u32 = cpu_infos.iter().map(|info| info.total_cores).sum();

        println!("Environment detection test:");
        println!("  CPU cores detected: {}", total_cores);
        println!(
            "  Memory detected: {} MB",
            memory_infos[0].total_bytes / 1024 / 1024
        );
    }
}

#[cfg(not(target_os = "linux"))]
mod container_integration_tests {
    #[test]
    fn test_non_linux_platform() {
        // On non-Linux platforms, container detection is not supported
        println!("Container detection tests are only supported on Linux");
    }
}
