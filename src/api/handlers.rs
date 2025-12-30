use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::path::PathBuf;
use serde::Deserialize;
use tracing::{error, info};
use uuid::Uuid;

use crate::config::Config;
use crate::{benchmarks, metrics, report, storage, BenchmarkType, FileSize, OutputFormat};

use super::types::{
    EndpointDoc, ErrorResponse, HealthResponse, InfoRequest, InfoResponse,
    JobResultsResponse, JobStatus, JobStatusResponse, JobsSummary, ListJobsResponse,
    ListMountsResponse, MountInfo, MountRequest, MountResponse, RunBenchmarkRequest,
    RunBenchmarkResponse, StatusResponse,
};
use super::state::AppState;

/// POST /api/v1/benchmarks/run
/// starts a new benchmark job in the background
pub async fn run_benchmark(
    State(state): State<AppState>,
    Json(req): Json<RunBenchmarkRequest>,
) -> impl IntoResponse {
    // validate path exists
    let path = PathBuf::from(&req.path);
    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_path".to_string(),
                message: format!("Path does not exist: {}", req.path),
            }),
        )
            .into_response();
    }

    // parse benchmark type
    let benchmark_type = match req.benchmark.as_deref() {
        Some("sequential") => BenchmarkType::Sequential,
        Some("random") => BenchmarkType::Random,
        Some("concurrent") => BenchmarkType::Concurrent,
        Some("metadata") => BenchmarkType::Metadata,
        Some("all") | None => BenchmarkType::All,
        Some(invalid) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "invalid_benchmark_type".to_string(),
                    message: format!(
                        "Invalid benchmark type: {}. Valid options: sequential, random, concurrent, metadata, all",
                        invalid
                    ),
                }),
            )
                .into_response();
        }
    };

    // parse file sizes
    let sizes: Result<Vec<FileSize>, String> = req
        .sizes
        .iter()
        .map(|s| match s.as_str() {
            "small" => Ok(FileSize::Small),
            "medium" => Ok(FileSize::Medium),
            "large" => Ok(FileSize::Large),
            invalid => Err(format!("Invalid file size: {}", invalid)),
        })
        .collect();

    let sizes = match sizes {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "invalid_file_size".to_string(),
                    message: e,
                }),
            )
                .into_response();
        }
    };

    // create the config
    let config = Config {
        path,
        output: None,
        benchmark: benchmark_type,
        sizes,
        iterations: req.iterations,
        concurrency: req.concurrency,
        prometheus_port: req.prometheus_port,
        warmup: !req.no_warmup,
        format: OutputFormat::Json,
    };

    // create a new job
    let job_id = state.create_job().await;

    // spawn background task to run the benchmark
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_benchmark_job(state_clone, job_id, config).await;
    });

    (
        StatusCode::ACCEPTED,
        Json(RunBenchmarkResponse {
            job_id,
            status: JobStatus::Pending,
            message: "Benchmark job queued".to_string(),
        }),
    )
        .into_response()
}

/// background task that runs the actual benchmark
async fn run_benchmark_job(state: AppState, job_id: Uuid, config: Config) {
    // mark job as running
    state.update_job(job_id, |job| job.mark_running()).await;

    info!(job_id = %job_id, path = %config.path.display(), "Starting benchmark job");

    // detect environment
    let env_info = match storage::detect_environment(&config.path).await {
        Ok(info) => info,
        Err(e) => {
            error!(job_id = %job_id, error = %e, "Failed to detect environment");
            state
                .update_job(job_id, |job| {
                    job.mark_failed(format!("Failed to detect environment: {}", e))
                })
                .await;
            return;
        }
    };

    state
        .update_job(job_id, |job| {
            job.update_progress(10, "Environment detected, starting benchmarks");
        })
        .await;

    // start prometheus server if enabled
    let _metrics_handle = if config.prometheus_port > 0 {
        match metrics::start_server(config.prometheus_port).await {
            Ok(handle) => Some(handle),
            Err(e) => {
                error!(job_id = %job_id, error = %e, "Failed to start metrics server");
                // continue without metrics
                None
            }
        }
    } else {
        None
    };

    // initialize metrics collector
    let collector = metrics::Collector::new();

    state
        .update_job(job_id, |job| {
            job.update_progress(20, "Running benchmarks");
        })
        .await;

    // run benchmarks
    let results = match benchmarks::run_all(&config, &collector, &env_info).await {
        Ok(r) => r,
        Err(e) => {
            error!(job_id = %job_id, error = %e, "Benchmark execution failed");
            state
                .update_job(job_id, |job| {
                    job.mark_failed(format!("Benchmark execution failed: {}", e))
                })
                .await;
            return;
        }
    };

    state
        .update_job(job_id, |job| {
            job.update_progress(90, "Generating report");
        })
        .await;

    // generate report
    let benchmark_report = match report::generate(&results, &env_info, &config) {
        Ok(r) => r,
        Err(e) => {
            error!(job_id = %job_id, error = %e, "Failed to generate report");
            state
                .update_job(job_id, |job| {
                    job.mark_failed(format!("Failed to generate report: {}", e))
                })
                .await;
            return;
        }
    };

    // mark job as completed
    state
        .update_job(job_id, |job| job.mark_completed(benchmark_report))
        .await;

    info!(job_id = %job_id, "Benchmark job completed successfully");
}

