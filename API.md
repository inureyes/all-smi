# API Metrics Reference

`all-smi` provides comprehensive hardware metrics in Prometheus format through its API mode. This document details all available metrics across different hardware platforms.

## Starting API Mode

```bash
# Start API server
all-smi api --port 9090

# Custom update interval (default: 3 seconds)
all-smi api --port 9090 --interval 5

# Include process information
all-smi api --port 9090 --processes
```

Metrics are available at `http://localhost:9090/metrics`

## Available Metrics

### GPU Metrics (All Platforms)

| Metric                                | Description                | Unit    | Labels                                    |
|---------------------------------------|----------------------------|---------|-------------------------------------------|
| `all_smi_gpu_utilization`             | GPU utilization percentage | percent | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_memory_used_bytes`       | GPU memory used            | bytes   | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_memory_total_bytes`      | GPU memory total           | bytes   | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_temperature_celsius`     | GPU temperature            | celsius | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_power_consumption_watts` | GPU power consumption      | watts   | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_frequency_mhz`           | GPU frequency              | MHz     | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_info`                    | GPU device information     | info    | `gpu_index`, `gpu_name`, `driver_version` |

### NVIDIA GPU Specific Metrics

| Metric                                  | Description                              | Unit  | Labels                  |
|-----------------------------------------|------------------------------------------|-------|-------------------------|
| `all_smi_gpu_pcie_gen_current`          | Current PCIe generation                  | -     | `gpu_index`, `gpu_name` |
| `all_smi_gpu_pcie_width_current`        | Current PCIe link width                  | -     | `gpu_index`, `gpu_name` |
| `all_smi_gpu_performance_state`         | GPU performance state (P0=0, P1=1, etc.) | -     | `gpu_index`, `gpu_name` |
| `all_smi_gpu_clock_graphics_max_mhz`    | Maximum graphics clock                   | MHz   | `gpu_index`, `gpu_name` |
| `all_smi_gpu_clock_memory_max_mhz`      | Maximum memory clock                     | MHz   | `gpu_index`, `gpu_name` |
| `all_smi_gpu_power_limit_current_watts` | Current power limit                      | watts | `gpu_index`, `gpu_name` |
| `all_smi_gpu_power_limit_max_watts`     | Maximum power limit                      | watts | `gpu_index`, `gpu_name` |

### NVIDIA Jetson Specific Metrics

| Metric                    | Description                                 | Unit    | Labels                  |
|---------------------------|---------------------------------------------|---------|-------------------------|
| `all_smi_dla_utilization` | DLA (Deep Learning Accelerator) utilization | percent | `gpu_index`, `gpu_name` |

### Apple Silicon GPU Specific Metrics

| Metric                          | Description            | Unit  | Labels                           |
|---------------------------------|------------------------|-------|----------------------------------|
| `all_smi_ane_utilization`       | ANE utilization        | mW    | `gpu_index`, `gpu_name`          |
| `all_smi_ane_power_watts`       | ANE power consumption  | watts | `gpu_index`, `gpu_name`          |
| `all_smi_thermal_pressure_info` | Thermal pressure level | info  | `gpu_index`, `gpu_name`, `level` |

Note: For Apple Silicon, `gpu_temperature_celsius` is not available; thermal pressure level is provided instead.

### Tenstorrent NPU Metrics

