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

/// Data structure for Intel Gaudi accelerator metrics
/// Parses CSV output from hl-smi command
#[derive(Debug, Default, Clone)]
pub struct GaudiMetricsData {
    /// Per-device metrics
    pub devices: Vec<GaudiDeviceMetrics>,
}

#[derive(Debug, Clone)]
pub struct GaudiDeviceMetrics {
    /// Device index
    pub index: u32,
    /// Device UUID
    pub uuid: String,
    /// Device name (e.g., "HL-325L")
    pub name: String,
    /// Total memory in MiB
    pub memory_total: u64,
    /// Used memory in MiB
    pub memory_used: u64,
    /// Free memory in MiB
    pub memory_free: u64,
    /// Current power draw in Watts
    pub power_draw: f64,
    /// Maximum power limit in Watts
    pub power_max: f64,
    /// Temperature in Celsius
    pub temperature: u32,
    /// Utilization percentage (0-100)
    pub utilization: f64,
}

impl Default for GaudiDeviceMetrics {
    fn default() -> Self {
        Self {
            index: 0,
            uuid: String::new(),
            name: String::new(),
            memory_total: 0,
            memory_used: 0,
            memory_free: 0,
            power_draw: 0.0,
            power_max: 0.0,
            temperature: 0,
            utilization: 0.0,
        }
    }
}

/// Parse hl-smi CSV output
/// Expected format: index,uuid,name,memory.total,memory.used,memory.free,power.draw,power.max,temperature.aip,utilization.aip
/// Example: 0, 01P4-HL3090A0-18-U4V193-22-07-00, HL-325L, 131072 MiB, 672 MiB, 130400 MiB, 226 W, 850 W, 36 C, 0 %
pub fn parse_hlsmi_output(output: &str) -> Result<GaudiMetricsData, Box<dyn std::error::Error>> {
    let mut data = GaudiMetricsData::default();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() < 10 {
            continue; // Skip malformed lines
        }

        let device = GaudiDeviceMetrics {
            index: parse_index(parts[0])?,
            uuid: parts[1].to_string(),
            name: parts[2].to_string(),
            memory_total: parse_memory_mib(parts[3])?,
            memory_used: parse_memory_mib(parts[4])?,
            memory_free: parse_memory_mib(parts[5])?,
            power_draw: parse_power(parts[6])?,
            power_max: parse_power(parts[7])?,
            temperature: parse_temperature(parts[8])?,
            utilization: parse_utilization(parts[9])?,
        };

        data.devices.push(device);
    }

    Ok(data)
}

/// Parse device index
fn parse_index(s: &str) -> Result<u32, Box<dyn std::error::Error>> {
    s.trim()
        .parse::<u32>()
        .map_err(|e| format!("Failed to parse index '{s}': {e}").into())
}

/// Parse memory value in MiB format (e.g., "131072 MiB" -> 131072)
fn parse_memory_mib(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let s = s.trim().trim_end_matches("MiB").trim();
    s.parse::<u64>()
        .map_err(|e| format!("Failed to parse memory '{s}': {e}").into())
}

/// Parse power value in Watts format (e.g., "226 W" -> 226.0)
fn parse_power(s: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let s = s.trim().trim_end_matches('W').trim();
    s.parse::<f64>()
        .map_err(|e| format!("Failed to parse power '{s}': {e}").into())
}

/// Parse temperature value in Celsius format (e.g., "36 C" -> 36)
fn parse_temperature(s: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let s = s.trim().trim_end_matches('C').trim();
    s.parse::<u32>()
        .map_err(|e| format!("Failed to parse temperature '{s}': {e}").into())
}

/// Parse utilization percentage (e.g., "0 %" -> 0.0)
fn parse_utilization(s: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let s = s.trim().trim_end_matches('%').trim();
    s.parse::<f64>()
        .map_err(|e| format!("Failed to parse utilization '{s}': {e}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hlsmi_output() {
        let output = "0, 01P4-HL3090A0-18-U4V193-22-07-00, HL-325L, 131072 MiB, 672 MiB, 130400 MiB, 226 W, 850 W, 36 C, 0 %\n\
                      1, 01P4-HL3090A0-18-U4V298-03-04-04, HL-325L, 131072 MiB, 672 MiB, 130400 MiB, 230 W, 850 W, 39 C, 0 %";

        let result = parse_hlsmi_output(output);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert_eq!(data.devices.len(), 2);

        // Check first device
        assert_eq!(data.devices[0].index, 0);
        assert_eq!(data.devices[0].uuid, "01P4-HL3090A0-18-U4V193-22-07-00");
        assert_eq!(data.devices[0].name, "HL-325L");
        assert_eq!(data.devices[0].memory_total, 131072);
        assert_eq!(data.devices[0].memory_used, 672);
        assert_eq!(data.devices[0].memory_free, 130400);
        assert_eq!(data.devices[0].power_draw, 226.0);
        assert_eq!(data.devices[0].power_max, 850.0);
        assert_eq!(data.devices[0].temperature, 36);
        assert_eq!(data.devices[0].utilization, 0.0);

        // Check second device
        assert_eq!(data.devices[1].index, 1);
        assert_eq!(data.devices[1].temperature, 39);
    }

    #[test]
    fn test_parse_memory_mib() {
        assert_eq!(parse_memory_mib("131072 MiB").unwrap(), 131072);
        assert_eq!(parse_memory_mib("672 MiB").unwrap(), 672);
        assert_eq!(parse_memory_mib("130400 MiB").unwrap(), 130400);
    }

    #[test]
    fn test_parse_power() {
        assert_eq!(parse_power("226 W").unwrap(), 226.0);
        assert_eq!(parse_power("850 W").unwrap(), 850.0);
        assert_eq!(parse_power("0 W").unwrap(), 0.0);
    }

    #[test]
    fn test_parse_temperature() {
        assert_eq!(parse_temperature("36 C").unwrap(), 36);
        assert_eq!(parse_temperature("39 C").unwrap(), 39);
        assert_eq!(parse_temperature("0 C").unwrap(), 0);
    }

    #[test]
    fn test_parse_utilization() {
        assert_eq!(parse_utilization("0 %").unwrap(), 0.0);
        assert_eq!(parse_utilization("50 %").unwrap(), 50.0);
        assert_eq!(parse_utilization("100 %").unwrap(), 100.0);
    }
}
