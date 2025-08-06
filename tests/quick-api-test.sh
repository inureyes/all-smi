#!/bin/bash

echo "Quick container API test"
echo "========================"
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Kill any existing container
docker stop all-smi-quick 2>/dev/null || true

# Run container
docker run -d --rm \
    --name all-smi-quick \
    --memory="512m" \
    -v "$PROJECT_ROOT/target/debug/all-smi":/app/all-smi \
    -p 9999:9999 \
    ubuntu:22.04 \
    /app/all-smi api --port 9999

echo "Waiting for API to start..."
sleep 3

echo ""
echo "Checking stderr logs for debug output:"
docker logs all-smi-quick 2>&1 | grep DEBUG || echo "No debug output found"

echo ""
echo "Fetching metrics:"
curl -s http://localhost:9999/metrics | grep -E "memory_(total|used|available)" | head -10

echo ""
echo "Full container logs:"
docker logs all-smi-quick 2>&1

echo ""
echo "Stopping container..."
docker stop all-smi-quick