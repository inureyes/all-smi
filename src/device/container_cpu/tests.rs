#[cfg(test)]
mod tests {
    use super::super::container_cpu::*;

    #[test]
    fn test_parse_cpuset_range() {
        // Test single CPU
        let result = ContainerInfo::parse_cpuset_range("0");
        assert_eq!(result, Some(vec![0]));

        // Test CPU range
        let result = ContainerInfo::parse_cpuset_range("0-3");
        assert_eq!(result, Some(vec![0, 1, 2, 3]));

        // Test multiple CPUs
        let result = ContainerInfo::parse_cpuset_range("0,2,4");
        assert_eq!(result, Some(vec![0, 2, 4]));

        // Test mixed range and individual CPUs
        let result = ContainerInfo::parse_cpuset_range("0-2,5,7-8");
        assert_eq!(result, Some(vec![0, 1, 2, 5, 7, 8]));

        // Test empty string
        let result = ContainerInfo::parse_cpuset_range("");
        assert_eq!(result, None);

        // Test invalid input
        let result = ContainerInfo::parse_cpuset_range("invalid");
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_effective_cpus() {
        // Test with no limits
        let effective = ContainerInfo::calculate_effective_cpus(None, None, None, &None);
        assert!(effective > 0.0); // Should be system CPU count

        // Test with quota limit (2 CPUs worth)
        let effective = ContainerInfo::calculate_effective_cpus(
            Some(200000), // 200ms quota
            Some(100000), // 100ms period
            None,
            &None,
        );
        assert_eq!(effective, 2.0);

        // Test with quota limit (0.5 CPUs)
        let effective = ContainerInfo::calculate_effective_cpus(
            Some(50000),  // 50ms quota
            Some(100000), // 100ms period
            None,
            &None,
        );
        assert_eq!(effective, 0.5);

        // Test with cpuset limit
        let cpuset = Some(vec![0, 1, 2, 3]);
        let effective = ContainerInfo::calculate_effective_cpus(None, None, None, &cpuset);
        assert_eq!(effective, 4.0);

        // Test with both quota and cpuset (quota more restrictive)
        let cpuset = Some(vec![0, 1, 2, 3]);
        let effective = ContainerInfo::calculate_effective_cpus(
            Some(150000), // 1.5 CPUs
            Some(100000),
            None,
            &cpuset,
        );
        assert_eq!(effective, 1.5);

        // Test with both quota and cpuset (cpuset more restrictive)
        let cpuset = Some(vec![0, 1]);
        let effective = ContainerInfo::calculate_effective_cpus(
            Some(300000), // 3 CPUs
            Some(100000),
            None,
            &cpuset,
        );
        assert_eq!(effective, 2.0);
    }

    #[test]
    fn test_parse_cpu_stat_with_container_limits() {
        let stat_content = r#"cpu  1000 0 2000 7000 0 0 0 0 0 0
cpu0 250 0 500 1750 0 0 0 0 0 0
cpu1 250 0 500 1750 0 0 0 0 0 0
cpu2 250 0 500 1750 0 0 0 0 0 0
cpu3 250 0 500 1750 0 0 0 0 0 0"#;

        // Test without container limits
        let container_info = ContainerInfo {
            is_container: false,
            cpu_quota: None,
            cpu_period: None,
            cpu_shares: None,
            cpuset_cpus: None,
            effective_cpu_count: 4.0,
        };

        let (utilization, active_cores) =
            parse_cpu_stat_with_container_limits(stat_content, &container_info);
        assert_eq!(utilization, 30.0); // (1000 + 2000) / 10000 * 100
        assert_eq!(active_cores.len(), 4);

        // Test with cpuset limiting to cpu0 and cpu1
        let container_info = ContainerInfo {
            is_container: true,
            cpu_quota: None,
            cpu_period: None,
            cpu_shares: None,
            cpuset_cpus: Some(vec![0, 1]),
            effective_cpu_count: 2.0,
        };

        let (utilization, active_cores) =
            parse_cpu_stat_with_container_limits(stat_content, &container_info);
        assert_eq!(utilization, 30.0); // Same calculation but scaled by effective CPU count
        assert_eq!(active_cores, vec![0, 1]);

        // Test with quota limiting to 0.5 CPUs
        let container_info = ContainerInfo {
            is_container: true,
            cpu_quota: Some(50000),
            cpu_period: Some(100000),
            cpu_shares: None,
            cpuset_cpus: None,
            effective_cpu_count: 0.5,
        };

        let (utilization, active_cores) =
            parse_cpu_stat_with_container_limits(stat_content, &container_info);
        // Utilization should be scaled: 30.0 * (0.5 / 4.0) = 3.75
        assert!((utilization - 3.75).abs() < 0.01);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_container_detection() {
        // This test will pass/fail depending on whether it's run in a container
        let info = ContainerInfo::detect();

        // Just verify the struct is created properly
        assert!(info.effective_cpu_count > 0.0);

        if info.is_container {
            println!(
                "Running in container with {} effective CPUs",
                info.effective_cpu_count
            );
            if let Some(quota) = info.cpu_quota {
                println!("CPU quota: {}", quota);
            }
            if let Some(period) = info.cpu_period {
                println!("CPU period: {}", period);
            }
            if let Some(cpuset) = &info.cpuset_cpus {
                println!("CPUSet: {:?}", cpuset);
            }
        } else {
            println!("Not running in a container");
        }
    }
}