#### Basic NPU Metrics
| Metric                                | Description                | Unit    | Labels                                    |
|---------------------------------------|----------------------------|---------|-------------------------------------------|
| `all_smi_gpu_utilization`             | NPU utilization percentage | percent | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_memory_used_bytes`       | NPU memory used            | bytes   | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_memory_total_bytes`      | NPU memory total           | bytes   | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_temperature_celsius`     | NPU ASIC temperature       | celsius | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_power_consumption_watts` | NPU power consumption      | watts   | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_frequency_mhz`           | NPU AI clock frequency     | MHz     | `gpu_index`, `gpu_name`                   |
| `all_smi_gpu_info`                    | NPU device information     | info    | `gpu_index`, `gpu_name`, `driver_version` |
| `all_smi_npu_firmware_info`           | NPU firmware version       | info    | `npu`, `instance`, `uuid`, `index`, `firmware` |

#### Tenstorrent-Specific Metrics
| Metric                                          | Description                        | Unit    | Labels                                                    |
|-------------------------------------------------|------------------------------------|---------|-----------------------------------------------------------|
| `all_smi_tenstorrent_board_info`                | Board and architecture information | info    | `npu`, `instance`, `uuid`, `index`, `board_type`, `board_id`, `architecture` |
| `all_smi_tenstorrent_collection_method_info`    | Data collection method used        | info    | `npu`, `instance`, `uuid`, `index`, `method`             |
| **Firmware Versions**                           |                                    |         |                                                           |
| `all_smi_tenstorrent_arc_firmware_info`         | ARC firmware version               | info    | `npu`, `instance`, `uuid`, `index`, `version`            |
| `all_smi_tenstorrent_eth_firmware_info`         | Ethernet firmware version          | info    | `npu`, `instance`, `uuid`, `index`, `version`            |
| `all_smi_tenstorrent_ddr_firmware_info`         | DDR firmware version               | info    | `npu`, `instance`, `uuid`, `index`, `version`            |
| `all_smi_tenstorrent_spibootrom_firmware_info`  | SPI Boot ROM firmware version      | info    | `npu`, `instance`, `uuid`, `index`, `version`            |
| `all_smi_tenstorrent_firmware_date_info`        | Firmware build date                | info    | `npu`, `instance`, `uuid`, `index`, `date`               |
| **Temperature Sensors**                         |                                    |         |                                                           |
| `all_smi_tenstorrent_asic_temperature_celsius`  | ASIC temperature                   | celsius | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_vreg_temperature_celsius`  | Voltage regulator temperature      | celsius | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_inlet_temperature_celsius` | Inlet temperature                  | celsius | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_outlet1_temperature_celsius`| Outlet 1 temperature              | celsius | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_outlet2_temperature_celsius`| Outlet 2 temperature              | celsius | `npu`, `instance`, `uuid`, `index`                       |
| **Clock Frequencies**                           |                                    |         |                                                           |
| `all_smi_tenstorrent_aiclk_mhz`                | AI clock frequency                 | MHz     | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_axiclk_mhz`               | AXI clock frequency                | MHz     | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_arcclk_mhz`               | ARC clock frequency                | MHz     | `npu`, `instance`, `uuid`, `index`                       |
| **Power and Electrical**                        |                                    |         |                                                           |
| `all_smi_tenstorrent_voltage_volts`            | Core voltage                       | volts   | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_current_amperes`          | Current draw                       | amperes | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_power_raw_watts`          | Raw power consumption              | watts   | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_tdp_limit_watts`          | TDP limit                          | watts   | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_tdc_limit_amperes`        | TDC limit                          | amperes | `npu`, `instance`, `uuid`, `index`                       |
| **Status and Health**                           |                                    |         |                                                           |
| `all_smi_tenstorrent_heartbeat`                | Device heartbeat counter           | counter | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_arc0_health`              | ARC0 health counter                | counter | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_arc3_health`              | ARC3 health counter                | counter | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_faults`                   | Fault register value               | gauge   | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_throttler`                | Throttler state register           | gauge   | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_pcie_status_info`         | PCIe status register               | info    | `npu`, `instance`, `uuid`, `index`, `status`             |
| `all_smi_tenstorrent_eth_status_info`          | Ethernet status register           | info    | `npu`, `instance`, `uuid`, `index`, `port`, `status`     |
| `all_smi_tenstorrent_ddr_status`               | DDR status register                | gauge   | `npu`, `instance`, `uuid`, `index`                       |
| **Fan Metrics**                                 |                                    |         |                                                           |
| `all_smi_tenstorrent_fan_speed_percent`        | Fan speed percentage               | percent | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_fan_rpm`                  | Fan speed in RPM                   | gauge   | `npu`, `instance`, `uuid`, `index`                       |
| **PCIe Information**                            |                                    |         |                                                           |
| `all_smi_tenstorrent_pcie_generation`          | PCIe generation                    | gauge   | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_pcie_width`               | PCIe link width                    | gauge   | `npu`, `instance`, `uuid`, `index`                       |
| `all_smi_tenstorrent_pcie_address_info`        | PCIe address                       | info    | `npu`, `instance`, `uuid`, `index`, `address`            |
| `all_smi_tenstorrent_pcie_device_info`         | PCIe device identification         | info    | `npu`, `instance`, `uuid`, `index`, `vendor_id`, `device_id` |
| **DRAM Information**                            |                                    |         |                                                           |
| `all_smi_tenstorrent_dram_info`                | DRAM configuration                 | info    | `npu`, `instance`, `uuid`, `index`, `speed`              |

Note: Tenstorrent NPUs use the same basic metric names as GPUs for compatibility with existing monitoring infrastructure. Additional Tenstorrent-specific metrics provide detailed hardware monitoring capabilities.

