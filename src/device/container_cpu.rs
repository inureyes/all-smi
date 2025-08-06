#[cfg(target_os = "linux")]
use cgroups_rs::{cpu::CpuController, cpuset::CpuSetController, Cgroup, CgroupPid};
use std::fs;
use std::path::Path;

pub struct ContainerInfo {
    pub is_container: bool,
    pub cpu_quota: Option<i64>,
    pub cpu_period: Option<u64>,
    pub cpu_shares: Option<u64>,
    pub cpuset_cpus: Option<Vec<u32>>,
    pub effective_cpu_count: f64,
}

impl ContainerInfo {
    pub fn detect() -> Self {
        // Check if we're running in a container by examining /proc/self/cgroup
        let is_container = Self::is_in_container();

        if !is_container {
            return ContainerInfo {
                is_container: false,
                cpu_quota: None,
                cpu_period: None,
                cpu_shares: None,
                cpuset_cpus: None,
                effective_cpu_count: num_cpus::get() as f64,
            };
        }

        // Try to get CPU limits from cgroups
        let (cpu_quota, cpu_period, cpu_shares) = Self::get_cpu_limits();
        let cpuset_cpus = Self::get_cpuset_cpus();

        // Calculate effective CPU count based on quota and period
        let effective_cpu_count =
            Self::calculate_effective_cpus(cpu_quota, cpu_period, cpu_shares, &cpuset_cpus);

        ContainerInfo {
            is_container,
            cpu_quota,
            cpu_period,
            cpu_shares,
            cpuset_cpus,
            effective_cpu_count,
        }
    }

    fn is_in_container() -> bool {
        // Check common container indicators
        if Path::new("/.dockerenv").exists() {
            return true;
        }

        // Check /proc/self/cgroup for container indicators
        if let Ok(cgroup_content) = fs::read_to_string("/proc/self/cgroup") {
            for line in cgroup_content.lines() {
                if line.contains("/docker/")
                    || line.contains("/lxc/")
                    || line.contains("/kubepods/")
                    || line.contains("/containerd/")
                    || line.contains("/podman/")
                {
                    return true;
                }
            }
        }

        false
    }

    fn get_cpu_limits() -> (Option<i64>, Option<u64>, Option<u64>) {
        let mut cpu_quota = None;
        let mut cpu_period = None;
        let mut cpu_shares = None;

        // Try cgroups v2 first
        if let Ok(content) = fs::read_to_string("/sys/fs/cgroup/cpu.max") {
            let parts: Vec<&str> = content.trim().split_whitespace().collect();
            if parts.len() == 2 {
                if parts[0] != "max" {
                    cpu_quota = parts[0].parse::<i64>().ok();
                }
                cpu_period = parts[1].parse::<u64>().ok();
            }
        }

        // Try cgroups v1
        if cpu_quota.is_none() {
            if let Ok(content) = fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us") {
                cpu_quota = content.trim().parse::<i64>().ok();
            }
        }

