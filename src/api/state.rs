use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::report::BenchmarkReport;

use super::types::JobStatus;

/// represents a benchmark job
#[derive(Debug, Clone)]
pub struct Job {
    pub id: Uuid,
    pub status: JobStatus,
    pub message: String,
    pub progress: Option<u8>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub report: Option<BenchmarkReport>,
    pub error: Option<String>,
}

impl Job {
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            status: JobStatus::Pending,
            message: "Job queued".to_string(),
            progress: Some(0),
            started_at: None,
            completed_at: None,
            report: None,
            error: None,
        }
    }

    pub fn mark_running(&mut self) {
        self.status = JobStatus::Running;
        self.message = "Benchmark running".to_string();
        self.started_at = Some(chrono::Utc::now());
    }

    pub fn mark_completed(&mut self, report: BenchmarkReport) {
        self.status = JobStatus::Completed;
        self.message = "Benchmark completed successfully".to_string();
        self.progress = Some(100);
        self.completed_at = Some(chrono::Utc::now());
        self.report = Some(report);
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.message = format!("Benchmark failed: {}", error);
        self.completed_at = Some(chrono::Utc::now());
        self.error = Some(error);
    }

    pub fn update_progress(&mut self, progress: u8, message: &str) {
        self.progress = Some(progress);
        self.message = message.to_string();
    }
}

/// shared application state for tracking benchmark jobs
#[derive(Clone)]
pub struct AppState {
    jobs: Arc<RwLock<HashMap<Uuid, Job>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// create a new job and return its id
    pub async fn create_job(&self) -> Uuid {
        let id = Uuid::new_v4();
        let job = Job::new(id);
        self.jobs.write().await.insert(id, job);
        id
    }

    /// get a job by id
    pub async fn get_job(&self, id: Uuid) -> Option<Job> {
        self.jobs.read().await.get(&id).cloned()
    }

    /// update a job's state
    pub async fn update_job<F>(&self, id: Uuid, updater: F)
    where
        F: FnOnce(&mut Job),
    {
        if let Some(job) = self.jobs.write().await.get_mut(&id) {
            updater(job);
        }
    }

    /// list all jobs
    pub async fn list_jobs(&self) -> Vec<Job> {
        self.jobs.read().await.values().cloned().collect()
    }

    /// delete a job by id
    pub async fn delete_job(&self, id: Uuid) -> bool {
        self.jobs.write().await.remove(&id).is_some()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
