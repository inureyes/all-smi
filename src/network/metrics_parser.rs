use std::collections::HashMap;

use chrono::Local;
use regex::Regex;

use crate::device::{AppleSiliconCpuInfo, CpuInfo, CpuPlatformType, GpuInfo, MemoryInfo};
use crate::storage::info::StorageInfo;

pub struct MetricsParser;

impl MetricsParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse_metrics(
        &self,
        text: &str,
        host: &str,
        re: &Regex,
    ) -> (
        Vec<GpuInfo>,
        Vec<CpuInfo>,
        Vec<MemoryInfo>,
        Vec<StorageInfo>,
    ) {
        let mut gpu_info_map: HashMap<String, GpuInfo> = HashMap::new();
        let mut cpu_info_map: HashMap<String, CpuInfo> = HashMap::new();
        let mut memory_info_map: HashMap<String, MemoryInfo> = HashMap::new();
        let mut storage_info_map: HashMap<String, StorageInfo> = HashMap::new();
        let mut host_instance_name: Option<String> = None;

        for line in text.lines() {
            if let Some(cap) = re.captures(line.trim()) {
                let metric_name = &cap[1];
                let labels_str = &cap[2];
                let value = cap[3].parse::<f64>().unwrap_or(0.0);

                let labels = self.parse_labels(labels_str);

                // Extract instance name from the first metric that has it
                if host_instance_name.is_none() {
                    if let Some(instance) = labels.get("instance") {
                        host_instance_name = Some(instance.clone());
                    }
                }

                // Process different metric types
                if metric_name.starts_with("gpu_")
                    || metric_name.starts_with("npu_")
                    || metric_name == "ane_utilization"
                {
                    self.process_gpu_metrics(&mut gpu_info_map, metric_name, &labels, value, host);
                } else if metric_name.starts_with("cpu_") {
                    self.process_cpu_metrics(&mut cpu_info_map, metric_name, &labels, value, host);
                } else if metric_name.starts_with("memory_") {
                    self.process_memory_metrics(
                        &mut memory_info_map,
                        metric_name,
                        &labels,
                        value,
                        host,
                    );
                } else if metric_name.starts_with("storage_") || metric_name.starts_with("disk_") {
                    self.process_storage_metrics(
                        &mut storage_info_map,
                        metric_name,
                        &labels,
                        value,
                        host,
                    );
                }
            }
        }

        // Store instance name in detail field if available, but keep host as the key
        if let Some(instance_name) = host_instance_name {
            self.update_instance_names(
                &mut gpu_info_map,
                &mut cpu_info_map,
                &mut memory_info_map,
                &mut storage_info_map,
                &instance_name,
            );
        }

        (
            gpu_info_map.into_values().collect(),
            cpu_info_map.into_values().collect(),
            memory_info_map.into_values().collect(),
            storage_info_map.into_values().collect(),
        )
    }

    fn parse_labels(&self, labels_str: &str) -> HashMap<String, String> {
        let mut labels: HashMap<String, String> = HashMap::new();
        for label in labels_str.split(',') {
            let label_parts: Vec<&str> = label.split('=').collect();
            if label_parts.len() == 2 {
                let key = label_parts[0].trim().to_string();
                let value = label_parts[1].replace('"', "").to_string();
                labels.insert(key, value);
            }
        }
        labels
    }

    fn process_gpu_metrics(
        &self,
        gpu_info_map: &mut HashMap<String, GpuInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
        let gpu_name = labels.get("gpu").cloned().unwrap_or_default();
        let gpu_uuid = labels.get("uuid").cloned().unwrap_or_default();
        let gpu_index = labels.get("index").cloned().unwrap_or_default();

        if gpu_name.is_empty() || gpu_uuid.is_empty() {
            return;
        }

        let gpu_info = gpu_info_map.entry(gpu_uuid.clone()).or_insert_with(|| {
            let mut detail = HashMap::new();
            detail.insert("index".to_string(), gpu_index.clone());
            GpuInfo {
                uuid: gpu_uuid.clone(),
                time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                name: gpu_name,
                device_type: "GPU".to_string(), // Default to GPU, can be overridden by gpu_info metric
                host_id: host.to_string(),      // Host identifier (e.g., "10.82.128.41:9090")
                hostname: labels
                    .get("instance")
                    .cloned()
                    .unwrap_or_else(|| host.to_string()), // DNS hostname from instance label
                instance: labels
                    .get("instance")
                    .cloned()
                    .unwrap_or_else(|| host.to_string()),
                utilization: 0.0,
                ane_utilization: 0.0,
                dla_utilization: None,
                temperature: 0,
                used_memory: 0,
                total_memory: 0,
                frequency: 0,
                power_consumption: 0.0,
                detail,
            }
        });

        match metric_name {
            "gpu_utilization" => gpu_info.utilization = value,
            "gpu_memory_used_bytes" => gpu_info.used_memory = value as u64,
            "gpu_memory_total_bytes" => gpu_info.total_memory = value as u64,
            "gpu_temperature_celsius" => gpu_info.temperature = value as u32,
            "gpu_power_consumption_watts" => gpu_info.power_consumption = value,
            "gpu_frequency_mhz" => gpu_info.frequency = value as u32,
            "ane_utilization" => gpu_info.ane_utilization = value,
            "gpu_power_limit_max_watts" => {
                gpu_info
                    .detail
                    .insert("power_limit_max".to_string(), value.to_string());
            }
            "gpu_info" => {
                // Extract device type
                if let Some(device_type) = labels.get("type") {
                    gpu_info.device_type = device_type.clone();
                }

                // Extract CUDA and driver info from labels
                if let Some(cuda_version) = labels.get("cuda_version") {
                    gpu_info
                        .detail
                        .insert("cuda_version".to_string(), cuda_version.clone());
                }
                if let Some(driver_version) = labels.get("driver_version") {
                    gpu_info
                        .detail
                        .insert("driver_version".to_string(), driver_version.clone());
                }
                // Also extract other useful info from gpu_info metric
                if let Some(arch) = labels.get("architecture") {
                    gpu_info
                        .detail
                        .insert("architecture".to_string(), arch.clone());
                }
                if let Some(compute_cap) = labels.get("compute_capability") {
                    gpu_info
                        .detail
                        .insert("compute_capability".to_string(), compute_cap.clone());
                }
                // Extract NPU-specific info
                if let Some(firmware) = labels.get("firmware") {
                    gpu_info
                        .detail
                        .insert("firmware".to_string(), firmware.clone());
                }
                if let Some(serial_number) = labels.get("serial_number") {
                    gpu_info
                        .detail
                        .insert("serial_number".to_string(), serial_number.clone());
                }
                if let Some(pci_address) = labels.get("pci_address") {
                    gpu_info
                        .detail
                        .insert("pci_address".to_string(), pci_address.clone());
                }
                if let Some(pci_device) = labels.get("pci_device") {
                    gpu_info
                        .detail
                        .insert("pci_device".to_string(), pci_device.clone());
                }
            }
            "npu_firmware_info" => {
                // Handle NPU-specific firmware info metric
                if let Some(firmware) = labels.get("firmware") {
                    gpu_info
                        .detail
                        .insert("firmware".to_string(), firmware.clone());
                }
            }
            _ => {}
        }
    }

    fn process_cpu_metrics(
        &self,
        cpu_info_map: &mut HashMap<String, CpuInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
        let cpu_model = labels.get("cpu_model").cloned().unwrap_or_default();
        // Keep the full host address including port
        let cpu_index = labels.get("index").cloned().unwrap_or("0".to_string());

        let cpu_key = format!("{host}:{cpu_index}");

        let cpu_info = cpu_info_map.entry(cpu_key).or_insert_with(|| {
            let platform_type = if cpu_model.contains("Apple") {
                CpuPlatformType::AppleSilicon
            } else if cpu_model.contains("Intel") {
                CpuPlatformType::Intel
            } else if cpu_model.contains("AMD") {
                CpuPlatformType::Amd
            } else {
                CpuPlatformType::Other("Unknown".to_string())
            };

            CpuInfo {
                host_id: host.to_string(), // Host identifier (e.g., "10.82.128.41:9090")
                hostname: labels
                    .get("instance")
                    .cloned()
                    .unwrap_or_else(|| host.to_string()), // DNS hostname from instance label
                instance: labels
                    .get("instance")
                    .cloned()
                    .unwrap_or_else(|| host.to_string()),
                cpu_model: cpu_model.clone(),
                architecture: "".to_string(),
                platform_type,
                socket_count: 1,
                total_cores: 0,
                total_threads: 0,
                base_frequency_mhz: 0,
                max_frequency_mhz: 0,
                cache_size_mb: 0,
                utilization: 0.0,
                temperature: None,
                power_consumption: None,
                per_socket_info: Vec::new(),
                apple_silicon_info: None,
                per_core_utilization: Vec::new(),
                time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            }
        });

        match metric_name {
            "cpu_utilization" => cpu_info.utilization = value,
            "cpu_socket_count" => cpu_info.socket_count = value as u32,
            "cpu_core_count" => cpu_info.total_cores = value as u32,
            "cpu_thread_count" => cpu_info.total_threads = value as u32,
            "cpu_frequency_mhz" => {
                cpu_info.base_frequency_mhz = value as u32;
                cpu_info.max_frequency_mhz = value as u32;
            }
            "cpu_temperature_celsius" => cpu_info.temperature = Some(value as u32),
            "cpu_power_consumption_watts" => cpu_info.power_consumption = Some(value),
            "cpu_p_core_count" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.p_core_count = value as u32;
                }
            }
            "cpu_e_core_count" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.e_core_count = value as u32;
                }
            }
            "cpu_p_core_utilization" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.p_core_utilization = value;
                }
            }
            "cpu_e_core_utilization" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.e_core_utilization = value;
                }
            }
            "cpu_core_utilization" => {
                // Parse per-core utilization
                if let (Some(core_id_str), Some(core_type_str)) =
                    (labels.get("core_id"), labels.get("core_type"))
                {
                    if let Ok(core_id) = core_id_str.parse::<u32>() {
                        let core_type = match core_type_str.as_str() {
                            "P" => crate::device::CoreType::Performance,
                            "E" => crate::device::CoreType::Efficiency,
                            _ => crate::device::CoreType::Standard,
                        };

                        // Ensure vector is large enough
                        while cpu_info.per_core_utilization.len() <= core_id as usize {
                            cpu_info
                                .per_core_utilization
                                .push(crate::device::CoreUtilization {
                                    core_id: cpu_info.per_core_utilization.len() as u32,
                                    core_type: crate::device::CoreType::Standard,
                                    utilization: 0.0,
                                });
                        }

                        // Update the specific core
                        cpu_info.per_core_utilization[core_id as usize] =
                            crate::device::CoreUtilization {
                                core_id,
                                core_type,
                                utilization: value,
                            };
                    }
                }
            }
            _ => {}
        }
    }

    fn process_memory_metrics(
        &self,
        memory_info_map: &mut HashMap<String, MemoryInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
        // Keep the full host address including port
        let memory_index = labels.get("index").cloned().unwrap_or("0".to_string());
        let memory_key = format!("{host}:{memory_index}");

        let memory_info = memory_info_map
            .entry(memory_key)
            .or_insert_with(|| MemoryInfo {
                host_id: host.to_string(), // Host identifier (e.g., "10.82.128.41:9090")
                hostname: labels
                    .get("instance")
                    .cloned()
                    .unwrap_or_else(|| host.to_string()), // DNS hostname from instance label
                instance: labels
                    .get("instance")
                    .cloned()
                    .unwrap_or_else(|| host.to_string()),
                total_bytes: 0,
                used_bytes: 0,
                available_bytes: 0,
                free_bytes: 0,
                buffers_bytes: 0,
                cached_bytes: 0,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
                swap_free_bytes: 0,
                utilization: 0.0,
                time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            });

        match metric_name {
            "memory_total_bytes" => memory_info.total_bytes = value as u64,
            "memory_used_bytes" => memory_info.used_bytes = value as u64,
            "memory_available_bytes" => memory_info.available_bytes = value as u64,
            "memory_buffers_bytes" => memory_info.buffers_bytes = value as u64,
            "memory_cached_bytes" => memory_info.cached_bytes = value as u64,
            "memory_utilization" => memory_info.utilization = value,
            _ => {}
        }
    }

    fn process_storage_metrics(
        &self,
        storage_info_map: &mut HashMap<String, StorageInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
        // Keep the full host address including port
        let mount_point = labels.get("mount_point").cloned().unwrap_or_default();
        let storage_index = labels.get("index").cloned().unwrap_or("0".to_string());

        if mount_point.is_empty() {
            return;
        }

        let storage_key = format!("{host}:{mount_point}");
        let storage_info = storage_info_map
            .entry(storage_key)
            .or_insert_with(|| StorageInfo {
                host_id: host.to_string(), // Host identifier (e.g., "10.82.128.41:9090")
                hostname: labels
                    .get("instance")
                    .cloned()
                    .unwrap_or_else(|| host.to_string()), // DNS hostname from instance label
                mount_point: mount_point.clone(),
                total_bytes: 0,
                available_bytes: 0,
                index: storage_index.parse().unwrap_or(0),
            });

        match metric_name {
            "disk_total_bytes" => storage_info.total_bytes = value as u64,
            "disk_available_bytes" => storage_info.available_bytes = value as u64,
            _ => {}
        }
    }

    fn ensure_apple_silicon_info(&self, cpu_info: &mut CpuInfo) {
        if cpu_info.apple_silicon_info.is_none() {
            cpu_info.apple_silicon_info = Some(AppleSiliconCpuInfo {
                p_core_count: 0,
                e_core_count: 0,
                gpu_core_count: 0,
                p_core_utilization: 0.0,
                e_core_utilization: 0.0,
                ane_ops_per_second: None,
                p_cluster_frequency_mhz: None,
                e_cluster_frequency_mhz: None,
                p_core_l2_cache_mb: None,
                e_core_l2_cache_mb: None,
            });
        }
    }

    fn update_instance_names(
        &self,
        gpu_info_map: &mut HashMap<String, GpuInfo>,
        cpu_info_map: &mut HashMap<String, CpuInfo>,
        memory_info_map: &mut HashMap<String, MemoryInfo>,
        storage_info_map: &mut HashMap<String, StorageInfo>,
        instance_name: &str,
    ) {
        // Store instance name in detail field but keep hostname as the host address
        for gpu_info in gpu_info_map.values_mut() {
            gpu_info
                .detail
                .insert("instance_name".to_string(), instance_name.to_string());
        }
        for _cpu_info in cpu_info_map.values_mut() {
            // For CPU info, we may want to store instance name differently
            // since it doesn't have a detail field by default
        }
        for _memory_info in memory_info_map.values_mut() {
            // Similarly for memory info
        }
        for _storage_info in storage_info_map.values_mut() {
            // And storage info
        }
    }
}

