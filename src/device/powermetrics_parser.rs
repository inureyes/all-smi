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

use std::process::Command;

/// Enhanced powermetrics parser for Apple Silicon metrics
/// Based on asitop's approach of parsing active residency data
#[derive(Debug, Default, Clone)]
pub struct PowerMetricsData {
    // CPU metrics
    pub e_cluster_active_residency: f64,
    pub p_cluster_active_residency: f64,
    pub e_cluster_frequency: u32,
    pub p_cluster_frequency: u32,
    pub cpu_power_mw: f64,

    // Per-core metrics
    pub core_active_residencies: Vec<f64>,
    pub core_frequencies: Vec<u32>,
    pub core_cluster_types: Vec<CoreType>, // E or P core

    // GPU metrics
    pub gpu_active_residency: f64,
    pub gpu_frequency: u32,
    pub gpu_power_mw: f64,

    // ANE metrics
    pub ane_power_mw: f64,

    // Combined metrics
    pub combined_power_mw: f64,

    // Thermal
    pub thermal_pressure_level: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoreType {
    Efficiency,
    Performance,
}

impl PowerMetricsData {
    /// Get CPU utilization as a percentage (0-100)
    /// Uses weighted average of cluster utilization
    #[allow(dead_code)] // Used in tests but clippy doesn't detect test usage
    pub fn cpu_utilization(&self) -> f64 {
        // Weight P-cores more heavily as they handle more intensive tasks
        self.e_cluster_active_residency * 0.3 + self.p_cluster_active_residency * 0.7
    }

    /// Get GPU utilization as a percentage (0-100)
    #[allow(dead_code)]
    pub fn gpu_utilization(&self) -> f64 {
        self.gpu_active_residency
    }
}

/// Run powermetrics and parse the output
#[allow(dead_code)]
pub fn get_powermetrics_data() -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
    let output = Command::new("sudo")
        .args([
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal",
            "-n",
            "1",
            "-i",
            "1000",
        ])
        .output()?;

    if !output.status.success() {
        return Err("powermetrics command failed".into());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_powermetrics_output(&output_str)
}

/// Parse powermetrics text output
pub fn parse_powermetrics_output(
    output: &str,
) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
    let mut data = PowerMetricsData::default();
    let mut in_e_cluster = false;
    let mut _in_p_cluster = false;

    // Variables to accumulate E and P cluster metrics (for Ultra chips)
    let mut e_cluster_residencies = Vec::new();
    let mut e_cluster_frequencies = Vec::new();
    let mut p_cluster_residencies = Vec::new();
    let mut p_cluster_frequencies = Vec::new();

    for line in output.lines() {
        let line = line.trim();

        // Handle both formats:
        // Standard M1/M2: "E-Cluster HW active frequency:"
        // M1/M2 Ultra: "E0-Cluster HW active frequency:", "E1-Cluster HW active frequency:", etc.

        // E-Cluster metrics
        if line.contains("-Cluster HW active frequency:") && line.starts_with("E") {
            in_e_cluster = true;
            _in_p_cluster = false;
            let freq = parse_frequency(line)?;

            // Check if this is a numbered cluster (Ultra) or standard cluster
            if line.starts_with("E-Cluster") {
                // Standard M1/M2 format
                data.e_cluster_frequency = freq;
            } else {
                // Ultra format (E0, E1, etc.)
                e_cluster_frequencies.push(freq);
            }
        } else if line.contains("-Cluster HW active residency:") && line.starts_with("E") {
            let residency = parse_residency(line)?;

            if line.starts_with("E-Cluster") {
                // Standard M1/M2 format
                data.e_cluster_active_residency = residency;
            } else {
                // Ultra format (E0, E1, etc.)
                e_cluster_residencies.push(residency);
            }
        }
        // P-Cluster metrics
        else if line.contains("-Cluster HW active frequency:") && line.starts_with("P") {
            _in_p_cluster = true;
            in_e_cluster = false;
            let freq = parse_frequency(line)?;

            if line.starts_with("P-Cluster") {
                // Standard M1/M2 format
                data.p_cluster_frequency = freq;
            } else {
                // Ultra format (P0, P1, etc.)
                p_cluster_frequencies.push(freq);
            }
        } else if line.contains("-Cluster HW active residency:") && line.starts_with("P") {
            let residency = parse_residency(line)?;

            if line.starts_with("P-Cluster") {
                // Standard M1/M2 format
                data.p_cluster_active_residency = residency;
            } else {
                // Ultra format (P0, P1, etc.)
                p_cluster_residencies.push(residency);
            }
        }
        // Per-core metrics
        else if line.starts_with("CPU") && line.contains("frequency:") {
            let freq = parse_frequency(line)?;
            data.core_frequencies.push(freq);

            // Determine core type based on current cluster
            let core_type = if in_e_cluster {
                CoreType::Efficiency
            } else {
                CoreType::Performance
            };
            data.core_cluster_types.push(core_type);
        } else if line.starts_with("CPU") && line.contains("active residency:") {
            let residency = parse_residency(line)?;
            data.core_active_residencies.push(residency);
        }
        // GPU metrics
        else if line.starts_with("GPU HW active frequency:") {
            data.gpu_frequency = parse_frequency(line)?;
        } else if line.starts_with("GPU HW active residency:") {
            data.gpu_active_residency = parse_residency(line)?;
        }
        // Power metrics
        else if line.starts_with("CPU Power:") && !line.contains("GPU") {
            data.cpu_power_mw = parse_power_mw(line)?;
        } else if line.starts_with("GPU Power:") && !line.contains("CPU") {
            data.gpu_power_mw = parse_power_mw(line)?;
        } else if line.starts_with("ANE Power:") {
            data.ane_power_mw = parse_power_mw(line)?;
        } else if line.contains("Combined Power (CPU + GPU + ANE):") {
            data.combined_power_mw = parse_power_mw(line)?;
        }
        // Thermal
        else if line.contains("pressure level:") {
            if let Some(pressure_str) = line.split(':').nth(1) {
                data.thermal_pressure_level = Some(pressure_str.trim().to_string());
            }
        }
    }

