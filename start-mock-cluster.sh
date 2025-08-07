#!/bin/bash
# Script to start multiple mock server processes for large port ranges

# Color support - check if colors should be disabled
if { [[ -n "${NO_COLOR}" ]] && [[ ! "${NO_COLOR,,}" =~ ^(0|false)$ ]]; } || [[ ! -t 1 ]]; then
    # Colors disabled
    RED=""
    GREEN=""
    YELLOW=""
    BLUE=""
    CYAN=""
    MAGENTA=""
    BOLD=""
    RESET=""
else
    # ANSI color codes
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    CYAN='\033[0;36m'
    MAGENTA='\033[0;35m'
    BOLD='\033[1m'
    RESET='\033[0m'
fi

# Color printing functions
error() {
    echo -e "${RED}${BOLD}Error:${RESET} ${RED}$*${RESET}" >&2
}

success() {
    echo -e "${GREEN}${BOLD}[OK]${RESET} ${GREEN}$*${RESET}"
}

warning() {
    echo -e "${YELLOW}${BOLD}Warning:${RESET} ${YELLOW}$*${RESET}"
}

info() {
    echo -e "${CYAN}$*${RESET}"
}

header() {
    echo -e "${BLUE}${BOLD}$*${RESET}"
}

# Default values (matching mock server defaults)
PORT_RANGE=""
GPU_NAME="NVIDIA H200 141GB HBM3"
PLATFORM="nvidia"
OUTPUT="hosts.csv"
FAILURE_NODES=0
PORTS_PER_PROCESS=50

# Function to show help
show_help() {
    echo -e "${BLUE}${BOLD}Start or stop multiple mock server processes for large port ranges${RESET}"
    echo ""
    echo -e "${YELLOW}${BOLD}USAGE:${RESET}"
    echo -e "    $(basename "$0") [OPTIONS]"
    echo -e "    $(basename "$0") stop"
    echo ""
    echo -e "${YELLOW}${BOLD}COMMANDS:${RESET}"
    echo -e "    ${GREEN}stop${RESET}                        Stop all running mock servers"
    echo ""
    echo -e "${YELLOW}${BOLD}OPTIONS:${RESET}"
    echo -e "    ${GREEN}--port-range${RESET} <range>        Port range, e.g., 10001-10010 or 10001"
    echo -e "    ${GREEN}--gpu-name${RESET} <name>           GPU name (default: ${CYAN}$GPU_NAME${RESET})"
    echo -e "    ${GREEN}--platform${RESET} <type>           Platform type: nvidia, apple, jetson, intel, amd (default: ${CYAN}$PLATFORM${RESET})"
    echo -e "    ${GREEN}-o, --output${RESET} <file>         Output CSV file name (default: ${CYAN}$OUTPUT${RESET})"
    echo -e "    ${GREEN}--failure-nodes${RESET} <count>     Number of nodes to simulate random failures (default: ${CYAN}$FAILURE_NODES${RESET})"
    echo -e "    ${GREEN}--ports-per-process${RESET} <num>   Maximum ports per process (default: ${CYAN}$PORTS_PER_PROCESS${RESET}, limited by system file descriptors)"
    echo -e "    ${GREEN}-h, --help${RESET}                  Show this help message"
    echo ""
    echo -e "${YELLOW}${BOLD}EXAMPLES:${RESET}"
    echo -e "    ${MAGENTA}# Start 125 mock servers on ports 10001-10125${RESET}"
    echo -e "    $(basename "$0") --port-range 10001-10125"
    echo ""
    echo -e "    ${MAGENTA}# Start with GPU name and failure simulation${RESET}"
    echo -e "    $(basename "$0") --port-range 10001-10200 --gpu-name \"NVIDIA A100\" --failure-nodes 5"
    echo ""
    echo -e "    ${MAGENTA}# Stop all mock servers${RESET}"
    echo -e "    $(basename "$0") stop"
}

# Check for stop command
if [[ "$1" == "stop" ]]; then
    info "Stopping all mock servers..."
    pkill -f all-smi-mock-server
    KILLED=$?
    if [ $KILLED -eq 0 ]; then
        success "Mock servers stopped successfully"
    else
        info "No mock servers were running"
    fi
    exit $KILLED
fi

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --port-range)
            PORT_RANGE="$2"
            shift 2
            ;;
        --gpu-name)
            GPU_NAME="$2"
            shift 2
            ;;
        --platform)
            PLATFORM="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT="$2"
            shift 2
            ;;
        --failure-nodes)
            FAILURE_NODES="$2"
            shift 2
            ;;
        --ports-per-process)
            PORTS_PER_PROCESS="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Check if port range is provided
