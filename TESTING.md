# Testing Guide for all-smi

## Running Tests

### Basic Tests (No sudo required)
```bash
cargo test
```

This will run all tests except those marked with `#[ignore]` which require sudo privileges on macOS.

### Platform-Specific Behavior
- **Linux**: All tests run without requiring sudo
- **macOS**: Some tests are skipped by default as they require sudo for PowerMetrics functionality
- **CI/GitHub Actions**: Tests automatically skip sudo requirements

### Tests Requiring sudo (macOS only)
On macOS, some tests require elevated privileges for PowerMetrics functionality.

#### Run all tests including sudo tests:
```bash
sudo cargo test -- --include-ignored --test-threads=1
```

#### Run only sudo tests:
```bash
sudo cargo test -- --ignored --test-threads=1
```

### Skipping sudo Tests
Tests that require sudo are automatically skipped in the following scenarios:
- When running in CI/GitHub Actions
- When running without sudo privileges
- When `SKIP_SUDO_TESTS=1` environment variable is set

```bash
# Skip sudo tests explicitly
SKIP_SUDO_TESTS=1 cargo test
```

### Test Categories

#### Unit Tests
```bash
cargo test --lib
```

#### Integration Tests
```bash
cargo test --test '*'
```

#### Specific Module Tests
```bash
# Test specific module
cargo test device::
cargo test ui::
cargo test utils::
```

### Continuous Integration
Tests in CI automatically skip sudo-requiring tests to avoid failures. The CI environment is detected through:
- `CI` environment variable
- `GITHUB_ACTIONS` environment variable

### Writing Tests

#### Tests requiring sudo
Mark tests that require sudo with `#[ignore]`:

```rust
#[test]
#[ignore] // Requires sudo
fn test_powermetrics_manager() {
    // Test code
}
```

#### Conditional test skipping
Use the helper macros provided in `utils::test_helpers`:

```rust
use crate::skip_without_sudo;

#[test]
fn test_that_might_need_sudo() {
    skip_without_sudo!();
    // Test code that requires sudo
}
```

Or skip in CI:

```rust
use crate::skip_in_ci;

#[test]
fn test_not_for_ci() {
    skip_in_ci!();
    // Test code that shouldn't run in CI
}
```

### Shell Script Tests

The `tests/` directory contains comprehensive shell scripts for testing various aspects of all-smi, particularly containerized environments and resource detection. These scripts test real-world scenarios that unit tests cannot easily cover.

### Quick Start
```bash
# Show all available test targets
cd tests && make help

# Run all structured tests
make all

# Run specific test
make container-cpu-frequency
```

For detailed information about shell script tests, see: [tests/README.md](tests/README.md)

### Key Test Categories
- **Container Tests**: Test all-smi behavior inside Docker containers with resource limits
- **CPU Frequency Detection**: Validate frequency detection in various environments (x86, ARM, containers)
- **Memory Limit Detection**: Test cgroups v1/v2 memory limit detection
- **Build-in-Container Tests**: All tests build all-smi inside containers for realistic scenarios

## Troubleshooting

#### "sudo: a terminal is required to read the password"
This error occurs when running tests that require sudo without proper authentication.
Solutions:
1. Run tests with sudo: `sudo cargo test`
2. Skip sudo tests: `SKIP_SUDO_TESTS=1 cargo test`
3. Use `--exclude` to skip specific test files

#### Tests hanging
Some tests might hang if they're waiting for sudo password input. Use:
```bash
cargo test -- --test-threads=1 --nocapture
```
This will show what's happening and run tests sequentially.