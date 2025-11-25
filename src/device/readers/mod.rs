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

// Module for device readers with reduced code duplication

// Common caching utilities shared across all readers
pub mod common_cache;

#[cfg(target_os = "macos")]
pub mod apple_silicon;

pub mod furiosa;
pub mod gaudi;
pub mod nvidia;
pub mod nvidia_jetson;
pub mod rebellions;

#[cfg(target_os = "linux")]
pub mod tenstorrent;

#[cfg(all(target_os = "linux", not(target_env = "musl")))]
pub mod amd;
