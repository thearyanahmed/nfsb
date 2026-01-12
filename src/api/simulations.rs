use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use super::state::AppState;

// ============================================================================
// common types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SimulationResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_written: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_read: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<FileOwnership>,
}

#[derive(Debug, Serialize)]
pub struct FileOwnership {
    pub uid: u32,
    pub gid: u32,
    pub user: String,
    pub group: String,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<DateTime<Utc>>,
    pub ownership: Option<FileOwnership>,
}

#[derive(Debug, Serialize)]
pub struct SimulationError {
    pub error: String,
    pub message: String,
}

// ============================================================================
// log-writer: simulates nginx/app writing logs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct WriteLogRequest {
    pub base_path: String,
    pub app_name: String,
    #[serde(default = "default_log_entries")]
    pub entries: u32,
    #[serde(default)]
    pub log_type: Option<String>,
}

fn default_log_entries() -> u32 {
    10
}

/// POST /api/v1/log-writer/write
/// simulates an app (nginx, laravel, etc.) writing log entries
pub async fn write_logs(
    State(_state): State<AppState>,
    Json(req): Json<WriteLogRequest>,
) -> impl IntoResponse {
    let log_type = req.log_type.as_deref().unwrap_or("access");
    let log_dir = PathBuf::from(&req.base_path)
        .join("logs")
        .join(&req.app_name);
    let log_file = log_dir.join(format!("{}.log", log_type));

    info!(
        app = %req.app_name,
        log_type = %log_type,
        path = %log_file.display(),
        entries = req.entries,
        "Writing log entries"
    );

    // create directory structure
    if let Err(e) = fs::create_dir_all(&log_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "mkdir_failed".into(),
                message: format!("Failed to create log directory: {}", e),
            }),
        )
            .into_response();
    }

    // check directory ownership after creation (useful for debugging)
    let _dir_ownership = get_file_ownership(&log_dir).await;

    // write log entries
    let mut file = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SimulationError {
                    error: "open_failed".into(),
                    message: format!("Failed to open log file: {}", e),
                }),
            )
                .into_response();
        }
    };

    let mut bytes_written = 0u64;
    let now = Utc::now();

    for i in 0..req.entries {
        let log_line = match log_type {
            "access" => format!(
                "{} - {} [{}] \"GET /api/v1/resource/{} HTTP/1.1\" 200 {} \"-\" \"nfsb-simulator/1.0\"\n",
                "192.168.1.100",
                req.app_name,
                now.format("%d/%b/%Y:%H:%M:%S %z"),
                i,
                rand::random::<u16>() % 10000
            ),
            "error" => format!(
                "[{}] [error] [client 192.168.1.100] {} error #{}: simulated error message\n",
                now.format("%Y-%m-%d %H:%M:%S"),
                req.app_name,
                i
            ),
            _ => format!(
                "[{}] {} log entry #{}\n",
                now.format("%Y-%m-%d %H:%M:%S"),
                req.app_name,
                i
            ),
        };

        bytes_written += log_line.len() as u64;
        if let Err(e) = file.write_all(log_line.as_bytes()).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SimulationError {
                    error: "write_failed".into(),
                    message: format!("Failed to write log entry: {}", e),
                }),
            )
                .into_response();
        }
    }

    if let Err(e) = file.flush().await {
        warn!(error = %e, "Failed to flush log file");
    }

    let file_ownership = get_file_ownership(&log_file).await;

    (
        StatusCode::OK,
        Json(SimulationResponse {
            success: true,
            message: format!(
                "Wrote {} log entries for {} ({})",
                req.entries, req.app_name, log_type
            ),
            path: Some(log_file.display().to_string()),
            bytes_written: Some(bytes_written),
            bytes_read: None,
            file_count: Some(1),
            ownership: file_ownership,
        }),
    )
        .into_response()
}