### CPU Metrics (All Platforms)

| Metric                                | Description                | Unit    | Labels   |
|---------------------------------------|----------------------------|---------|----------|
| `all_smi_cpu_utilization`             | CPU utilization percentage | percent | -        |
| `all_smi_cpu_socket_count`            | Number of CPU sockets      | count   | -        |
| `all_smi_cpu_core_count`              | Total number of CPU cores  | count   | -        |
| `all_smi_cpu_thread_count`            | Total number of CPU threads| count   | -        |
| `all_smi_cpu_frequency_mhz`           | CPU frequency              | MHz     | -        |
| `all_smi_cpu_temperature_celsius`     | CPU temperature            | celsius | -        |
| `all_smi_cpu_power_consumption_watts` | CPU power consumption      | watts   | -        |
| `all_smi_cpu_socket_utilization`      | Per-socket CPU utilization | percent | `socket` |

### Apple Silicon CPU Specific Metrics

| Metric                                | Description                    | Unit    | Labels |
|---------------------------------------|--------------------------------|---------|--------|
| `all_smi_cpu_p_core_count`            | Number of performance cores    | count   | -      |
| `all_smi_cpu_e_core_count`            | Number of efficiency cores     | count   | -      |
| `all_smi_cpu_gpu_core_count`          | Number of integrated GPU cores | count   | -      |
| `all_smi_cpu_p_core_utilization`      | P-core utilization percentage  | percent | -      |
| `all_smi_cpu_e_core_utilization`      | E-core utilization percentage  | percent | -      |
| `all_smi_cpu_p_cluster_frequency_mhz` | P-cluster frequency            | MHz     | -      |
| `all_smi_cpu_e_cluster_frequency_mhz` | E-cluster frequency            | MHz     | -      |

### Memory Metrics (All Platforms)

| Metric                           | Description                   | Unit    | Labels |
|----------------------------------|-------------------------------|---------|--------|
| `all_smi_memory_total_bytes`     | Total system memory           | bytes   | -      |
| `all_smi_memory_used_bytes`      | Used system memory            | bytes   | -      |
| `all_smi_memory_available_bytes` | Available system memory       | bytes   | -      |
| `all_smi_memory_free_bytes`      | Free system memory            | bytes   | -      |
| `all_smi_memory_utilization`     | Memory utilization percentage | percent | -      |
| `all_smi_swap_total_bytes`       | Total swap space              | bytes   | -      |
| `all_smi_swap_used_bytes`        | Used swap space               | bytes   | -      |
| `all_smi_swap_free_bytes`        | Free swap space               | bytes   | -      |

### Linux-Specific Memory Metrics

| Metric                         | Description             | Unit  | Labels |
|--------------------------------|-------------------------|-------|--------|
| `all_smi_memory_buffers_bytes` | Memory used for buffers | bytes | -      |
| `all_smi_memory_cached_bytes`  | Memory used for cache   | bytes | -      |

### Storage Metrics

| Metric                         | Description          | Unit  | Labels        |
|--------------------------------|----------------------|-------|---------------|
| `all_smi_disk_total_bytes`     | Total disk space     | bytes | `mount_point` |
| `all_smi_disk_available_bytes` | Available disk space | bytes | `mount_point` |

Note: Storage metrics exclude Docker bind mounts and are filtered to show only relevant filesystems.

### Process Metrics (When --processes Flag is Used)

| Metric                             | Description                     | Unit    | Labels                                                 |
|------------------------------------|---------------------------------|---------|--------------------------------------------------------|
| `all_smi_gpu_process_memory_bytes` | GPU memory used by process      | bytes   | `gpu_index`, `gpu_name`, `pid`, `process_name`, `user` |
| `all_smi_gpu_process_sm_util`      | Process GPU SM utilization      | percent | `gpu_index`, `gpu_name`, `pid`, `process_name`, `user` |
| `all_smi_gpu_process_mem_util`     | Process GPU memory utilization  | percent | `gpu_index`, `gpu_name`, `pid`, `process_name`, `user` |
| `all_smi_gpu_process_enc_util`     | Process GPU encoder utilization | percent | `gpu_index`, `gpu_name`, `pid`, `process_name`, `user` |
| `all_smi_gpu_process_dec_util`     | Process GPU decoder utilization | percent | `gpu_index`, `gpu_name`, `pid`, `process_name`, `user` |

## Platform Support Matrix

