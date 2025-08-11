# All-SMI Refactoring TODO List

This document provides a checklist for incremental refactoring of the all-smi project.
Each task can be completed independently and allows for immediate testing and deployment upon completion.

## Work Progress Guide

Use the Work Progress Guide checkboxes below to record the status of each phase's subsections as they are executed and verified. Clear the checkboxes when complete.

- [ ] Verify all tests pass on current branch before starting work
- [ ] Work on each task in a separate feature branch
- [x] Run `cargo test` after completing task
- [x] Run `cargo clippy` after completing task
- [x] Run `cargo fmt --check` after completing task
- [ ] Test actual behavior with relevant mock server before creating PR

---
Read the guide below and check the box when you have completed each step.

## Phase 1: Foundation

### 1.1 Extract Common Device Patterns
- [x] Create `src/device/common/` directory
- [x] Create `src/device/common/command_executor.rs`
  - [x] Analyze Command execution patterns in existing device files
  - [x] Implement common `execute_command()` function
  - [x] Standardize timeout and error handling
  - [x] Write unit tests
- [x] Create `src/device/common/error_handling.rs`
  - [x] Define `DeviceError` enum
  - [x] Define Result type aliases
  - [x] Implement error conversion traits
- [x] Create `src/device/common/json_parser.rs`
  - [x] Implement common JSON parsing utilities
  - [x] Implement generic parsing functions
- [x] Migrate one device (nvidia.rs) to use common modules
- [x] Migrate other devices to use common modules
- [x] Test: `cargo test --test device_tests`
- [x] Verify NVIDIA GPU operation with mock server

### 1.2 Create Parsing Macros and Utilities
- [x] Create `src/parsing/` directory
- [x] Create `src/parsing/macros.rs`
  - [x] Implement `parse_metric!` macro
  - [x] Implement `parse_prometheus!` macro
  - [x] Write macro tests
- [x] Create `src/parsing/common.rs`
  - [x] Number parsing utilities
  - [x] Unit conversion functions
  - [x] String sanitization functions
- [x] Replace some functions in powermetrics_parser.rs with macros
  - [x] Select 5 parsing functions to apply macros
  - [x] Verify existing tests still pass
- [x] Replace some functions in other parsers with macros
- [x] Test: `cargo test --lib parsing`

### 1.3 Define Base Traits
- [x] Create `src/traits/` directory
- [x] Create `src/traits/renderer.rs`
  - [x] Define `DeviceRenderer` trait
  - [x] Define common rendering methods
- [x] Create `src/traits/collector.rs`
  - [x] Define `DataCollector` trait
  - [x] Define local/remote collection interfaces
- [x] Create `src/traits/exporter.rs`
  - [x] Define `MetricsExporter` trait
  - [x] Define Prometheus metrics export interface
- [x] Create `src/traits/mock_generator.rs`
  - [x] Define `MockGenerator` trait
  - [x] Define template generation interface
- [x] Validate trait usability with existing code (actual application in Phase 2)

---

## Phase 2: Split Large Files

### 2.1 Mock Template Refactoring
- [x] Create `src/mock/templates/` directory
- [x] Backup: `cp src/mock/template.rs src/mock/template.rs.backup`
- [x] Create `src/mock/templates/nvidia.rs`
  - [x] Move NVIDIA-related templates only (~200 lines)
  - [x] Implement `NvidiaMockGenerator`
  - [x] Remove NVIDIA section from existing template.rs
  - [ ] Test: Run mock server in NVIDIA mode to verify operation
- [x] Create `src/mock/templates/apple_silicon.rs`
  - [x] Move Apple Silicon templates (~150 lines)
  - [x] Implement `AppleSiliconMockGenerator`
  - [x] Remove Apple Silicon section from existing template.rs
  - [ ] Test: Run mock server in Apple mode to verify operation
- [x] Create `src/mock/templates/jetson.rs`
  - [x] Move Jetson templates (~180 lines)
  - [ ] Test: Run mock server in Jetson mode
- [x] Create `src/mock/templates/tenstorrent.rs`
  - [x] Move Tenstorrent templates (~220 lines)
  - [ ] Test: Run mock server in Tenstorrent mode
- [x] Create `src/mock/templates/rebellions.rs`
  - [x] Move Rebellions templates (~160 lines)
  - [ ] Test: Run mock server in Rebellions mode
- [x] Create `src/mock/templates/furiosa.rs`
  - [x] Move Furiosa templates (~180 lines)
  - [ ] Test: Run mock server in Furiosa mode
- [x] Create `src/mock/templates/disk.rs`
  - [x] Move disk metrics templates (~100 lines)
- [x] Create `src/mock/template_engine.rs`
  - [x] Implement common template rendering logic
  - [x] Platform-specific MockGenerator selection logic
