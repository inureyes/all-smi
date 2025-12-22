// Copyright 2025 Lablup Inc.
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

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock, OnceLock};
use std::thread;
use std::time::Duration;

static RUNNER: OnceLock<TpuInfoRunner> = OnceLock::new();

pub fn get_runner() -> &'static TpuInfoRunner {
    RUNNER.get_or_init(TpuInfoRunner::new)
}

#[derive(Clone)]
pub struct TpuInfoRunner {
    /// Latest captured metrics (raw JSON or text)
    pub latest_data: Arc<RwLock<Option<String>>>,
    /// Status message for notification (e.g. "Running", "Failed")
    pub status: Arc<Mutex<String>>,
}

impl TpuInfoRunner {
    pub fn new() -> Self {
        let runner = Self {
            latest_data: Arc::new(RwLock::new(None)),
            status: Arc::new(Mutex::new("Initializing tpu-info...".to_string())),
        };
        runner.start_background_thread();
        runner
    }

    fn start_background_thread(&self) {
        let latest_data = self.latest_data.clone();
        let status = self.status.clone();

        thread::spawn(move || {
            loop {
                // Attempt to run tpu-info in streaming mode
                // Flags: --csv or --json might be better if available, but user suggested --streaming
                let child_res = Command::new("tpu-info")
                    // .arg("--json") // Try JSON if supported for easier parsing?
                    // .arg("--streaming") // As requested
                    // .arg("--rate").arg("2")
                    // Since we don't know exact flags, we'll try a common monitoring pattern.
                    // If tpu-info supports standard unix style:
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn();

                match child_res {
                    Ok(mut child) => {
                        {
                            let mut s = status.lock().unwrap();
                            *s = "tpu-info running".to_string();
                        }

                        if let Some(stdout) = child.stdout.take() {
                            let reader = BufReader::new(stdout);
                            // We assume line-based JSON or text output
                            for line_res in reader.lines() {
                                if let Ok(line) = line_res {
                                    if !line.trim().is_empty() {
                                        // Update latest data
                                        let mut data = latest_data.write().unwrap();
                                        *data = Some(line);
                                        
                                        // Once we get data, update status to empty (Ready)
                                        // or keep "Running" if we want persistent status?
                                        // Usually notifications are for setup/errors.
                                        let mut s = status.lock().unwrap();
                                        if s.contains("Initializing") {
                                            *s = "Ready".to_string();
                                        }
                                    }
                                }
                            }
                        }

                        // Process exited
                        let _ = child.wait();
                        {
                            let mut s = status.lock().unwrap();
                            *s = "tpu-info exited, restarting...".to_string();
                        }
                    }
                    Err(e) => {
                        {
                            let mut s = status.lock().unwrap();
                            *s = format!("Failed to start tpu-info: {}", e); // Show error in notification
                        }
                        // Sleep longer if failed to start (e.g. not installed)
                        thread::sleep(Duration::from_secs(10));
                    }
                }

                // Sleep before restart
                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    pub fn get_status(&self) -> Option<String> {
        let s = self.status.lock().unwrap().clone();
        if s == "Ready" {
            None
        } else {
            Some(s)
        }
    }
    
    #[allow(dead_code)]
    pub fn get_latest_data(&self) -> Option<String> {
        self.latest_data.read().unwrap().clone()
    }
}