// ============================================================================
// log-analyzer: reads logs and writes reports
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AnalyzeLogsRequest {
    pub base_path: String,
    pub source_app: String,
    #[serde(default)]
    pub log_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AnalysisResult {
    pub success: bool,
    pub source_file: String,
    pub report_file: String,
    pub lines_analyzed: u32,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub source_ownership: Option<FileOwnership>,
    pub report_ownership: Option<FileOwnership>,
}

/// POST /api/v1/log-analyzer/analyze
/// reads logs from one app and writes analysis report
pub async fn analyze_logs(
    State(_state): State<AppState>,
    Json(req): Json<AnalyzeLogsRequest>,
) -> impl IntoResponse {
    let log_type = req.log_type.as_deref().unwrap_or("access");
    let log_file = PathBuf::from(&req.base_path)
        .join("logs")
        .join(&req.source_app)
        .join(format!("{}.log", log_type));

    let report_dir = PathBuf::from(&req.base_path)
        .join("logs")
        .join(&req.source_app);
    let report_file = report_dir.join(format!("report-{}.txt", Utc::now().format("%Y%m%d-%H%M%S")));

    info!(
        source = %log_file.display(),
        report = %report_file.display(),
        "Analyzing logs"
    );

    // read source log file
    let content = match fs::read_to_string(&log_file).await {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(SimulationError {
                    error: "read_failed".into(),
                    message: format!("Failed to read log file: {}", e),
                }),
            )
                .into_response();
        }
    };

    let bytes_read = content.len() as u64;
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len() as u32;

    let source_ownership = get_file_ownership(&log_file).await;

    // generate analysis report
    let report = format!(
        "Log Analysis Report\n\
         ====================\n\
         Source: {}\n\
         Generated: {}\n\
         \n\
         Summary:\n\
         - Total lines: {}\n\
         - Total bytes: {}\n\
         - First line: {}\n\
         - Last line: {}\n\
         \n\
         Analysis complete.\n",
        log_file.display(),
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        line_count,
        bytes_read,
        lines.first().unwrap_or(&"(empty)"),
        lines.last().unwrap_or(&"(empty)"),
    );

    // write report file
    if let Err(e) = fs::write(&report_file, &report).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "write_failed".into(),
                message: format!("Failed to write report: {}", e),
            }),
        )
            .into_response();
    }

    let report_ownership = get_file_ownership(&report_file).await;

    (
        StatusCode::OK,
        Json(AnalysisResult {
            success: true,
            source_file: log_file.display().to_string(),
            report_file: report_file.display().to_string(),
            lines_analyzed: line_count,
            bytes_read,
            bytes_written: report.len() as u64,
            source_ownership,
            report_ownership,
        }),
    )
        .into_response()
}

// ============================================================================
// file-uploader: simulates Laravel-style file uploads
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct UploadFileRequest {
    pub base_path: String,
    pub app_name: String,
    pub filename: String,
    #[serde(default = "default_file_size")]
    pub size_bytes: u64,
    #[serde(default)]
    pub content: Option<String>,
}

fn default_file_size() -> u64 {
    1024
}

/// POST /api/v1/file-uploader/upload
/// simulates a file upload (like Laravel storage)
pub async fn upload_file(
    State(_state): State<AppState>,
    Json(req): Json<UploadFileRequest>,
) -> impl IntoResponse {
    let upload_dir = PathBuf::from(&req.base_path)
        .join("storage")
        .join("uploads")
        .join(&req.app_name);
    let file_path = upload_dir.join(&req.filename);

    info!(
        app = %req.app_name,
        file = %req.filename,
        size = req.size_bytes,
        path = %file_path.display(),
        "Uploading file"
    );

    // create directory structure
    if let Err(e) = fs::create_dir_all(&upload_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "mkdir_failed".into(),
                message: format!("Failed to create upload directory: {}", e),
            }),
        )
            .into_response();
    }

    // generate or use provided content
    let content = if let Some(c) = req.content {
        c.into_bytes()
    } else {
        // generate random content of specified size
        (0..req.size_bytes).map(|_| rand::random::<u8>()).collect()
    };

    // write file
    if let Err(e) = fs::write(&file_path, &content).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "write_failed".into(),
                message: format!("Failed to write file: {}", e),
            }),
        )
            .into_response();
    }

    let ownership = get_file_ownership(&file_path).await;

    (
        StatusCode::OK,
        Json(SimulationResponse {
            success: true,
            message: format!("Uploaded {} to {}", req.filename, req.app_name),
            path: Some(file_path.display().to_string()),
            bytes_written: Some(content.len() as u64),
            bytes_read: None,
            file_count: Some(1),
            ownership,
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub base_path: String,
    pub app_name: String,
}

/// GET /api/v1/file-uploader/list
/// lists uploaded files for an app
pub async fn list_uploads(Query(params): Query<ListFilesQuery>) -> impl IntoResponse {
    let upload_dir = PathBuf::from(&params.base_path)
        .join("storage")
        .join("uploads")
        .join(&params.app_name);

    info!(path = %upload_dir.display(), "Listing uploads");

    let mut files = Vec::new();

    match fs::read_dir(&upload_dir).await {
        Ok(mut entries) => {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let metadata = entry.metadata().await.ok();
                let ownership = get_file_ownership(&entry.path()).await;

                files.push(FileInfo {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: entry.path().display().to_string(),
                    size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                    is_dir: metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                    modified: metadata.and_then(|m| {
                        m.modified().ok().map(|t| DateTime::<Utc>::from(t))
                    }),
                    ownership,
                });
            }
        }
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(SimulationError {
                    error: "read_failed".into(),
                    message: format!("Failed to list directory: {}", e),
                }),
            )
                .into_response();
        }
    }

    (StatusCode::OK, Json(files)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct DeleteFileRequest {
    pub base_path: String,
    pub app_name: String,
    pub filename: String,
}

/// DELETE /api/v1/file-uploader/delete
/// deletes an uploaded file
pub async fn delete_upload(Json(req): Json<DeleteFileRequest>) -> impl IntoResponse {
    let file_path = PathBuf::from(&req.base_path)
        .join("storage")
        .join("uploads")
        .join(&req.app_name)
        .join(&req.filename);

    info!(path = %file_path.display(), "Deleting file");

    if !file_path.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(SimulationError {
                error: "not_found".into(),
                message: format!("File not found: {}", req.filename),
            }),
        )
            .into_response();
    }

    if let Err(e) = fs::remove_file(&file_path).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "delete_failed".into(),
                message: format!("Failed to delete file: {}", e),
            }),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(SimulationResponse {
            success: true,
            message: format!("Deleted {}", req.filename),
            path: Some(file_path.display().to_string()),
            bytes_written: None,
            bytes_read: None,
            file_count: None,
            ownership: None,
        }),
    )
        .into_response()
}

