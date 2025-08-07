#!/bin/bash
# Script to check container lifecycle events

echo "=== Checking Docker container lifecycle ==="

# Function to check container state
check_container() {
    local name=$1
    echo ""
    echo "Checking container: $name"
    
    # Check if container exists
    if ! docker ps -a --format "table {{.Names}}" | grep -q "^${name}$"; then
        echo "  Status: Container does not exist"
        return
    fi
    
    # Get container info
    local state=$(docker inspect "$name" --format='{{.State.Status}}' 2>/dev/null || echo "unknown")
    local exit_code=$(docker inspect "$name" --format='{{.State.ExitCode}}' 2>/dev/null || echo "unknown")
    local started_at=$(docker inspect "$name" --format='{{.State.StartedAt}}' 2>/dev/null || echo "unknown")
    local finished_at=$(docker inspect "$name" --format='{{.State.FinishedAt}}' 2>/dev/null || echo "unknown")
    local oom_killed=$(docker inspect "$name" --format='{{.State.OOMKilled}}' 2>/dev/null || echo "unknown")
    
    echo "  Status: $state"
    echo "  Exit Code: $exit_code"
    echo "  Started At: $started_at"
    echo "  Finished At: $finished_at"
    echo "  OOM Killed: $oom_killed"
    
    # Get last 10 lines of logs
    echo "  Last logs:"
    docker logs --tail 10 "$name" 2>&1 | sed 's/^/    /'
}

# Check both test containers
check_container "all-smi-test-cpuset"
check_container "all-smi-test-quota"

# Also check Docker events for these containers
echo ""
echo "=== Recent Docker events for test containers ==="
docker events --since 5m --filter name=all-smi-test-cpuset --filter name=all-smi-test-quota --format "{{.Time}} {{.Actor.Attributes.name}} {{.Action}}" 2>/dev/null || echo "No recent events"

echo ""
echo "=== System resources ==="
echo "Docker info:"
docker system df
echo ""
echo "Available disk space:"
df -h /var/lib/docker 2>/dev/null || df -h /