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

//! IOReport API bindings for macOS
//!
//! This module provides FFI bindings to Apple's private IOReport framework,
//! which is used to collect power and performance metrics on Apple Silicon.
//!
//! ## Channel Groups
//! - `Energy Model`: Power consumption (CPU, GPU, ANE, DRAM)
//! - `CPU Stats`: CPU core performance states and residency
//! - `GPU Stats`: GPU performance states and residency
//!
//! ## References
//! - macmon project by vladkens
//! - asitop project by tlkh
//! - OSXPrivateSDK IOReport.h

use core_foundation::base::{CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef, CFMutableDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::marker::{PhantomData, PhantomPinned};
use std::ptr;
use std::time::Instant;

/// Opaque IOReport subscription reference
#[repr(C)]
struct IOReportSubscription {
    _data: [u8; 0],
    _phantom: PhantomData<(*mut u8, PhantomPinned)>,
}

type IOReportSubscriptionRef = *const IOReportSubscription;

// FFI declarations for IOReport library
#[link(name = "IOReport", kind = "dylib")]
unsafe extern "C" {
    fn IOReportCopyChannelsInGroup(
        group: CFStringRef,
        subgroup: CFStringRef,
        a: u64,
        b: u64,
        c: u64,
    ) -> CFDictionaryRef;

    fn IOReportMergeChannels(
        a: CFDictionaryRef,
        b: CFDictionaryRef,
        nil: CFTypeRef,
    ) -> CFDictionaryRef;

    fn IOReportCreateSubscription(
        a: *const c_void,
        desired_channels: CFMutableDictionaryRef,
        subscribed_channels: *mut CFMutableDictionaryRef,
        channel_id: u64,
        b: CFTypeRef,
    ) -> IOReportSubscriptionRef;

    fn IOReportCreateSamples(
        subscription: IOReportSubscriptionRef,
        channels: CFMutableDictionaryRef,
        a: CFTypeRef,
    ) -> CFDictionaryRef;

    fn IOReportCreateSamplesDelta(
        prev: CFDictionaryRef,
        curr: CFDictionaryRef,
        a: CFTypeRef,
    ) -> CFDictionaryRef;

    fn IOReportChannelGetGroup(channel: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetSubGroup(channel: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetChannelName(channel: CFDictionaryRef) -> CFStringRef;
    fn IOReportChannelGetUnitLabel(channel: CFDictionaryRef) -> CFStringRef;
    fn IOReportSimpleGetIntegerValue(channel: CFDictionaryRef, a: i32) -> i64;
    fn IOReportStateGetCount(channel: CFDictionaryRef) -> i32;
    fn IOReportStateGetNameForIndex(channel: CFDictionaryRef, index: i32) -> CFStringRef;
    fn IOReportStateGetResidency(channel: CFDictionaryRef, index: i32) -> i64;
}

// Core Foundation helper functions
fn cfstr_to_string(cfstr: CFStringRef) -> Option<String> {
    if cfstr.is_null() {
        return None;
    }
    unsafe {
        let cf_string = CFString::wrap_under_get_rule(cfstr);
        Some(cf_string.to_string())
    }
}

/// Get array of dictionaries from CFDictionary
fn get_io_channels(dict: CFDictionaryRef) -> Vec<CFDictionaryRef> {
    if dict.is_null() {
        return vec![];
    }

    unsafe {
        let cf_dict = CFDictionary::<CFType, CFType>::wrap_under_get_rule(dict);
        let key = CFString::new("IOReportChannels");

        if let Some(channels) = cf_dict.find(key.as_CFType().as_CFTypeRef()) {
            // The channels value is a CFArray - get its raw pointer
            let arr_ref = channels.as_CFTypeRef() as core_foundation::array::CFArrayRef;
            if arr_ref.is_null() {
                return vec![];
            }

            let arr = core_foundation::array::CFArray::<CFType>::wrap_under_get_rule(arr_ref);
            let count = arr.len();

            (0..count)
                .filter_map(|i| arr.get(i).map(|v| v.as_CFTypeRef() as CFDictionaryRef))
                .filter(|d| !d.is_null())
                .collect()
        } else {
            vec![]
        }
    }
}

/// Item from IOReport iteration
#[derive(Debug, Clone)]
pub struct IOReportChannelItem {
    pub group: String,
    pub subgroup: String,
    pub channel: String,
    pub unit: String,
    pub item: CFDictionaryRef,
}

impl IOReportChannelItem {
    /// Get simple integer value from this channel
    pub fn get_integer_value(&self) -> i64 {
        if self.item.is_null() {
            return 0;
        }
        unsafe { IOReportSimpleGetIntegerValue(self.item, 0) }
    }

    /// Get state residencies as (name, residency) pairs
    pub fn get_residencies(&self) -> Vec<(String, i64)> {
        if self.item.is_null() {
            return vec![];
        }

        unsafe {
            let count = IOReportStateGetCount(self.item);
            (0..count)
                .filter_map(|i| {
                    let name_ref = IOReportStateGetNameForIndex(self.item, i);
                    let name = cfstr_to_string(name_ref)?;
                    let residency = IOReportStateGetResidency(self.item, i);
                    Some((name, residency))
                })
                .collect()
        }
    }

    /// Calculate power consumption in watts from energy value
    pub fn calculate_watts(&self, duration_ns: u64) -> f64 {
        let value = self.get_integer_value();
        if value <= 0 || duration_ns == 0 {
            return 0.0;
        }

        // Determine conversion factor based on unit
        let unit_factor = match self.unit.as_str() {
            "mJ" => 1e-3, // millijoules to joules
            "uJ" => 1e-6, // microjoules to joules
            "nJ" => 1e-9, // nanojoules to joules
            _ => 1e-9,    // Default to nanojoules
        };

        // Convert energy to watts: W = J / s
        let energy_joules = value as f64 * unit_factor;
        let duration_secs = duration_ns as f64 / 1e9;
        energy_joules / duration_secs
    }
}

/// Iterator over IOReport sample channels
pub struct IOReportIterator {
    channels: Vec<CFDictionaryRef>,
    index: usize,
}

impl IOReportIterator {
    fn new(sample: CFDictionaryRef) -> Self {
        let channels = get_io_channels(sample);
        Self { channels, index: 0 }
    }
}

impl Iterator for IOReportIterator {
    type Item = IOReportChannelItem;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.channels.len() {
            return None;
        }

        let item = self.channels[self.index];
        self.index += 1;

        if item.is_null() {
            return self.next();
        }

        unsafe {
            let group = cfstr_to_string(IOReportChannelGetGroup(item)).unwrap_or_default();
            let subgroup = cfstr_to_string(IOReportChannelGetSubGroup(item)).unwrap_or_default();
            let channel = cfstr_to_string(IOReportChannelGetChannelName(item)).unwrap_or_default();
            let unit = cfstr_to_string(IOReportChannelGetUnitLabel(item)).unwrap_or_default();

            Some(IOReportChannelItem {
                group,
                subgroup,
                channel,
                unit,
                item,
            })
        }
    }
}

/// Channel groups to subscribe to
const ENERGY_MODEL: &str = "Energy Model";
const CPU_STATS: &str = "CPU Stats";
const CPU_PERF_STATES: &str = "CPU Core Performance States";
const GPU_STATS: &str = "GPU Stats";
const GPU_PERF_STATES: &str = "GPU Performance States";

/// IOReport subscription manager
pub struct IOReport {
    subscription: IOReportSubscriptionRef,
    channels: CFMutableDictionaryRef,
    prev_sample: Option<(CFDictionaryRef, Instant)>,
}

impl IOReport {
    /// Create a new IOReport subscription for the specified channel groups
    pub fn new() -> Result<Self, &'static str> {
        unsafe {
            // Get channels for each group
            let energy_channels =
                IOReportCopyChannelsInGroup(cfstring(ENERGY_MODEL), ptr::null(), 0, 0, 0);
            let cpu_channels = IOReportCopyChannelsInGroup(
                cfstring(CPU_STATS),
                cfstring(CPU_PERF_STATES),
                0,
                0,
                0,
            );
            let gpu_channels = IOReportCopyChannelsInGroup(
                cfstring(GPU_STATS),
                cfstring(GPU_PERF_STATES),
                0,
                0,
                0,
            );

            if energy_channels.is_null() {
                return Err("Failed to get Energy Model channels");
            }

            // Merge all channels into one dictionary
            if !cpu_channels.is_null() {
                IOReportMergeChannels(energy_channels, cpu_channels, ptr::null());
                CFRelease(cpu_channels as *const c_void);
            }
            if !gpu_channels.is_null() {
                IOReportMergeChannels(energy_channels, gpu_channels, ptr::null());
                CFRelease(gpu_channels as *const c_void);
            }

            // Create mutable copy for subscription
            let count = core_foundation::dictionary::CFDictionaryGetCount(energy_channels) as isize;
            let channels = core_foundation::dictionary::CFDictionaryCreateMutableCopy(
                core_foundation::base::kCFAllocatorDefault,
                count,
                energy_channels,
            );
            CFRelease(energy_channels as *const c_void);

            if channels.is_null() {
                return Err("Failed to create mutable channel dictionary");
            }

            // Create subscription
            let mut subscribed_channels: CFMutableDictionaryRef = ptr::null_mut();
            let subscription = IOReportCreateSubscription(
                ptr::null(),
                channels,
                &mut subscribed_channels,
                0,
                ptr::null(),
            );

            if subscription.is_null() {
                CFRelease(channels as *const c_void);
                return Err("Failed to create IOReport subscription");
            }

            Ok(Self {
                subscription,
                channels,
                prev_sample: None,
            })
        }
    }

    /// Get a delta sample over the specified duration
    pub fn get_sample(
        &mut self,
        duration_ms: u64,
    ) -> Result<(IOReportIterator, u64), &'static str> {
        let sample1 = self.take_sample()?;
        let start = Instant::now();

        std::thread::sleep(std::time::Duration::from_millis(duration_ms));

        let sample2 = self.take_sample()?;
        let duration_ns = start.elapsed().as_nanos() as u64;

        // Calculate delta
        let delta = unsafe {
            let d = IOReportCreateSamplesDelta(sample1, sample2, ptr::null());
            CFRelease(sample1 as *const c_void);
            CFRelease(sample2 as *const c_void);
            d
        };

        if delta.is_null() {
            return Err("Failed to create sample delta");
        }

        Ok((IOReportIterator::new(delta), duration_ns))
    }

    /// Take a single sample
    fn take_sample(&self) -> Result<CFDictionaryRef, &'static str> {
        unsafe {
            let sample = IOReportCreateSamples(self.subscription, self.channels, ptr::null());
            if sample.is_null() {
                return Err("Failed to create IOReport sample");
            }
            Ok(sample)
        }
    }
}

