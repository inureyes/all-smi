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

use std::time::Instant;

pub struct StartupProfiler {
    start_time: Instant,
    last_checkpoint: Instant,
    enabled: bool,
}

impl Default for StartupProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupProfiler {
    pub fn new() -> Self {
        let enabled =
            std::env::var("DEBUG_STARTUP").is_ok() || std::env::var("PROFILE_STARTUP").is_ok();
        Self {
            start_time: Instant::now(),
            last_checkpoint: Instant::now(),
            enabled,
        }
    }

    pub fn checkpoint(&mut self, label: &str) -> Option<()> {
        if !self.enabled {
            return None;
        }

        let now = Instant::now();
        let from_start = now.duration_since(self.start_time);
        let from_last = now.duration_since(self.last_checkpoint);

        eprintln!(
            "[PROFILE] {label}: {:.3}s (delta: {:.3}s)",
            from_start.as_secs_f64(),
            from_last.as_secs_f64()
        );

        self.last_checkpoint = now;
        Some(())
    }

    pub fn finish(&self) {
        if !self.enabled {
            return;
        }

        let total = Instant::now().duration_since(self.start_time);
        eprintln!("[PROFILE] Total startup time: {:.3}s", total.as_secs_f64());
    }
}
