use crate::device::container_info::{parse_cpu_stat_with_container_limits, ContainerInfo};
use crate::device::{
    CoreType, CoreUtilization, CpuInfo, CpuPlatformType, CpuReader, CpuSocketInfo,
};
use crate::utils::system::get_hostname;
use chrono::Local;
use std::cell::RefCell;
use std::fs;
use std::process::Command;

type CpuInfoParseResult = Result<
    (
        String,
        String,
        CpuPlatformType,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
    ),
    Box<dyn std::error::Error>,
>;

type CpuStatParseResult =
    Result<(f64, Vec<CpuSocketInfo>, Vec<CoreUtilization>), Box<dyn std::error::Error>>;

pub struct LinuxCpuReader {
    // Use Option<Option<u32>> to distinguish:
    // - None: not cached yet
    // - Some(None): lscpu was called but failed
    // - Some(Some(value)): lscpu succeeded with value
    cached_lscpu_cache_size: RefCell<Option<Option<u32>>>,
    container_info: ContainerInfo,
}

impl Default for LinuxCpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxCpuReader {
    pub fn new() -> Self {
        Self {
            cached_lscpu_cache_size: RefCell::new(None),
            container_info: ContainerInfo::detect(),
        }
    }

    fn get_cpu_info_from_proc(&self) -> Result<CpuInfo, Box<dyn std::error::Error>> {
        let hostname = get_hostname();
        let instance = hostname.clone();
        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Read /proc/cpuinfo for CPU details
        let cpuinfo_content = fs::read_to_string("/proc/cpuinfo")?;
        let (
            cpu_model,
            architecture,
            platform_type,
            mut socket_count,
            mut total_cores,
            mut total_threads,
            base_frequency,
            max_frequency,
            mut cache_size,
        ) = self.parse_cpuinfo(&cpuinfo_content)?;

        // Adjust core/thread counts based on container limits
        if self.container_info.is_container {
            // If in a container, adjust the reported cores based on CPU quota
            let effective_cores = self.container_info.effective_cpu_count.ceil() as u32;

            // If cpuset is specified, use its count
            if let Some(cpuset) = &self.container_info.cpuset_cpus {
                total_cores = cpuset.len() as u32;
                total_threads = total_cores; // Assume no hyperthreading for simplicity
            } else if effective_cores < total_cores {
                // Use quota-based limit if it's more restrictive
                total_cores = effective_cores;
                total_threads = effective_cores;
            }

            // Container typically appears as single socket
            socket_count = 1;
        }

        // If cache_size is 0, try to get it from lscpu
        if cache_size == 0 {
            if let Some(lscpu_cache) = self.get_cache_size_from_lscpu() {
                cache_size = lscpu_cache;
            }
        }

        // Read /proc/stat for CPU utilization
        let stat_content = fs::read_to_string("/proc/stat")?;
        let (overall_utilization, per_socket_info, per_core_utilization) =
            if self.container_info.is_container {
                // Use container-aware parsing
                let (utilization, active_cores) =
                    parse_cpu_stat_with_container_limits(&stat_content, &self.container_info);

                // Convert active cores to per-core utilization
                let mut core_utils = Vec::new();
                for &core_id in &active_cores {
                    // Parse individual core stats
                    let core_util = self.get_core_utilization_from_stat(&stat_content, core_id);
                    core_utils.push(CoreUtilization {
                        core_id,
                        core_type: CoreType::Standard,
                        utilization: core_util,
                    });
                }

                // Create socket info for container
                let socket_info = vec![CpuSocketInfo {
                    socket_id: 0,
                    utilization,
                    cores: total_cores,
                    threads: total_threads,
                    temperature: None,
                    frequency_mhz: base_frequency,
                }];

                (utilization, socket_info, core_utils)
            } else {
                self.parse_cpu_stat(&stat_content, socket_count)?
            };

        // Try to get CPU temperature (may not be available on all systems)
        let temperature = self.get_cpu_temperature();

        // Power consumption is not readily available on most Linux systems
        let power_consumption = None;

        Ok(CpuInfo {
            host_id: hostname.clone(), // For local mode, host_id is just the hostname
            hostname,
            instance,
            cpu_model,
            architecture,
            platform_type,
            socket_count,
            total_cores,
            total_threads,
            base_frequency_mhz: base_frequency,
            max_frequency_mhz: max_frequency,
            cache_size_mb: cache_size,
            utilization: overall_utilization,
            temperature,
            power_consumption,
            per_socket_info,
            apple_silicon_info: None, // Not applicable for Linux
            per_core_utilization,
            time,
        })
    }

