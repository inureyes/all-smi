// Common parsing utilities for number extraction, unit conversion, and string sanitization.

use std::str::FromStr;

/// Parse a number from a string after sanitizing by removing commas, underscores, and trimming.
/// Returns None if parsing fails.
pub fn parse_number<T: FromStr>(s: &str) -> Option<T> {
    let cleaned = s.trim().replace([',', '_'], "");
    cleaned.parse::<T>().ok()
}

/// Convert a floating-point quantity with a unit into bytes.
/// Supported units (case-insensitive): B, KB, KiB, MB, MiB, GB, GiB, TB, TiB
#[allow(dead_code)]
pub fn to_bytes(value: f64, unit: &str) -> Option<u64> {
    let mul = match unit.trim().to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1_000.0,
        "KIB" => 1024.0,
        "MB" => 1_000_000.0,
        "MIB" => 1024.0_f64.powi(2),
        "GB" => 1_000_000_000.0,
        "GIB" => 1024.0_f64.powi(3),
        "TB" => 1_000_000_000_000.0,
        "TIB" => 1024.0_f64.powi(4),
        _ => return None,
    };
    let bytes = value * mul;
    if bytes.is_finite() && bytes >= 0.0 {
        Some(bytes as u64)
    } else {
        None
    }
}

/// Sanitize a quoted label/value by trimming whitespace and removing surrounding double quotes.
pub fn sanitize_label_value(s: &str) -> String {
    let trimmed = s.trim();
    trimmed.trim_matches('"').to_string()
}

/// Extract the substring that appears after the first ':' character, trimmed.
/// Returns None if ':' is not present.
pub fn after_colon_trimmed(line: &str) -> Option<&str> {
    line.split_once(':').map(|x| x.1).map(|s| s.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_number_util() {
        assert_eq!(parse_number::<u32>("1_234"), Some(1234));
        assert_eq!(parse_number::<u64>("1,234,567"), Some(1_234_567));
        assert_eq!(parse_number::<f64>("  3.1234 "), Some(3.1234));
        assert_eq!(parse_number::<i32>("abc"), None);
    }

    #[test]
    fn test_to_bytes() {
        assert_eq!(to_bytes(1.0, "B"), Some(1));
        assert_eq!(to_bytes(1.0, "KB"), Some(1_000));
        assert_eq!(to_bytes(1.0, "KiB"), Some(1024));
        assert_eq!(to_bytes(1.5, "MiB"), Some((1.5 * 1024.0 * 1024.0) as u64));
        assert_eq!(to_bytes(2.0, "GB"), Some(2_000_000_000));
        assert_eq!(to_bytes(1.0, "unknown"), None);
    }

    #[test]
    fn test_sanitize_label_value() {
        assert_eq!(sanitize_label_value(r#" "hello" "#), "hello".to_string());
        assert_eq!(sanitize_label_value("world"), "world".to_string());
    }

    #[test]
    fn test_after_colon_trimmed() {
        assert_eq!(after_colon_trimmed("Key: Value"), Some("Value"));
        assert_eq!(after_colon_trimmed("NoColon"), None);
    }
}