// ============================================================================
// report-generator: generates periodic reports
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GenerateReportRequest {
    pub base_path: String,
    pub app_name: String,
    #[serde(default)]
    pub report_type: Option<String>,
    #[serde(default = "default_data_points")]
    pub data_points: u32,
}

fn default_data_points() -> u32 {
    100
}

/// POST /api/v1/report-generator/generate
/// generates a report file (simulates periodic data generation)
pub async fn generate_report(
    State(_state): State<AppState>,
    Json(req): Json<GenerateReportRequest>,
) -> impl IntoResponse {
    let report_type = req.report_type.as_deref().unwrap_or("metrics");
    let report_dir = PathBuf::from(&req.base_path)
        .join("reports")
        .join(&req.app_name);

    let timestamp = Utc::now();
    let report_file = report_dir.join(format!(
        "{}-{}.json",
        report_type,
        timestamp.format("%Y%m%d-%H%M%S")
    ));

    info!(
        app = %req.app_name,
        report_type = %report_type,
        data_points = req.data_points,
        path = %report_file.display(),
        "Generating report"
    );

    // create directory structure
    if let Err(e) = fs::create_dir_all(&report_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "mkdir_failed".into(),
                message: format!("Failed to create report directory: {}", e),
            }),
        )
            .into_response();
    }

    // generate report data
    let data_points: Vec<serde_json::Value> = (0..req.data_points)
        .map(|i| {
            serde_json::json!({
                "index": i,
                "timestamp": timestamp.to_rfc3339(),
                "value": rand::random::<f64>() * 100.0,
                "metric": format!("{}_{}", report_type, i % 10),
            })
        })
        .collect();

    let report = serde_json::json!({
        "app": req.app_name,
        "report_type": report_type,
        "generated_at": timestamp.to_rfc3339(),
        "data_points": data_points.len(),
        "data": data_points,
    });

    let content = serde_json::to_string_pretty(&report).unwrap();

    // write report file
    if let Err(e) = fs::write(&report_file, &content).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "write_failed".into(),
                message: format!("Failed to write report: {}", e),
            }),
        )
            .into_response();
    }

    let ownership = get_file_ownership(&report_file).await;

    (
        StatusCode::OK,
        Json(SimulationResponse {
            success: true,
            message: format!(
                "Generated {} report with {} data points",
                report_type, req.data_points
            ),
            path: Some(report_file.display().to_string()),
            bytes_written: Some(content.len() as u64),
            bytes_read: None,
            file_count: Some(1),
            ownership,
        }),
    )
        .into_response()
}

