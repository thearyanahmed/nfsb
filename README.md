# nfsb - NFS Benchmark Tool

A Rust CLI and REST API tool for benchmarking NFS performance on DigitalOcean App Platform, specifically measuring the impact of gVisor on file I/O operations.

## Purpose

Before building NFS support for App Platform, we need to benchmark how gVisor impacts NFS performance. This tool helps determine if NFS + gVisor is viable or if performance bottlenecks make it unusable.

## Test Environments

| Environment | Runtime | Storage | Purpose |
|-------------|---------|---------|---------|
| **Env 1** | gVisor | NFS | Default App Platform with NFS mounted |
| **Env 2** | gVisor | Ephemeral | Default App Platform baseline |
| **Env 3** | runc | NFS | Container without gVisor sandbox |
| **Env 4** | runc | Ephemeral | Container baseline without gVisor |

## Quick Start

```bash
# build
make build-release

# run REST API server
make serve

# or with docker
make docker-build
make docker-run
```

Then open http://localhost:8080/ to see system status and API documentation.

## Features

- **Sequential I/O**: Measures throughput for sequential read/write operations
- **Random I/O**: Measures IOPS and latency for random access patterns
- **Concurrent I/O**: Tests multi-threaded file operations
- **Metadata Operations**: Benchmarks file create/delete, directory operations, and stat calls
- **Prometheus Metrics**: Built-in HTTP server for metric scraping
- **JSON Reports**: Structured output for analysis
- **Environment Detection**: Auto-detects gVisor vs runc runtime

## Installation

```bash
# build from source
make build-release

# or with docker
make docker-build
```

## REST API

Start the server:

```bash
nfsb serve --port 8080
```

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | System status, jobs, mounts, and API docs |
| GET | `/health` | Health check |
| POST | `/api/v1/mounts` | Mount a filesystem |
| GET | `/api/v1/mounts` | List all mounts |
| DELETE | `/api/v1/mounts?target=<path>` | Unmount a filesystem |
| POST | `/api/v1/benchmarks/run` | Start a benchmark job |
| GET | `/api/v1/benchmarks/:id/status` | Get job status |
| GET | `/api/v1/benchmarks/:id/results` | Get job results |
| DELETE | `/api/v1/benchmarks/:id` | Delete a job |
| GET | `/api/v1/jobs` | List all jobs |
| GET | `/api/v1/info?path=<path>` | Get environment info |

### Example: Mount NFS and Run Benchmark

```bash
# 1. mount nfs share
curl -X POST http://localhost:8080/api/v1/mounts \
  -H "Content-Type: application/json" \
  -d '{"source": "10.0.0.1:/export", "target": "/mnt/nfs"}'

# 2. start benchmark on nfs
curl -X POST http://localhost:8080/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{"path": "/mnt/nfs", "benchmark": "all", "iterations": 50}'

# response: {"job_id": "uuid-here", "status": "pending", ...}

# 3. check status
curl http://localhost:8080/api/v1/benchmarks/<job_id>/status

# 4. get results when completed
curl http://localhost:8080/api/v1/benchmarks/<job_id>/results
```

### Example: Compare NFS vs Ephemeral Storage

```bash
# run on nfs
curl -X POST http://localhost:8080/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{"path": "/mnt/nfs", "benchmark": "sequential"}'

# run on ephemeral (local) storage
curl -X POST http://localhost:8080/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{"path": "/data", "benchmark": "sequential"}'

# compare results from both job IDs
```

### Benchmark Request Options

```json
{
  "path": "/mnt/nfs",
  "benchmark": "all",
  "sizes": ["small", "medium", "large"],
  "iterations": 100,
  "concurrency": [1, 4, 8, 16],
  "prometheus_port": 9090,
  "no_warmup": false
}
```

| Field | Default | Options |
|-------|---------|---------|
| `path` | required | Path to benchmark directory |
| `benchmark` | `all` | `sequential`, `random`, `concurrent`, `metadata`, `all` |
| `sizes` | `["small","medium","large"]` | `small` (4KB), `medium` (1MB), `large` (100MB) |
| `iterations` | `100` | Number of iterations per test |
| `concurrency` | `[1,4,8,16]` | Concurrency levels for concurrent tests |
| `prometheus_port` | `9090` | Port for metrics (0 to disable) |
| `no_warmup` | `false` | Skip warmup phase |

## CLI Usage

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

#### How test files work

Test files are **generated dynamically at runtime** and **cleaned up after each benchmark**. No test files are stored in the repository.