/// GET /api/v1/benchmarks/{job_id}/status
/// get the status of a benchmark job
pub async fn get_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.get_job(job_id).await {
        Some(job) => (
            StatusCode::OK,
            Json(JobStatusResponse {
                job_id: job.id,
                status: job.status,
                message: job.message,
                progress: job.progress,
                started_at: job.started_at.map(|t| t.to_rfc3339()),
                completed_at: job.completed_at.map(|t| t.to_rfc3339()),
            }),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: format!("Job not found: {}", job_id),
            }),
        )
            .into_response(),
    }
}

/// GET /api/v1/benchmarks/{job_id}/results
/// get the results of a completed benchmark job
pub async fn get_job_results(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.get_job(job_id).await {
        Some(job) => {
            let response = JobResultsResponse {
                job_id: job.id,
                status: job.status,
                report: job.report,
                error: job.error,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: format!("Job not found: {}", job_id),
            }),
        )
            .into_response(),
    }
}

/// GET /api/v1/jobs
/// list all benchmark jobs
pub async fn list_jobs(State(state): State<AppState>) -> impl IntoResponse {
    let jobs = state.list_jobs().await;
    let job_responses: Vec<JobStatusResponse> = jobs
        .into_iter()
        .map(|job| JobStatusResponse {
            job_id: job.id,
            status: job.status,
            message: job.message,
            progress: job.progress,
            started_at: job.started_at.map(|t| t.to_rfc3339()),
            completed_at: job.completed_at.map(|t| t.to_rfc3339()),
        })
        .collect();

    (StatusCode::OK, Json(ListJobsResponse { jobs: job_responses }))
}

/// GET /api/v1/info
/// get environment information
pub async fn get_info(Query(req): Query<InfoRequest>) -> impl IntoResponse {
    let path = PathBuf::from(&req.path);

    match storage::detect_environment(&path).await {
        Ok(env_info) => (StatusCode::OK, Json(InfoResponse { environment: env_info })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "detection_failed".to_string(),
                message: format!("Failed to detect environment: {}", e),
            }),
        )
            .into_response(),
    }
}

/// GET /health
/// health check endpoint
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// GET /metrics
/// prometheus metrics endpoint
pub async fn metrics() -> impl IntoResponse {
    use prometheus::{Encoder, TextEncoder};

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();

    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, encoder.format_type())],
            buffer,
        )
            .into_response(),
        Err(e) => {
            error!(error = %e, "Failed to encode metrics");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to encode metrics",
            )
                .into_response()
        }
    }
}

/// GET /
/// root status endpoint with overview of the system
pub async fn root_status(State(state): State<AppState>) -> impl IntoResponse {
    // get jobs summary
    let jobs = state.list_jobs().await;
    let jobs_summary = JobsSummary {
        total: jobs.len(),
        pending: jobs.iter().filter(|j| j.status == JobStatus::Pending).count(),
        running: jobs.iter().filter(|j| j.status == JobStatus::Running).count(),
        completed: jobs.iter().filter(|j| j.status == JobStatus::Completed).count(),
        failed: jobs.iter().filter(|j| j.status == JobStatus::Failed).count(),
        jobs: jobs
            .into_iter()
            .map(|job| JobStatusResponse {
                job_id: job.id,
                status: job.status,
                message: job.message,
                progress: job.progress,
                started_at: job.started_at.map(|t| t.to_rfc3339()),
                completed_at: job.completed_at.map(|t| t.to_rfc3339()),
            })
            .collect(),
    };

    // get mounts (filter to show only relevant ones)
    let mounts = get_relevant_mounts().await;

    // endpoint documentation
    let endpoints = get_endpoint_docs();

    let status = if jobs_summary.running > 0 {
        "running benchmarks"
    } else {
        "idle"
    };

    Json(StatusResponse {
        name: "nfsb".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        status: status.to_string(),
        jobs: jobs_summary,
        mounts,
        endpoints,
    })
}