// ============================================================================
// report-aggregator: aggregates reports from multiple sources
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AggregateReportsRequest {
    pub base_path: String,
    pub source_apps: Vec<String>,
    pub output_name: String,
    #[serde(default)]
    pub report_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AggregationResult {
    pub success: bool,
    pub output_file: String,
    pub sources_processed: u32,
    pub files_processed: u32,
    pub total_data_points: u32,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub source_files: Vec<SourceFileInfo>,
    pub output_ownership: Option<FileOwnership>,
}

#[derive(Debug, Serialize)]
pub struct SourceFileInfo {
    pub app: String,
    pub file: String,
    pub data_points: u32,
    pub ownership: Option<FileOwnership>,
}

/// POST /api/v1/report-aggregator/aggregate
/// reads reports from multiple apps and creates aggregated report
pub async fn aggregate_reports(
    State(_state): State<AppState>,
    Json(req): Json<AggregateReportsRequest>,
) -> impl IntoResponse {
    let report_type = req.report_type.as_deref().unwrap_or("metrics");
    let output_dir = PathBuf::from(&req.base_path).join("aggregated");
    let output_file = output_dir.join(format!(
        "{}-aggregated-{}.json",
        req.output_name,
        Utc::now().format("%Y%m%d-%H%M%S")
    ));

    info!(
        sources = ?req.source_apps,
        output = %output_file.display(),
        "Aggregating reports"
    );

    // create output directory
    if let Err(e) = fs::create_dir_all(&output_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "mkdir_failed".into(),
                message: format!("Failed to create output directory: {}", e),
            }),
        )
            .into_response();
    }

    let mut all_data_points = Vec::new();
    let mut source_files = Vec::new();
    let mut total_bytes_read = 0u64;
    let mut files_processed = 0u32;

    // process each source app
    for app in &req.source_apps {
        let report_dir = PathBuf::from(&req.base_path).join("reports").join(app);

        if !report_dir.exists() {
            warn!(app = %app, "Report directory not found, skipping");
            continue;
        }

        // find matching report files
        let mut entries = match fs::read_dir(&report_dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy();

            // match report type
            if !name.starts_with(report_type) || !name.ends_with(".json") {
                continue;
            }

            // read and parse report
            let content = match fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to read report");
                    continue;
                }
            };

            total_bytes_read += content.len() as u64;

            let report: serde_json::Value = match serde_json::from_str(&content) {
                Ok(r) => r,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to parse report");
                    continue;
                }
            };

            let data = report.get("data").and_then(|d| d.as_array());
            let data_points_count = data.map(|d| d.len() as u32).unwrap_or(0);

            let ownership = get_file_ownership(&path).await;

            source_files.push(SourceFileInfo {
                app: app.clone(),
                file: path.display().to_string(),
                data_points: data_points_count,
                ownership,
            });

            if let Some(data) = data {
                for point in data {
                    let mut point = point.clone();
                    if let Some(obj) = point.as_object_mut() {
                        obj.insert("source_app".into(), serde_json::json!(app));
                    }
                    all_data_points.push(point);
                }
            }

            files_processed += 1;
        }
    }

    // create aggregated report
    let aggregated = serde_json::json!({
        "aggregation": req.output_name,
        "report_type": report_type,
        "generated_at": Utc::now().to_rfc3339(),
        "sources": req.source_apps,
        "files_processed": files_processed,
        "total_data_points": all_data_points.len(),
        "data": all_data_points,
    });

    let output_content = serde_json::to_string_pretty(&aggregated).unwrap();

    // write aggregated report
    if let Err(e) = fs::write(&output_file, &output_content).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "write_failed".into(),
                message: format!("Failed to write aggregated report: {}", e),
            }),
        )
            .into_response();
    }

    let output_ownership = get_file_ownership(&output_file).await;

    (
        StatusCode::OK,
        Json(AggregationResult {
            success: true,
            output_file: output_file.display().to_string(),
            sources_processed: req.source_apps.len() as u32,
            files_processed,
            total_data_points: all_data_points.len() as u32,
            bytes_read: total_bytes_read,
            bytes_written: output_content.len() as u64,
            source_files,
            output_ownership,
        }),
    )
        .into_response()
}

// ============================================================================
// ownership verification: check file ownership (key for nobody:nogroup testing)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CheckOwnershipQuery {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct OwnershipCheckResult {
    pub path: String,
    pub exists: bool,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub ownership: Option<FileOwnership>,
    pub permissions: Option<String>,
    pub readable: bool,
    pub writable: bool,
}

