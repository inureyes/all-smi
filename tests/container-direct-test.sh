#!/bin/bash

echo "Direct container test with debug output"
echo "======================================="
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Run container interactively
docker run -it --rm \
    --name all-smi-direct \
    --memory="512m" \
    -v "$PROJECT_ROOT/target/debug/all-smi":/app/all-smi \
    ubuntu:22.04 \
    /bin/bash -c "
        apt-get update && apt-get install -y stress-ng
        echo ''
        echo 'Starting all-smi API with debug output...'
        /app/all-smi api --port 9999 2>&1 | grep DEBUG &
        API_PID=\$!
        
        sleep 3
        
        echo ''
        echo 'Starting memory stress (100MB)...'
        stress-ng --vm 1 --vm-bytes 100M --timeout 30s &
        
        sleep 5
        
        echo ''
        echo 'Checking metrics (should see debug output)...'
        curl -s http://localhost:9999/metrics | grep memory_ | head -10
        
        echo ''
        echo 'Waiting for more debug output...'
        sleep 10
        
        kill \$API_PID 2>/dev/null
    "