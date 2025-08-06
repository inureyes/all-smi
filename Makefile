.PHONY: help local remote mock release test lint clean docker-dev

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
	@echo "  docker-dev           Run container dev env with bash"
	@echo ""
	@echo "Quality & Testing:"
	@echo "  test                 Run tests"
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

docker-dev:
	docker run -it --rm --name all-smi-container --memory="2g" --cpus="2" -v "$(PWD)":/all-smi ubuntu:24.04 bash

release:
	cargo build --release

test:
	cargo test --all

lint:
	cargo fmt --features=all -- --check
	cargo clippy --features=all -- -D warnings

clean:
	cargo clean