if [ -z "$PORT_RANGE" ]; then
    error "--port-range is required"
    show_help
    exit 1
fi

# Parse port range
if [[ "$PORT_RANGE" =~ ^([0-9]+)-([0-9]+)$ ]]; then
    START_PORT="${BASH_REMATCH[1]}"
    END_PORT="${BASH_REMATCH[2]}"
    TOTAL_PORTS=$((END_PORT - START_PORT + 1))
elif [[ "$PORT_RANGE" =~ ^[0-9]+$ ]]; then
    START_PORT="$PORT_RANGE"
    END_PORT="$PORT_RANGE"
    TOTAL_PORTS=1
else
    error "Invalid port range format. Use 'start-end' or single port number"
    exit 1
fi

header "Starting mock cluster with $TOTAL_PORTS ports ($START_PORT-$END_PORT)"
header "Configuration:"
info "  GPU Name: $GPU_NAME"
info "  Platform: $PLATFORM"
info "  Failure Nodes: $FAILURE_NODES"
info "  Ports per Process: $PORTS_PER_PROCESS"

# Calculate number of processes needed
NUM_PROCESSES=$(( (TOTAL_PORTS + PORTS_PER_PROCESS - 1) / PORTS_PER_PROCESS ))
info "  Processes: $NUM_PROCESSES"

# Check file descriptor limit
SOFT_LIMIT=$(ulimit -Sn)
if [ $SOFT_LIMIT -lt 1024 ] && [ $PORTS_PER_PROCESS -gt 20 ]; then
    echo ""
    warning "Low file descriptor limit detected: $SOFT_LIMIT"
    info "  Each process can only handle ~20 ports with this limit."
    info "  To increase: ulimit -n 1024"
fi

# Clean up old temporary files
rm -f hosts_*.csv

# Start each process
PIDS=()
for i in $(seq 0 $((NUM_PROCESSES - 1))); do
    PROCESS_START=$((START_PORT + i * PORTS_PER_PROCESS))
    PROCESS_END=$((PROCESS_START + PORTS_PER_PROCESS - 1))
    
    # Don't exceed total ports
    MAX_PORT=$((START_PORT + TOTAL_PORTS - 1))
    if [ $PROCESS_END -gt $MAX_PORT ]; then
        PROCESS_END=$MAX_PORT
    fi
    
    if [ $PROCESS_START -le $MAX_PORT ]; then
        # Calculate start index for this process
        NODE_START_INDEX=$((1 + i * PORTS_PER_PROCESS))
        
        info "Starting process $((i + 1)): ports $PROCESS_START-$PROCESS_END (nodes starting from node-$(printf %04d $NODE_START_INDEX))"
        
        # Build command with all arguments
        CMD="./target/release/all-smi-mock-server"
        CMD="$CMD --port-range $PROCESS_START-$PROCESS_END"
        CMD="$CMD --gpu-name \"$GPU_NAME\""
        CMD="$CMD --platform $PLATFORM"
        CMD="$CMD -o hosts_$i.csv"
        CMD="$CMD --start-index $NODE_START_INDEX"
        
        # Only add failure-nodes to first process to avoid conflicts
        if [ $i -eq 0 ] && [ $FAILURE_NODES -gt 0 ]; then
            CMD="$CMD --failure-nodes $FAILURE_NODES"
        fi
        
        # Execute in background with explicit ulimit and save PID
        (ulimit -n 1048575 && eval "$CMD") &
        PIDS+=($!)
    fi
done

# Wait a moment for all processes to start
info "Waiting for processes to start..."
sleep 2

# Check if processes are running
RUNNING=0
for pid in "${PIDS[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then
        ((RUNNING++))
    fi
done

if [ $RUNNING -eq 0 ]; then
    error "No mock server processes started successfully"
    exit 1
fi

success "$RUNNING/$NUM_PROCESSES processes started successfully"

# Combine all CSV files
info "Combining host files..."
if ls hosts_*.csv 1> /dev/null 2>&1; then
    cat hosts_*.csv > "$OUTPUT"
    rm -f hosts_*.csv
    success "Created $OUTPUT with $TOTAL_PORTS hosts"
else
    warning "No host CSV files found"
fi

echo ""
header "Mock cluster started with PIDs: ${PIDS[*]}"
info "To stop all servers, run: $(basename "$0") stop"
info "To view logs: ps aux | grep all-smi-mock-server"