impl Drop for IOReport {
    fn drop(&mut self) {
        unsafe {
            if let Some((prev, _)) = self.prev_sample.take() {
                if !prev.is_null() {
                    CFRelease(prev as *const c_void);
                }
            }
            if !self.channels.is_null() {
                CFRelease(self.channels as *const c_void);
            }
            // Note: subscription cleanup is handled by the system
        }
    }
}

// Safety: IOReport is safe to send between threads
// The FFI calls are thread-safe and we don't share mutable state
unsafe impl Send for IOReport {}
unsafe impl Sync for IOReport {}

/// Helper to create CFStringRef from &str
fn cfstring(s: &str) -> CFStringRef {
    CFString::new(s).as_concrete_TypeRef()
}

/// Collected metrics from IOReport
#[derive(Debug, Default, Clone)]
pub struct IOReportMetrics {
    // Power metrics (in watts)
    pub cpu_power: f64,
    pub gpu_power: f64,
    pub ane_power: f64,
    pub dram_power: f64,
    pub package_power: f64,

    // CPU frequency metrics (in MHz)
    pub e_cluster_freq: u32,
    pub p_cluster_freq: u32,
    pub e_cluster_residency: f64,
    pub p_cluster_residency: f64,

    // GPU metrics
    pub gpu_freq: u32,
    pub gpu_residency: f64,