impl Default for MetricsParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn create_test_parser() -> MetricsParser {
        MetricsParser::new()
    }

    fn create_test_regex() -> Regex {
        Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap()
    }

    #[test]
    fn test_parse_labels() {
        let parser = create_test_parser();

        let labels = parser.parse_labels(r#"instance="node-0058", mount_point="/", index="0""#);
        assert_eq!(labels.get("instance").unwrap(), "node-0058");
        assert_eq!(labels.get("mount_point").unwrap(), "/");
        assert_eq!(labels.get("index").unwrap(), "0");

        let labels = parser.parse_labels(r#"gpu="NVIDIA H200 141GB HBM3", uuid="GPU-12345""#);
        assert_eq!(labels.get("gpu").unwrap(), "NVIDIA H200 141GB HBM3");
        assert_eq!(labels.get("uuid").unwrap(), "GPU-12345");

        let labels = parser.parse_labels("");
        assert!(labels.is_empty());

        let labels = parser.parse_labels("malformed");
        assert!(labels.is_empty());
    }

    #[test]
    fn test_parse_gpu_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_gpu_utilization{gpu="NVIDIA H200 141GB HBM3", instance="node-0058", uuid="GPU-12345", index="0"} 25.5
all_smi_gpu_memory_used_bytes{gpu="NVIDIA H200 141GB HBM3", instance="node-0058", uuid="GPU-12345", index="0"} 8589934592
all_smi_gpu_memory_total_bytes{gpu="NVIDIA H200 141GB HBM3", instance="node-0058", uuid="GPU-12345", index="0"} 34359738368
all_smi_gpu_temperature_celsius{gpu="NVIDIA H200 141GB HBM3", instance="node-0058", uuid="GPU-12345", index="0"} 65
all_smi_gpu_power_consumption_watts{gpu="NVIDIA H200 141GB HBM3", instance="node-0058", uuid="GPU-12345", index="0"} 400.5
all_smi_ane_utilization{gpu="NVIDIA H200 141GB HBM3", instance="node-0058", uuid="GPU-12345", index="0"} 15.2
"#;

        let (gpu_info, _, _, _) = parser.parse_metrics(test_data, host, &re);

        assert_eq!(gpu_info.len(), 1);
        let gpu = &gpu_info[0];
        assert_eq!(gpu.uuid, "GPU-12345");
        assert_eq!(gpu.name, "NVIDIA H200 141GB HBM3");
        assert_eq!(gpu.host_id, host);
        assert_eq!(gpu.hostname, "node-0058");
        assert_eq!(gpu.instance, "node-0058");
        assert_eq!(gpu.utilization, 25.5);
        assert_eq!(gpu.used_memory, 8589934592);
        assert_eq!(gpu.total_memory, 34359738368);
        assert_eq!(gpu.temperature, 65);
        assert_eq!(gpu.power_consumption, 400.5);
        assert_eq!(gpu.ane_utilization, 15.2);
    }

    #[test]
    fn test_parse_cpu_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_cpu_utilization{cpu_model="Intel Xeon", instance="node-0058", hostname="node-0058", index="0"} 45.2
all_smi_cpu_socket_count{cpu_model="Intel Xeon", instance="node-0058", hostname="node-0058", index="0"} 2
all_smi_cpu_core_count{cpu_model="Intel Xeon", instance="node-0058", hostname="node-0058", index="0"} 16
all_smi_cpu_thread_count{cpu_model="Intel Xeon", instance="node-0058", hostname="node-0058", index="0"} 32
all_smi_cpu_frequency_mhz{cpu_model="Intel Xeon", instance="node-0058", hostname="node-0058", index="0"} 2400
all_smi_cpu_temperature_celsius{cpu_model="Intel Xeon", instance="node-0058", hostname="node-0058", index="0"} 55
all_smi_cpu_power_consumption_watts{cpu_model="Intel Xeon", instance="node-0058", hostname="node-0058", index="0"} 125.5
"#;

        let (_, cpu_info, _, _) = parser.parse_metrics(test_data, host, &re);

        assert_eq!(cpu_info.len(), 1);
        let cpu = &cpu_info[0];
        assert_eq!(cpu.host_id, host);
        assert_eq!(cpu.hostname, "node-0058");
        assert_eq!(cpu.instance, "node-0058");
        assert_eq!(cpu.cpu_model, "Intel Xeon");
        assert_eq!(cpu.utilization, 45.2);
        assert_eq!(cpu.socket_count, 2);
        assert_eq!(cpu.total_cores, 16);
        assert_eq!(cpu.total_threads, 32);
        assert_eq!(cpu.base_frequency_mhz, 2400);
        assert_eq!(cpu.max_frequency_mhz, 2400);
        assert_eq!(cpu.temperature, Some(55));
        assert_eq!(cpu.power_consumption, Some(125.5));
        assert!(matches!(
            cpu.platform_type,
            crate::device::CpuPlatformType::Intel
        ));
    }

    #[test]
    fn test_parse_apple_silicon_cpu_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_cpu_utilization{cpu_model="Apple M2 Max", instance="node-0058", hostname="node-0058", index="0"} 30.5
