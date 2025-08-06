#!/bin/bash

echo "Testing container-aware metrics in all-smi API mode"
echo "==================================================="
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Build the release binary
echo "Building all-smi..."
cd "$PROJECT_ROOT" && cargo build --release

echo ""
echo "Test 1: Running outside container (baseline)"
echo "--------------------------------------------"
echo "Starting API mode..."
"$PROJECT_ROOT/target/release/all-smi" api --port 9999 &
API_PID=$!

sleep 3

echo "Fetching metrics..."
curl -s http://localhost:9999/metrics | grep -E "(all_smi_cpu_core_count|all_smi_cpu_utilization|all_smi_memory_total_bytes|all_smi_memory_used_bytes)" | head -10

echo ""
echo "Stopping API server..."
kill $API_PID
wait $API_PID 2>/dev/null

echo ""
echo "Test 2: Running inside Docker container with CPU/Memory limits"
echo "--------------------------------------------------------------"
echo "Note: This requires Docker to be installed and running"
echo ""

# Create a simple Dockerfile if it doesn't exist
DOCKERFILE_PATH="$PROJECT_ROOT/Dockerfile.test"
if [ ! -f "$DOCKERFILE_PATH" ]; then
cat > "$DOCKERFILE_PATH" << 'EOF'
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY target/release/all-smi /app/

EXPOSE 9999

CMD ["/app/all-smi", "api", "--port", "9999"]
EOF
fi

# Build Docker image
echo "Building Docker image..."
docker build -f "$DOCKERFILE_PATH" -t all-smi-test "$PROJECT_ROOT"

# Run container with resource limits
echo "Running container with CPU limit=1.5 and Memory limit=512MB..."
docker run -d --rm \
    --name all-smi-test \
    --cpus="1.5" \
    --memory="512m" \
    -p 9999:9999 \
    all-smi-test

sleep 5

echo "Fetching metrics from containerized all-smi..."
curl -s http://localhost:9999/metrics | grep -E "(all_smi_cpu_core_count|all_smi_cpu_utilization|all_smi_memory_total_bytes|all_smi_memory_used_bytes)" | head -10

echo ""
echo "Container runtime info:"
curl -s http://localhost:9999/metrics | grep "all_smi_container_runtime_info"

echo ""
echo "Stopping container..."
docker stop all-smi-test

echo ""
echo "Test complete!"
echo ""
echo "Expected results:"
echo "- Outside container: Shows full system CPU cores and memory"
echo "- Inside container: Shows limited CPU cores (1-2) and memory (512MB)"