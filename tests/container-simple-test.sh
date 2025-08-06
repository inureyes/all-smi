#!/bin/bash

echo "Simple container memory test"
echo "============================"
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Run a simple test
docker run --rm -it \
    --memory="512m" \
    -v "$PROJECT_ROOT/target/debug/all-smi":/app/all-smi \
    ubuntu:22.04 \
    /bin/bash -c "
        echo 'Container cgroup info:'
        cat /proc/self/cgroup
        echo ''
        echo 'Memory files (cgroups v2):'
        ls -la /sys/fs/cgroup/memory.* 2>/dev/null || echo 'No cgroups v2 memory files'
        echo ''
        echo 'Memory files (cgroups v1):'
        ls -la /sys/fs/cgroup/memory/ 2>/dev/null | head -10 || echo 'No cgroups v1 memory files'
        echo ''
        echo 'Running all-smi view to see memory detection:'
        /app/all-smi view 2>&1 | head -20
    "