all_smi_cpu_p_core_count{cpu_model="Apple M2 Max", instance="node-0058", hostname="node-0058", index="0"} 8
all_smi_cpu_e_core_count{cpu_model="Apple M2 Max", instance="node-0058", hostname="node-0058", index="0"} 4
all_smi_cpu_p_core_utilization{cpu_model="Apple M2 Max", instance="node-0058", hostname="node-0058", index="0"} 25.2
all_smi_cpu_e_core_utilization{cpu_model="Apple M2 Max", instance="node-0058", hostname="node-0058", index="0"} 10.8
"#;

        let (_, cpu_info, _, _) = parser.parse_metrics(test_data, host, &re);

        assert_eq!(cpu_info.len(), 1);
        let cpu = &cpu_info[0];
        assert_eq!(cpu.cpu_model, "Apple M2 Max");
        assert_eq!(cpu.utilization, 30.5);
        assert!(matches!(
            cpu.platform_type,
            crate::device::CpuPlatformType::AppleSilicon
        ));

        let apple_info = cpu.apple_silicon_info.as_ref().unwrap();
        assert_eq!(apple_info.p_core_count, 8);
        assert_eq!(apple_info.e_core_count, 4);
        assert_eq!(apple_info.p_core_utilization, 25.2);
        assert_eq!(apple_info.e_core_utilization, 10.8);
    }

    #[test]
    fn test_parse_memory_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_memory_total_bytes{instance="node-0058", hostname="node-0058", index="0"} 137438953472