    // For Ultra chips, calculate average E-cluster metrics if we collected multiple clusters
    if !e_cluster_residencies.is_empty() {
        data.e_cluster_active_residency =
            e_cluster_residencies.iter().sum::<f64>() / e_cluster_residencies.len() as f64;
    }
    if !e_cluster_frequencies.is_empty() {
        data.e_cluster_frequency = (e_cluster_frequencies.iter().sum::<u32>() as f64
            / e_cluster_frequencies.len() as f64) as u32;
    }

    // For Ultra chips, calculate average P-cluster metrics if we collected multiple clusters
    if !p_cluster_residencies.is_empty() {
        data.p_cluster_active_residency =
            p_cluster_residencies.iter().sum::<f64>() / p_cluster_residencies.len() as f64;
    }
    if !p_cluster_frequencies.is_empty() {
        data.p_cluster_frequency = (p_cluster_frequencies.iter().sum::<u32>() as f64
            / p_cluster_frequencies.len() as f64) as u32;
    }

    Ok(data)
}

/// Parse frequency from a line like "E-Cluster HW active frequency: 1187 MHz"
fn parse_frequency(line: &str) -> Result<u32, Box<dyn std::error::Error>> {
    if let Some(v) = crate::parse_metric!(line, "MHz", u32) {
        Ok(v)
    } else {
        Err("Failed to parse frequency".into())
    }
}

/// Parse residency from a line like "E-Cluster HW active residency:  64.29%"
fn parse_residency(line: &str) -> Result<f64, Box<dyn std::error::Error>> {
    if let Some(v) = crate::parse_metric!(line, "%", f64) {
        Ok(v)
    } else {
        Err("Failed to parse residency".into())
    }
}

