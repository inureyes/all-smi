#!/bin/bash

echo "Testing container memory detection"
echo "=================================="
echo ""

# Get the project root directory (parent of tests)
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Clean up any existing container
docker stop all-smi-memory-test 2>/dev/null || true
docker rm all-smi-memory-test 2>/dev/null || true

# Create memory test program
cat > /tmp/memory-eater.c << 'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main() {
    printf("Allocating 100MB of memory...\n");
    
    size_t size = 100 * 1024 * 1024;
    char *buffer = malloc(size);
    
    if (buffer == NULL) {
        printf("Failed to allocate memory\n");
        return 1;
    }
    
    memset(buffer, 'A', size);
    printf("Memory allocated and written. Sleeping for 30 seconds...\n");
    
    sleep(30);
    
    free(buffer);
    printf("Memory freed.\n");
    return 0;
}
EOF

# Compile memory test program
gcc -o /tmp/memory-eater /tmp/memory-eater.c

# Create Dockerfile for test
cat > /tmp/Dockerfile.memtest << 'EOF'
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    curl \
    gcc \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY all-smi /app/
COPY memory-eater /app/

EXPOSE 9999

# Create entrypoint script
RUN echo '#!/bin/bash' > /app/entrypoint.sh && \
    echo '/app/all-smi api --port 9999 &' >> /app/entrypoint.sh && \
    echo 'API_PID=$!' >> /app/entrypoint.sh && \
    echo 'sleep 5' >> /app/entrypoint.sh && \
    echo '/app/memory-eater &' >> /app/entrypoint.sh && \
    echo 'EATER_PID=$!' >> /app/entrypoint.sh && \
    echo 'wait $EATER_PID' >> /app/entrypoint.sh && \
    echo 'tail -f /dev/null' >> /app/entrypoint.sh && \
    chmod +x /app/entrypoint.sh

CMD ["/bin/bash", "/app/entrypoint.sh"]
EOF

# Copy binary to temp directory for Docker build
cp "$PROJECT_ROOT/target/release/all-smi" /tmp/all-smi 2>/dev/null || \
cp "$PROJECT_ROOT/target/debug/all-smi" /tmp/all-smi || \
{ echo "Error: all-smi binary not found. Please build first."; exit 1; }

# Build Docker image
echo "Building Docker image..."
docker build -f /tmp/Dockerfile.memtest -t all-smi-memtest /tmp

# Run container with memory limit
echo ""
echo "Running container with 512MB memory limit..."
docker run -d --rm \
    --name all-smi-memory-test \
    --memory="512m" \
    -p 9999:9999 \
    all-smi-memtest

echo ""
echo "Waiting for container to start..."
sleep 8

echo ""
echo "Initial memory usage (before allocation):"
curl -s http://localhost:9999/metrics | grep -E "all_smi_memory_(total|used|available)_bytes" | head -5

echo ""
echo "Waiting for memory allocation..."
sleep 10

echo ""
echo "Memory usage after allocating 100MB:"
curl -s http://localhost:9999/metrics | grep -E "all_smi_memory_(total|used|available)_bytes" | head -5

echo ""
echo "Container runtime info:"
curl -s http://localhost:9999/metrics | grep "all_smi_container_runtime_info"

echo ""
echo "Stopping container..."
docker stop all-smi-memory-test

# Cleanup
rm -f /tmp/memory-eater /tmp/memory-eater.c /tmp/Dockerfile.memtest /tmp/all-smi

echo ""
echo "Test complete!"