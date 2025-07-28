//! GPU metrics structures and utilities

use rand::{rng, Rng};

#[derive(Clone)]
pub struct GpuMetrics {
    pub uuid: String,
    pub utilization: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub temperature_celsius: u32,
    pub power_consumption_watts: f32,
    pub frequency_mhz: u32,
    pub ane_utilization_watts: f32, // ANE power consumption in watts (Apple Silicon only)
    pub thermal_pressure_level: Option<String>, // Thermal pressure level (Apple Silicon only)
}

impl GpuMetrics {
    /// Update GPU metrics with realistic variations
    pub fn update(&mut self, platform: &super::PlatformType) {
        let mut rng = rng();

        // GPU utilization: gradual changes
        let utilization_delta = rng.random_range(-5.0..5.0);
        self.utilization = (self.utilization + utilization_delta).clamp(0.0, 100.0);

        // GPU memory: change by less than 3GB
        let memory_delta = rng.random_range(-(3 * 1024 * 1024 * 1024)..(3 * 1024 * 1024 * 1024));
        self.memory_used_bytes = self
            .memory_used_bytes
            .saturating_add_signed(memory_delta)
            .min(self.memory_total_bytes);

        // Calculate realistic power consumption based on utilization and memory usage
        let memory_usage_percent =
            (self.memory_used_bytes as f32 / self.memory_total_bytes as f32) * 100.0;

        // Base power consumption (idle state) - varies by GPU type
        let base_power = rng.random_range(80.0..120.0);

        // Power contribution from GPU utilization (strong correlation)
        let util_power_contribution = self.utilization * rng.random_range(4.0..6.0); // 4-6W per % utilization

        // Power contribution from memory usage (moderate correlation)
        let memory_power_contribution = memory_usage_percent * rng.random_range(1.0..2.0); // 1-2W per % memory usage

        // Individual GPU bias (some GPUs naturally consume more/less power)
        let gpu_bias = rng.random_range(-30.0..30.0);

        // Random variation (±15W)
        let random_variation = rng.random_range(-15.0..15.0);

        // Calculate total power consumption with platform-specific limits
        let max_power = match platform {
            super::PlatformType::Furiosa => 180.0, // Furiosa RNGD TDP is 180W
            _ => 700.0,
        };

        self.power_consumption_watts = (base_power
            + util_power_contribution
            + memory_power_contribution
            + gpu_bias
            + random_variation)
            .clamp(80.0, max_power);

        // GPU temperature: correlate with power consumption and utilization
        let base_temp = 45.0;
        let util_temp_contribution = self.utilization * 0.25; // 0.25°C per % utilization
        let power_temp_contribution = (self.power_consumption_watts - 200.0) * 0.05; // Temperature increases with power
        let temp_variation = rng.random_range(-3.0..3.0);

        self.temperature_celsius =
            (base_temp + util_temp_contribution + power_temp_contribution + temp_variation)
                .clamp(35.0, 85.0) as u32;

        // GPU frequency: platform-specific behavior
        self.frequency_mhz = match platform {
            super::PlatformType::Furiosa => {
                // Furiosa RNGD runs at fixed 1.0GHz
                1000
            }
            _ => {
                // Other platforms: correlate with utilization (higher util = higher freq)
                let base_freq = 1200.0;
                let util_freq_contribution = self.utilization * 6.0; // Up to 600MHz boost at 100% util
                let freq_variation = rng.random_range(-100.0..100.0);
                (base_freq + util_freq_contribution + freq_variation).clamp(1000.0, 1980.0) as u32
            }
        };

        // Update ANE utilization for Apple Silicon
        if self.ane_utilization_watts > 0.0 {
            let ane_delta = rng.random_range(-0.3..0.3);
            self.ane_utilization_watts = (self.ane_utilization_watts + ane_delta).clamp(0.0, 3.0);
        }
    }
}

/// Generate a unique GPU UUID
pub fn generate_uuid() -> String {
    let mut rng = rng();
    let bytes: [u8; 16] = rng.random();
    format!(
        "GPU-{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}
