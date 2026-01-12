pub mod handlers;
pub mod simulations;
pub mod state;
pub mod types;

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use state::AppState;
use crate::metrics::SystemMetrics;
use std::sync::Arc;

/// create the API router with all endpoints
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // root status
        .route("/", get(handlers::root_status))
        // health check
        .route("/health", get(handlers::health))
        // prometheus metrics
        .route("/metrics", get(handlers::metrics))
        // benchmark operations
        .route("/api/v1/benchmarks/run", post(handlers::run_benchmark))
        .route(
            "/api/v1/benchmarks/:job_id/status",
            get(handlers::get_job_status),
        )
        .route(
            "/api/v1/benchmarks/:job_id/results",
            get(handlers::get_job_results),
        )
        .route("/api/v1/benchmarks/:job_id", delete(handlers::delete_job))
        // job listing
        .route("/api/v1/jobs", get(handlers::list_jobs))
        // environment info
        .route("/api/v1/info", get(handlers::get_info))
        // cleanup test directory
        .route("/api/v1/cleanup", delete(handlers::cleanup_directory))
        // mount operations
        .route(
            "/api/v1/mounts",
            get(handlers::list_mounts)
                .post(handlers::create_mount)
                .delete(handlers::delete_mount),
        )
        // ====================================================================
        // simulation endpoints for multi-app NFS testing
        // ====================================================================
        // log-writer: simulates nginx/app writing logs
        .route("/api/v1/log-writer/write", post(simulations::write_logs))
        // log-analyzer: reads logs and writes analysis reports
        .route("/api/v1/log-analyzer/analyze", post(simulations::analyze_logs))
        // file-uploader: simulates Laravel-style file uploads
        .route("/api/v1/file-uploader/upload", post(simulations::upload_file))
        .route("/api/v1/file-uploader/list", get(simulations::list_uploads))
        .route("/api/v1/file-uploader/delete", delete(simulations::delete_upload))
        // report-generator: generates periodic reports
        .route("/api/v1/report-generator/generate", post(simulations::generate_report))
        // report-aggregator: aggregates reports from multiple sources
        .route("/api/v1/report-aggregator/aggregate", post(simulations::aggregate_reports))
        // ownership checking utilities
        .route("/api/v1/ownership/check", get(simulations::check_ownership))
        .route("/api/v1/ownership/tree", get(simulations::ownership_tree))
        // exec: run shell commands for debugging
        .route("/api/v1/exec", get(simulations::exec_command))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// start the REST API server
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let state = AppState::new();
    let app = create_router(state);

    // start system metrics collection (refresh every 5 seconds)
    let system_metrics = Arc::new(SystemMetrics::new());
    system_metrics.clone().start_background_refresh(5);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    info!(port = port, "Starting REST API server");
    info!("API endpoints:");
    info!("  GET    /                                    - System status & API docs");
    info!("  GET    /health                              - Health check");
    info!("  GET    /metrics                             - Prometheus metrics");
    info!("  POST   /api/v1/mounts                       - Mount filesystem");
    info!("  GET    /api/v1/mounts                       - List mounts");
    info!("  DELETE /api/v1/mounts?target=<path>         - Unmount filesystem");
    info!("  POST   /api/v1/benchmarks/run               - Start a benchmark");
    info!("  GET    /api/v1/benchmarks/:id/status        - Get job status");
    info!("  GET    /api/v1/benchmarks/:id/results       - Get job results");
    info!("  DELETE /api/v1/benchmarks/:id               - Delete a job");
    info!("  GET    /api/v1/jobs                         - List all jobs");
    info!("  GET    /api/v1/info?path=<path>             - Get environment info");
    info!("Simulation endpoints (multi-app NFS testing):");
    info!("  POST   /api/v1/log-writer/write             - Write log entries");
    info!("  POST   /api/v1/log-analyzer/analyze         - Analyze logs, write reports");
    info!("  POST   /api/v1/file-uploader/upload         - Upload file");
    info!("  GET    /api/v1/file-uploader/list           - List uploaded files");
    info!("  DELETE /api/v1/file-uploader/delete         - Delete uploaded file");
    info!("  POST   /api/v1/report-generator/generate    - Generate report");
    info!("  POST   /api/v1/report-aggregator/aggregate  - Aggregate reports");
    info!("  GET    /api/v1/ownership/check              - Check file ownership");
    info!("  GET    /api/v1/ownership/tree               - List files with ownership");
    info!("  GET    /api/v1/exec?cmd=<cmd>&cwd=<path>    - Execute shell command");

    axum::serve(listener, app).await?;

    Ok(())
}
