#!/bin/bash

echo "Testing powermetrics cleanup on app termination..."
echo "============================================="

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALL_SMI_BIN="$PROJECT_ROOT/target/release/all-smi"

# Check if all-smi binary exists
if [ ! -f "$ALL_SMI_BIN" ]; then
    echo "ERROR: all-smi binary not found at $ALL_SMI_BIN"
    echo "Please build the project first: cargo build --release"
    exit 1
fi

# Kill any existing powermetrics processes first
echo "Cleaning up any existing powermetrics processes..."
pkill -f powermetrics 2>/dev/null
sleep 1

# Check initial state
echo "Initial check for powermetrics processes:"
pgrep -fl powermetrics
if [ $? -eq 0 ]; then
    echo "WARNING: Found existing powermetrics processes before test"
else
    echo "✓ No powermetrics processes running (good)"
fi

echo ""
echo "Starting all-smi in background (will auto-terminate after 5 seconds)..."
# Start all-smi and send 'q' after 5 seconds to quit
(sleep 5 && echo 'q') | sudo "$ALL_SMI_BIN" &
APP_PID=$!

# Wait for app to start and initialize powermetrics
sleep 3

echo ""
echo "Checking for powermetrics while app is running:"
pgrep -fl powermetrics
if [ $? -eq 0 ]; then
    echo "✓ powermetrics is running (expected)"
else
    echo "WARNING: powermetrics not found while app is running"
fi

# Wait for app to exit (total 7 seconds: 3 already waited + 4 more)
sleep 4

echo ""
echo "App should have exited. Checking for powermetrics processes:"
pgrep -fl powermetrics
if [ $? -eq 0 ]; then
    echo "❌ FAILED: powermetrics processes still running after app exit!"
    echo "Cleaning up zombie processes..."
    pkill -f powermetrics
else
    echo "✅ SUCCESS: No powermetrics processes found after app exit!"
fi

echo ""
echo "Test complete."