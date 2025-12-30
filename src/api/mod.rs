pub mod handlers;
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
        // mount operations
        .route(
            "/api/v1/mounts",
            get(handlers::list_mounts)
                .post(handlers::create_mount)
                .delete(handlers::delete_mount),
        )
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// start the REST API server
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let state = AppState::new();
    let app = create_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    info!(port = port, "Starting REST API server");
    info!("API endpoints:");
    info!("  GET    /                               - System status & API docs");
    info!("  GET    /health                         - Health check");
    info!("  POST   /api/v1/mounts                  - Mount filesystem");
    info!("  GET    /api/v1/mounts                  - List mounts");
    info!("  DELETE /api/v1/mounts?target=<path>    - Unmount filesystem");
    info!("  POST   /api/v1/benchmarks/run          - Start a benchmark");
    info!("  GET    /api/v1/benchmarks/:id/status   - Get job status");
    info!("  GET    /api/v1/benchmarks/:id/results  - Get job results");
    info!("  DELETE /api/v1/benchmarks/:id          - Delete a job");
    info!("  GET    /api/v1/jobs                    - List all jobs");
    info!("  GET    /api/v1/info?path=<path>        - Get environment info");

    axum::serve(listener, app).await?;

    Ok(())
}