all_smi_memory_used_bytes{instance="node-0058", hostname="node-0058", index="0"} 68719476736
all_smi_memory_available_bytes{instance="node-0058", hostname="node-0058", index="0"} 68719476736
all_smi_memory_utilization{instance="node-0058", hostname="node-0058", index="0"} 50.0
"#;

        let (_, _, memory_info, _) = parser.parse_metrics(test_data, host, &re);

        assert_eq!(memory_info.len(), 1);
        let memory = &memory_info[0];
        assert_eq!(memory.host_id, host);
        assert_eq!(memory.hostname, "node-0058");
        assert_eq!(memory.instance, "node-0058");
        assert_eq!(memory.total_bytes, 137438953472);
        assert_eq!(memory.used_bytes, 68719476736);
        assert_eq!(memory.available_bytes, 68719476736);
        assert_eq!(memory.utilization, 50.0);
    }

    #[test]
    fn test_parse_storage_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_disk_total_bytes{instance="node-0058", mount_point="/", index="0"} 4398046511104
all_smi_disk_available_bytes{instance="node-0058", mount_point="/", index="0"} 891915494941
all_smi_disk_total_bytes{instance="node-0058", mount_point="/home", index="1"} 1099511627776
all_smi_disk_available_bytes{instance="node-0058", mount_point="/home", index="1"} 549755813888
"#;

        let (_, _, _, storage_info) = parser.parse_metrics(test_data, host, &re);

        assert_eq!(storage_info.len(), 2);

        let root_storage = storage_info.iter().find(|s| s.mount_point == "/").unwrap();
        assert_eq!(root_storage.host_id, host);
        assert_eq!(root_storage.hostname, "node-0058");
        assert_eq!(root_storage.total_bytes, 4398046511104);
        assert_eq!(root_storage.available_bytes, 891915494941);
        assert_eq!(root_storage.index, 0);

        let home_storage = storage_info
            .iter()
            .find(|s| s.mount_point == "/home")
            .unwrap();
        assert_eq!(home_storage.host_id, host);
        assert_eq!(home_storage.hostname, "node-0058");
        assert_eq!(home_storage.total_bytes, 1099511627776);
        assert_eq!(home_storage.available_bytes, 549755813888);
        assert_eq!(home_storage.index, 1);
    }

    #[test]
    fn test_parse_mixed_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_gpu_utilization{gpu="NVIDIA RTX 4090", instance="node-0001", uuid="GPU-ABCDE", index="0"} 75.0
