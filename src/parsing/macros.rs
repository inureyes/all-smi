//! Parsing macros for repeated text parsing patterns.

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

/// Parse a numeric metric value from a "Key: Value <SUFFIX>" line.
/// - Extracts substring after the first ':'.
/// - Takes the first whitespace-separated token.
/// - Strips the provided suffix (e.g., "MHz", "mW") if present.
/// - Additionally strips a trailing '%' if present (common in residency values).
/// - Parses the remainder into the requested numeric type.
///
/// Returns Option<T> (None if parsing fails).
#[macro_export]
macro_rules! parse_metric {
    ($line:expr, $suffix:expr, $ty:ty) => {{
        let opt = $crate::parsing::common::after_colon_trimmed($line)
            .and_then(|rest| rest.split_whitespace().next())
            .map(|tok| {
                let no_suffix = tok.trim_end_matches($suffix);
                // Common pattern: percentages like "64.29%"
                no_suffix.trim_end_matches('%').to_string()
            })
            .and_then(|num| $crate::parsing::common::parse_number::<$ty>(&num));
        opt
    }};
}

/// Parse a Prometheus-formatted metric line using a regex with 3 capture groups:
/// 1) metric name without the `all_smi_` prefix
/// 2) labels content inside braces `{}`
/// 3) numeric value
///
/// Example regex: r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$"
/// Returns Option<(String, String, f64)>
#[macro_export]
macro_rules! parse_prometheus {
    ($line:expr, $re:expr) => {{
        if let Some(cap) = $re.captures($line.trim()) {
            let name = cap.get(1).map(|m| m.as_str().to_string());
            let labels = cap.get(2).map(|m| m.as_str().to_string());
            let value = cap
                .get(3)
                .and_then(|m| m.as_str().parse::<f64>().ok())
                .unwrap_or(0.0);
            if let (Some(name), Some(labels)) = (name, labels) {
                Some((name, labels, value))
            } else {
                None
            }
        } else {
            None
        }
    }};
}

/// Extract a label value from a HashMap and insert it into a detail HashMap with a given key.
/// Useful for processing Prometheus label data.
///
/// Example usage:
/// ```
/// extract_label_to_detail!(labels, "cuda_version", gpu_info.detail, "cuda_version");
/// ```
#[macro_export]
macro_rules! extract_label_to_detail {
    ($labels:expr, $label_key:expr, $detail_map:expr, $detail_key:expr) => {
        if let Some(value) = $labels.get($label_key) {
            $detail_map.insert($detail_key.to_string(), value.clone());
        }
    };
    // Variant that uses the same key for both label and detail
    ($labels:expr, $key:expr, $detail_map:expr) => {
        extract_label_to_detail!($labels, $key, $detail_map, $key);
    };
}

/// Process multiple label extractions in a batch.
/// Takes a list of label keys and inserts them into the detail map.
///
/// Example usage:
/// ```
/// extract_labels_batch!(
///     labels, gpu_info.detail,
///     ["cuda_version", "driver_version", "architecture", "compute_capability"]
/// );
/// ```
#[macro_export]
macro_rules! extract_labels_batch {
    ($labels:expr, $detail_map:expr, [$($key:expr),* $(,)?]) => {
        $(
            extract_label_to_detail!($labels, $key, $detail_map);
        )*
    };
}

/// Update a struct field based on a metric name match.
/// Reduces repetitive match arms to single macro calls.
///
/// Example usage:
/// ```
/// update_metric_field!(metric_name, value, gpu_info, {
///     "gpu_utilization" => utilization as f64,
///     "gpu_memory_used_bytes" => used_memory as u64,
///     "gpu_temperature_celsius" => temperature as u32
/// });
/// ```
#[macro_export]
macro_rules! update_metric_field {
    ($metric_name:expr, $value:expr, $target:expr, {
        $($name:expr => $field:ident as $type:ty),* $(,)?
    }) => {
        match $metric_name {
            $(
                $name => $target.$field = $value as $type,
            )*
            _ => {}
        }
    };
}

/// Extract a label value from a HashMap with a default if not present.
/// Returns the value or a default.
///
/// Example usage:
/// ```
/// let gpu_name = get_label_or_default!(labels, "gpu");
/// let gpu_index = get_label_or_default!(labels, "index", "0");
/// ```
#[macro_export]
macro_rules! get_label_or_default {
    ($labels:expr, $key:expr) => {
        $labels.get($key).cloned().unwrap_or_default()
    };
    ($labels:expr, $key:expr, $default:expr) => {
        $labels
            .get($key)
            .cloned()
            .unwrap_or_else(|| $default.to_string())
    };
}