    // Raw per-cluster data for Ultra chips
    pub e_cluster_data: Vec<(u32, f64)>, // (freq_mhz, residency_percent)
    pub p_cluster_data: Vec<(u32, f64)>,
}

impl IOReportMetrics {
    /// Collect metrics from an IOReport sample
    pub fn from_sample(iterator: IOReportIterator, duration_ns: u64) -> Self {
        let mut metrics = Self::default();

        let mut e_cluster_freqs: Vec<(u32, f64)> = vec![];
        let mut p_cluster_freqs: Vec<(u32, f64)> = vec![];
        let mut gpu_freqs: Vec<(u32, f64)> = vec![];

        for item in iterator {
            match (item.group.as_str(), item.subgroup.as_str()) {
                ("Energy Model", _) => {
                    Self::process_energy_channel(&item, duration_ns, &mut metrics);
                }
                ("CPU Stats", "CPU Core Performance States") => {
                    Self::process_cpu_channel(&item, &mut e_cluster_freqs, &mut p_cluster_freqs);
                }
                ("GPU Stats", "GPU Performance States") => {
                    if item.channel == "GPUPH" {
                        Self::process_gpu_channel(&item, &mut gpu_freqs);
                    }
                }
                _ => {}
            }
        }

        // Calculate averages for clusters
        metrics.e_cluster_data = e_cluster_freqs.clone();
        metrics.p_cluster_data = p_cluster_freqs.clone();

        if let Some((freq, residency)) = Self::calculate_cluster_average(&e_cluster_freqs) {
            metrics.e_cluster_freq = freq;
            metrics.e_cluster_residency = residency;
        }
        if let Some((freq, residency)) = Self::calculate_cluster_average(&p_cluster_freqs) {
            metrics.p_cluster_freq = freq;
            metrics.p_cluster_residency = residency;
        }
        if let Some((freq, residency)) = Self::calculate_cluster_average(&gpu_freqs) {
            metrics.gpu_freq = freq;
            metrics.gpu_residency = residency;
        }

        metrics
    }