all_smi_cpu_utilization{cpu_model="AMD Ryzen", instance="node-0001", hostname="node-0001", index="0"} 60.0
all_smi_memory_total_bytes{instance="node-0001", hostname="node-0001", index="0"} 68719476736
all_smi_disk_total_bytes{instance="node-0001", mount_point="/", index="0"} 2199023255552
"#;

        let (gpu_info, cpu_info, memory_info, storage_info) =
            parser.parse_metrics(test_data, host, &re);

        assert_eq!(gpu_info.len(), 1);
        assert_eq!(cpu_info.len(), 1);
        assert_eq!(memory_info.len(), 1);
        assert_eq!(storage_info.len(), 1);

        assert_eq!(gpu_info[0].name, "NVIDIA RTX 4090");
        assert_eq!(gpu_info[0].utilization, 75.0);
        assert_eq!(gpu_info[0].host_id, host);
        assert_eq!(gpu_info[0].hostname, "node-0001");
        assert_eq!(gpu_info[0].instance, "node-0001");

        assert_eq!(cpu_info[0].cpu_model, "AMD Ryzen");
        assert_eq!(cpu_info[0].utilization, 60.0);
        assert!(matches!(
            cpu_info[0].platform_type,
            crate::device::CpuPlatformType::Amd
        ));

        assert_eq!(memory_info[0].total_bytes, 68719476736);
        assert_eq!(storage_info[0].total_bytes, 2199023255552);
    }

    #[test]
    fn test_invalid_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