/// Update a field within an optional struct field.
/// Useful for updating fields in optional nested structures like apple_silicon_info.
///
/// Example usage:
/// ```
/// update_optional_field!(cpu_info, apple_silicon_info, p_core_count, value as u32);
/// ```
#[macro_export]
macro_rules! update_optional_field {
    ($parent:expr, $optional_field:ident, $field:ident, $value:expr) => {
        if let Some(ref mut inner) = $parent.$optional_field {
            inner.$field = $value;
        }
    };
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    #[test]
    fn test_parse_metric_frequency() {
        let line = "GPU HW active frequency: 444 MHz";
        let v = parse_metric!(line, "MHz", u32);
        assert_eq!(v, Some(444u32));
    }

    #[test]
    fn test_parse_metric_percentage() {
        let line = "E-Cluster HW active residency:  64.29% (details omitted)";
        let v = parse_metric!(line, "%", f64);
        assert!(v.is_some());
        assert!((v.unwrap() - 64.29).abs() < 1e-6);
    }

    #[test]
    fn test_parse_metric_power() {
        let line = "CPU Power: 475 mW";
        let v = parse_metric!(line, "mW", f64);
        assert_eq!(v, Some(475.0));
    }

    #[test]
    fn test_parse_metric_invalid() {
        let line = "Invalid Line";
        let v = parse_metric!(line, "MHz", u32);
        assert!(v.is_none());
    }

    #[test]
    fn test_parse_prometheus_success() {
        let re = Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap();
        let line = r#"all_smi_gpu_utilization{gpu="RTX", uuid="GPU-1"} 25.5"#;
        let parsed = parse_prometheus!(line, re);
        assert!(parsed.is_some());
        let (name, labels, value) = parsed.unwrap();
        assert_eq!(name, "gpu_utilization");
        assert!(labels.contains(r#"gpu="RTX""#));
        assert_eq!(value, 25.5);
    }

    #[test]
    fn test_parse_prometheus_invalid() {
        let re = Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap();
        let line = "bad format";
        let parsed = parse_prometheus!(line, re);
        assert!(parsed.is_none());
    }

    #[test]
    fn test_extract_label_to_detail() {
        use std::collections::HashMap;

        let mut labels = HashMap::new();
        labels.insert("cuda_version".to_string(), "11.8".to_string());
        labels.insert("driver_version".to_string(), "525.60.13".to_string());

        let mut detail = HashMap::new();

        extract_label_to_detail!(labels, "cuda_version", detail, "cuda_version");
        assert_eq!(detail.get("cuda_version"), Some(&"11.8".to_string()));

        extract_label_to_detail!(labels, "driver_version", detail);
        assert_eq!(detail.get("driver_version"), Some(&"525.60.13".to_string()));

        // Test non-existent label
        extract_label_to_detail!(labels, "non_existent", detail);
        assert_eq!(detail.get("non_existent"), None);
    }

    #[test]
    fn test_extract_labels_batch() {
        use std::collections::HashMap;

        let mut labels = HashMap::new();
        labels.insert("cuda_version".to_string(), "11.8".to_string());
        labels.insert("driver_version".to_string(), "525.60.13".to_string());
        labels.insert("architecture".to_string(), "Ampere".to_string());

        let mut detail = HashMap::new();

        extract_labels_batch!(
            labels,
            detail,
            [
                "cuda_version",
                "driver_version",
                "architecture",
                "non_existent"
            ]
        );

        assert_eq!(detail.get("cuda_version"), Some(&"11.8".to_string()));
        assert_eq!(detail.get("driver_version"), Some(&"525.60.13".to_string()));
        assert_eq!(detail.get("architecture"), Some(&"Ampere".to_string()));
        assert_eq!(detail.get("non_existent"), None);
    }

    #[test]
    fn test_update_metric_field() {
        struct TestStruct {
            utilization: f64,
            memory: u64,
            temperature: u32,
        }

        let mut test = TestStruct {
            utilization: 0.0,
            memory: 0,
            temperature: 0,
        };

        let metric_name = "gpu_utilization";
        let value = 75.5;

        update_metric_field!(metric_name, value, test, {
            "gpu_utilization" => utilization as f64,
            "gpu_memory_used_bytes" => memory as u64,
            "gpu_temperature_celsius" => temperature as u32
        });

        assert_eq!(test.utilization, 75.5);

        let metric_name = "gpu_memory_used_bytes";
        let value = 1024.0;

        update_metric_field!(metric_name, value, test, {
            "gpu_utilization" => utilization as f64,
            "gpu_memory_used_bytes" => memory as u64,
            "gpu_temperature_celsius" => temperature as u32
        });

        assert_eq!(test.memory, 1024);
    }

    #[test]
    fn test_get_label_or_default() {
        use std::collections::HashMap;

        let mut labels = HashMap::new();
        labels.insert("gpu".to_string(), "RTX 4090".to_string());
        labels.insert("index".to_string(), "2".to_string());

        let gpu_name = get_label_or_default!(labels, "gpu");
        assert_eq!(gpu_name, "RTX 4090");

        let non_existent = get_label_or_default!(labels, "non_existent");
        assert_eq!(non_existent, "");

        let custom_default = get_label_or_default!(labels, "non_existent", "N/A");
        assert_eq!(custom_default, "N/A");

        let index = get_label_or_default!(labels, "index", "0");
        assert_eq!(index, "2");
    }
}
