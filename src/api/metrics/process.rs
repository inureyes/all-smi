use super::{MetricBuilder, MetricExporter};
use crate::device::ProcessInfo;

pub struct ProcessMetricExporter<'a> {
    pub process_info: &'a [ProcessInfo],
}

impl<'a> ProcessMetricExporter<'a> {
    pub fn new(process_info: &'a [ProcessInfo]) -> Self {
        Self { process_info }
    }

    fn export_process_metrics(&self, builder: &mut MetricBuilder, process: &ProcessInfo) {
        let pid_str = process.pid.to_string();
        let device_id_str = process.device_id.to_string();

        let labels = [
            ("pid", pid_str.as_str()),
            ("name", process.process_name.as_str()),
            ("device_id", device_id_str.as_str()),
            ("device_uuid", process.device_uuid.as_str()),
        ];

        // Process memory usage
        builder
            .help(
                "all_smi_process_memory_used_bytes",
                "Process memory used in bytes",
            )
            .type_("all_smi_process_memory_used_bytes", "gauge")
            .metric(
                "all_smi_process_memory_used_bytes",
                &labels,
                process.used_memory,
            );
    }
}

impl<'a> MetricExporter for ProcessMetricExporter<'a> {
    fn export_metrics(&self) -> String {
        if self.process_info.is_empty() {
            return String::new();
        }

        let mut builder = MetricBuilder::new();

        for process in self.process_info {
            self.export_process_metrics(&mut builder, process);
        }

        builder.build()
    }
}
