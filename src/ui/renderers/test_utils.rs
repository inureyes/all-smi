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

#[cfg(test)]
pub mod tests {
    use std::io::{self, Write};

    /// Mock writer that captures output for testing
    pub struct MockWriter {
        pub buffer: Vec<u8>,
    }

    impl MockWriter {
        pub fn new() -> Self {
            Self { buffer: Vec::new() }
        }

        pub fn get_output(&self) -> String {
            String::from_utf8_lossy(&self.buffer).to_string()
        }

        pub fn clear(&mut self) {
            self.buffer.clear();
        }
    }

    impl Write for MockWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buffer.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}