#!/bin/bash

# Update Furiosa exporter
sed -i '' 's/fn export_vendor_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize)/fn export_vendor_metrics(\&self, builder: \&mut MetricBuilder, info: \&GpuInfo, index: usize, index_str: \&str)/' src/api/metrics/npu/furiosa.rs

# Update Rebellions exporter  
sed -i '' 's/fn export_vendor_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize)/fn export_vendor_metrics(\&self, builder: \&mut MetricBuilder, info: \&GpuInfo, index: usize, index_str: \&str)/' src/api/metrics/npu/rebellions.rs

# Update Tenstorrent exporter
sed -i '' 's/fn export_vendor_metrics(&self, builder: &mut MetricBuilder, info: &GpuInfo, index: usize)/fn export_vendor_metrics(\&self, builder: \&mut MetricBuilder, info: \&GpuInfo, index: usize, index_str: \&str)/' src/api/metrics/npu/tenstorrent.rs