- [x] Remove existing `template.rs` file
- [x] Integration test: Verify mock server runs with all platform types

### 2.2 Split UI Device Renderers
- [ ] Create `src/ui/renderers/` directory
- [ ] Backup: `cp src/ui/device_renderers.rs src/ui/device_renderers.rs.backup`
- [ ] Create `src/ui/renderers/widgets/` directory
- [ ] Create `src/ui/renderers/widgets/tables.rs`
  - [ ] Extract common table rendering functions
  - [ ] Move `render_info_table()` function
  - [ ] Define table style constants
- [ ] Create `src/ui/renderers/widgets/gauges.rs`
  - [ ] Extract common gauge rendering functions
  - [ ] Move `render_gauge()` function
  - [ ] Define gauge style constants
- [ ] Create `src/ui/renderers/gpu_renderer.rs`
  - [ ] Move GPU-related rendering functions only (~300 lines)
  - [ ] Implement `GpuRenderer` struct
  - [ ] Implement DeviceRenderer trait
  - [ ] Remove GPU section from existing file
  - [ ] Test: Verify GPU tab works in TUI view mode
- [ ] Create `src/ui/renderers/cpu_renderer.rs`
  - [ ] Move CPU-related rendering functions (~250 lines)
  - [ ] Implement `CpuRenderer` struct
  - [ ] Test: Verify CPU tab works in TUI view mode
- [ ] Create `src/ui/renderers/memory_renderer.rs`
  - [ ] Move Memory-related rendering functions (~200 lines)
  - [ ] Test: Verify Memory tab works in TUI view mode
- [ ] Create `src/ui/renderers/storage_renderer.rs`
  - [ ] Move Storage-related rendering functions (~150 lines)
  - [ ] Test: Verify Storage tab works in TUI view mode
- [ ] Create `src/ui/renderers/mod.rs`
  - [ ] Define public interface
  - [ ] Renderer factory functions
- [ ] Remove existing `device_renderers.rs`
- [ ] Integration test: Verify all tab switching and rendering

### 2.3 Split PowerMetrics Manager
- [ ] Create `src/device/powermetrics/` directory
- [ ] Backup: `cp src/device/powermetrics_manager.rs src/device/powermetrics_manager.rs.backup`
- [ ] Create `src/device/powermetrics/config.rs`
  - [ ] Move configuration constants
  - [ ] Define `PowerMetricsConfig` struct
  - [ ] Implement defaults
- [ ] Create `src/device/powermetrics/store.rs`
  - [ ] Extract `MetricsStore` struct (~300 lines)
  - [ ] Move circular buffer logic
  - [ ] Implement data storage/retrieval methods
  - [ ] Write unit tests
- [ ] Create `src/device/powermetrics/process.rs`
  - [ ] Extract process management logic (~400 lines)
  - [ ] Implement `ProcessManager` struct
  - [ ] Process start/stop/restart logic
  - [ ] Write unit tests
- [ ] Create `src/device/powermetrics/collector.rs`
  - [ ] Background collection task logic (~200 lines)
  - [ ] Implement `DataCollector` struct
  - [ ] Parsing and storage integration
- [ ] Create `src/device/powermetrics/manager.rs`
  - [ ] Refactored `PowerMetricsManager` (~300 lines)
  - [ ] Maintain existing singleton pattern but use modules internally
  - [ ] Maintain public API (backward compatibility)
- [ ] Create `src/device/powermetrics/mod.rs`
  - [ ] Define public interface
  - [ ] Setup re-exports
- [ ] Remove existing `powermetrics_manager.rs`
- [ ] Test: `sudo cargo test` on macOS
- [ ] Test: Verify actual view mode execution on macOS

---

## Phase 3: Eliminate Duplication

### 3.1 Consolidate Device Implementations
- [ ] Create `src/device/readers/` directory
- [ ] Utilize common modules from Phase 1.1
- [ ] Refactor `src/device/readers/nvidia.rs`
  - [ ] Replace with common/command_executor usage
  - [ ] Replace with common/error_handling usage
  - [ ] Verify code reduced to ~300 lines
  - [ ] Verify existing tests pass
- [ ] Refactor `src/device/readers/furiosa.rs`
  - [ ] Apply common modules (reduce to ~400 lines)
  - [ ] Test: Verify mock server Furiosa mode
- [ ] Refactor `src/device/readers/tenstorrent.rs`
  - [ ] Apply common modules (reduce to ~350 lines)
  - [ ] Test: Verify mock server Tenstorrent mode
- [ ] Refactor `src/device/readers/rebellions.rs`
  - [ ] Apply common modules (reduce to ~300 lines)
  - [ ] Test: Verify mock server Rebellions mode
- [ ] Sequential migration of remaining device files
- [ ] Move existing device files to readers/ directory
- [ ] Update `src/device/mod.rs`

