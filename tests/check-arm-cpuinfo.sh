#!/bin/bash
# Check ARM CPU info format in container

echo "=== Checking ARM CPU info format ==="

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

docker run --rm \
    -v "$PROJECT_ROOT":/all-smi \
    -w /all-smi \
    ubuntu:22.04 \
    /bin/bash -c "
        echo '=== Full cpuinfo for first CPU ==='
        grep -A 20 'processor.*: 0$' /proc/cpuinfo | head -30
        
        echo ''
        echo '=== Checking for frequency fields ==='
        grep -i 'mhz\|freq\|bogomips' /proc/cpuinfo | head -10 || echo 'No frequency fields found'
        
        echo ''
        echo '=== Checking cpufreq sysfs ==='
        ls -la /sys/devices/system/cpu/cpu0/cpufreq/ 2>/dev/null || echo 'No cpufreq directory'
        
        echo ''
        echo '=== Checking if lscpu is available ==='
        which lscpu >/dev/null 2>&1 && lscpu | grep -i mhz || echo 'lscpu not available or no MHz info'
    "