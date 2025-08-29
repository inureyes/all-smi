// Copyright 2025 Lablup Inc. and Jeongkyu Shin
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::{MetricBuilder, MetricExporter};
use crate::device::GpuInfo;

pub struct NpuMetricExporter<'a> {
    pub npu_info: &'a [GpuInfo],
}

impl<'a> NpuMetricExporter<'a> {
    pub fn new(npu_info: &'a [GpuInfo]) -> Self {
        Self { npu_info }
    }

    fn export_generic_npu_metrics(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        if info.device_type != "NPU" {
            return;
        }

        // NPU-specific firmware version
        if let Some(firmware) = info.detail.get("firmware") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("firmware", firmware.as_str()),
            ];
            builder
                .help("all_smi_npu_firmware_info", "NPU firmware version")
                .type_("all_smi_npu_firmware_info", "gauge")
                .metric("all_smi_npu_firmware_info", &fw_labels, 1);
        }
    }

    fn export_rebellions_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize) {
        if !info.name.contains("Rebellions") {
            return;
        }

        // Base labels
        let _base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // Rebellions firmware info
        if let Some(fw_version) = info.detail.get("Firmware Version") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("firmware", fw_version.as_str()),
            ];
            builder
                .help(
                    "all_smi_rebellions_firmware_info",
                    "Rebellions NPU firmware version",
                )
                .type_("all_smi_rebellions_firmware_info", "gauge")
                .metric("all_smi_rebellions_firmware_info", &fw_labels, 1);
        }

        // KMD version
        if let Some(kmd_version) = info.detail.get("KMD Version") {
            let kmd_labels = [
                ("instance", info.instance.as_str()),
                ("version", kmd_version.as_str()),
            ];
            builder
                .help("all_smi_rebellions_kmd_info", "Rebellions KMD version")
                .type_("all_smi_rebellions_kmd_info", "gauge")
                .metric("all_smi_rebellions_kmd_info", &kmd_labels, 1);
        }

        // Device info
        if let Some(_device_name) = info.detail.get("Device Name") {
            if let Some(sid) = info.detail.get("Serial ID") {
                let model_type = if info.name.contains("ATOM Max") {
                    "ATOM-Max"
                } else if info.name.contains("ATOM+") {
                    "ATOM-Plus"
                } else {
                    "ATOM"
                };

                let device_labels = [
                    ("npu", info.name.as_str()),
                    ("instance", info.instance.as_str()),
                    ("uuid", info.uuid.as_str()),
                    ("index", &index.to_string()),
                    ("model", model_type),
                    ("sid", sid.as_str()),
                    ("location", "5"), // Default location from mock server
                ];
                builder
                    .help(
                        "all_smi_rebellions_device_info",
                        "Rebellions device information",
                    )
                    .type_("all_smi_rebellions_device_info", "gauge")
                    .metric("all_smi_rebellions_device_info", &device_labels, 1);
            }
        }

        // Performance state
        if let Some(pstate) = info.detail.get("Performance State") {
            let pstate_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("pstate", pstate.as_str()),
            ];
            builder
                .help(
                    "all_smi_rebellions_pstate_info",
                    "Current performance state",
                )
                .type_("all_smi_rebellions_pstate_info", "gauge")
                .metric("all_smi_rebellions_pstate_info", &pstate_labels, 1);
        }

        // Device status
        if let Some(status) = info.detail.get("Status") {
            let status_value = if status == "normal" { 1.0 } else { 0.0 };
            let status_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("status", status.as_str()),
            ];
            builder
                .help("all_smi_rebellions_status", "Device operational status")
                .type_("all_smi_rebellions_status", "gauge")
                .metric("all_smi_rebellions_status", &status_labels, status_value);
        }
    }

    fn export_tenstorrent_metrics(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        if !info.name.contains("Tenstorrent") {
            return;
        }

        // Firmware versions
        self.export_tenstorrent_firmware(builder, info, index);

        // Temperature sensors
        self.export_tenstorrent_temperatures(builder, info, index);

        // Clock frequencies
        self.export_tenstorrent_clocks(builder, info, index);

        // Power metrics
        self.export_tenstorrent_power(builder, info, index);

        // Status and health metrics
        self.export_tenstorrent_status_health(builder, info, index);

        // Board and system info
        self.export_tenstorrent_board_info(builder, info, index);

        // PCIe and DRAM info
        self.export_tenstorrent_pcie_dram(builder, info, index);
    }

    fn export_tenstorrent_firmware(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        // ARC firmware
        if let Some(arc_fw) = info.detail.get("arc_fw_version") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("version", arc_fw.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_arc_firmware_info",
                    "ARC firmware version",
                )
                .type_("all_smi_tenstorrent_arc_firmware_info", "gauge")
                .metric("all_smi_tenstorrent_arc_firmware_info", &fw_labels, 1);
        }

        // Ethernet firmware
        if let Some(eth_fw) = info.detail.get("eth_fw_version") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("version", eth_fw.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_eth_firmware_info",
                    "Ethernet firmware version",
                )
                .type_("all_smi_tenstorrent_eth_firmware_info", "gauge")
                .metric("all_smi_tenstorrent_eth_firmware_info", &fw_labels, 1);
        }

        // Firmware date
        if let Some(fw_date) = info.detail.get("fw_date") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("date", fw_date.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_firmware_date_info",
                    "Firmware build date",
                )
                .type_("all_smi_tenstorrent_firmware_date_info", "gauge")
                .metric("all_smi_tenstorrent_firmware_date_info", &fw_labels, 1);
        }

        // DDR firmware
        if let Some(ddr_fw) = info.detail.get("ddr_fw_version") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("version", ddr_fw.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_ddr_firmware_info",
                    "DDR firmware version",
                )
                .type_("all_smi_tenstorrent_ddr_firmware_info", "gauge")
                .metric("all_smi_tenstorrent_ddr_firmware_info", &fw_labels, 1);
        }

        // SPI Boot ROM firmware
        if let Some(spi_fw) = info.detail.get("spibootrom_fw_version") {
            let fw_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("version", spi_fw.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_spibootrom_firmware_info",
                    "SPI Boot ROM firmware version",
                )
                .type_("all_smi_tenstorrent_spibootrom_firmware_info", "gauge")
                .metric(
                    "all_smi_tenstorrent_spibootrom_firmware_info",
                    &fw_labels,
                    1,
                );
        }
    }

    fn export_tenstorrent_temperatures(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        let base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // ASIC temperature (main chip temperature)
        if let Some(asic_temp) = info.detail.get("asic_temperature") {
            if let Ok(temp) = asic_temp.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_asic_temperature_celsius",
                        "ASIC temperature in celsius",
                    )
                    .type_("all_smi_tenstorrent_asic_temperature_celsius", "gauge")
                    .metric(
                        "all_smi_tenstorrent_asic_temperature_celsius",
                        &base_labels,
                        temp,
                    );
            }
        }

        // Voltage regulator temperature
        if let Some(vreg_temp) = info.detail.get("vreg_temperature") {
            if let Ok(temp) = vreg_temp.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_vreg_temperature_celsius",
                        "Voltage regulator temperature in celsius",
                    )
                    .type_("all_smi_tenstorrent_vreg_temperature_celsius", "gauge")
                    .metric(
                        "all_smi_tenstorrent_vreg_temperature_celsius",
                        &base_labels,
                        temp,
                    );
            }
        }

        // Inlet temperature
        if let Some(inlet_temp) = info.detail.get("inlet_temperature") {
            if let Ok(temp) = inlet_temp.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_inlet_temperature_celsius",
                        "Inlet temperature in celsius",
                    )
                    .type_("all_smi_tenstorrent_inlet_temperature_celsius", "gauge")
                    .metric(
                        "all_smi_tenstorrent_inlet_temperature_celsius",
                        &base_labels,
                        temp,
                    );
            }
        }

        // Outlet temperatures
        if let Some(outlet_temp1) = info.detail.get("outlet_temperature1") {
            if let Ok(temp) = outlet_temp1.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_outlet1_temperature_celsius",
                        "Outlet 1 temperature in celsius",
                    )
                    .type_("all_smi_tenstorrent_outlet1_temperature_celsius", "gauge")
                    .metric(
                        "all_smi_tenstorrent_outlet1_temperature_celsius",
                        &base_labels,
                        temp,
                    );
            }
        }

        if let Some(outlet_temp2) = info.detail.get("outlet_temperature2") {
            if let Ok(temp) = outlet_temp2.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_outlet2_temperature_celsius",
                        "Outlet 2 temperature in celsius",
                    )
                    .type_("all_smi_tenstorrent_outlet2_temperature_celsius", "gauge")
                    .metric(
                        "all_smi_tenstorrent_outlet2_temperature_celsius",
                        &base_labels,
                        temp,
                    );
            }
        }
    }

    fn export_tenstorrent_clocks(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize) {
        let base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // AI clock
        if let Some(aiclk) = info.detail.get("aiclk_mhz") {
            if let Ok(freq) = aiclk.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_aiclk_mhz", "AI clock frequency in MHz")
                    .type_("all_smi_tenstorrent_aiclk_mhz", "gauge")
                    .metric("all_smi_tenstorrent_aiclk_mhz", &base_labels, freq);
            }
        }

        // AXI clock
        if let Some(axiclk) = info.detail.get("axiclk_mhz") {
            if let Ok(freq) = axiclk.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_axiclk_mhz",
                        "AXI clock frequency in MHz",
                    )
                    .type_("all_smi_tenstorrent_axiclk_mhz", "gauge")
                    .metric("all_smi_tenstorrent_axiclk_mhz", &base_labels, freq);
            }
        }

        // ARC clock
        if let Some(arcclk) = info.detail.get("arcclk_mhz") {
            if let Ok(freq) = arcclk.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_arcclk_mhz",
                        "ARC clock frequency in MHz",
                    )
                    .type_("all_smi_tenstorrent_arcclk_mhz", "gauge")
                    .metric("all_smi_tenstorrent_arcclk_mhz", &base_labels, freq);
            }
        }
    }

    fn export_tenstorrent_power(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize) {
        let base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // Voltage
        if let Some(voltage) = info.detail.get("voltage") {
            if let Ok(v) = voltage.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_voltage_volts", "Core voltage in volts")
                    .type_("all_smi_tenstorrent_voltage_volts", "gauge")
                    .metric("all_smi_tenstorrent_voltage_volts", &base_labels, v);
            }
        }

        // Current
        if let Some(current) = info.detail.get("current") {
            if let Ok(c) = current.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_current_amperes", "Current in amperes")
                    .type_("all_smi_tenstorrent_current_amperes", "gauge")
                    .metric("all_smi_tenstorrent_current_amperes", &base_labels, c);
            }
        }

        // Power limits
        if let Some(tdp_limit) = info.detail.get("power_limit_tdp") {
            if let Ok(power) = tdp_limit.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_power_limit_tdp_watts",
                        "TDP power limit in watts",
                    )
                    .type_("all_smi_tenstorrent_power_limit_tdp_watts", "gauge")
                    .metric(
                        "all_smi_tenstorrent_power_limit_tdp_watts",
                        &base_labels,
                        power,
                    );
            }
        }

        if let Some(tdc_limit) = info.detail.get("power_limit_tdc") {
            if let Ok(current) = tdc_limit.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_power_limit_tdc_amperes",
                        "TDC current limit in amperes",
                    )
                    .type_("all_smi_tenstorrent_power_limit_tdc_amperes", "gauge")
                    .metric(
                        "all_smi_tenstorrent_power_limit_tdc_amperes",
                        &base_labels,
                        current,
                    );
            }
        }

        // TDP limit (new field from enhanced metrics)
        if let Some(tdp_limit) = info.detail.get("tdp_limit") {
            if let Ok(power) = tdp_limit.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_tdp_limit_watts", "TDP limit in watts")
                    .type_("all_smi_tenstorrent_tdp_limit_watts", "gauge")
                    .metric("all_smi_tenstorrent_tdp_limit_watts", &base_labels, power);
            }
        }

        // TDC limit (new field from enhanced metrics)
        if let Some(tdc_limit) = info.detail.get("tdc_limit") {
            if let Ok(current) = tdc_limit.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_tdc_limit_amperes",
                        "TDC limit in amperes",
                    )
                    .type_("all_smi_tenstorrent_tdc_limit_amperes", "gauge")
                    .metric(
                        "all_smi_tenstorrent_tdc_limit_amperes",
                        &base_labels,
                        current,
                    );
            }
        }

        // Thermal limit
        if let Some(thermal_limit) = info.detail.get("thermal_limit") {
            if let Ok(temp) = thermal_limit.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_thermal_limit_celsius",
                        "Thermal limit in celsius",
                    )
                    .type_("all_smi_tenstorrent_thermal_limit_celsius", "gauge")
                    .metric(
                        "all_smi_tenstorrent_thermal_limit_celsius",
                        &base_labels,
                        temp,
                    );
            }
        }

        // Heartbeat
        if let Some(heartbeat) = info.detail.get("heartbeat") {
            if let Ok(hb) = heartbeat.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_heartbeat", "Device heartbeat counter")
                    .type_("all_smi_tenstorrent_heartbeat", "counter")
                    .metric("all_smi_tenstorrent_heartbeat", &base_labels, hb);
            }
        }

        // Raw power consumption in watts
        if let Some(power_watts) = info.detail.get("power_watts") {
            if let Ok(power) = power_watts.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_power_raw_watts",
                        "Raw power consumption in watts",
                    )
                    .type_("all_smi_tenstorrent_power_raw_watts", "gauge")
                    .metric("all_smi_tenstorrent_power_raw_watts", &base_labels, power);
            }
        }
    }

    fn export_tenstorrent_status_health(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        let base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // PCIe status
        if let Some(pcie_status) = info.detail.get("pcie_status") {
            let status_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("status", pcie_status.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_pcie_status_info",
                    "PCIe status register value",
                )
                .type_("all_smi_tenstorrent_pcie_status_info", "gauge")
                .metric("all_smi_tenstorrent_pcie_status_info", &status_labels, 1);
        }

        // Ethernet status
        if let Some(eth_status0) = info.detail.get("eth_status0") {
            let status_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("port", "0"),
                ("status", eth_status0.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_eth_status_info",
                    "Ethernet status register value",
                )
                .type_("all_smi_tenstorrent_eth_status_info", "gauge")
                .metric("all_smi_tenstorrent_eth_status_info", &status_labels, 1);
        }

        if let Some(eth_status1) = info.detail.get("eth_status1") {
            let status_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("port", "1"),
                ("status", eth_status1.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_eth_status_info",
                    "Ethernet status register value",
                )
                .type_("all_smi_tenstorrent_eth_status_info", "gauge")
                .metric("all_smi_tenstorrent_eth_status_info", &status_labels, 1);
        }

        // DDR status (as numeric register value)
        if let Some(ddr_status) = info.detail.get("ddr_status") {
            // Try to parse hex string to numeric value
            if let Ok(status_val) = u32::from_str_radix(ddr_status.trim_start_matches("0x"), 16) {
                builder
                    .help(
                        "all_smi_tenstorrent_ddr_status",
                        "DDR status register value",
                    )
                    .type_("all_smi_tenstorrent_ddr_status", "gauge")
                    .metric(
                        "all_smi_tenstorrent_ddr_status",
                        &base_labels,
                        status_val as f64,
                    );
            }
        }

        // ARC health counters
        if let Some(arc0_health) = info.detail.get("arc0_health") {
            if let Ok(health) = arc0_health.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_arc0_health", "ARC0 health counter")
                    .type_("all_smi_tenstorrent_arc0_health", "counter")
                    .metric("all_smi_tenstorrent_arc0_health", &base_labels, health);
            }
        }

        if let Some(arc3_health) = info.detail.get("arc3_health") {
            if let Ok(health) = arc3_health.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_arc3_health", "ARC3 health counter")
                    .type_("all_smi_tenstorrent_arc3_health", "counter")
                    .metric("all_smi_tenstorrent_arc3_health", &base_labels, health);
            }
        }

        // Faults register
        if let Some(faults) = info.detail.get("faults") {
            // Try to parse hex string to numeric value
            if let Ok(faults_val) = u32::from_str_radix(faults.trim_start_matches("0x"), 16) {
                builder
                    .help("all_smi_tenstorrent_faults", "Fault register value")
                    .type_("all_smi_tenstorrent_faults", "gauge")
                    .metric(
                        "all_smi_tenstorrent_faults",
                        &base_labels,
                        faults_val as f64,
                    );
            }
        }

        // Throttler state
        if let Some(throttler) = info.detail.get("throttler") {
            // Try to parse hex string to numeric value
            if let Ok(throttler_val) = u32::from_str_radix(throttler.trim_start_matches("0x"), 16) {
                builder
                    .help(
                        "all_smi_tenstorrent_throttler",
                        "Throttler state register value",
                    )
                    .type_("all_smi_tenstorrent_throttler", "gauge")
                    .metric(
                        "all_smi_tenstorrent_throttler",
                        &base_labels,
                        throttler_val as f64,
                    );
            }
        }

        // Fan metrics
        if let Some(fan_speed) = info.detail.get("fan_speed") {
            if let Ok(speed) = fan_speed.parse::<f64>() {
                builder
                    .help(
                        "all_smi_tenstorrent_fan_speed_percent",
                        "Fan speed percentage",
                    )
                    .type_("all_smi_tenstorrent_fan_speed_percent", "gauge")
                    .metric("all_smi_tenstorrent_fan_speed_percent", &base_labels, speed);
            }
        }

        if let Some(fan_rpm) = info.detail.get("fan_rpm") {
            if let Ok(rpm) = fan_rpm.parse::<f64>() {
                builder
                    .help("all_smi_tenstorrent_fan_rpm", "Fan speed in RPM")
                    .type_("all_smi_tenstorrent_fan_rpm", "gauge")
                    .metric("all_smi_tenstorrent_fan_rpm", &base_labels, rpm);
            }
        }
    }

    fn export_tenstorrent_board_info(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        // Board type and architecture
        if let Some(board_type) = info.detail.get("board_type") {
            let arch = if info.name.contains("Grayskull") {
                "grayskull"
            } else if info.name.contains("Wormhole") {
                "wormhole"
            } else if info.name.contains("Blackhole") {
                "blackhole"
            } else {
                "unknown"
            };

            let board_id = info
                .detail
                .get("board_id")
                .map(|s| s.as_str())
                .unwrap_or("");

            let board_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("board_type", board_type.as_str()),
                ("board_id", board_id),
                ("architecture", arch),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_board_info",
                    "Tenstorrent board information",
                )
                .type_("all_smi_tenstorrent_board_info", "gauge")
                .metric("all_smi_tenstorrent_board_info", &board_labels, 1);
        }

        // Collection method
        if let Some(method) = info.detail.get("collection_method") {
            let method_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("method", method.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_collection_method_info",
                    "Data collection method used",
                )
                .type_("all_smi_tenstorrent_collection_method_info", "gauge")
                .metric(
                    "all_smi_tenstorrent_collection_method_info",
                    &method_labels,
                    1,
                );
        }
    }

    fn export_tenstorrent_pcie_dram(
        &self,
        builder: &mut MetricBuilder,
        info: &GpuInfo,
        index: usize,
    ) {
        let base_labels = [
            ("npu", info.name.as_str()),
            ("instance", info.instance.as_str()),
            ("uuid", info.uuid.as_str()),
            ("index", &index.to_string()),
        ];

        // PCIe address
        if let Some(pcie_addr) = info.detail.get("pcie_address") {
            let pcie_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("address", pcie_addr.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_pcie_address_info",
                    "PCIe address information",
                )
                .type_("all_smi_tenstorrent_pcie_address_info", "gauge")
                .metric("all_smi_tenstorrent_pcie_address_info", &pcie_labels, 1);
        }

        // PCIe vendor and device ID
        if let Some(vendor_id) = info.detail.get("pcie_vendor_id") {
            if let Some(device_id) = info.detail.get("pcie_device_id") {
                let pcie_labels = [
                    ("npu", info.name.as_str()),
                    ("instance", info.instance.as_str()),
                    ("uuid", info.uuid.as_str()),
                    ("index", &index.to_string()),
                    ("vendor_id", vendor_id.as_str()),
                    ("device_id", device_id.as_str()),
                ];
                builder
                    .help(
                        "all_smi_tenstorrent_pcie_device_info",
                        "PCIe device identification",
                    )
                    .type_("all_smi_tenstorrent_pcie_device_info", "gauge")
                    .metric("all_smi_tenstorrent_pcie_device_info", &pcie_labels, 1);
            }
        }

        // PCIe generation (from enhanced metrics)
        if let Some(pcie_gen) = info.detail.get("pcie_link_gen") {
            if let Some(gen_str) = pcie_gen.strip_prefix("Gen") {
                if let Ok(gen) = gen_str.parse::<f64>() {
                    builder
                        .help("all_smi_tenstorrent_pcie_generation", "PCIe generation")
                        .type_("all_smi_tenstorrent_pcie_generation", "gauge")
                        .metric("all_smi_tenstorrent_pcie_generation", &base_labels, gen);
                }
            }
        }

        // PCIe width (from enhanced metrics)
        if let Some(pcie_width) = info.detail.get("pcie_link_width") {
            if let Some(width_str) = pcie_width.strip_prefix("x") {
                if let Ok(width) = width_str.parse::<f64>() {
                    builder
                        .help("all_smi_tenstorrent_pcie_width", "PCIe link width")
                        .type_("all_smi_tenstorrent_pcie_width", "gauge")
                        .metric("all_smi_tenstorrent_pcie_width", &base_labels, width);
                }
            }
        }

        // DRAM status - removed as it's now exported as numeric register value in status_health

        // DRAM speed
        if let Some(dram_speed) = info.detail.get("dram_speed") {
            let dram_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("speed", dram_speed.as_str()),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_dram_info",
                    "DRAM configuration information",
                )
                .type_("all_smi_tenstorrent_dram_info", "gauge")
                .metric("all_smi_tenstorrent_dram_info", &dram_labels, 1);
        }
    }
}

impl<'a> MetricExporter for NpuMetricExporter<'a> {
    fn export_metrics(&self) -> String {
        let mut builder = MetricBuilder::new();

        for (i, info) in self.npu_info.iter().enumerate() {
            self.export_generic_npu_metrics(&mut builder, info, i);
            self.export_tenstorrent_metrics(&mut builder, info, i);
            self.export_rebellions_metrics(&mut builder, info, i);
        }

        builder.build()
    }
}
