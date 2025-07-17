use super::{MetricBuilder, MetricExporter};
use crate::utils::disk_filter::filter_docker_aware_disks;
use crate::utils::system::get_hostname;
use sysinfo::{Disk, Disks};

pub struct DiskMetricExporter {
    pub instance: String,
}

impl DiskMetricExporter {
    pub fn new(instance: Option<String>) -> Self {
        Self {
            instance: instance.unwrap_or_else(get_hostname),
        }
    }

    fn export_disk_metrics(&self, builder: &mut MetricBuilder, disk: &Disk, index: usize) {
        let labels = [
            ("instance", self.instance.as_str()),
            ("mount_point", &disk.mount_point().to_string_lossy()),
            ("index", &index.to_string()),
        ];

        // Total disk space
        builder
            .help("all_smi_disk_total_bytes", "Total disk space in bytes")
            .type_("all_smi_disk_total_bytes", "gauge")
            .metric("all_smi_disk_total_bytes", &labels, disk.total_space());

        // Available disk space
        builder
            .help(
                "all_smi_disk_available_bytes",
                "Available disk space in bytes",
            )
            .type_("all_smi_disk_available_bytes", "gauge")
            .metric(
                "all_smi_disk_available_bytes",
                &labels,
                disk.available_space(),
            );
    }
}

impl MetricExporter for DiskMetricExporter {
    fn export_metrics(&self) -> String {
        let mut builder = MetricBuilder::new();

        let disks = Disks::new_with_refreshed_list();
        let filtered_disks = filter_docker_aware_disks(&disks);

        for (index, disk) in filtered_disks.iter().enumerate() {
            self.export_disk_metrics(&mut builder, disk, index);
        }

        builder.build()
    }
}
