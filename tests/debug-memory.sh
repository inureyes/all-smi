#!/bin/bash

echo "Debugging container memory usage detection"
echo "=========================================="
echo ""

# Check cgroup version
echo "Checking cgroup version..."
if [ -f "/sys/fs/cgroup/cgroup.controllers" ]; then
    echo "Cgroups v2 detected"
    echo ""
    echo "Memory files:"
    ls -la /sys/fs/cgroup/memory.* 2>/dev/null || echo "No memory files found"
    echo ""
    echo "memory.current content:"
    cat /sys/fs/cgroup/memory.current 2>/dev/null || echo "Cannot read memory.current"
    echo ""
    echo "memory.max content:"
    cat /sys/fs/cgroup/memory.max 2>/dev/null || echo "Cannot read memory.max"
    echo ""
    echo "memory.stat content (first 10 lines):"
    head -10 /sys/fs/cgroup/memory.stat 2>/dev/null || echo "Cannot read memory.stat"
else
    echo "Cgroups v1 detected"
    echo ""
    echo "Memory controller path:"
    ls -la /sys/fs/cgroup/memory/ 2>/dev/null | head -10 || echo "No memory controller found"
    echo ""
    echo "memory.usage_in_bytes content:"
    cat /sys/fs/cgroup/memory/memory.usage_in_bytes 2>/dev/null || echo "Cannot read memory.usage_in_bytes"
    echo ""
    echo "memory.limit_in_bytes content:"
    cat /sys/fs/cgroup/memory/memory.limit_in_bytes 2>/dev/null || echo "Cannot read memory.limit_in_bytes"
fi

echo ""
echo "Process cgroup membership:"
cat /proc/self/cgroup

echo ""
echo "Container detection files:"
ls -la /.dockerenv 2>/dev/null && echo "Docker detected" || echo "No .dockerenv"
ls -la /run/.containerenv 2>/dev/null && echo "Podman detected" || echo "No .containerenv"