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
                .type_("all_smi_npu_firmware_info", "info")
                .metric("all_smi_npu_firmware_info", &fw_labels, 1);
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
                .type_("all_smi_tenstorrent_arc_firmware_info", "info")
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
                .type_("all_smi_tenstorrent_eth_firmware_info", "info")
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
                .type_("all_smi_tenstorrent_firmware_date_info", "info")
                .metric("all_smi_tenstorrent_firmware_date_info", &fw_labels, 1);
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

            let board_labels = [
                ("npu", info.name.as_str()),
                ("instance", info.instance.as_str()),
                ("uuid", info.uuid.as_str()),
                ("index", &index.to_string()),
                ("board_type", board_type.as_str()),
                ("architecture", arch),
            ];
            builder
                .help(
                    "all_smi_tenstorrent_board_info",
                    "Tenstorrent board information",
                )
                .type_("all_smi_tenstorrent_board_info", "info")
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
                .type_("all_smi_tenstorrent_collection_method_info", "info")
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

        // PCIe generation
        if let Some(pcie_speed) = info.detail.get("pcie_speed") {
            if let Some(gen_str) = pcie_speed.strip_prefix("Gen") {
                if let Ok(gen) = gen_str.parse::<f64>() {
                    builder
                        .help("all_smi_tenstorrent_pcie_generation", "PCIe generation")
                        .type_("all_smi_tenstorrent_pcie_generation", "gauge")
                        .metric("all_smi_tenstorrent_pcie_generation", &base_labels, gen);
                }
            }
        }

        // PCIe width
        if let Some(pcie_width) = info.detail.get("pcie_width") {
            if let Some(width_str) = pcie_width.strip_prefix("x") {
                if let Ok(width) = width_str.parse::<f64>() {
                    builder
                        .help("all_smi_tenstorrent_pcie_width", "PCIe link width")
                        .type_("all_smi_tenstorrent_pcie_width", "gauge")
                        .metric("all_smi_tenstorrent_pcie_width", &base_labels, width);
                }
            }
        }

        // DRAM status
        if let Some(dram_status) = info.detail.get("dram_status") {
            let dram_enabled = if dram_status == "Y" { 1.0 } else { 0.0 };
            builder
                .help(
                    "all_smi_tenstorrent_dram_enabled",
                    "DRAM enabled status (1=enabled, 0=disabled)",
                )
                .type_("all_smi_tenstorrent_dram_enabled", "gauge")
                .metric(
                    "all_smi_tenstorrent_dram_enabled",
                    &base_labels,
                    dram_enabled,
                );
        }

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
                .type_("all_smi_tenstorrent_dram_info", "info")
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
        }

        builder.build()
    }
}
