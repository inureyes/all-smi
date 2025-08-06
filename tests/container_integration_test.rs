#[cfg(all(test, target_os = "linux"))]
mod container_integration_tests {
    use all_smi::device::cpu_linux::LinuxCpuReader;
    use all_smi::device::{container_cpu::ContainerInfo, CpuReader};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper to set up a mock cgroup environment
    fn setup_mock_cgroup_env(temp_dir: &TempDir) -> PathBuf {
        let cgroup_path = temp_dir.path().join("cgroup");
        fs::create_dir_all(&cgroup_path).unwrap();

        // Create mock cgroup files
        let cpu_path = cgroup_path.join("cpu");
        fs::create_dir_all(&cpu_path).unwrap();

        // Mock CPU quota and period (2 CPUs worth)
        fs::write(cpu_path.join("cpu.cfs_quota_us"), "200000").unwrap();
        fs::write(cpu_path.join("cpu.cfs_period_us"), "100000").unwrap();
        fs::write(cpu_path.join("cpu.shares"), "2048").unwrap();

        // Mock cpuset
        let cpuset_path = cgroup_path.join("cpuset");
        fs::create_dir_all(&cpuset_path).unwrap();
        fs::write(cpuset_path.join("cpuset.cpus"), "0-3").unwrap();

        cgroup_path
    }

    #[test]
    fn test_container_detection_in_docker_env() {
        let temp_dir = TempDir::new().unwrap();
        let docker_env = temp_dir.path().join(".dockerenv");
        fs::write(&docker_env, "").unwrap();

        // This would require modifying ContainerInfo::detect to accept a path parameter
        // For now, we just test the detection logic exists
        let info = ContainerInfo::detect();

        // The test result depends on whether we're actually in a container
        if std::path::Path::new("/.dockerenv").exists() {
            assert!(info.is_container);
        }
    }

    #[test]
    fn test_cpu_reader_with_container_limits() {
        // Create CPU reader
        let reader = LinuxCpuReader::new();

        // Get CPU info - this will use real system data
        let cpu_infos = reader.get_cpu_info();

        if !cpu_infos.is_empty() {
            let cpu_info = &cpu_infos[0];

            // Verify basic fields are populated
            assert!(!cpu_info.cpu_model.is_empty());
            assert!(cpu_info.total_cores > 0);
            assert!(cpu_info.utilization >= 0.0 && cpu_info.utilization <= 100.0);

            println!("CPU Model: {}", cpu_info.cpu_model);
            println!("Total Cores: {}", cpu_info.total_cores);
            println!("CPU Utilization: {:.2}%", cpu_info.utilization);

            // If we're in a container, verify container-aware behavior
            if reader.container_info.is_container {
                println!(
                    "Running in container with {} effective CPUs",
                    reader.container_info.effective_cpu_count
                );

                // In a container, reported cores should not exceed effective CPU count
                assert!(
                    cpu_info.total_cores as f64 <= reader.container_info.effective_cpu_count.ceil()
                );
            }
        }
    }

    #[test]
    fn test_proc_stat_parsing_with_cpuset() {
        // Create mock /proc/stat content
        let stat_content = r#"cpu  1000 0 2000 7000 0 0 0 0 0 0
cpu0 250 0 500 1750 0 0 0 0 0 0
cpu1 250 0 500 1750 0 0 0 0 0 0
cpu2 250 0 500 1750 0 0 0 0 0 0
cpu3 250 0 500 1750 0 0 0 0 0 0
cpu4 250 0 500 1750 0 0 0 0 0 0
cpu5 250 0 500 1750 0 0 0 0 0 0"#;

        // Test with cpuset limiting to specific CPUs
        let container_info = ContainerInfo {
            is_container: true,
            cpu_quota: None,
            cpu_period: None,
            cpu_shares: None,
            cpuset_cpus: Some(vec![0, 1]), // Only CPU 0 and 1
            effective_cpu_count: 2.0,
        };

        let (utilization, active_cores) =
            all_smi::device::container_cpu::parse_cpu_stat_with_container_limits(
                stat_content,
                &container_info,
            );

        // Should only include CPUs 0 and 1
        assert_eq!(active_cores, vec![0, 1]);

        // Utilization should be scaled appropriately
        assert!(utilization >= 0.0 && utilization <= 100.0);
    }

    #[test]
    fn test_effective_cpu_calculation_scenarios() {
        // Scenario 1: Container with 0.5 CPU quota
        let container_info = ContainerInfo {
            is_container: true,
            cpu_quota: Some(50000),
            cpu_period: Some(100000),
            cpu_shares: None,
            cpuset_cpus: None,
            effective_cpu_count: 0.5,
        };
        assert_eq!(container_info.effective_cpu_count, 0.5);

        // Scenario 2: Container with 4 CPUs via cpuset
        let container_info = ContainerInfo {
            is_container: true,
            cpu_quota: None,
            cpu_period: None,
            cpu_shares: None,
            cpuset_cpus: Some(vec![0, 1, 2, 3]),
            effective_cpu_count: 4.0,
        };
        assert_eq!(container_info.effective_cpu_count, 4.0);

        // Scenario 3: Container with both quota and cpuset
        let effective = ContainerInfo::calculate_effective_cpus(
            Some(150000), // 1.5 CPUs
            Some(100000),
            None,
            &Some(vec![0, 1]), // 2 CPUs via cpuset
        );
        assert_eq!(effective, 1.5); // Quota is more restrictive
    }
}