        if cpu_period.is_none() {
            if let Ok(content) = fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us") {
                cpu_period = content.trim().parse::<u64>().ok();
            }
        }

        // Get CPU shares
        if let Ok(content) = fs::read_to_string("/sys/fs/cgroup/cpu.weight") {
            // cgroups v2 uses weight (1-10000), convert to shares
            if let Ok(weight) = content.trim().parse::<u64>() {
                cpu_shares = Some((weight * 1024) / 100);
            }
        } else if let Ok(content) = fs::read_to_string("/sys/fs/cgroup/cpu/cpu.shares") {
            // cgroups v1 uses shares directly
            cpu_shares = content.trim().parse::<u64>().ok();
        }

        (cpu_quota, cpu_period, cpu_shares)
    }

    fn get_cpuset_cpus() -> Option<Vec<u32>> {
        // Try cgroups v2 first
        let cpuset_path = if Path::new("/sys/fs/cgroup/cpuset.cpus.effective").exists() {
            "/sys/fs/cgroup/cpuset.cpus.effective"
        } else if Path::new("/sys/fs/cgroup/cpuset.cpus").exists() {
            "/sys/fs/cgroup/cpuset.cpus"
        } else if Path::new("/sys/fs/cgroup/cpuset/cpuset.cpus").exists() {
            // cgroups v1
            "/sys/fs/cgroup/cpuset/cpuset.cpus"
        } else {
            return None;
        };

        if let Ok(content) = fs::read_to_string(cpuset_path) {
            Self::parse_cpuset_range(&content.trim())
        } else {
            None
        }
    }

    fn parse_cpuset_range(cpuset_str: &str) -> Option<Vec<u32>> {
        let mut cpus = Vec::new();

        for part in cpuset_str.split(',') {
            let part = part.trim();
            if part.contains('-') {
                // Range like "0-3"
                let range_parts: Vec<&str> = part.split('-').collect();
                if range_parts.len() == 2 {
                    if let (Ok(start), Ok(end)) =
                        (range_parts[0].parse::<u32>(), range_parts[1].parse::<u32>())
                    {
                        for cpu in start..=end {
                            cpus.push(cpu);
                        }
                    }
                }
            } else {
                // Single CPU like "0"
                if let Ok(cpu) = part.parse::<u32>() {
                    cpus.push(cpu);
                }
            }
        }

        if cpus.is_empty() {
            None
        } else {
            Some(cpus)
        }
    }

    fn calculate_effective_cpus(
        cpu_quota: Option<i64>,
        cpu_period: Option<u64>,
        cpu_shares: Option<u64>,
        cpuset_cpus: &Option<Vec<u32>>,
    ) -> f64 {
        let total_cpus = num_cpus::get() as f64;

        // Start with cpuset limit
        let cpuset_limit = if let Some(cpus) = cpuset_cpus {
            cpus.len() as f64
        } else {
            total_cpus
        };

        // Calculate quota-based limit
        let quota_limit = if let (Some(quota), Some(period)) = (cpu_quota, cpu_period) {
            if quota > 0 && period > 0 {
                (quota as f64) / (period as f64)
            } else {
                cpuset_limit
            }
        } else {
            cpuset_limit
        };

        // Calculate shares-based limit (rough approximation)
        let shares_limit = if let Some(shares) = cpu_shares {
            // Default shares is 1024, so we scale based on that
            let share_ratio = (shares as f64) / 1024.0;
            (share_ratio * total_cpus).min(cpuset_limit)
        } else {
            cpuset_limit
        };

        // Return the most restrictive limit
        quota_limit.min(shares_limit).min(cpuset_limit)
    }

    pub fn get_cpu_usage_from_cgroup(&self) -> Option<f64> {
        // Try to get CPU usage directly from cgroup stats
        // cgroups v2
        if let Ok(content) = fs::read_to_string("/sys/fs/cgroup/cpu.stat") {
            let mut usage_usec = 0u64;
            for line in content.lines() {
                if line.starts_with("usage_usec") {
                    if let Some(value) = line.split_whitespace().nth(1) {
                        usage_usec = value.parse().unwrap_or(0);
                        break;
                    }
                }
            }

            // This would need to be calculated as a delta over time
            // For now, return None to fall back to /proc/stat
            return None;
        }

        // cgroups v1
        if let Ok(content) = fs::read_to_string("/sys/fs/cgroup/cpuacct/cpuacct.usage") {
            // This is cumulative nanoseconds, would need delta calculation
            return None;
        }

        None
    }
}

// Helper to parse CPU stats considering container limits
pub fn parse_cpu_stat_with_container_limits(
    stat_content: &str,
    container_info: &ContainerInfo,
) -> (f64, Vec<u32>) {
    let mut overall_utilization = 0.0;
    let mut active_cores = Vec::new();

    // If we have a cpuset, only consider those CPUs
    let allowed_cpus = if let Some(cpuset) = &container_info.cpuset_cpus {
        cpuset.clone()
    } else {
        // Consider all CPUs
        (0..num_cpus::get() as u32).collect()
    };

    let lines: Vec<&str> = stat_content.lines().collect();

    // Parse overall CPU stats
    if let Some(cpu_line) = lines.iter().find(|line| line.starts_with("cpu ")) {
        let fields: Vec<&str> = cpu_line.split_whitespace().collect();
        if fields.len() >= 8 {
            let user: u64 = fields[1].parse().unwrap_or(0);
            let nice: u64 = fields[2].parse().unwrap_or(0);
            let system: u64 = fields[3].parse().unwrap_or(0);
            let idle: u64 = fields[4].parse().unwrap_or(0);
            let iowait: u64 = fields[5].parse().unwrap_or(0);
            let irq: u64 = fields[6].parse().unwrap_or(0);
            let softirq: u64 = fields[7].parse().unwrap_or(0);

            let total_time = user + nice + system + idle + iowait + irq + softirq;
            let active_time = total_time - idle - iowait;

            if total_time > 0 {
                let raw_utilization = (active_time as f64 / total_time as f64) * 100.0;

                // Scale utilization based on container's effective CPU count
                if container_info.is_container {
                    let scale_factor =
                        container_info.effective_cpu_count / allowed_cpus.len() as f64;
                    overall_utilization = (raw_utilization * scale_factor).min(100.0);
                } else {
                    overall_utilization = raw_utilization;
                }
            }
        }
    }

    // Track which cores are active
    for line in lines.iter() {
        if line.starts_with("cpu") && !line.starts_with("cpu ") {
            if let Some(cpu_num_str) = line.split_whitespace().next() {
                if let Some(cpu_num_str) = cpu_num_str.strip_prefix("cpu") {
                    if let Ok(core_id) = cpu_num_str.parse::<u32>() {
                        if allowed_cpus.contains(&core_id) {
                            active_cores.push(core_id);
                        }
                    }
                }
            }
        }
    }

    (overall_utilization, active_cores)
}

// Add dependency in the module
#[cfg(not(target_os = "linux"))]
pub struct ContainerInfo {
    pub is_container: bool,
    pub effective_cpu_count: f64,
}

#[cfg(not(target_os = "linux"))]
impl ContainerInfo {
    pub fn detect() -> Self {
        ContainerInfo {
            is_container: false,
            effective_cpu_count: 1.0,
        }
    }
}

#[cfg(test)]
#[path = "container_cpu/tests.rs"]
mod tests;
