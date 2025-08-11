//! Mock server module for all-smi
//!
//! This module provides a high-performance mock server that simulates
//! realistic GPU clusters with multiple nodes, each containing multiple GPUs.

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

pub mod args;
pub mod constants;
pub mod generator;
pub mod metrics;
pub mod node;
pub mod server;
pub mod template_engine;
pub mod templates;

pub use args::Args;
pub use server::start_servers;
