#[cfg(test)]
mod tests {
    use super::super::memory_linux::*;
    use crate::device::MemoryReader;

    #[test]
    fn test_memory_reader_creation() {
        let reader = LinuxMemoryReader::new();
        // Reader should be created successfully
        // Container info is detected during creation
        if reader.container_info.is_container {
            println!("Created memory reader in container environment");
        } else {
            println!("Created memory reader in non-container environment");
        }
    }

    #[test]
    fn test_memory_info_retrieval() {
        let reader = LinuxMemoryReader::new();
        let memory_infos = reader.get_memory_info();

        assert!(!memory_infos.is_empty());

        let memory_info = &memory_infos[0];

        // Basic sanity checks
        assert!(memory_info.total_bytes > 0);
        assert!(memory_info.utilization >= 0.0 && memory_info.utilization <= 100.0);

        if reader.container_info.is_container {
            println!("Container memory:");
            println!("  Total: {} MB", memory_info.total_bytes / 1024 / 1024);
            println!("  Used: {} MB", memory_info.used_bytes / 1024 / 1024);
            println!("  Utilization: {:.2}%", memory_info.utilization);

            // In container, total should match container limit if available
            if let Some(limit) = reader.container_info.memory_limit_bytes {
                assert_eq!(memory_info.total_bytes, limit);
            }
        } else {
            println!("System memory:");
            println!("  Total: {} MB", memory_info.total_bytes / 1024 / 1024);
            println!("  Used: {} MB", memory_info.used_bytes / 1024 / 1024);
            println!(
                "  Available: {} MB",
                memory_info.available_bytes / 1024 / 1024
            );
            println!("  Cached: {} MB", memory_info.cached_bytes / 1024 / 1024);
            println!("  Utilization: {:.2}%", memory_info.utilization);
        }
    }

    #[test]
    fn test_container_memory_simulation() {
        // Create a reader with mock container info
        let mut reader = LinuxMemoryReader::new();

        // Simulate container environment
        reader.container_info = crate::device::container_info::ContainerInfo {
            is_container: true,
            cpu_quota: None,
            cpu_period: None,
            cpu_shares: None,
            cpuset_cpus: None,
            effective_cpu_count: 2.0,
            memory_limit_bytes: Some(4 * 1024 * 1024 * 1024), // 4GB limit
            memory_soft_limit_bytes: None,
            memory_swap_limit_bytes: None,
            memory_usage_bytes: Some(2 * 1024 * 1024 * 1024), // 2GB used
        };

        let memory_infos = reader.get_memory_info();
        assert!(!memory_infos.is_empty());

        let memory_info = &memory_infos[0];

        // Verify container-aware values
        assert_eq!(memory_info.total_bytes, 4 * 1024 * 1024 * 1024);
        assert_eq!(memory_info.used_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(memory_info.utilization, 50.0);
    }
}
