use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::report::BenchmarkReport;
use crate::storage::EnvironmentInfo;

/// request body for POST /api/v1/benchmarks/run
#[derive(Debug, Clone, Deserialize)]
pub struct RunBenchmarkRequest {
    /// path to the directory to benchmark (e.g., /mnt/nfs)
    pub path: String,

    /// create a test subdirectory within path (e.g., "nfsb-test" creates /mnt/nfs/nfsb-test)
    #[serde(default)]
    pub test_dir: Option<String>,

    /// clean up test directory after benchmarks complete
    #[serde(default)]
    pub cleanup: bool,

    /// specific benchmark to run (default: all)
    /// options: sequential, random, concurrent, metadata, mixed, append, all
    #[serde(default)]
    pub benchmark: Option<String>,

    /// file sizes to test
    /// options: small (4KB), medium (1MB), large (100MB)
    #[serde(default = "default_sizes")]
    pub sizes: Vec<String>,

    /// number of iterations per benchmark
    #[serde(default = "default_iterations")]
    pub iterations: u32,

    /// concurrency levels to test
    #[serde(default = "default_concurrency")]
    pub concurrency: Vec<u32>,

    /// port for prometheus metrics http server (0 to disable)
    #[serde(default = "default_prometheus_port")]
    pub prometheus_port: u16,

    /// skip warmup phase
    #[serde(default)]
    pub no_warmup: bool,

    /// runtime environment (auto-detected if not specified)
    /// options: gvisor, native, droplet
    #[serde(default)]
    pub runtime: Option<String>,

    /// storage type being benchmarked (auto-detected if not specified)
    /// options: nfs, ephemeral, block
    #[serde(default)]
    pub storage_type: Option<String>,

    /// run identifier for grouping results in metrics
    #[serde(default)]
    pub run_id: Option<String>,

    /// read-only mode: skip all write benchmarks (for gVisor/NFS testing)
    #[serde(default)]
    pub read_only: bool,
}

fn default_sizes() -> Vec<String> {
    vec!["small".into(), "medium".into(), "large".into()]
}

fn default_iterations() -> u32 {
    100
}

fn default_concurrency() -> Vec<u32> {
    vec![1, 4, 8, 16]
}

fn default_prometheus_port() -> u16 {
    9090
}

/// response for POST /api/v1/benchmarks/run
#[derive(Debug, Clone, Serialize)]
pub struct RunBenchmarkResponse {
    /// unique job id to track the benchmark run
    pub job_id: Uuid,
    /// current status of the job
    pub status: JobStatus,
    /// message describing the current state
    pub message: String,
}

/// job status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// job is queued and waiting to start
    Pending,
    /// job is currently running
    Running,
    /// job completed successfully
    Completed,
    /// job failed with an error
    Failed,
}

/// response for GET /api/v1/benchmarks/{job_id}/status
#[derive(Debug, Clone, Serialize)]
pub struct JobStatusResponse {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub message: String,
    /// progress percentage (0-100) if available
    pub progress: Option<u8>,
    /// when the job started
    pub started_at: Option<String>,
    /// when the job completed (if done)
    pub completed_at: Option<String>,
}

/// response for GET /api/v1/benchmarks/{job_id}/results
#[derive(Debug, Clone, Serialize)]
pub struct JobResultsResponse {
    pub job_id: Uuid,
    pub status: JobStatus,
    /// the benchmark report (only present when status is completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<BenchmarkReport>,
    /// error message if the job failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// request for GET /api/v1/info
#[derive(Debug, Clone, Deserialize)]
pub struct InfoRequest {
    /// path to check (defaults to ".")
    #[serde(default = "default_path")]
    pub path: String,
}

fn default_path() -> String {
    ".".to_string()
}

/// response for GET /api/v1/info
#[derive(Debug, Clone, Serialize)]
pub struct InfoResponse {
    pub environment: EnvironmentInfo,
}

/// generic error response
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// response for GET /api/v1/jobs
#[derive(Debug, Clone, Serialize)]
pub struct ListJobsResponse {
    pub jobs: Vec<JobStatusResponse>,
}

/// health check response
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// request for POST /api/v1/mounts
#[derive(Debug, Clone, Deserialize)]
pub struct MountRequest {
    /// nfs server and export path (e.g., "10.0.0.1:/export" or "nfs.example.com:/data")
    pub source: String,

    /// local mount point (e.g., "/mnt/nfs")
    pub target: String,

    /// filesystem type (default: nfs)
    #[serde(default = "default_fstype")]
    pub fstype: String,

    /// mount options (e.g., "rw,hard,intr")
    #[serde(default)]
    pub options: Option<String>,
}

fn default_fstype() -> String {
    "nfs".to_string()
}

/// response for mount operations
#[derive(Debug, Clone, Serialize)]
pub struct MountResponse {
    pub id: String,
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub status: String,
}

/// response for GET /api/v1/mounts
#[derive(Debug, Clone, Serialize)]
pub struct ListMountsResponse {
    pub mounts: Vec<MountInfo>,
}

/// information about a mount
#[derive(Debug, Clone, Serialize)]
pub struct MountInfo {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub options: String,
}

/// root status response for GET /
#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub name: String,
    pub version: String,
    pub status: String,
    pub jobs: JobsSummary,
    pub mounts: Vec<MountInfo>,
    pub endpoints: Vec<EndpointDoc>,
}

/// summary of jobs
#[derive(Debug, Clone, Serialize)]
pub struct JobsSummary {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub jobs: Vec<JobStatusResponse>,
}

/// endpoint documentation
#[derive(Debug, Clone, Serialize)]
pub struct EndpointDoc {
    pub method: String,
    pub path: String,
    pub description: String,
    pub curl_example: String,
}
