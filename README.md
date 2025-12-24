# nfsb - NFS Benchmark Tool

A Rust CLI tool for benchmarking NFS performance on DigitalOcean App Platform, specifically measuring the impact of gVisor on file I/O operations.

## Features

- **Sequential I/O**: Measures throughput for sequential read/write operations
- **Random I/O**: Measures IOPS and latency for random access patterns
- **Concurrent I/O**: Tests multi-threaded file operations
- **Metadata Operations**: Benchmarks file create/delete, directory operations, and stat calls
- **Prometheus Metrics**: Built-in HTTP server for metric scraping
- **JSON Reports**: Structured output for analysis
- **Environment Detection**: Auto-detects gVisor vs runc runtime

## Installation

### Build from source

```bash
cargo build --release
```

### Docker

```bash
docker build -t nfsb .
```

## Usage

### Run all benchmarks

```bash
nfsb run --path /mnt/nfs --output results.json
```

### Run specific benchmark

```bash
nfsb run --path /mnt/nfs --benchmark sequential
nfsb run --path /mnt/nfs --benchmark random
nfsb run --path /mnt/nfs --benchmark concurrent
nfsb run --path /mnt/nfs --benchmark metadata
```

### Configure file sizes

```bash
# Test only small and medium files
nfsb run --path /mnt/nfs --sizes small,medium

# File sizes:
#   small:  4KB
#   medium: 1MB
#   large:  100MB
```

### Configure iterations

```bash
nfsb run --path /mnt/nfs --iterations 200
```

### Configure concurrency levels

```bash
nfsb run --path /mnt/nfs --concurrency 1,2,4,8,16,32
```

### JSON output

```bash
nfsb run --path /mnt/nfs --format json
```

### Show environment info

```bash
nfsb info --path /mnt/nfs
nfsb info --format json
```

## Prometheus Metrics

The tool exposes metrics on port 9090 by default:

```bash
# Start with Prometheus metrics
nfsb run --path /mnt/nfs --prometheus-port 9090

# Disable Prometheus
nfsb run --path /mnt/nfs --prometheus-port 0
```

Available metrics:

- `nfsb_bytes_written_total` - Total bytes written
- `nfsb_bytes_read_total` - Total bytes read
- `nfsb_operations_total` - Total operations performed
- `nfsb_throughput_mbps` - Current throughput in MB/s
- `nfsb_iops` - Current IOPS
- `nfsb_operation_duration_seconds` - Benchmark duration histogram
- `nfsb_latency_seconds` - I/O latency histogram

## Grafana Dashboard

Start the monitoring stack:

```bash
docker-compose up -d
```

Access:
- Grafana: http://localhost:3000 (admin/admin)
- Prometheus: http://localhost:9091

## Output Format

### JSON Report

```json
{
  "version": "0.1.0",
  "timestamp": "2025-01-01T00:00:00Z",
  "environment": {
    "runtime": "gvisor",
    "storage_type": "nfs",
    "mount_point": "/mnt/nfs",
    "filesystem": "nfs4"
  },
  "results": {
    "sequential": [
      {
        "name": "sequential_write",
        "size": "medium",
        "iterations": 100,
        "throughput_mbps": 150.5,
        "latency_stats": {
          "p50": 0.0012,
          "p95": 0.0034,
          "p99": 0.0081
        }
      }
    ]
  }
}
```

## Benchmark Scenarios

### 1. gVisor vs runc Comparison

Run benchmarks in both environments and compare:

```bash
# In gVisor container (default App Platform)
nfsb run --path /mnt/nfs --output gvisor-results.json

# In runc container (modified pod spec)
nfsb run --path /mnt/nfs --output runc-results.json
```

### 2. NFS vs Ephemeral Storage

```bash
# NFS mount
nfsb run --path /mnt/nfs --output nfs-results.json

# Ephemeral storage
nfsb run --path /tmp --output ephemeral-results.json
```

### 3. Concurrency Impact

```bash
nfsb run --path /mnt/nfs \
  --benchmark concurrent \
  --concurrency 1,2,4,8,16,32,64 \
  --output concurrency-results.json
```

## Environment Detection

The tool automatically detects:

- **Runtime**: gVisor, runc, or native (bare metal)
- **Storage Type**: NFS or ephemeral
- **System Info**: CPU cores, memory

Detection methods:
- `/proc/version` for gVisor signature
- `/proc/1/cgroup` for container detection
- `/proc/mounts` for filesystem type

## Development

### Run tests

```bash
cargo test
```

### Check code

```bash
cargo check
cargo clippy
```

### Format code

```bash
cargo fmt
```