/// get mounts, filtering to relevant ones (nfs, user mounts)
async fn get_relevant_mounts() -> Vec<MountInfo> {
    match tokio::fs::read_to_string("/proc/mounts").await {
        Ok(contents) => {
            contents
                .lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        let fstype = parts[2];
                        let target = parts[1];
                        // filter: nfs mounts, /mnt/*, /data/*
                        if fstype.starts_with("nfs")
                            || target.starts_with("/mnt")
                            || target.starts_with("/data")
                        {
                            return Some(MountInfo {
                                source: parts[0].to_string(),
                                target: target.to_string(),
                                fstype: fstype.to_string(),
                                options: parts[3].to_string(),
                            });
                        }
                    }
                    None
                })
                .collect()
        }
        Err(_) => vec![],
    }
}

/// generate endpoint documentation with curl examples
fn get_endpoint_docs() -> Vec<EndpointDoc> {
    vec![
        EndpointDoc {
            method: "GET".to_string(),
            path: "/".to_string(),
            description: "Get system status, jobs, mounts, and API docs".to_string(),
            curl_example: "curl http://localhost:8080/".to_string(),
        },
        EndpointDoc {
            method: "GET".to_string(),
            path: "/health".to_string(),
            description: "Health check".to_string(),
            curl_example: "curl http://localhost:8080/health".to_string(),
        },
        EndpointDoc {
            method: "GET".to_string(),
            path: "/metrics".to_string(),
            description: "Prometheus metrics".to_string(),
            curl_example: "curl http://localhost:8080/metrics".to_string(),
        },
        EndpointDoc {
            method: "POST".to_string(),
            path: "/api/v1/mounts".to_string(),
            description: "Mount a filesystem (NFS, etc)".to_string(),
            curl_example: r#"curl -X POST http://localhost:8080/api/v1/mounts -H "Content-Type: application/json" -d '{"source": "10.0.0.1:/export", "target": "/mnt/nfs"}'"#.to_string(),
        },
        EndpointDoc {
            method: "GET".to_string(),
            path: "/api/v1/mounts".to_string(),
            description: "List all mounts".to_string(),
            curl_example: "curl http://localhost:8080/api/v1/mounts".to_string(),
        },
        EndpointDoc {
            method: "DELETE".to_string(),
            path: "/api/v1/mounts?target=<path>".to_string(),
            description: "Unmount a filesystem".to_string(),
            curl_example: r#"curl -X DELETE "http://localhost:8080/api/v1/mounts?target=/mnt/nfs""#.to_string(),
        },
        EndpointDoc {
            method: "POST".to_string(),
            path: "/api/v1/benchmarks/run".to_string(),
            description: "Start a benchmark job".to_string(),
            curl_example: r#"curl -X POST http://localhost:8080/api/v1/benchmarks/run -H "Content-Type: application/json" -d '{"path": "/mnt/nfs", "benchmark": "sequential", "sizes": ["small", "medium"], "iterations": 50}'"#.to_string(),
        },
        EndpointDoc {
            method: "GET".to_string(),
            path: "/api/v1/benchmarks/:id/status".to_string(),
            description: "Get job status".to_string(),
            curl_example: "curl http://localhost:8080/api/v1/benchmarks/<job_id>/status".to_string(),
        },
        EndpointDoc {
            method: "GET".to_string(),
            path: "/api/v1/benchmarks/:id/results".to_string(),
            description: "Get job results".to_string(),
            curl_example: "curl http://localhost:8080/api/v1/benchmarks/<job_id>/results".to_string(),
        },
        EndpointDoc {
            method: "DELETE".to_string(),
            path: "/api/v1/benchmarks/:id".to_string(),
            description: "Delete a job".to_string(),
            curl_example: "curl -X DELETE http://localhost:8080/api/v1/benchmarks/<job_id>".to_string(),
        },
        EndpointDoc {
            method: "GET".to_string(),
            path: "/api/v1/jobs".to_string(),
            description: "List all jobs".to_string(),
            curl_example: "curl http://localhost:8080/api/v1/jobs".to_string(),
        },
        EndpointDoc {
            method: "GET".to_string(),
            path: "/api/v1/info?path=<path>".to_string(),
            description: "Get environment info for a path".to_string(),
            curl_example: r#"curl "http://localhost:8080/api/v1/info?path=/mnt/nfs""#.to_string(),
        },
    ]
}