    fn parse_cpuinfo(&self, content: &str) -> CpuInfoParseResult {
        let mut cpu_model = String::new();
        let mut architecture = String::new();
        let mut platform_type = CpuPlatformType::Other("Unknown".to_string());

        let mut base_frequency = 0u32;
        let mut max_frequency = 0u32;
        let mut cache_size = 0u32;

        let mut physical_ids = std::collections::HashSet::new();
        let mut processor_count = 0u32;
        let mut cpu_implementer = String::new();
        let mut cpu_part = String::new();

        for line in content.lines() {
            if line.starts_with("model name") {
                if let Some(value) = line.split(':').nth(1) {
                    cpu_model = value.trim().to_string();

                    // Determine platform type from model name
                    if cpu_model.to_lowercase().contains("intel") {
                        platform_type = CpuPlatformType::Intel;
                    } else if cpu_model.to_lowercase().contains("amd") {
                        platform_type = CpuPlatformType::Amd;
                    } else if cpu_model.to_lowercase().contains("arm") {
                        platform_type = CpuPlatformType::Arm;
                    }
                }
            } else if line.starts_with("processor") {
                processor_count += 1;
            } else if line.starts_with("physical id") {
                if let Some(value) = line.split(':').nth(1) {
                    if let Ok(id) = value.trim().parse::<u32>() {
                        physical_ids.insert(id);
                    }
                }
            } else if line.starts_with("cpu MHz") && base_frequency == 0 {
                if let Some(value) = line.split(':').nth(1) {
                    if let Ok(freq) = value.trim().parse::<f64>() {
                        base_frequency = freq as u32;
                    }
                }
            } else if line.starts_with("cache size") && cache_size == 0 {
                if let Some(value) = line.split(':').nth(1) {
                    let value = value.trim();
                    if let Some(size_str) = value.split_whitespace().next() {
                        if let Ok(size) = size_str.parse::<u32>() {
                            cache_size = size / 1024; // Convert KB to MB
                        }
                    }
                }
            } else if line.starts_with("CPU implementer") {
                if let Some(value) = line.split(':').nth(1) {
                    cpu_implementer = value.trim().to_string();
                }
            } else if line.starts_with("CPU part") {
                if let Some(value) = line.split(':').nth(1) {
                    cpu_part = value.trim().to_string();
                }
            }
        }

        let socket_count = if physical_ids.is_empty() {
            1
        } else {
            physical_ids.len() as u32
        };
        let total_threads = processor_count;

        // Try to get core count from /proc/cpuinfo siblings field or estimate
        let total_cores = total_threads; // Default assumption, may be incorrect with hyperthreading

        // Try to get architecture from uname
        if let Ok(output) = std::process::Command::new("uname").arg("-m").output() {
            architecture = String::from_utf8_lossy(&output.stdout).trim().to_string();

            // If architecture is ARM and we don't have a CPU model, construct one
            if (architecture == "aarch64"
                || architecture == "arm64"
                || architecture.starts_with("arm"))
                && cpu_model.is_empty()
            {
                platform_type = CpuPlatformType::Arm;

                // Try to construct a model name from implementer and part
                if !cpu_implementer.is_empty() || !cpu_part.is_empty() {
                    let implementer_name = match cpu_implementer.as_str() {
                        "0x41" => "ARM",
                        "0x42" => "Broadcom",
                        "0x43" => "Cavium",
                        "0x44" => "DEC",
                        "0x4e" => "NVIDIA",
                        "0x50" => "APM",
                        "0x51" => "Qualcomm",
                        "0x53" => "Samsung",
                        "0x54" => "HiSilicon",
                        "0x56" => "Marvell",
                        "0x61" => "Apple",
                        "0x66" => "Faraday",
                        "0x69" => "Intel",
                        _ => "Unknown",
                    };

                    cpu_model = format!("{implementer_name} ARM Processor");
                    if !cpu_part.is_empty() {
                        cpu_model.push_str(&format!(" (Part: {cpu_part})"));
                    }
                } else {
                    cpu_model = "ARM Processor".to_string();
                }
            }
        }

        // Try to get max frequency from cpufreq
        if let Ok(content) =
            fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        {
            if let Ok(freq_khz) = content.trim().parse::<u32>() {
                max_frequency = freq_khz / 1000; // Convert kHz to MHz
            }
        }

        // Try to get current frequency for base frequency if we don't have it
        if base_frequency == 0 {
            if let Ok(content) =
                fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
            {
                if let Ok(freq_khz) = content.trim().parse::<u32>() {
                    base_frequency = freq_khz / 1000; // Convert kHz to MHz
                }
            }
        }

        // If still no base frequency, try cpuinfo_min_freq
        if base_frequency == 0 {
            if let Ok(content) =
                fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq")
            {
                if let Ok(freq_khz) = content.trim().parse::<u32>() {
                    base_frequency = freq_khz / 1000; // Convert kHz to MHz
                }
            }
        }

        if max_frequency == 0 {
            max_frequency = base_frequency;
        }

        // If we still don't have frequencies, try to use a reasonable default
        if base_frequency == 0 && max_frequency == 0 {
            // Default to 1000 MHz as a reasonable guess
            base_frequency = 1000;
            max_frequency = 1000;
        }

        Ok((
            cpu_model,
            architecture,
            platform_type,
            socket_count,
            total_cores,
            total_threads,
            base_frequency,
            max_frequency,
            cache_size,
        ))
    }