| Platform                     | GPU Metrics    | CPU Metrics    | Memory Metrics | Process Metrics |
|------------------------------|----------------|----------------|----------------|-----------------|  
| Linux + NVIDIA               | ✓ Full         | ✓ Full         | ✓ Full         | ✓ Full          |
| Linux + Tenstorrent          | ✓ Full***      | ✓ Full         | ✓ Full         | ✗ N/A****       |
| macOS + Apple Silicon        | ✓ Partial*     | ✓ Enhanced**   | ✓ Full         | ✓ Basic         |
| NVIDIA Jetson                | ✓ Full + DLA   | ✓ Full         | ✓ Full         | ✓ Full          |

*Apple Silicon GPU metrics do not include temperature (thermal pressure provided instead)  
**Apple Silicon provides enhanced P-core/E-core metrics and cluster frequencies  
***Tenstorrent provides extensive hardware monitoring including multiple temperature sensors, health counters, and status registers  
****Tenstorrent NPUs do not expose per-process GPU usage information

## Example Prometheus Queries

### Basic Monitoring
```promql
# Average GPU utilization across all GPUs
avg(all_smi_gpu_utilization)

# Memory usage percentage per GPU
(all_smi_gpu_memory_used_bytes / all_smi_gpu_memory_total_bytes) * 100

# GPUs running above 80°C
all_smi_gpu_temperature_celsius > 80
```

### Power Monitoring
```promql
# Total power consumption across all GPUs
sum(all_smi_gpu_power_consumption_watts)

# Power efficiency (utilization per watt)
all_smi_gpu_utilization / all_smi_gpu_power_consumption_watts
```

### Apple Silicon Specific
```promql
# P-core vs E-core utilization comparison
all_smi_cpu_p_core_utilization - all_smi_cpu_e_core_utilization

# ANE power consumption in watts
all_smi_ane_power_watts
```

### Tenstorrent NPU Specific
```promql
# NPUs with high temperature on any sensor
max by (instance) ({
  __name__=~"all_smi_tenstorrent_.*_temperature_celsius",
  instance=~"tt.*"
}) > 80

# Power efficiency by board type
all_smi_gpu_utilization / on(instance) group_left(board_type) 
  (all_smi_tenstorrent_board_info * 0 + all_smi_gpu_power_consumption_watts)

# Throttling detection
all_smi_tenstorrent_throttler > 0

# Health monitoring - ARC processors not incrementing
rate(all_smi_tenstorrent_arc0_health[5m]) == 0
```

### Process Monitoring
```promql
# Top 5 GPU memory consumers
topk(5, all_smi_gpu_process_memory_bytes)

# Processes using more than 1GB GPU memory
all_smi_gpu_process_memory_bytes > 1073741824
```

## Integration Examples

### Grafana Dashboard
Create a comprehensive monitoring dashboard with:
- GPU utilization heatmap
- Memory usage time series
- Power consumption stacked graph
- Temperature alerts
- Process resource usage table

### AlertManager Rules
```yaml
groups:
  - name: gpu_alerts
    rules:
      - alert: HighGPUTemperature
        expr: all_smi_gpu_temperature_celsius > 85
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "GPU {{ $labels.gpu_name }} is running hot"
          
      - alert: GPUMemoryExhausted
        expr: (all_smi_gpu_memory_used_bytes / all_smi_gpu_memory_total_bytes) > 0.95
        for: 5m
        labels:
          severity: critical
          
      - alert: TenstorrentNPUFault
        expr: all_smi_tenstorrent_faults > 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Tenstorrent NPU {{ $labels.instance }} has fault condition"
          
      - alert: TenstorrentNPUThrottling
        expr: all_smi_tenstorrent_throttler > 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Tenstorrent NPU {{ $labels.instance }} is throttling"
```

## Update Intervals

The metrics update interval can be configured:
- Default: 3 seconds
- Minimum recommended: 1 second
- Maximum recommended: 60 seconds

Higher update rates provide more real-time data but increase system load. For production monitoring, 5-10 seconds is typically sufficient.

## Notes

1. All metrics follow Prometheus naming conventions
2. Labels are used to differentiate between multiple devices
3. Info metrics (ending in `_info`) provide static metadata
4. Some metrics may not be available on all platforms
5. Process metrics require the `--processes` flag and may impact performance
6. Tenstorrent NPU metrics include comprehensive hardware monitoring data:
   - Multiple temperature sensors (ASIC, voltage regulator, inlet/outlet)
   - Detailed firmware versions and health counters
   - Power limits (TDP/TDC) and throttling information
   - PCIe and DDR status registers for diagnostics
7. Tenstorrent utilization is calculated based on power consumption as a proxy metric