/// DELETE /api/v1/benchmarks/{job_id}
/// cancel a running job or delete a completed job
pub async fn delete_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.get_job(job_id).await {
        Some(job) => {
            if matches!(job.status, JobStatus::Running) {
                // TODO: implement job cancellation with tokio cancellation tokens
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "cannot_cancel".to_string(),
                        message: "Cannot cancel a running job (not yet implemented)".to_string(),
                    }),
                )
                    .into_response();
            }

            state.delete_job(job_id).await;
            (StatusCode::NO_CONTENT, ()).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: format!("Job not found: {}", job_id),
            }),
        )
            .into_response(),
    }
}

/// POST /api/v1/mounts
/// mount a filesystem (e.g., NFS share)
pub async fn create_mount(Json(req): Json<MountRequest>) -> impl IntoResponse {
    // create target directory if it doesn't exist
    if let Err(e) = tokio::fs::create_dir_all(&req.target).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "mkdir_failed".to_string(),
                message: format!("Failed to create mount point: {}", e),
            }),
        )
            .into_response();
    }

    // build mount command
    let mut cmd = tokio::process::Command::new("mount");
    cmd.arg("-t").arg(&req.fstype);

    if let Some(opts) = &req.options {
        cmd.arg("-o").arg(opts);
    }

    cmd.arg(&req.source).arg(&req.target);

    info!(
        source = %req.source,
        target = %req.target,
        fstype = %req.fstype,
        "Mounting filesystem"
    );

    match cmd.output().await {
        Ok(output) => {
            if output.status.success() {
                let id = format!(
                    "{:x}",
                    md5_hash(&format!("{}:{}", req.source, req.target))
                );
                (
                    StatusCode::CREATED,
                    Json(MountResponse {
                        id,
                        source: req.source,
                        target: req.target,
                        fstype: req.fstype,
                        status: "mounted".to_string(),
                    }),
                )
                    .into_response()
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(error = %stderr, "Mount failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "mount_failed".to_string(),
                        message: format!("Mount failed: {}", stderr.trim()),
                    }),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "mount_error".to_string(),
                message: format!("Failed to execute mount: {}", e),
            }),
        )
            .into_response(),
    }
}

/// DELETE /api/v1/mounts
/// unmount a filesystem by target path
pub async fn delete_mount(Query(params): Query<UnmountParams>) -> impl IntoResponse {
    let target = &params.target;

    info!(target = %target, "Unmounting filesystem");

    let output = tokio::process::Command::new("umount")
        .arg(target)
        .output()
        .await;

    match output {
        Ok(output) => {
            if output.status.success() {
                (StatusCode::NO_CONTENT, ()).into_response()
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(error = %stderr, "Unmount failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "unmount_failed".to_string(),
                        message: format!("Unmount failed: {}", stderr.trim()),
                    }),
                )
                    .into_response()
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "unmount_error".to_string(),
                message: format!("Failed to execute umount: {}", e),
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UnmountParams {
    pub target: String,
}

/// GET /api/v1/mounts
/// list all current mounts
pub async fn list_mounts() -> impl IntoResponse {
    match tokio::fs::read_to_string("/proc/mounts").await {
        Ok(contents) => {
            let mounts: Vec<MountInfo> = contents
                .lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        Some(MountInfo {
                            source: parts[0].to_string(),
                            target: parts[1].to_string(),
                            fstype: parts[2].to_string(),
                            options: parts[3].to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();

            (StatusCode::OK, Json(ListMountsResponse { mounts })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "read_mounts_failed".to_string(),
                message: format!("Failed to read mounts: {}", e),
            }),
        )
            .into_response(),
    }
}

/// simple hash for generating mount ids
fn md5_hash(input: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}
