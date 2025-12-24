#!/bin/bash
set -euo pipefail

# NFS Benchmark Runner Script
# Usage: ./run-benchmark.sh [path] [output-prefix]

BENCHMARK_PATH="${1:-/mnt/nfs}"
OUTPUT_PREFIX="${2:-benchmark}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTPUT_FILE="${OUTPUT_PREFIX}_${TIMESTAMP}.json"

echo "========================================"
echo "NFS Benchmark Runner"
echo "========================================"
echo "Path: ${BENCHMARK_PATH}"
echo "Output: ${OUTPUT_FILE}"
echo "========================================"

# Check if nfsb is available
if ! command -v nfsb &> /dev/null; then
    if [ -f "./target/release/nfsb" ]; then
        NFSB="./target/release/nfsb"
    elif [ -f "./target/debug/nfsb" ]; then
        NFSB="./target/debug/nfsb"
    else
        echo "Error: nfsb binary not found. Run 'cargo build --release' first."
        exit 1
    fi
else
    NFSB="nfsb"
fi

# Show environment info
echo ""
echo "Environment Detection:"
$NFSB info --path "${BENCHMARK_PATH}"

echo ""
echo "Starting benchmarks..."
echo ""

# Run all benchmarks
$NFSB run \
    --path "${BENCHMARK_PATH}" \
    --output "${OUTPUT_FILE}" \
    --prometheus-port 9090 \
    --log-level info

echo ""
echo "========================================"
echo "Benchmark Complete"
echo "Results saved to: ${OUTPUT_FILE}"
echo "========================================"