### 3.2 Consolidate NPU Metrics Export
- [ ] Create `src/api/metrics/npu/` directory
- [ ] Create `src/api/metrics/npu/exporter_trait.rs`
  - [ ] Define `NpuExporter` trait
  - [ ] Define common export methods
- [ ] Create `src/api/metrics/npu/common.rs`
  - [ ] Extract common NPU metric patterns
  - [ ] Implement helper functions
- [ ] Create `src/api/metrics/npu/tenstorrent.rs`
  - [ ] Keep only Tenstorrent-specific logic (~200 lines)
  - [ ] Implement NpuExporter trait
- [ ] Create `src/api/metrics/npu/rebellions.rs`
  - [ ] Keep only Rebellions-specific logic (~180 lines)
  - [ ] Implement NpuExporter trait
- [ ] Create `src/api/metrics/npu/furiosa.rs`
  - [ ] Keep only Furiosa-specific logic (~160 lines)
  - [ ] Implement NpuExporter trait
- [ ] Remove existing `npu.rs` file
- [ ] Test: Verify `/metrics` endpoint in API mode

### 3.3 Remove Parser Boilerplate
- [ ] Extend parsing macros from Phase 1.2
- [ ] Refactor `src/device/powermetrics_parser.rs`
  - [ ] Replace repetitive parsing functions with macros
  - [ ] Target: reduce to ~400 lines
  - [ ] Verify all existing tests pass
- [ ] Refactor `src/network/metrics_parser.rs`
  - [ ] Replace Prometheus parsing patterns with macros
  - [ ] Target: reduce to ~300 lines
  - [ ] Verify remote monitoring test

---

## Phase 4: Architecture Improvements

### 4.1 Data Collection Strategy Pattern
- [ ] Create `src/view/data_collection/` directory
- [ ] Create `src/view/data_collection/strategy.rs`
  - [ ] Define `DataCollectionStrategy` trait
  - [ ] Define `collect()` method
- [ ] Create `src/view/data_collection/local_collector.rs`
  - [ ] Move local data collection logic (~300 lines)
  - [ ] Implement DataCollectionStrategy
- [ ] Create `src/view/data_collection/remote_collector.rs`
  - [ ] Move remote data collection logic (~250 lines)
  - [ ] Implement DataCollectionStrategy
- [ ] Create `src/view/data_collection/aggregator.rs`
  - [ ] Data aggregation logic (~200 lines)
- [ ] Refactor existing `data_collector.rs`
  - [ ] Simplify using Strategy pattern
  - [ ] Remove `if is_remote()` checks
- [ ] Test: Verify local and remote monitoring operations

### 4.2 Standardize Error Handling
- [ ] Review project-wide `.unwrap()` usage
  - [ ] Run `rg "\.unwrap\(\)" --type rust | wc -l` to check current count
- [ ] Remove unwraps from critical paths (batch of 10)
  - [ ] Remove unwraps from `src/main.rs`
  - [ ] Remove unwraps from `src/view/` directory
  - [ ] Remove unwraps from `src/api/` directory
  - [ ] Run tests after each batch
- [ ] Add error context using `anyhow::Context`
- [ ] Define and apply custom error types
- [ ] Verify final unwrap count (target: 50%+ reduction)

### 4.3 Improve Dependency Injection
- [ ] Create `src/di/` directory (optional)
- [ ] Improve PowerMetricsManager singleton pattern
  - [ ] Add mock implementation for testing
  - [ ] Improve interface for dependency injection
- [ ] Implement factory pattern for device reader creation
- [ ] Verify improved testability

---

## Completion Criteria

Upon completion of each Phase:
- [ ] All existing tests pass
- [ ] No `cargo clippy` warnings
- [ ] `cargo fmt --check` passes
- [ ] Mock server works for all platform modes
- [ ] View mode all tabs function normally
- [ ] API mode `/metrics` endpoint responds correctly
- [ ] Remote monitoring works correctly

## Refactoring Success Metrics

### Code Quality Metrics
- [ ] Average file size: 1000 lines → under 400 lines
- [ ] Maximum file size: 1227 lines → under 500 lines
- [ ] Unwrap usage: 408 → under 200
- [ ] Code duplication rate: 30% reduction target

### Development Efficiency Metrics
- [ ] New hardware support addition time: 50% reduction
- [ ] Average bug fix time: 30% reduction
- [ ] Test coverage: 20% increase from baseline

---

## Important Notes

1. **Never work on multiple Phases simultaneously**
2. **Always run full test suite after each task**
3. **Must verify actual behavior with mock server**
4. **Rollback immediately if performance degrades**
5. **Must maintain API backward compatibility**

## Rollback Plan

Create backup files before each task (.backup extension)
If issues occur:
1. Rollback to previous commit with Git
2. Restore from backup files
3. Analyze problem and retry

---

Last updated: 2025-08-09
