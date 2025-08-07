#!/bin/bash

echo "Benchmarking container metrics performance"
echo "=========================================="
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Check if binary exists
if [ ! -f "$PROJECT_ROOT/target/release/all-smi" ]; then
    echo "Error: Release binary not found. Please run 'cargo build --release' first."
    exit 1
fi

# Clean up any existing container
docker stop all-smi-benchmark 2>/dev/null || true
docker rm all-smi-benchmark 2>/dev/null || true

# Run container with memory limit
docker run -d --rm \
    --name all-smi-benchmark \
    --memory="512m" \
    --cpus="2" \
    -v "$PROJECT_ROOT/target/release/all-smi":/app/all-smi:ro \
    -p 9999:9999 \
    ubuntu:22.04 \
    /app/all-smi api --port 9999

echo "Waiting for API to start..."
sleep 5

# Check if API is responding
if ! curl -s http://localhost:9999/metrics > /dev/null; then
    echo "Error: API is not responding"
    docker logs all-smi-benchmark
    docker stop all-smi-benchmark
    exit 1
fi

echo ""
echo "Running benchmark (100 requests)..."
START_TIME=$(date +%s.%N)

for i in {1..100}; do
    curl -s http://localhost:9999/metrics > /dev/null
    if [ $((i % 20)) -eq 0 ]; then
        echo -n "."
    fi
done
echo ""

END_TIME=$(date +%s.%N)
DURATION=$(echo "$END_TIME - $START_TIME" | bc)

echo ""
echo "Benchmark Results:"
echo "=================="
echo "Total requests: 100"
echo "Total time: ${DURATION} seconds"
echo "Requests per second: $(echo "scale=2; 100 / $DURATION" | bc)"
echo "Average response time: $(echo "scale=3; $DURATION / 100 * 1000" | bc) ms"

echo ""
echo "Memory usage of all-smi process:"
docker exec all-smi-benchmark ps aux | grep all-smi | grep -v grep

echo ""
echo "Sample metrics output (first 20 lines):"
curl -s http://localhost:9999/metrics | head -20

echo ""
echo "Stopping container..."
docker stop all-smi-benchmark

echo ""
echo "Benchmark complete!"