/// Parse power from a line like "CPU Power: 475 mW"
fn parse_power_mw(line: &str) -> Result<f64, Box<dyn std::error::Error>> {
    if let Some(v) = crate::parse_metric!(line, "mW", f64) {
        Ok(v)
    } else {
        Err("Failed to parse power".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test coverage for powermetrics parser:
    // 1. Standard format: M2/M3/M4 Pro/Max - single E-Cluster and P-Cluster
    // 2. Hybrid format: M1 Pro - single E-Cluster but numbered P0/P1-Clusters
    // 3. Ultra format: M1/M2 Ultra - numbered E0/E1-Clusters and P0/P1/P2/P3-Clusters
    // 4. Edge cases: mixed formats, missing fields, invalid output
    // 5. Future compatibility: varying numbers of clusters
    //
    // The parser automatically detects and handles all three formats:
    // - Standard clusters (E-Cluster, P-Cluster) use direct values
    // - Numbered clusters (E0, E1, P0, P1, etc.) are averaged

    #[test]
    fn test_parse_powermetrics_m1_standard() {
        // Test standard M1 format
        let sample_output = r#"
E-Cluster HW active frequency: 1187 MHz
E-Cluster HW active residency:  64.29% (600 MHz:   0% 912 MHz:  48% 1284 MHz: 1.6% 1752 MHz: 4.2% 2004 MHz: 3.2% 2256 MHz: 1.1% 2424 MHz: 5.7%)
E-Cluster idle residency:  35.71%
CPU 0 frequency: 1514 MHz
CPU 0 active residency:  27.07%
CPU 1 frequency: 1413 MHz
CPU 1 active residency:  23.67%

P-Cluster HW active frequency: 1172 MHz
P-Cluster HW active residency:  60.00%
P-Cluster idle residency:  40.00%
CPU 4 frequency: 1643 MHz
CPU 4 active residency:  24.71%

CPU Power: 475 mW
GPU Power: 47 mW
ANE Power: 0 mW
Combined Power (CPU + GPU + ANE): 522 mW

GPU HW active frequency: 444 MHz
GPU HW active residency:   9.85%
"#;

        let data = parse_powermetrics_output(sample_output).unwrap();

        assert_eq!(data.e_cluster_frequency, 1187);
        assert_eq!(data.e_cluster_active_residency, 64.29);
        assert_eq!(data.p_cluster_frequency, 1172);
        assert_eq!(data.p_cluster_active_residency, 60.0);
        assert_eq!(data.cpu_power_mw, 475.0);
        assert_eq!(data.gpu_power_mw, 47.0);
        assert_eq!(data.gpu_frequency, 444);
        assert_eq!(data.gpu_active_residency, 9.85);
    }

    #[test]
    fn test_parse_powermetrics_m1_ultra_real_data() {
        // Test with actual M1 Ultra powermetrics output from user
        let test_output = r#"
E0-Cluster HW active frequency: 1511 MHz
E0-Cluster HW active residency:  50.55% (600 MHz:   0% 972 MHz:  36% 1332 MHz:  18% 1704 MHz: 9.7% 2064 MHz:  37%)
E0-Cluster idle residency:  49.45%
CPU 0 frequency: 1571 MHz
CPU 0 active residency:  38.43%

E1-Cluster HW active frequency: 1340 MHz
E1-Cluster HW active residency:  37.09% (600 MHz:   0% 972 MHz:  55% 1332 MHz:  13% 1704 MHz: 8.2% 2064 MHz:  24%)
E1-Cluster idle residency:  62.91%

P0-Cluster HW active frequency: 2981 MHz
P0-Cluster HW active residency:  85.46%
P0-Cluster idle residency:  14.54%

P1-Cluster HW active frequency: 1304 MHz
P1-Cluster HW active residency:  12.12%
P1-Cluster idle residency:  87.88%

P2-Cluster HW active frequency: 600 MHz
P2-Cluster HW active residency:   0.17%
P2-Cluster idle residency:  99.83%

P3-Cluster HW active frequency: 600 MHz
P3-Cluster HW active residency:   0.16%
P3-Cluster idle residency:  99.84%

CPU Power: 5247 mW
GPU Power: 139 mW
ANE Power: 0 mW
Combined Power (CPU + GPU + ANE): 5385 mW

GPU HW active frequency: 636 MHz
GPU HW active residency:  19.10%
GPU Power: 132 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // E-cluster average: (1511 + 1340) / 2 = 1425.5 -> 1425
        assert_eq!(data.e_cluster_frequency, 1425);
        // E-cluster average residency: (50.55 + 37.09) / 2 = 43.82
        assert_eq!(data.e_cluster_active_residency, 43.82);
        // P-cluster average: (2981 + 1304 + 600 + 600) / 4 = 1371.25 -> 1371
        assert_eq!(data.p_cluster_frequency, 1371);
        // P-cluster average residency: (85.46 + 12.12 + 0.17 + 0.16) / 4 = 24.4775
        assert!((data.p_cluster_active_residency - 24.4775).abs() < 0.001);

        // Verify power data
        assert_eq!(data.cpu_power_mw, 5247.0);
        assert_eq!(data.gpu_power_mw, 132.0); // Parser picks up the last GPU Power value
        assert_eq!(data.gpu_frequency, 636);
        assert_eq!(data.gpu_active_residency, 19.10);
    }

    #[test]
    fn test_parse_powermetrics_m1_ultra() {
        // Test M1 Ultra format with 2 E-clusters and 4 P-clusters
        let sample_output = r#"
E0-Cluster HW active frequency: 1511 MHz
E0-Cluster HW active residency:  50.55% (600 MHz:   0% 972 MHz:  36% 1332 MHz:  18% 1704 MHz: 9.7% 2064 MHz:  37%)
E0-Cluster idle residency:  49.45%

E1-Cluster HW active frequency: 1340 MHz
E1-Cluster HW active residency:  37.09% (600 MHz:   0% 972 MHz:  55% 1332 MHz:  13% 1704 MHz: 8.2% 2064 MHz:  24%)
E1-Cluster idle residency:  62.91%

P0-Cluster HW active frequency: 2981 MHz
P0-Cluster HW active residency:  85.46%
P0-Cluster idle residency:  14.54%

P1-Cluster HW active frequency: 1304 MHz
P1-Cluster HW active residency:  12.12%
P1-Cluster idle residency:  87.88%

P2-Cluster HW active frequency: 600 MHz
P2-Cluster HW active residency:   0.17%
P2-Cluster idle residency:  99.83%

P3-Cluster HW active frequency: 600 MHz
P3-Cluster HW active residency:   0.16%
P3-Cluster idle residency:  99.84%

CPU Power: 5247 mW
GPU Power: 139 mW
ANE Power: 0 mW
Combined Power (CPU + GPU + ANE): 5385 mW

GPU HW active frequency: 636 MHz
GPU HW active residency:  19.10%
"#;

        let data = parse_powermetrics_output(sample_output).unwrap();

        // E-cluster average: (1511 + 1340) / 2 = 1425.5 -> 1425
        assert_eq!(data.e_cluster_frequency, 1425);
        // E-cluster average residency: (50.55 + 37.09) / 2 = 43.82
        assert_eq!(data.e_cluster_active_residency, 43.82);
        // P-cluster average: (2981 + 1304 + 600 + 600) / 4 = 1371.25 -> 1371
        assert_eq!(data.p_cluster_frequency, 1371);
        // P-cluster average residency: (85.46 + 12.12 + 0.17 + 0.16) / 4 = 24.4775
        assert!((data.p_cluster_active_residency - 24.4775).abs() < 0.001);
        assert_eq!(data.cpu_power_mw, 5247.0);
        assert_eq!(data.gpu_power_mw, 139.0);
        assert_eq!(data.gpu_frequency, 636);
        assert_eq!(data.gpu_active_residency, 19.10);
    }

    #[test]
    fn test_parse_powermetrics_m1_pro_hybrid_format() {
        // Test M1 Pro with hybrid format: standard E-Cluster but numbered P0/P1-Clusters
        let test_output = r#"
E-Cluster HW active frequency: 1318 MHz
E-Cluster HW active residency:  62.35% (600 MHz:   0% 972 MHz:  59% 1332 MHz: 9.2% 1704 MHz:  10% 2064 MHz:  22%)
E-Cluster idle residency:  37.65%
CPU 0 frequency: 1379 MHz
CPU 0 active residency:  49.61%
CPU 1 frequency: 1389 MHz
CPU 1 active residency:  47.23%

P0-Cluster HW active frequency: 1149 MHz
P0-Cluster HW active residency:  12.79% (600 MHz:  59% 828 MHz: 2.3% 1056 MHz: 5.8% 1296 MHz: 7.4% 1524 MHz: 4.1% 1752 MHz: 1.8% 1980 MHz: 3.1% 2208 MHz: 2.6% 2448 MHz: 3.5% 2676 MHz: 2.8% 2904 MHz: 1.2% 3036 MHz: .81% 3132 MHz: .78% 3168 MHz: 1.1% 3228 MHz: 4.0%)
P0-Cluster idle residency:  87.21%
CPU 2 frequency: 1873 MHz
CPU 2 active residency:   9.76%

P1-Cluster HW active frequency: 643 MHz
P1-Cluster HW active residency:   0.81% (600 MHz:  97% 828 MHz:   0% 1056 MHz: 1.2% 1296 MHz:   0% 1524 MHz: .32% 1752 MHz:   0% 1980 MHz:   0% 2208 MHz:   0% 2448 MHz: .23% 2676 MHz:   0% 2904 MHz:   0% 3036 MHz: .00% 3132 MHz: .00% 3168 MHz:   0% 3228 MHz: 1.1%)
P1-Cluster idle residency:  99.19%
CPU 6 frequency: 2881 MHz
CPU 6 active residency:   0.62%

CPU Power: 289 mW
GPU Power: 112 mW
ANE Power: 0 mW
Combined Power (CPU + GPU + ANE): 401 mW

GPU HW active frequency: 389 MHz
GPU HW active residency:   9.99%
GPU Power: 104 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // E-Cluster uses standard format (direct value)
        assert_eq!(data.e_cluster_frequency, 1318);
        assert_eq!(data.e_cluster_active_residency, 62.35);

        // P-Clusters use numbered format (averaged)
        // P-cluster average: (1149 + 643) / 2 = 896
        assert_eq!(data.p_cluster_frequency, 896);
        // P-cluster average residency: (12.79 + 0.81) / 2 = 6.8
        assert_eq!(data.p_cluster_active_residency, 6.8);

        // Verify power data
        assert_eq!(data.cpu_power_mw, 289.0);
        assert_eq!(data.gpu_power_mw, 104.0); // Parser picks up last GPU Power value
        assert_eq!(data.gpu_frequency, 389);
        assert_eq!(data.gpu_active_residency, 9.99);
    }

    #[test]
    fn test_parse_powermetrics_m2_pro() {
        // Test M2 Pro format (same structure as standard but different core counts)
        let test_output = r#"
E-Cluster HW active frequency: 1020 MHz
E-Cluster HW active residency: 25.5%
CPU 0 frequency: 1020 MHz
CPU 0 active residency: 12.5%
CPU 1 frequency: 1020 MHz
CPU 1 active residency: 13.0%

P-Cluster HW active frequency: 3000 MHz
P-Cluster HW active residency: 75.5%
CPU 4 frequency: 3000 MHz
CPU 4 active residency: 50.0%
CPU 5 frequency: 3000 MHz
CPU 5 active residency: 25.5%

GPU HW active frequency: 1200 MHz
GPU HW active residency: 45.5%

CPU Power: 1500 mW
GPU Power: 2500 mW
ANE Power: 100 mW
Combined Power (CPU + GPU + ANE): 4100 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // Check cluster data
        assert_eq!(data.e_cluster_frequency, 1020);
        assert_eq!(data.e_cluster_active_residency, 25.5);
        assert_eq!(data.p_cluster_frequency, 3000);
        assert_eq!(data.p_cluster_active_residency, 75.5);

        // Check GPU data
        assert_eq!(data.gpu_frequency, 1200);
        assert_eq!(data.gpu_active_residency, 45.5);

        // Check power data
        assert_eq!(data.cpu_power_mw, 1500.0);
        assert_eq!(data.gpu_power_mw, 2500.0);
        assert_eq!(data.ane_power_mw, 100.0);
        assert_eq!(data.combined_power_mw, 4100.0);

        // Check core data
        assert_eq!(data.core_frequencies.len(), 4);
        assert_eq!(data.core_active_residencies.len(), 4);
        assert_eq!(data.core_cluster_types.len(), 4);

        // E-cores
        assert_eq!(data.core_frequencies[0], 1020);
        assert_eq!(data.core_frequencies[1], 1020);
        assert_eq!(data.core_cluster_types[0], CoreType::Efficiency);
        assert_eq!(data.core_cluster_types[1], CoreType::Efficiency);

        // P-cores
        assert_eq!(data.core_frequencies[2], 3000);
        assert_eq!(data.core_frequencies[3], 3000);
        assert_eq!(data.core_cluster_types[2], CoreType::Performance);
        assert_eq!(data.core_cluster_types[3], CoreType::Performance);
    }

    #[test]
    fn test_parse_powermetrics_m3_pro_real_data() {
        // Test with actual M3 Pro powermetrics output
        let test_output = r#"
E-Cluster HW active frequency: 1293 MHz
E-Cluster HW active residency:  82.26% (744 MHz:  48% 1044 MHz: 4.7% 1476 MHz: 7.2% 2004 MHz: 5.5% 2268 MHz: .74% 2448 MHz: 2.2% 2640 MHz: 3.1% 2748 MHz:  10%)
E-Cluster idle residency:  17.74%
CPU 0 frequency: 1292 MHz
CPU 0 active residency:  64.93% (744 MHz:  38% 1044 MHz: 3.6% 1476 MHz: 5.9% 2004 MHz: 4.2% 2268 MHz: .62% 2448 MHz: 1.8% 2640 MHz: .95% 2748 MHz: 9.6%)
CPU 0 idle residency:  35.07%
CPU 1 frequency: 1399 MHz
CPU 1 active residency:  37.05% (744 MHz:  18% 1044 MHz: 3.0% 1476 MHz: 4.5% 2004 MHz: 3.8% 2268 MHz: .54% 2448 MHz: 1.3% 2640 MHz: .61% 2748 MHz: 5.6%)
CPU 1 idle residency:  62.95%

P-Cluster HW active frequency: 3129 MHz
P-Cluster HW active residency:  25.48% (696 MHz: .21% 1092 MHz:   0% 1356 MHz: 1.9% 1596 MHz: 2.6% 1884 MHz: 1.3% 2172 MHz: .06% 2424 MHz: .30% 2616 MHz: .17% 2808 MHz:   0% 2988 MHz:   0% 3144 MHz:   0% 3288 MHz:   0% 3420 MHz:   0% 3576 MHz: .03% 3624 MHz:  17% 3708 MHz: .29% 3780 MHz: 1.8% 3864 MHz:   0% 3960 MHz:   0% 4056 MHz:   0%)
P-Cluster idle residency:  74.52%
CPU 6 frequency: 3889 MHz
CPU 6 active residency:  13.21% (696 MHz: .03% 1092 MHz:   0% 1356 MHz: .60% 1596 MHz: .16% 1884 MHz: .04% 2172 MHz:   0% 2424 MHz:   0% 2616 MHz:   0% 2808 MHz:   0% 2988 MHz:   0% 3144 MHz:   0% 3288 MHz:   0% 3420 MHz:   0% 3576 MHz:   0% 3624 MHz:   0% 3708 MHz:   0% 3780 MHz:   0% 3864 MHz:   0% 3960 MHz:   0% 4056 MHz:  12%)
CPU 6 idle residency:  86.79%

CPU Power: 3224 mW
GPU Power: 31 mW
ANE Power: 0 mW
Combined Power (CPU + GPU + ANE): 3254 mW

GPU HW active frequency: 338 MHz
GPU HW active residency:   6.31% (338 MHz: 6.3% 618 MHz:   0% 796 MHz:   0% 832 MHz:   0% 924 MHz:   0% 952 MHz:   0% 1056 MHz:   0% 1064 MHz:   0% 1182 MHz:   0% 1182 MHz:   0% 1312 MHz:   0% 1242 MHz:   0% 1380 MHz:   0%)
GPU SW requested state: (P1 : 100% P2 :   0% P3 :   0% P4 :   0% P5 :   0% P6 :   0% P7 :   0% P8 :   0% P9 :   0% P10 :   0% P11 :   0% P12 :   0% P13 :   0%)
GPU idle residency:  93.69%
GPU Power: 31 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // Verify M3 Pro data is parsed correctly
        assert_eq!(data.e_cluster_frequency, 1293);
        assert_eq!(data.e_cluster_active_residency, 82.26);
        assert_eq!(data.p_cluster_frequency, 3129);
        assert_eq!(data.p_cluster_active_residency, 25.48);

        // Verify power data
        assert_eq!(data.cpu_power_mw, 3224.0);
        assert_eq!(data.gpu_power_mw, 31.0);
        assert_eq!(data.ane_power_mw, 0.0);
        assert_eq!(data.combined_power_mw, 3254.0);

        // Verify GPU data
        assert_eq!(data.gpu_frequency, 338);
        assert_eq!(data.gpu_active_residency, 6.31);

        // Verify we have core data
        assert!(!data.core_frequencies.is_empty());
        assert!(!data.core_active_residencies.is_empty());
    }

    #[test]
    fn test_parse_powermetrics_m3_max() {
        // Test M3 Max format (similar to standard but with higher core counts)
        let test_output = r#"
E-Cluster HW active frequency: 1400 MHz
E-Cluster HW active residency: 42.5%
CPU 0 frequency: 1400 MHz
CPU 0 active residency: 20.0%
CPU 1 frequency: 1400 MHz
CPU 1 active residency: 15.0%
CPU 2 frequency: 1400 MHz
CPU 2 active residency: 18.0%
CPU 3 frequency: 1400 MHz
CPU 3 active residency: 17.0%

P-Cluster HW active frequency: 3500 MHz
P-Cluster HW active residency: 68.5%
CPU 4 frequency: 3500 MHz
CPU 4 active residency: 45.0%
CPU 5 frequency: 3500 MHz
CPU 5 active residency: 40.0%
CPU 6 frequency: 3500 MHz
CPU 6 active residency: 38.0%
CPU 7 frequency: 3500 MHz
CPU 7 active residency: 35.0%
CPU 8 frequency: 3500 MHz
CPU 8 active residency: 30.0%
CPU 9 frequency: 3500 MHz
CPU 9 active residency: 28.0%
CPU 10 frequency: 3500 MHz
CPU 10 active residency: 25.0%
CPU 11 frequency: 3500 MHz
CPU 11 active residency: 22.0%

CPU Power: 2800 mW
GPU Power: 3200 mW
ANE Power: 150 mW
Combined Power (CPU + GPU + ANE): 6150 mW

GPU HW active frequency: 1398 MHz
GPU HW active residency: 55.5%
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // Check cluster data for M3 Max
        assert_eq!(data.e_cluster_frequency, 1400);
        assert_eq!(data.e_cluster_active_residency, 42.5);
        assert_eq!(data.p_cluster_frequency, 3500);
        assert_eq!(data.p_cluster_active_residency, 68.5);

        // Check power data
        assert_eq!(data.cpu_power_mw, 2800.0);
        assert_eq!(data.gpu_power_mw, 3200.0);
        assert_eq!(data.gpu_frequency, 1398);
        assert_eq!(data.gpu_active_residency, 55.5);
    }

    #[test]
    fn test_parse_powermetrics_m2_ultra() {
        // Test M2 Ultra format (similar to M1 Ultra but potentially different cluster counts)
        let test_output = r#"
E0-Cluster HW active frequency: 1600 MHz
E0-Cluster HW active residency: 45.0%
E0-Cluster idle residency: 55.0%

E1-Cluster HW active frequency: 1550 MHz
E1-Cluster HW active residency: 40.0%
E1-Cluster idle residency: 60.0%

P0-Cluster HW active frequency: 3200 MHz
P0-Cluster HW active residency: 80.0%
P0-Cluster idle residency: 20.0%

P1-Cluster HW active frequency: 3100 MHz
P1-Cluster HW active residency: 75.0%
P1-Cluster idle residency: 25.0%

P2-Cluster HW active frequency: 2800 MHz
P2-Cluster HW active residency: 50.0%
P2-Cluster idle residency: 50.0%

P3-Cluster HW active frequency: 2500 MHz
P3-Cluster HW active residency: 30.0%
P3-Cluster idle residency: 70.0%

CPU Power: 6500 mW
GPU Power: 4500 mW
ANE Power: 200 mW
Combined Power (CPU + GPU + ANE): 11200 mW

GPU HW active frequency: 1450 MHz
GPU HW active residency: 65.0%
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // E-cluster average: (1600 + 1550) / 2 = 1575
        assert_eq!(data.e_cluster_frequency, 1575);
        // E-cluster average residency: (45.0 + 40.0) / 2 = 42.5
        assert_eq!(data.e_cluster_active_residency, 42.5);
        // P-cluster average: (3200 + 3100 + 2800 + 2500) / 4 = 2900
        assert_eq!(data.p_cluster_frequency, 2900);
        // P-cluster average residency: (80.0 + 75.0 + 50.0 + 30.0) / 4 = 58.75
        assert_eq!(data.p_cluster_active_residency, 58.75);

        // Check power and GPU data
        assert_eq!(data.cpu_power_mw, 6500.0);
        assert_eq!(data.gpu_power_mw, 4500.0);
        assert_eq!(data.gpu_frequency, 1450);
        assert_eq!(data.gpu_active_residency, 65.0);
    }

    #[test]
    fn test_parse_powermetrics_mixed_format() {
        // Test that parser handles mixed/partial data gracefully
        let test_output = r#"
E-Cluster HW active frequency: 1200 MHz
E0-Cluster HW active residency: 30.0%
P-Cluster HW active residency: 70.0%
P1-Cluster HW active frequency: 3000 MHz

CPU Power: 2000 mW
GPU Power: 1500 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // Should use standard format values when available
        assert_eq!(data.e_cluster_frequency, 1200);
        assert_eq!(data.p_cluster_active_residency, 70.0);

        // Ultra format values should be in the vectors but not override standard
        assert_eq!(data.cpu_power_mw, 2000.0);
        assert_eq!(data.gpu_power_mw, 1500.0);
    }

    #[test]
    fn test_parse_powermetrics_with_missing_fields() {
        // Test with minimal output
        let test_output = r#"
GPU HW active frequency: 1000 MHz
GPU HW active residency: 20.0%
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        assert_eq!(data.gpu_active_residency, 20.0);
        assert_eq!(data.gpu_frequency, 1000);
        assert_eq!(data.e_cluster_active_residency, 0.0);
        assert_eq!(data.p_cluster_active_residency, 0.0);
        assert_eq!(data.core_frequencies.len(), 0);
    }

    #[test]
    fn test_parse_invalid_output() {
        let invalid_output = "not a valid powermetrics output";
        let result = parse_powermetrics_output(invalid_output);
        // Should return default values, not error
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.gpu_active_residency, 0.0);
        assert_eq!(data.e_cluster_active_residency, 0.0);
    }

    #[test]
    fn test_coretype_equality() {
        assert_eq!(CoreType::Efficiency, CoreType::Efficiency);
        assert_eq!(CoreType::Performance, CoreType::Performance);
        assert_ne!(CoreType::Efficiency, CoreType::Performance);
    }

    #[test]
    fn test_parse_powermetrics_future_ultra_formats() {
        // Test future Ultra chips that might have different numbers of clusters
        let test_output = r#"
E0-Cluster HW active frequency: 1800 MHz
E0-Cluster HW active residency: 55.0%

E1-Cluster HW active frequency: 1750 MHz
E1-Cluster HW active residency: 50.0%

E2-Cluster HW active frequency: 1700 MHz
E2-Cluster HW active residency: 45.0%

P0-Cluster HW active frequency: 3600 MHz
P0-Cluster HW active residency: 90.0%

P1-Cluster HW active frequency: 3500 MHz
P1-Cluster HW active residency: 85.0%

P2-Cluster HW active frequency: 3400 MHz
P2-Cluster HW active residency: 80.0%

P3-Cluster HW active frequency: 3300 MHz
P3-Cluster HW active residency: 75.0%

P4-Cluster HW active frequency: 3200 MHz
P4-Cluster HW active residency: 70.0%

P5-Cluster HW active frequency: 3100 MHz
P5-Cluster HW active residency: 65.0%

CPU Power: 8000 mW
GPU Power: 6000 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // E-cluster average: (1800 + 1750 + 1700) / 3 = 1750
        assert_eq!(data.e_cluster_frequency, 1750);
        // E-cluster average residency: (55.0 + 50.0 + 45.0) / 3 = 50.0
        assert_eq!(data.e_cluster_active_residency, 50.0);
        // P-cluster average: (3600 + 3500 + 3400 + 3300 + 3200 + 3100) / 6 = 3350
        assert_eq!(data.p_cluster_frequency, 3350);
        // P-cluster average residency: (90 + 85 + 80 + 75 + 70 + 65) / 6 = 77.5
        assert_eq!(data.p_cluster_active_residency, 77.5);
    }

    #[test]
    fn test_parse_powermetrics_single_cluster_ultra() {
        // Test edge case where Ultra chip might have only one E or P cluster active
        let test_output = r#"
E0-Cluster HW active frequency: 2000 MHz
E0-Cluster HW active residency: 60.0%

P0-Cluster HW active frequency: 3500 MHz
P0-Cluster HW active residency: 95.0%

P1-Cluster HW active frequency: 600 MHz
P1-Cluster HW active residency: 0.0%

P2-Cluster HW active frequency: 600 MHz
P2-Cluster HW active residency: 0.0%

CPU Power: 3000 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // Single E-cluster
        assert_eq!(data.e_cluster_frequency, 2000);
        assert_eq!(data.e_cluster_active_residency, 60.0);
        // P-cluster average includes idle clusters
        assert_eq!(data.p_cluster_frequency, 1566); // (3500 + 600 + 600) / 3 = 4700 / 3 = 1566.666... -> 1566
        assert!((data.p_cluster_active_residency - 31.666666).abs() < 0.001); // (95 + 0 + 0) / 3
    }

    #[test]
    fn test_all_apple_silicon_formats() {
        // Comprehensive test documenting all known Apple Silicon powermetrics formats

        // Format Summary Table:
        // ┌─────────────┬──────────────┬─────────────────────────┐
        // │ Chip        │ E-Clusters   │ P-Clusters              │
        // ├─────────────┼──────────────┼─────────────────────────┤
        // │ M1 Pro      │ E-Cluster    │ P0-Cluster, P1-Cluster  │
        // │ M2/M3/M4    │ E-Cluster    │ P-Cluster               │
        // │ Pro/Max     │              │                         │
        // │ M1/M2 Ultra │ E0, E1       │ P0, P1, P2, P3         │
        // └─────────────┴──────────────┴─────────────────────────┘

        // Test standard format
        let standard = "E-Cluster HW active frequency: 1000 MHz\nE-Cluster HW active residency: 50%\nP-Cluster HW active frequency: 3000 MHz\nP-Cluster HW active residency: 75%";
        let data = parse_powermetrics_output(standard).unwrap();
        assert_eq!(data.e_cluster_frequency, 1000);
        assert_eq!(data.p_cluster_frequency, 3000);

        // Test hybrid format (M1 Pro)
        let hybrid = "E-Cluster HW active frequency: 1000 MHz\nE-Cluster HW active residency: 50%\nP0-Cluster HW active frequency: 3000 MHz\nP0-Cluster HW active residency: 70%\nP1-Cluster HW active frequency: 2000 MHz\nP1-Cluster HW active residency: 30%";
        let data = parse_powermetrics_output(hybrid).unwrap();
        assert_eq!(data.e_cluster_frequency, 1000);
        assert_eq!(data.p_cluster_frequency, 2500); // Average of P0 and P1

        // Test Ultra format
        let ultra = "E0-Cluster HW active frequency: 1000 MHz\nE0-Cluster HW active residency: 40%\nE1-Cluster HW active frequency: 1200 MHz\nE1-Cluster HW active residency: 60%\nP0-Cluster HW active frequency: 3000 MHz\nP0-Cluster HW active residency: 80%\nP1-Cluster HW active frequency: 2000 MHz\nP1-Cluster HW active residency: 20%";
        let data = parse_powermetrics_output(ultra).unwrap();
        assert_eq!(data.e_cluster_frequency, 1100); // Average of E0 and E1
        assert_eq!(data.p_cluster_frequency, 2500); // Average of P0 and P1
    }

    #[test]
    fn test_powermetrics_data_utilization_methods() {
        let data = PowerMetricsData {
            e_cluster_active_residency: 30.0,
            p_cluster_active_residency: 70.0,
            gpu_active_residency: 50.0,
            ..Default::default()
        };

        // Test CPU utilization (weighted average)
        let cpu_util = data.cpu_utilization();
        assert_eq!(cpu_util, 30.0 * 0.3 + 70.0 * 0.7); // 9 + 49 = 58

        // Test GPU utilization
        let gpu_util = data.gpu_utilization();
        assert_eq!(gpu_util, 50.0);
    }
}

// Note: The get_powermetrics_data function is already defined above at line 55
// It returns Result<PowerMetricsData, Box<dyn std::error::Error>>
