#!/bin/bash
# Script to test CPU frequency monitoring in container (simplified version)

echo "=== Testing CPU frequency monitoring in container (simplified) ==="

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo "Project root: $PROJECT_ROOT"
echo ""

# Ensure we have a release build
if [ ! -f "$PROJECT_ROOT/target/release/all-smi" ]; then
    echo "ERROR: Release binary not found. Please run 'cargo build --release' first."
    exit 1
fi

# Clean up any existing containers first
echo "0. Cleaning up any existing test containers..."
docker stop all-smi-cpu-test-cpuset all-smi-cpu-test-quota 2>/dev/null || true
docker rm all-smi-cpu-test-cpuset all-smi-cpu-test-quota 2>/dev/null || true

# Run container with cpuset limitation
echo "1. Running container with cpuset limitation (CPU 0,1)..."
docker run -d --rm \
    --name all-smi-cpu-test-cpuset \
    --cpuset-cpus="0,1" \
    -p 9090:9090 \
    -v "$PROJECT_ROOT/target/release/all-smi":/app/all-smi:ro \
    ubuntu:22.04 \
    /app/all-smi api --port 9090

# Run container with CPU quota limitation (2 CPUs worth)
echo "2. Running container with CPU quota limitation..."
docker run -d --rm \
    --name all-smi-cpu-test-quota \
    --cpu-quota=200000 --cpu-period=100000 \
    -p 9091:9091 \
    -v "$PROJECT_ROOT/target/release/all-smi":/app/all-smi:ro \
    ubuntu:22.04 \
    /app/all-smi api --port 9091

# Give containers a moment to start
echo "3. Waiting for containers to start..."
sleep 3

# Check if containers are still running
echo "4. Checking container status..."
docker ps | grep all-smi-cpu-test || echo "Warning: Some containers may have exited"

# Check container logs for CPU frequency detection
echo ""
echo "5. Checking container logs for CPU frequency detection..."
echo "=== Container logs (cpuset mode) ==="
docker logs all-smi-cpu-test-cpuset 2>&1 | head -20

echo ""
echo "=== Container logs (quota mode) ==="
docker logs all-smi-cpu-test-quota 2>&1 | head -20

# Check if API is responding
echo ""
echo "6. Checking API endpoints..."
echo "=== Cpuset container API check ==="
if curl -s -f http://localhost:9090/metrics >/dev/null 2>&1; then
    echo "API is responding on port 9090"
    # Check CPU frequency metrics
    echo "CPU frequency metrics:"
    curl -s http://localhost:9090/metrics | grep -E "all_smi_cpu_frequency_mhz|all_smi_cpu_socket_frequency_mhz" || echo "No frequency metrics found"
else
    echo "API is not responding on port 9090"
fi

echo ""
echo "=== Quota container API check ==="
if curl -s -f http://localhost:9091/metrics >/dev/null 2>&1; then
    echo "API is responding on port 9091"
    # Check CPU frequency metrics
    echo "CPU frequency metrics:"
    curl -s http://localhost:9091/metrics | grep -E "all_smi_cpu_frequency_mhz|all_smi_cpu_socket_frequency_mhz" || echo "No frequency metrics found"
else
    echo "API is not responding on port 9091"
fi

# Show CPU info in container for debugging
echo ""
echo "7. Debugging: CPU info in cpuset container..."
docker exec all-smi-cpu-test-cpuset bash -c "grep -E 'processor|cpu MHz|model name' /proc/cpuinfo 2>/dev/null | head -20" || echo "Container not running"

echo ""
echo "8. Debugging: Cgroup info in cpuset container..."
docker exec all-smi-cpu-test-cpuset bash -c "cat /sys/fs/cgroup/cpuset/cpuset.cpus 2>/dev/null || cat /sys/fs/cgroup/cpuset.cpus 2>/dev/null || echo 'cpuset info not available'" || echo "Container not running"

# Compare with host CPU frequency
echo ""
echo "9. Host CPU frequency for comparison:"
grep "cpu MHz" /proc/cpuinfo | head -5

# Cleanup
echo ""
echo "10. Cleaning up..."
docker stop all-smi-cpu-test-cpuset all-smi-cpu-test-quota 2>/dev/null || true

echo ""
echo "=== Test complete ==="