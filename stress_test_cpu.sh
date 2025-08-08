#!/bin/bash

# Script to stress specific CPU cores and verify all-smi is reading the correct ones

echo "=== CPU Stress Test for Container Detection ==="
echo ""

# Function to stress a specific CPU core
stress_cpu() {
    local cpu=$1
    local duration=$2
    echo "Stressing CPU $cpu for $duration seconds..."
    taskset -c $cpu timeout $duration bash -c 'while true; do :; done' &
}

# Check if we're in a container and get assigned CPUs
if [ -f /sys/fs/cgroup/cpuset.cpus ]; then
    CPUSET=$(cat /sys/fs/cgroup/cpuset.cpus)
elif [ -f /sys/fs/cgroup/cpuset.cpus.effective ]; then
    CPUSET=$(cat /sys/fs/cgroup/cpuset.cpus.effective)
elif [ -f /sys/fs/cgroup/cpuset/cpuset.cpus ]; then
    CPUSET=$(cat /sys/fs/cgroup/cpuset/cpuset.cpus)
else
    CPUSET=""
fi

if [ -n "$CPUSET" ] && [ "$CPUSET" != "" ]; then
    echo "Container is assigned to CPUs: $CPUSET"
    echo ""
    
    # Parse the first CPU from the cpuset
    FIRST_CPU=$(echo $CPUSET | cut -d',' -f1 | cut -d'-' -f1)
    
    echo "Will stress CPU $FIRST_CPU to verify all-smi reads the correct core"
    echo "Starting all-smi in 3 seconds, then stressing CPU $FIRST_CPU..."
    echo "Press 'c' in all-smi to see per-core utilization"
    echo ""
    
    sleep 3
    
    # Start stress in background
    stress_cpu $FIRST_CPU 30 &
    STRESS_PID=$!
    
    # Run all-smi
    ./target/release/all-smi local 
    
    # Clean up
    kill $STRESS_PID 2>/dev/null
else
    echo "No cpuset limits detected, running general CPU stress test"
    echo ""
    
    # Stress CPU 0
    echo "Stressing CPU 0 for verification..."
    stress_cpu 0 30 &
    STRESS_PID=$!
    
    # Run all-smi
    ./target/release/all-smi view
    
    # Clean up
    kill $STRESS_PID 2>/dev/null
fi

echo ""
echo "Test completed"
