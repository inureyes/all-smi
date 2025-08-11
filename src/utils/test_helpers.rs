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

/// Helper macro to skip tests that require sudo privileges
#[macro_export]
macro_rules! skip_without_sudo {
    () => {
        if !$crate::utils::system::has_sudo_privileges() {
            eprintln!("Test requires sudo privileges, skipping...");
            eprintln!("Run with: sudo cargo test -- --test-threads=1");
            return;
        }
    };
}

/// Helper macro to skip tests in CI environment
#[macro_export]
macro_rules! skip_in_ci {
    () => {
        if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
            eprintln!("Test skipped in CI environment");
            return;
        }
    };
}

/// Check if we're running in a test environment that should skip sudo tests
#[allow(dead_code)]
pub fn should_skip_sudo_tests() -> bool {
    // Skip if we're in CI
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        return true;
    }

    // Skip if explicitly requested
    if std::env::var("SKIP_SUDO_TESTS").is_ok() {
        return true;
    }

    // Skip if we don't have sudo privileges
    #[cfg(target_os = "macos")]
    {
        !super::system::has_sudo_privileges()
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_sudo_tests() {
        // This should always run without requiring sudo
        let _result = should_skip_sudo_tests();
    }
}