    fn parse_cpu_stat(&self, content: &str, socket_count: u32) -> CpuStatParseResult {
        let mut overall_utilization = 0.0;
        let mut per_socket_info = Vec::new();
        let mut per_core_utilization = Vec::new();

        let lines: Vec<&str> = content.lines().collect();

        // Parse overall CPU stats (first line starting with "cpu ")
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
                    overall_utilization = (active_time as f64 / total_time as f64) * 100.0;
                }
            }
        }

        // Parse individual CPU core stats
        for line in lines.iter() {
            if line.starts_with("cpu") && !line.starts_with("cpu ") {
                // Extract CPU core number
                if let Some(cpu_num_str) = line.split_whitespace().next() {
                    if let Some(cpu_num_str) = cpu_num_str.strip_prefix("cpu") {
                        if let Ok(core_id) = cpu_num_str.parse::<u32>() {
                            let fields: Vec<&str> = line.split_whitespace().collect();
                            if fields.len() >= 8 {
                                let user: u64 = fields[1].parse().unwrap_or(0);
                                let nice: u64 = fields[2].parse().unwrap_or(0);
                                let system: u64 = fields[3].parse().unwrap_or(0);
                                let idle: u64 = fields[4].parse().unwrap_or(0);
                                let iowait: u64 = fields[5].parse().unwrap_or(0);
                                let irq: u64 = fields[6].parse().unwrap_or(0);
                                let softirq: u64 = fields[7].parse().unwrap_or(0);

                                let total_time =
                                    user + nice + system + idle + iowait + irq + softirq;
                                let active_time = total_time - idle - iowait;

                                let utilization = if total_time > 0 {
                                    (active_time as f64 / total_time as f64) * 100.0
                                } else {
                                    0.0
                                };

                                // Check if this is a P-core or E-core based on CPU topology
                                // For now, we'll use Standard type for all Linux cores
                                // In the future, we could check /sys/devices/system/cpu/cpu*/cpufreq/base_frequency
                                // to distinguish between P-cores and E-cores on Intel systems
                                let core_type = CoreType::Standard;

                                per_core_utilization.push(CoreUtilization {
                                    core_id,
                                    core_type,
                                    utilization,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Sort cores by ID for consistent display
        per_core_utilization.sort_by_key(|c| c.core_id);

        // Create per-socket info (simplified - assumes even distribution across sockets)
        for socket_id in 0..socket_count {
            per_socket_info.push(CpuSocketInfo {
                socket_id,
                utilization: overall_utilization, // Simplified - same as overall
                cores: 0,          // Will be calculated based on total_cores / socket_count
                threads: 0,        // Will be calculated based on total_threads / socket_count
                temperature: None, // Not easily available per socket
                frequency_mhz: 0,  // Will be set from base frequency
            });
        }

        Ok((overall_utilization, per_socket_info, per_core_utilization))
    }

    fn get_cpu_temperature(&self) -> Option<u32> {
        // Try to read from various thermal zone files
        let thermal_paths = [
            "/sys/class/thermal/thermal_zone0/temp",
            "/sys/class/thermal/thermal_zone1/temp",
            "/sys/class/hwmon/hwmon0/temp1_input",
            "/sys/class/hwmon/hwmon1/temp1_input",
        ];

        for path in &thermal_paths {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(temp_millicelsius) = content.trim().parse::<u32>() {
                    return Some(temp_millicelsius / 1000); // Convert millicelsius to celsius
                }
            }
        }

        None
    }

    fn get_cache_size_from_lscpu(&self) -> Option<u32> {
        // Check if we have cached value
        if let Some(cached_result) = &*self.cached_lscpu_cache_size.borrow() {
            // We've already tried lscpu, return the cached result
            return *cached_result;
        }

        // Try to get cache size from lscpu
        let result = if let Ok(output) = Command::new("lscpu").output() {
            let output_str = String::from_utf8_lossy(&output.stdout);

            // Look for cache lines (L3 preferred, then L2 as fallback)
            // Note: On some systems like Jetson, the lines might be indented
            let mut found_l3_cache = None;
            let mut found_l2_cache = None;

            for line in output_str.lines() {
                let line = line.trim();

                // Check for L3 cache (handle both "L3:" and "L3 cache:" formats)
                if line.starts_with("L3:") || line.starts_with("L3 cache:") {
                    if let Some(size_part) = line.split(':').nth(1) {
                        let size_part = size_part.trim();

                        // Parse different formats: "4 MiB", "4MiB", "4096 KiB", etc.
                        // Also handle format with instances: "4 MiB (2 instances)"
                        let parts: Vec<&str> = size_part.split_whitespace().collect();
                        if !parts.is_empty() {
                            if let Ok(size) = parts[0].parse::<f64>() {
                                let unit = if parts.len() > 1 {
                                    parts[1].to_lowercase()
                                } else {
                                    // Try to extract unit from the first part if it's like "4MiB"
                                    let num_end = parts[0]
                                        .find(|c: char| !c.is_numeric() && c != '.')
                                        .unwrap_or(parts[0].len());
                                    parts[0][num_end..].to_lowercase()
                                };

                                let size_mb = match unit.as_str() {
                                    "mib" | "mb" => size as u32,
                                    "kib" | "kb" => (size / 1024.0) as u32,
                                    "gib" | "gb" => (size * 1024.0) as u32,
                                    _ => 0,
                                };

                                if size_mb > 0 {
                                    found_l3_cache = Some(size_mb);
                                }
                            }
                        }
                    }
                }

                // Check for L2 cache as fallback (handle both "L2:" and "L2 cache:" formats)
                if (line.starts_with("L2:") || line.starts_with("L2 cache:"))
                    && found_l3_cache.is_none()
                {
                    if let Some(size_part) = line.split(':').nth(1) {
                        let size_part = size_part.trim();

                        let parts: Vec<&str> = size_part.split_whitespace().collect();
                        if !parts.is_empty() {
                            if let Ok(size) = parts[0].parse::<f64>() {
                                let unit = if parts.len() > 1 {
                                    parts[1].to_lowercase()
                                } else {
                                    let num_end = parts[0]
                                        .find(|c: char| !c.is_numeric() && c != '.')
                                        .unwrap_or(parts[0].len());
                                    parts[0][num_end..].to_lowercase()
                                };

                                let size_mb = match unit.as_str() {
                                    "mib" | "mb" => size as u32,
                                    "kib" | "kb" => (size / 1024.0) as u32,
                                    "gib" | "gb" => (size * 1024.0) as u32,
                                    _ => 0,
                                };

                                if size_mb > 0 {
                                    found_l2_cache = Some(size_mb);
                                }
                            }
                        }
                    }
                }
            }

            // Return L3 if found, otherwise L2
            found_l3_cache.or(found_l2_cache)
        } else {
            None
        };

        // Cache the result (whether success or failure)
        *self.cached_lscpu_cache_size.borrow_mut() = Some(result);

        result
    }

    fn get_core_utilization_from_stat(&self, stat_content: &str, core_id: u32) -> f64 {
        let cpu_line_prefix = format!("cpu{core_id}");

        for line in stat_content.lines() {
            if line.starts_with(&cpu_line_prefix)
                && !line[cpu_line_prefix.len()..].starts_with(|c: char| c.is_ascii_digit())
            {
                let fields: Vec<&str> = line.split_whitespace().collect();
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
                        return (active_time as f64 / total_time as f64) * 100.0;
                    }
                }
                break;
            }
        }

        0.0
    }
}

impl CpuReader for LinuxCpuReader {
    fn get_cpu_info(&self) -> Vec<CpuInfo> {
        match self.get_cpu_info_from_proc() {
            Ok(mut cpu_info) => {
                // Fill in cores and threads per socket
                let cores_per_socket = cpu_info.total_cores / cpu_info.socket_count;
                let threads_per_socket = cpu_info.total_threads / cpu_info.socket_count;

                for socket_info in &mut cpu_info.per_socket_info {
                    socket_info.cores = cores_per_socket;
                    socket_info.threads = threads_per_socket;
                    socket_info.frequency_mhz = cpu_info.base_frequency_mhz;
                }

                vec![cpu_info]
            }
            Err(e) => {
                eprintln!("Error reading CPU info: {e}");
                vec![]
            }
        }
    }
}

#[cfg(test)]
#[path = "cpu_linux/tests.rs"]
mod tests;
