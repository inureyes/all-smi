#!/bin/bash

echo "=== Container CPU Information Test ==="
echo ""
echo "1. Container Detection:"
if [ -f /.dockerenv ]; then
    echo "   ✓ Running in Docker container"
else
    echo "   ✗ Not in Docker container"
fi

echo ""
echo "2. CPU Information from /proc/cpuinfo:"
echo "   Model name:"
grep "model name" /proc/cpuinfo | head -1 | sed 's/^/   /'
echo "   CPU MHz:"
grep "cpu MHz" /proc/cpuinfo | head -5 | sed 's/^/   /'

echo ""
echo "3. CPU Frequency from sysfs:"
echo "   Max frequency:"
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq ]; then
    freq=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq)
    echo "   $(( freq / 1000 )) MHz"
else
    echo "   Not available (common in containers)"
fi

echo ""
echo "4. Current frequency:"
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq ]; then
    freq=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq)
    echo "   $(( freq / 1000 )) MHz"
else
    echo "   Not available (common in containers)"
fi

echo ""
echo "5. Container CPU limits:"
if [ -f /sys/fs/cgroup/cpu/cpu.cfs_quota_us ]; then
    quota=$(cat /sys/fs/cgroup/cpu/cpu.cfs_quota_us)
    period=$(cat /sys/fs/cgroup/cpu/cpu.cfs_period_us)
    if [ "$quota" -gt 0 ]; then
        cpus=$(echo "scale=2; $quota / $period" | bc -l 2>/dev/null || echo "N/A")
        echo "   CPU quota: $cpus CPUs"
    else
        echo "   No CPU quota limit"
    fi
else
    echo "   cgroup v1 not found, checking cgroup v2..."
    if [ -f /sys/fs/cgroup/cpu.max ]; then
        cpu_max=$(cat /sys/fs/cgroup/cpu.max)
        echo "   CPU max: $cpu_max"
    else
        echo "   No cgroup CPU limits found"
    fi
fi

echo ""
echo "6. Building and running all-smi..."
cargo build --release --quiet
if [ $? -eq 0 ]; then
    echo ""
    echo "7. all-smi API output (running for 10 seconds):"
    ./target/release/all-smi api --port 9090 &
    API_PID=$!
    sleep 5
    
    echo ""
    echo "8. CPU metrics from API:"
    curl -s http://localhost:9090/metrics | grep -E "all_smi_cpu_base_frequency_mhz|all_smi_cpu_max_frequency_mhz|all_smi_cpu_info" | head -10
    
    kill $API_PID 2>/dev/null
    wait $API_PID 2>/dev/null
else
    echo "   Build failed!"
fi