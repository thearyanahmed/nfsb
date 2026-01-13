# nfsb - NFS Benchmark Tool

A Rust CLI and REST API tool for benchmarking NFS performance. 

## gVisor + NFS Status

**Status: WORKING** (with patched gVisor)

NFS read/write operations work on gVisor with patches 0.1 and 0.2 applied to the runsc binary.

### Patches Required

| Patch | Purpose | File |
|-------|---------|------|
| **0.1** | Skip `disable_file_handle_sharing` for NFS | `runsc/cmd/gofer.go` |
| **0.2** | Skip `fchown` on NFS when EPERM (root_squash) | `runsc/fsgofer/lisafs.go` |

### File Ownership Behavior

Due to NFS root_squash, all files created from gVisor containers are owned by `nobody:nogroup` (uid 65534). This is expected:

- gVisor gofer process runs as root
- NFS server squashes root to nobody
- Files are created with nobody ownership
- Applications can still read/write files (permissions are 644/755)

**Implications:**
- `chown()` calls silently succeed but files remain owned by nobody
- Files created from non-root users will have different ownership
- Write access depends on file permissions (default 644 allows owner-only writes)

## Test Environments

| Environment | Runtime | Storage | Purpose |
|-------------|---------|---------|---------|
| **Env 1** | gVisor (patched) | NFS | gVisor container with NFS mounted |
| **Env 2** | gVisor (patched) | Ephemeral | gVisor container baseline |
| **Env 3** | runc | NFS | Standard container with NFS |
| **Env 4** | runc | Ephemeral | Standard container baseline |

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
- **Random I/O**: Measures IOPS and latency for random access patterns (4KB blocks)
- **Concurrent I/O**: Tests multi-threaded file operations with configurable concurrency
- **Metadata Operations**: Benchmarks file create/delete, directory operations, and stat calls
- **Mixed Workload**: 70% read / 30% write mixed operations
- **Append Operations**: Continuous append to growing files
- **Read-Only Mode**: Skip write benchmarks for environments where writes are blocked (unpatched gVisor)
- **Prometheus Metrics**: Built-in HTTP server for metric scraping with gauge reset on benchmark completion
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

#### Core Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | System status, jobs, mounts, and API docs |
| GET | `/health` | Health check |
| GET | `/metrics` | Prometheus metrics |

#### Benchmark Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/benchmarks/run` | Start a benchmark job |
| GET | `/api/v1/benchmarks/:id/status` | Get job status |
| GET | `/api/v1/benchmarks/:id/results` | Get job results |
| DELETE | `/api/v1/benchmarks/:id` | Delete a job |
| GET | `/api/v1/jobs` | List all jobs |
| GET | `/api/v1/info?path=<path>` | Get environment info |
| DELETE | `/api/v1/cleanup?path=<path>` | Remove a test directory |

#### Mount Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/mounts` | Mount a filesystem |
| GET | `/api/v1/mounts` | List all mounts |
| DELETE | `/api/v1/mounts?target=<path>` | Unmount a filesystem |

#### Simulation Endpoints (Multi-App NFS Testing)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/v1/log-writer/write` | Write log entries (simulates nginx/app logs) |
| POST | `/api/v1/log-analyzer/analyze` | Analyze logs and write reports |
| POST | `/api/v1/file-uploader/upload` | Upload file (simulates Laravel-style uploads) |
| GET | `/api/v1/file-uploader/list` | List uploaded files |
| DELETE | `/api/v1/file-uploader/delete` | Delete uploaded file |
| POST | `/api/v1/report-generator/generate` | Generate periodic reports |
| POST | `/api/v1/report-aggregator/aggregate` | Aggregate reports from multiple sources |

#### Utility Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/ownership/check?path=<path>` | Check file ownership |
| GET | `/api/v1/ownership/tree?path=<path>` | List files with ownership info |
| GET | `/api/v1/exec?cmd=<cmd>&cwd=<path>` | Execute shell command (for debugging) |

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

### Example: Cleanup Test Directory

```bash
# remove test files after benchmarks
curl -X DELETE "http://localhost:8080/api/v1/cleanup?path=/mnt/nfs/nfsb-bench"
```

Only directories under `/tmp/`, `/mnt/`, `/data/`, or `/workspace/` can be cleaned up (safety restriction).

### Example: Execute Shell Command