    fn process_energy_channel(item: &IOReportChannelItem, duration_ns: u64, metrics: &mut Self) {
        let watts = item.calculate_watts(duration_ns);
        let channel = item.channel.as_str();

        // Match known energy channels
        if channel.contains("CPU") && !channel.contains("GPU") {
            metrics.cpu_power += watts;
        } else if channel.contains("GPU") && !channel.contains("CPU") {
            metrics.gpu_power += watts;
        } else if channel.contains("ANE") {
            metrics.ane_power += watts;
        } else if channel.contains("DRAM") {
            metrics.dram_power += watts;
        }

        // Track package power
        if channel == "CPU Energy" || channel.starts_with("CPU") {
            // Package includes CPU, GPU, ANE
            metrics.package_power = metrics.cpu_power + metrics.gpu_power + metrics.ane_power;
        }
    }

    fn process_cpu_channel(
        item: &IOReportChannelItem,
        e_cluster_freqs: &mut Vec<(u32, f64)>,
        p_cluster_freqs: &mut Vec<(u32, f64)>,
    ) {
        let residencies = item.get_residencies();
        if residencies.is_empty() {
            return;
        }

        let (freq, residency) = Self::calc_freq_from_residencies(&residencies);
        let channel = &item.channel;

        // Determine cluster type from channel name
        if channel.starts_with("E") || channel.contains("ECPU") {
            e_cluster_freqs.push((freq, residency));
        } else if channel.starts_with("P") || channel.contains("PCPU") {
            p_cluster_freqs.push((freq, residency));
        }
    }

    fn process_gpu_channel(item: &IOReportChannelItem, gpu_freqs: &mut Vec<(u32, f64)>) {
        let residencies = item.get_residencies();
        if residencies.is_empty() {
            return;
        }

        let (freq, residency) = Self::calc_freq_from_residencies(&residencies);
        gpu_freqs.push((freq, residency));
    }

    /// Calculate frequency and residency from state residencies
    fn calc_freq_from_residencies(residencies: &[(String, i64)]) -> (u32, f64) {
        let mut total_residency: i64 = 0;
        let mut weighted_freq: i64 = 0;
        let mut active_residency: i64 = 0;

        for (name, residency) in residencies {
            total_residency += residency;

            // Skip idle/off states
            if name.contains("IDLE") || name.contains("OFF") || name.contains("DOWN") {
                continue;
            }

            active_residency += residency;

            // Parse frequency from state name (e.g., "2064" for 2064 MHz)
            if let Ok(freq) = name.trim().parse::<i64>() {
                weighted_freq += freq * residency;
            }
        }

        if total_residency == 0 {
            return (0, 0.0);
        }

        let avg_freq = if active_residency > 0 {
            (weighted_freq / active_residency) as u32
        } else {
            0
        };

        let residency_pct = (active_residency as f64 / total_residency as f64) * 100.0;

        (avg_freq, residency_pct)
    }

    fn calculate_cluster_average(data: &[(u32, f64)]) -> Option<(u32, f64)> {
        if data.is_empty() {
            return None;
        }

        let count = data.len() as f64;
        let avg_freq = data.iter().map(|(f, _)| *f as f64).sum::<f64>() / count;
        let avg_residency = data.iter().map(|(_, r)| *r).sum::<f64>() / count;

        Some((avg_freq as u32, avg_residency))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calc_freq_from_residencies() {
        let residencies = vec![
            ("IDLE".to_string(), 500),
            ("600".to_string(), 100),
            ("1200".to_string(), 200),
            ("2400".to_string(), 200),
        ];

        let (freq, residency) = IOReportMetrics::calc_freq_from_residencies(&residencies);

        // Active residency: 100 + 200 + 200 = 500 out of 1000 total = 50%
        assert!((residency - 50.0).abs() < 0.1);

        // Weighted freq: (600*100 + 1200*200 + 2400*200) / 500 = 1560
        assert_eq!(freq, 1560);
    }

    #[test]
    fn test_calculate_cluster_average() {
        let data = vec![(1000, 50.0), (2000, 60.0), (1500, 40.0)];

        let result = IOReportMetrics::calculate_cluster_average(&data);
        assert!(result.is_some());

        let (avg_freq, avg_residency) = result.unwrap();
        assert_eq!(avg_freq, 1500);
        assert!((avg_residency - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_calculate_cluster_average_empty() {
        let result = IOReportMetrics::calculate_cluster_average(&[]);
        assert!(result.is_none());
    }
}
