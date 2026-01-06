use std::path::PathBuf;

use crate::{BenchmarkType, FileSize, OutputFormat};

/// Configuration for benchmark execution
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to benchmark directory
    pub path: PathBuf,
    /// Output file path for JSON results
    pub output: Option<PathBuf>,
    /// Type of benchmark to run
    pub benchmark: BenchmarkType,
    /// File sizes to test
    pub sizes: Vec<FileSize>,
    /// Number of iterations per benchmark
    pub iterations: u32,
    /// Concurrency levels to test
    pub concurrency: Vec<u32>,
    /// Port for Prometheus HTTP server
    pub prometheus_port: u16,
    /// Whether to run warmup phase
    pub warmup: bool,
    /// Output format
    pub format: OutputFormat,
    /// Read-only mode: skip write benchmarks (for gVisor/NFS)
    pub read_only: bool,
    /// Preserve test files after benchmarks (for subsequent read-only tests)
    pub preserve_test_files: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: PathBuf::from("."),
            output: None,
            benchmark: BenchmarkType::All,
            sizes: vec![FileSize::Small, FileSize::Medium, FileSize::Large],
            iterations: 100,
            concurrency: vec![1, 4, 8, 16],
            prometheus_port: 9090,
            warmup: true,
            format: OutputFormat::Text,
            read_only: false,
            preserve_test_files: false,
        }
    }
}
