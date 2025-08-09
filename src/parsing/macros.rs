//! Parsing macros for repeated text parsing patterns.

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
}