```bash
# run a command in the container
curl "http://localhost:8080/api/v1/exec?cmd=ls%20-la&cwd=/mnt/nfs"

# check disk usage
curl "http://localhost:8080/api/v1/exec?cmd=df%20-h"

# view /proc/mounts
curl "http://localhost:8080/api/v1/exec?cmd=cat%20/proc/mounts"
```

### Example: Check File Ownership

```bash
# check ownership of a specific file
curl "http://localhost:8080/api/v1/ownership/check?path=/mnt/nfs/test.txt"

# list all files with ownership info
curl "http://localhost:8080/api/v1/ownership/tree?path=/mnt/nfs"
```

### Benchmark Request Options

```json
{
  "path": "/mnt/nfs",
  "test_dir": "nfsb-bench",
  "cleanup": true,
  "benchmark": "all",
  "sizes": ["small", "medium", "large"],
  "iterations": 100,
  "concurrency": [1, 4, 8, 16],
  "prometheus_port": 9090,
  "no_warmup": false,
  "read_only": false,
  "preserve_test_files": false,
  "runtime": "gvisor",
  "storage_type": "nfs",
  "run_id": "test-001"
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `path` | required | Path to benchmark directory |
| `test_dir` | `null` | Create subdirectory within path (e.g., `"nfsb-bench"` creates `/mnt/nfs/nfsb-bench`) |
| `cleanup` | `false` | Remove test directory after benchmarks complete |
| `benchmark` | `all` | `sequential`, `random`, `concurrent`, `metadata`, `mixed`, `append`, `all` |
| `sizes` | `["small","medium","large"]` | `small` (4KB), `medium` (1MB), `large` (100MB) |
| `iterations` | `100` | Number of iterations per test |
| `concurrency` | `[1,4,8,16]` | Concurrency levels for concurrent tests |
| `prometheus_port` | `9090` | Port for metrics (0 to disable) |
| `no_warmup` | `false` | Skip warmup phase |
| `read_only` | `false` | Skip all write benchmarks (only needed for unpatched gVisor + NFS) |
| `preserve_test_files` | `false` | Keep test files after benchmarks (for subsequent read-only tests) |
| `runtime` | auto-detected | Override runtime label: `gvisor`, `runc`, `native` |
| `storage_type` | auto-detected | Override storage type label: `nfs`, `ephemeral`, `block` |
| `run_id` | `null` | Identifier for grouping results in Prometheus metrics |

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
nfsb run --path /mnt/nfs --benchmark mixed
nfsb run --path /mnt/nfs --benchmark append
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

### Read-only mode (for unpatched gVisor + NFS)

When writes are blocked (unpatched gVisor with NFS due to [gVisor #11383](https://github.com/google/gvisor/issues/11383)), use read-only mode:

```bash
# Step 1: Create test files on a writable environment (e.g., runc or patched gVisor)
curl -X POST http://<writable-app>/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/mnt/nfs",
    "test_dir": "nfsb-bench",
    "preserve_test_files": true
  }'

# Step 2: Run read-only benchmarks on unpatched gVisor environment
curl -X POST http://<gvisor-app>/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/mnt/nfs/nfsb-bench",
    "read_only": true,
    "benchmark": "all"
  }'