/// GET /api/v1/ownership/check
/// checks file/directory ownership and permissions
pub async fn check_ownership(Query(params): Query<CheckOwnershipQuery>) -> impl IntoResponse {
    let path = PathBuf::from(&params.path);

    info!(path = %path.display(), "Checking ownership");

    let exists = path.exists();
    let metadata = fs::metadata(&path).await.ok();

    let ownership = get_file_ownership(&path).await;

    // check read/write access
    let readable = fs::read(&path).await.is_ok()
        || (path.is_dir() && fs::read_dir(&path).await.is_ok());
    let writable = if path.is_dir() {
        let test_file = path.join(".nfsb-write-test");
        let result = fs::write(&test_file, "test").await.is_ok();
        let _ = fs::remove_file(&test_file).await;
        result
    } else if path.exists() {
        // try appending to existing file
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .await
            .is_ok()
    } else {
        false
    };

    #[cfg(unix)]
    let permissions = metadata.as_ref().map(|m| {
        use std::os::unix::fs::PermissionsExt;
        format!("{:o}", m.permissions().mode() & 0o777)
    });

    #[cfg(not(unix))]
    let permissions = None;

    (
        StatusCode::OK,
        Json(OwnershipCheckResult {
            path: path.display().to_string(),
            exists,
            is_file: metadata.as_ref().map(|m| m.is_file()).unwrap_or(false),
            is_dir: metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
            size: metadata.as_ref().map(|m| m.len()),
            ownership,
            permissions,
            readable,
            writable,
        }),
    )
        .into_response()
}

/// GET /api/v1/ownership/tree
/// lists all files in a directory with ownership info
pub async fn ownership_tree(Query(params): Query<CheckOwnershipQuery>) -> impl IntoResponse {
    let path = PathBuf::from(&params.path);

    info!(path = %path.display(), "Getting ownership tree");

    if !path.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(SimulationError {
                error: "not_directory".into(),
                message: "Path is not a directory".into(),
            }),
        )
            .into_response();
    }

    let mut files = Vec::new();

    async fn walk_dir(dir: &std::path::Path, files: &mut Vec<FileInfo>, depth: u32) {
        if depth > 5 {
            return; // limit depth
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let metadata = fs::metadata(&path).await.ok();
            let ownership = get_file_ownership(&path).await;

            files.push(FileInfo {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.display().to_string(),
                size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                is_dir: metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                modified: metadata.and_then(|m| {
                    m.modified().ok().map(|t| DateTime::<Utc>::from(t))
                }),
                ownership,
            });

            if path.is_dir() {
                Box::pin(walk_dir(&path, files, depth + 1)).await;
            }
        }
    }

    walk_dir(&path, &mut files, 0).await;

    (StatusCode::OK, Json(files)).into_response()
}

// ============================================================================
// helper functions
// ============================================================================

#[cfg(unix)]
async fn get_file_ownership(path: &std::path::Path) -> Option<FileOwnership> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path).await.ok()?;
    let uid = metadata.uid();
    let gid = metadata.gid();

    // try to resolve username/group
    let user = resolve_username(uid);
    let group = resolve_groupname(gid);

    Some(FileOwnership {
        uid,
        gid,
        user,
        group,
    })
}

#[cfg(not(unix))]
async fn get_file_ownership(_path: &std::path::Path) -> Option<FileOwnership> {
    None
}

#[cfg(unix)]
fn resolve_username(uid: u32) -> String {
    // common mappings
    match uid {
        0 => "root".into(),
        65534 => "nobody".into(),
        _ => format!("uid:{}", uid),
    }
}

#[cfg(unix)]
fn resolve_groupname(gid: u32) -> String {
    // common mappings
    match gid {
        0 => "root".into(),
        65534 => "nogroup".into(),
        _ => format!("gid:{}", gid),
    }
}

// ============================================================================
// exec: run shell commands (for debugging/testing)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ExecQuery {
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExecResult {
    pub success: bool,
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub cwd: String,
}

/// GET /api/v1/exec?cmd=<command>&cwd=<optional-working-dir>
/// runs a shell command and returns the output
pub async fn exec_command(Query(params): Query<ExecQuery>) -> impl IntoResponse {
    use std::process::Command;

    let cwd = params.cwd.unwrap_or_else(|| "/".to_string());

    info!(cmd = %params.cmd, cwd = %cwd, "Executing command");

    let output = Command::new("sh")
        .arg("-c")
        .arg(&params.cmd)
        .current_dir(&cwd)
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            (
                StatusCode::OK,
                Json(ExecResult {
                    success: output.status.success(),
                    command: params.cmd,
                    exit_code,
                    stdout,
                    stderr,
                    cwd,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SimulationError {
                error: "exec_failed".into(),
                message: format!("Failed to execute command: {}", e),
            }),
        )
            .into_response(),
    }
}
