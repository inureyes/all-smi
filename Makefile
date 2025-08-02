.PHONY: help local remote mock release test lint clean

help:
	@echo "all-smi"
	@echo ""
	@echo "Available targets:"
	@echo ""
	@echo "Setup & Building:"
	@echo "  local                Run for local view mode"
	@echo "  remote               Run for remote view mode"
	@echo "  api                  Run for API mode"
	@echo "  mock                 Run mock server for testing"
	@echo ""
	@echo "Quality & Testing:"
	@echo "  test                 Run tests"
	@echo ""
	@echo "Quality & Testing:"
	@echo "  validate             Validate links and content"
	@echo "  lint                 Run linting on documentation"
	@echo "  test                 Run all tests"
	@echo ""
	@echo "Deployment:"
	@echo "  release              Build release binaries"
	@echo "  clean                Clean build artifacts"

local:
	cargo run --bin all-smi -- view 

api:
	cargo run --bin all-smi -- api


remote:
	cargo run --bin all-smi -- view --hostfile ./hosts.csv

mock:
	cargo run --features mock --bin all-smi-mock-server -- --port-range 10001-10050

release:
	cargo build --release

test:
	cargo test --all

lint:
	cargo fmt --features=all -- --check
	cargo clippy --features=all -- -D warnings

clean:
	cargo clean