```

In read-only mode:
- All write benchmarks are skipped
- Warmup phase is skipped (warmup writes to test filesystem)
- Test files must already exist (created by a previous writable benchmark run)

**Note:** With patched gVisor (patches 0.1 + 0.2), read-only mode is not needed.

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
# In gVisor container
nfsb run --path /mnt/nfs --output gvisor-results.json

# In runc container
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

## Source Code Structure

### Benchmark Implementations

| Benchmark | File | Entry Function |
|-----------|------|----------------|
| Sequential | [`src/benchmarks/sequential.rs`](https://github.com/thearyanahmed/nfsb/blob/master/src/benchmarks/sequential.rs) | `run_sequential()` |
| Random | [`src/benchmarks/random.rs`](https://github.com/thearyanahmed/nfsb/blob/master/src/benchmarks/random.rs) | `run_random()` |
| Concurrent | [`src/benchmarks/concurrent.rs`](https://github.com/thearyanahmed/nfsb/blob/master/src/benchmarks/concurrent.rs) | `run_concurrent()` |
| Metadata | [`src/benchmarks/metadata.rs`](https://github.com/thearyanahmed/nfsb/blob/master/src/benchmarks/metadata.rs) | `run_metadata()` |
| Mixed | [`src/benchmarks/mixed.rs`](https://github.com/thearyanahmed/nfsb/blob/master/src/benchmarks/mixed.rs) | `run_mixed()` |
| Append | [`src/benchmarks/append.rs`](https://github.com/thearyanahmed/nfsb/blob/master/src/benchmarks/append.rs) | `run_append()` |

### Key Modules

| Module | Description |
|--------|-------------|
| `src/api/` | REST API handlers and types |
| `src/benchmarks/` | Benchmark implementations |
| `src/metrics/` | Prometheus metrics collection |
| `src/report/` | JSON report generation |
| `src/storage/` | Environment and storage detection |

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

Testing NFS performance across different runtime environments to measure gVisor's overhead on NFS operations.

### Test Matrix

| Test | gVisor (patched) + NFS | gVisor + Ephemeral | runc + NFS | runc + Ephemeral |
|------|------------------------|-------------------|------------|------------------|
| Sequential Read | ✓ | ✓ | ✓ | ✓ |
| Sequential Write | ✓ | ✓ | ✓ | ✓ |
| Random Read | ✓ | ✓ | ✓ | ✓ |
| Random Write | ✓ | ✓ | ✓ | ✓ |
| Concurrent I/O | ✓ | ✓ | ✓ | ✓ |
| Metadata Ops | ✓ | ✓ | ✓ | ✓ |
| Mixed Workload | ✓ | ✓ | ✓ | ✓ |
| Append | ✓ | ✓ | ✓ | ✓ |

**Note:** With patched gVisor (patches 0.1 + 0.2), all operations work. Unpatched gVisor blocks NFS writes due to [gVisor #11383](https://github.com/google/gvisor/issues/11383).

### Step-by-Step Testing Guide

#### 1. Start the Server

```bash
# run locally
make serve

# or with docker
make docker-run
```

#### 2. Check System Status

```bash
curl http://localhost:8080/
```

This returns current jobs, mounts, and API documentation.

#### 3. Run Benchmarks

```bash
# test NFS performance (if NFS is mounted)
curl -X POST http://localhost:8080/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/mnt/nfs",
    "benchmark": "all",
    "sizes": ["small", "medium", "large"],
    "iterations": 100
  }'

# save the job_id from response
JOB_ID="<job-id-from-response>"

# test local storage for comparison
curl -X POST http://localhost:8080/api/v1/benchmarks/run \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/tmp",
    "benchmark": "all",
    "sizes": ["small", "medium", "large"],
    "iterations": 100
  }'
```

#### 4. Monitor Progress

```bash
curl http://localhost:8080/api/v1/benchmarks/$JOB_ID/status
```

#### 5. Get Results

```bash
curl http://localhost:8080/api/v1/benchmarks/$JOB_ID/results | jq .
```

#### 6. Compare Results

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
# verify NFS server connectivity
ping <nfs-ip>

# check if NFS mount point exists
ls -la /mnt/nfs

# check mount options
cat /proc/mounts | grep nfs
```

#### Benchmark hangs

```bash
# check job status
curl http://localhost:8080/api/v1/benchmarks/$JOB_ID/status

# check server logs
```

#### Permission denied on file creation

If running **unpatched gVisor**:
- NFS writes are blocked due to [gVisor #11383](https://github.com/google/gvisor/issues/11383)
- Use `read_only: true` mode or apply patches 0.1 + 0.2

If running **patched gVisor**:
- Check NFS export permissions on the server
- Verify the NFS share allows writes

#### Files owned by nobody:nogroup

This is expected behavior with patched gVisor due to NFS root_squash:
- gVisor runs as root
- NFS server squashes root to nobody (uid 65534)
- Files are created with nobody:nogroup ownership
- Applications can still read/write (permissions are 644/755)

## Docker Deployment

### Build and Run

```bash
# build docker image
make docker-build

# run container
make docker-run

# or manually
docker build -t nfsb .
docker run -p 8080:8080 nfsb
```

### With NFS Volume

```bash
docker run -p 8080:8080 \
  -v /mnt/nfs:/mnt/nfs \
  --cap-add SYS_ADMIN \
  nfsb
```

## References

- [gVisor #11383](https://github.com/google/gvisor/issues/11383) - `disable_file_handle_sharing` NFS write issue
- [gVisor #575](https://github.com/google/gvisor/issues/575) - NFS root_squash fchown EPERM issue
