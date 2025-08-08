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

use crate::device::{CpuInfo, GpuInfo, MemoryInfo, ProcessInfo};

pub trait GpuReader: Send + Sync {
    fn get_gpu_info(&self) -> Vec<GpuInfo>;
    fn get_process_info(&self) -> Vec<ProcessInfo>;
}

pub trait CpuReader: Send + Sync {
    fn get_cpu_info(&self) -> Vec<CpuInfo>;
}

pub trait MemoryReader: Send + Sync {
    fn get_memory_info(&self) -> Vec<MemoryInfo>;
}