| Size | Bytes | Example Filename |
|------|-------|------------------|
| small | 4 KB | `nfsb_seq_write_small.dat` |
| medium | 1 MB | `nfsb_seq_write_medium.dat` |
| large | 100 MB | `nfsb_seq_write_large.dat` |

The benchmark flow:
1. Random data is generated in memory
2. Data is written to the target path (e.g., `/mnt/nfs/nfsb_seq_write_small.dat`)
3. Benchmark operations run (read/write iterations)
4. Test file is deleted

This ensures:
- Fresh random data for each test (prevents filesystem caching tricks)
- Tests run on the actual target filesystem (NFS or ephemeral)
- No leftover files after benchmarks complete

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

## Testing Strategy

### Overview

We're testing NFS performance across different runtime environments to determine if gVisor's overhead makes NFS unusable on App Platform.

### Test Matrix

| Test | gVisor + NFS | gVisor + Ephemeral | runc + NFS | runc + Ephemeral |
|------|--------------|-------------------|------------|------------------|
| Sequential Read | ✓ | ✓ | ✓ | ✓ |
| Sequential Write | ✓ | ✓ | ✓ | ✓ |
| Random Read | ✓ | ✓ | ✓ | ✓ |
| Random Write | ✓ | ✓ | ✓ | ✓ |
| Concurrent I/O | ✓ | ✓ | ✓ | ✓ |
| Metadata Ops | ✓ | ✓ | ✓ | ✓ |

### Step-by-Step Testing Guide

#### 1. Deploy to App Platform

```bash
# deploy using the app spec
doctl apps create --spec app.yaml
```

#### 2. Get the App URL

```bash
doctl apps list
# note the live URL
```

#### 3. Check System Status

```bash
curl https://<app-url>/
```

This returns current jobs, mounts, and API documentation.

#### 4. Mount NFS Share

```bash
curl -X POST https://<app-url>/api/v1/mounts \
  -H "Content-Type: application/json" \
  -d '{
    "source": "<nfs-server>:/export",
    "target": "/mnt/nfs",
    "fstype": "nfs",
    "options": "rw,hard,intr"
  }'
```

#### 5. Run Benchmarks

```bash
# test NFS performance
curl -X POST https://<app-url>/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/mnt/nfs",
    "benchmark": "all",
    "sizes": ["small", "medium", "large"],
    "iterations": 100
  }'

# save the job_id from response
JOB_ID="<job-id-from-response>"

# test ephemeral storage for comparison
curl -X POST https://<app-url>/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/data",
    "benchmark": "all",
    "sizes": ["small", "medium", "large"],
    "iterations": 100
  }'
```

#### 6. Monitor Progress

```bash
curl https://<app-url>/api/v1/benchmarks/$JOB_ID/status
```

#### 7. Get Results

```bash
curl https://<app-url>/api/v1/benchmarks/$JOB_ID/results | jq .
```

#### 8. Compare Results

Key metrics to compare:
- **Throughput (MB/s)**: Higher is better
- **IOPS**: Higher is better for random I/O
- **Latency p50/p95/p99**: Lower is better

### Expected Outcomes

| Scenario | Expected Impact |
|----------|----------------|
| gVisor overhead | 10-50% slower than runc for file I/O |
| NFS vs Ephemeral | NFS typically 2-10x slower due to network |
| gVisor + NFS | Combined overhead - key metric for go/no-go |

### Success Criteria

- NFS read/write throughput > 50 MB/s
- Random IOPS > 1000
- p99 latency < 100ms
- No significant degradation under concurrent load

### Troubleshooting

#### Mount fails

```bash
# check if nfs-common is installed (should be in Docker image)
# check NFS server connectivity
# verify NFS export permissions
```

#### Benchmark hangs

```bash
# check job status
curl https://<app-url>/api/v1/benchmarks/$JOB_ID/status

# check app logs
doctl apps logs <app-id>
```

#### Permission denied

```bash
# NFS export may need to allow the container's UID
# Check NFS server exports: /etc/exports
```

## App Platform Deployment

### Using app.yaml

```yaml
name: nfsb
services:
- name: nfsb
  github:
    repo: thearyanahmed/nfsb
    branch: master
  dockerfile_path: Dockerfile
  http_port: 8080
  instance_size_slug: apps-s-1vcpu-2gb
  instance_count: 1
  health_check:
    http_path: /health
```

### Deploy

```bash
doctl apps create --spec app.yaml
```

### Update

```bash
doctl apps update <app-id> --spec app.yaml
```