invalid_metric_format
all_smi_gpu_utilization{malformed labels} invalid_value
all_smi_unknown_metric{instance="test"} 42.0
"#;

        let (gpu_info, cpu_info, memory_info, storage_info) =
            parser.parse_metrics(test_data, host, &re);

        assert!(gpu_info.is_empty());
        assert!(cpu_info.is_empty());
        assert!(memory_info.is_empty());
        assert!(storage_info.is_empty());
    }

    #[test]
    fn test_empty_metrics() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let (gpu_info, cpu_info, memory_info, storage_info) = parser.parse_metrics("", host, &re);

        assert!(gpu_info.is_empty());
        assert!(cpu_info.is_empty());
        assert!(memory_info.is_empty());
        assert!(storage_info.is_empty());
    }

    #[test]
    fn test_hostname_update() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_gpu_utilization{gpu="Tesla V100", instance="production-node-42", uuid="GPU-XYZ", index="0"} 85.0
all_smi_cpu_utilization{cpu_model="Intel Xeon", instance="production-node-42", hostname="node-0058", index="0"} 55.0
"#;

        let (gpu_info, cpu_info, _, _) = parser.parse_metrics(test_data, host, &re);

        assert_eq!(gpu_info[0].host_id, host);
        assert_eq!(gpu_info[0].hostname, "production-node-42");
        assert_eq!(gpu_info[0].instance, "production-node-42");
        assert_eq!(cpu_info[0].host_id, host);
        assert_eq!(cpu_info[0].hostname, "production-node-42");
        assert_eq!(cpu_info[0].instance, "production-node-42");
    }

    #[test]
    fn test_cpu_platform_detection() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_cases = [
            ("Apple M1 Pro", crate::device::CpuPlatformType::AppleSilicon),
            ("Intel Core i9", crate::device::CpuPlatformType::Intel),
            ("AMD Ryzen 9", crate::device::CpuPlatformType::Amd),
            (
                "Unknown Processor",
                crate::device::CpuPlatformType::Other("Unknown".to_string()),
            ),
        ];

        for (cpu_model, expected_type) in test_cases {
            let test_data = format!(
                r#"all_smi_cpu_utilization{{cpu_model="{cpu_model}", instance="test", hostname="test", index="0"}} 50.0"#
            );

            let (_, cpu_info, _, _) = parser.parse_metrics(&test_data, host, &re);
            assert_eq!(cpu_info.len(), 1);

            match (&cpu_info[0].platform_type, &expected_type) {
                (
                    crate::device::CpuPlatformType::AppleSilicon,
                    crate::device::CpuPlatformType::AppleSilicon,
                ) => {}
                (crate::device::CpuPlatformType::Intel, crate::device::CpuPlatformType::Intel) => {}
                (crate::device::CpuPlatformType::Amd, crate::device::CpuPlatformType::Amd) => {}
                (
                    crate::device::CpuPlatformType::Other(actual),
                    crate::device::CpuPlatformType::Other(expected),
                ) => {
                    assert_eq!(actual, expected);
                }
                _ => panic!(
                    "Platform type mismatch for {}: expected {:?}, got {:?}",
                    cpu_model, expected_type, cpu_info[0].platform_type
                ),
            }
        }
    }

    #[test]
    fn test_missing_required_fields() {
        let parser = create_test_parser();
        let re = create_test_regex();
        let host = "127.0.0.1:10058";

        let test_data = r#"
all_smi_gpu_utilization{instance="node-0058", index="0"} 25.5
all_smi_disk_total_bytes{instance="node-0058", index="0"} 1000000000
"#;

        let (gpu_info, _, _, storage_info) = parser.parse_metrics(test_data, host, &re);

        assert!(gpu_info.is_empty());
        assert!(storage_info.is_empty());
    }
}
