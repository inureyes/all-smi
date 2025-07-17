pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod memory;
pub mod npu;
pub mod process;

/// Trait for exporting metrics in Prometheus format
pub trait MetricExporter {
    /// Export metrics to Prometheus format string
    fn export_metrics(&self) -> String;
}

/// Helper struct to build Prometheus metrics
pub struct MetricBuilder {
    metrics: String,
}

impl MetricBuilder {
    pub fn new() -> Self {
        Self {
            metrics: String::new(),
        }
    }

    /// Add a comment line
    #[allow(dead_code)]
    pub fn comment(&mut self, text: &str) -> &mut Self {
        self.metrics.push_str("# ");
        self.metrics.push_str(text);
        self.metrics.push('\n');
        self
    }

    /// Add a HELP line
    pub fn help(&mut self, name: &str, description: &str) -> &mut Self {
        self.metrics
            .push_str(&format!("# HELP {name} {description}\n"));
        self
    }

    /// Add a TYPE line
    pub fn type_(&mut self, name: &str, metric_type: &str) -> &mut Self {
        self.metrics
            .push_str(&format!("# TYPE {name} {metric_type}\n"));
        self
    }

    /// Add a metric line with labels
    pub fn metric(
        &mut self,
        name: &str,
        labels: &[(&str, &str)],
        value: impl ToString,
    ) -> &mut Self {
        self.metrics.push_str(name);

        if !labels.is_empty() {
            self.metrics.push('{');
            for (i, (key, value)) in labels.iter().enumerate() {
                if i > 0 {
                    self.metrics.push_str(", ");
                }
                // Escape quotes in values for Prometheus format
                let escaped_value = value.replace('"', "\\\"");
                self.metrics.push_str(&format!("{key}=\"{escaped_value}\""));
            }
            self.metrics.push('}');
        }

        self.metrics.push(' ');
        self.metrics.push_str(&value.to_string());
        self.metrics.push('\n');
        self
    }

    /// Build the final metric string
    pub fn build(self) -> String {
        self.metrics
    }
}

impl Default for MetricBuilder {
    fn default() -> Self {
        Self::new()
    }
}
