use crate::utils::RuntimeEnvironment;

use super::MetricExporter;

pub struct RuntimeMetricExporter<'a> {
    runtime_env: &'a RuntimeEnvironment,
    hostname: String,
}

impl<'a> RuntimeMetricExporter<'a> {
    pub fn new(runtime_env: &'a RuntimeEnvironment) -> Self {
        Self {
            runtime_env,
            hostname: crate::utils::get_hostname(),
        }
    }
}

impl<'a> MetricExporter for RuntimeMetricExporter<'a> {
    fn export_metrics(&self) -> String {
        let mut output = String::new();

        // Container environment metrics
        if self.runtime_env.container.is_containerized() {
            let runtime_name = self.runtime_env.container.runtime.as_str();

            // Container runtime info metric
            output.push_str(&format!(
                "# HELP all_smi_container_runtime_info Container runtime environment information\n\
                 # TYPE all_smi_container_runtime_info gauge\n\
                 all_smi_container_runtime_info{{hostname=\"{}\",runtime=\"{}\",container_id=\"{}\"}} 1\n",
                self.hostname,
                runtime_name,
                self.runtime_env.container.container_id.as_deref().unwrap_or("unknown")
            ));

            // Additional Kubernetes-specific metrics
            if let crate::utils::ContainerRuntime::Kubernetes = self.runtime_env.container.runtime {
                if let Some(pod_name) = &self.runtime_env.container.pod_name {
                    output.push_str(&format!(
                        "# HELP all_smi_kubernetes_pod_info Kubernetes pod information\n\
                         # TYPE all_smi_kubernetes_pod_info gauge\n\
                         all_smi_kubernetes_pod_info{{hostname=\"{}\",pod_name=\"{}\",namespace=\"{}\"}} 1\n",
                        self.hostname,
                        pod_name,
                        self.runtime_env.container.namespace.as_deref().unwrap_or("default")
                    ));
                }
            }
        }

        // Virtualization environment metrics
        if self.runtime_env.virtualization.is_virtual {
            let vm_type = self.runtime_env.virtualization.vm_type.as_str();

            output.push_str(&format!(
                "# HELP all_smi_virtualization_info Virtualization environment information\n\
                 # TYPE all_smi_virtualization_info gauge\n\
                 all_smi_virtualization_info{{hostname=\"{}\",vm_type=\"{}\",hypervisor=\"{}\"}} 1\n",
                self.hostname,
                vm_type,
                self.runtime_env.virtualization.hypervisor.as_deref().unwrap_or(vm_type)
            ));
        }

        // Combined runtime environment metric (what would be displayed in UI)
        if let Some((name, _color)) = self.runtime_env.display_info() {
            output.push_str(&format!(
                "# HELP all_smi_runtime_environment Current runtime environment (container or VM)\n\
                 # TYPE all_smi_runtime_environment gauge\n\
                 all_smi_runtime_environment{{hostname=\"{}\",environment=\"{}\"}} 1\n",
                self.hostname,
                name
            ));
        }

        output
    }
}
