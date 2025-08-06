#!/bin/bash

echo "Testing container CPU detection"
echo "==============================="
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Clean up any existing container
docker stop all-smi-cpu-test 2>/dev/null || true
docker rm all-smi-cpu-test 2>/dev/null || true

echo "Starting container with CPU limits (1.5 CPUs)..."
docker run -d --rm \
    --name all-smi-cpu-test \
    --cpus="1.5" \
    --memory="512m" \
    -v "$PROJECT_ROOT/target/release/all-smi":/app/all-smi:ro \
    -p 9999:9999 \
    ubuntu:22.04 \
    /bin/bash -c "
        apt-get update && apt-get install -y stress-ng curl && 
        echo 'Starting all-smi API...' && 
        /app/all-smi api --port 9999 &
        API_PID=\$!
        
        sleep 5
        
        echo 'CPU info from container:' && 
        cat /sys/fs/cgroup/cpu.max 2>/dev/null || echo 'cgroups v2 not found' && 
        cat /sys/fs/cgroup/cpu/cpu.cfs_quota_us 2>/dev/null || echo 'cgroups v1 not found'
        
        echo 'Starting CPU stress...' && 
        stress-ng --cpu 4 --timeout 60s &
        
        tail -f /dev/null
    "

echo ""
echo "Waiting for container to start..."
sleep 10

echo ""
echo "CPU metrics (should show ~1.5 effective CPUs):"
curl -s http://localhost:9999/metrics | grep -E "all_smi_cpu_(core_count|utilization)" | grep -v "per_core" | head -5

echo ""
echo "Waiting for CPU stress to kick in..."
sleep 10

echo ""
echo "CPU metrics during stress:"
curl -s http://localhost:9999/metrics | grep -E "all_smi_cpu_(core_count|utilization)" | grep -v "per_core" | head -5

echo ""
echo "Container logs (last 20 lines):"
docker logs all-smi-cpu-test 2>&1 | tail -20

echo ""
echo "Stopping container..."
docker stop all-smi-cpu-test

echo ""
echo "Test complete!"