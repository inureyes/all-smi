#!/bin/bash

echo "Simple container memory test"
echo "============================"
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Clean up any existing container
docker stop all-smi-simple-test 2>/dev/null || true
docker rm all-smi-simple-test 2>/dev/null || true

echo "Starting container with 512MB memory limit..."
docker run -d --rm \
    --name all-smi-simple-test \
    --memory="512m" \
    -v "$PROJECT_ROOT/target/release/all-smi":/app/all-smi \
    -p 9999:9999 \
    ubuntu:22.04 \
    /bin/bash -c "
        apt-get update && apt-get install -y stress-ng && 
        echo 'Starting all-smi API...' && 
        /app/all-smi api --port 9999 &
        API_PID=\$!
        
        sleep 5
        
        echo 'Memory usage before stress:' && 
        cat /sys/fs/cgroup/memory.current 2>/dev/null || cat /sys/fs/cgroup/memory/memory.usage_in_bytes 2>/dev/null
        
        echo 'Starting memory stress (200MB)...' && 
        stress-ng --vm 1 --vm-bytes 200M --timeout 60s &
        
        sleep 10
        
        echo 'Memory usage during stress:' && 
        cat /sys/fs/cgroup/memory.current 2>/dev/null || cat /sys/fs/cgroup/memory/memory.usage_in_bytes 2>/dev/null
        
        tail -f /dev/null
    "

echo ""
echo "Waiting for container to start..."
sleep 10

echo ""
echo "Initial memory metrics:"
curl -s http://localhost:9999/metrics | grep -E "all_smi_memory_(total|used|available)_bytes" | head -5

echo ""
echo "Waiting for memory stress to kick in..."
sleep 15

echo ""
echo "Memory metrics during stress:"
curl -s http://localhost:9999/metrics | grep -E "all_smi_memory_(total|used|available)_bytes" | head -5

echo ""
echo "Container logs:"
docker logs all-smi-simple-test 2>&1 | tail -20

echo ""
echo "Stopping container..."
docker stop all-smi-simple-test

echo "Test complete!"