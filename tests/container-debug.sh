#!/bin/bash

echo "Debugging container environment"
echo "==============================="
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Run a debug container
docker run -it --rm \
    --name all-smi-debug \
    --memory="512m" \
    --cpus="2" \
    -v "$PROJECT_ROOT/target/debug/all-smi":/app/all-smi:ro \
    -v "$PROJECT_ROOT/tests/debug-memory.sh":/app/debug-memory.sh:ro \
    ubuntu:22.04 \
    /bin/bash -c "
        apt-get update && apt-get install -y procps stress-ng
        echo ''
        echo '=== Container Detection ==='
        ls -la /.dockerenv 2>/dev/null && echo 'Docker detected' || echo 'No .dockerenv'
        echo ''
        echo '=== Cgroup Info ==='
        cat /proc/self/cgroup
        echo ''
        echo '=== Running debug script ==='
        chmod +x /app/debug-memory.sh
        /app/debug-memory.sh
        echo ''
        echo '=== Memory Files ==='
        echo 'Cgroups v2:'
        ls -la /sys/fs/cgroup/memory.* 2>/dev/null | head -10
        echo ''
        echo 'Cgroups v1:'
        ls -la /sys/fs/cgroup/memory/ 2>/dev/null | head -10
        echo ''
        echo '=== CPU Files ==='
        echo 'Cgroups v2:'
        ls -la /sys/fs/cgroup/cpu.* 2>/dev/null | head -10
        echo ''
        echo 'Cgroups v1:'
        ls -la /sys/fs/cgroup/cpu/ 2>/dev/null | head -10
        echo ''
        echo '=== Running all-smi view ==='
        /app/all-smi view 2>&1 | head -30 || echo 'Failed to run all-smi view'
    "