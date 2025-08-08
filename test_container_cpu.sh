#!/bin/bash

# Test script to verify container CPU detection and utilization reading

echo "=== Testing Container CPU Detection ==="
echo "Host CPU count: $(nproc)"
echo ""

# Check if we're in a container
if [ -f /.dockerenv ] || [ -f /proc/self/cgroup ]; then
    echo "Container environment detected"
    
    # Check cpuset limits
    if [ -f /sys/fs/cgroup/cpuset.cpus ]; then
        echo "Cpuset CPUs (cgroups v2): $(cat /sys/fs/cgroup/cpuset.cpus)"
    elif [ -f /sys/fs/cgroup/cpuset/cpuset.cpus ]; then
        echo "Cpuset CPUs (cgroups v1): $(cat /sys/fs/cgroup/cpuset/cpuset.cpus)"
    else
        echo "No cpuset limits found"
    fi
    
    # Check CPU quota
    if [ -f /sys/fs/cgroup/cpu.max ]; then
        echo "CPU quota (cgroups v2): $(cat /sys/fs/cgroup/cpu.max)"
    elif [ -f /sys/fs/cgroup/cpu/cpu.cfs_quota_us ]; then
        quota=$(cat /sys/fs/cgroup/cpu/cpu.cfs_quota_us)
        period=$(cat /sys/fs/cgroup/cpu/cpu.cfs_period_us)
        echo "CPU quota (cgroups v1): $quota / $period"
        if [ "$quota" -gt 0 ] && [ "$period" -gt 0 ]; then
            effective_cpus=$(echo "scale=2; $quota / $period" | bc)
            echo "Effective CPUs from quota: $effective_cpus"
        fi
    fi
else
    echo "Not running in a container"
fi

echo ""
echo "=== Running all-smi to check CPU detection ==="
echo "Press 'c' to toggle per-core CPU display, 'q' to quit"
echo ""

# Run all-smi in view mode
./target/release/all-smi view