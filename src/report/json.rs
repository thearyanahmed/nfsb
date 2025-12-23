use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::config::Config;
use crate::storage::EnvironmentInfo;

/// Statistics for a series of measurements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub std_dev: f64,
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
    pub count: usize,
}

impl Statistics {
    /// Calculate statistics from a slice of values
    pub fn from_values(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                std_dev: 0.0,
                p50: 0.0,
                p90: 0.0,
                p95: 0.0,
                p99: 0.0,
                count: 0,
            };
        }

        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let count = sorted.len();
        let min = sorted[0];
        let max = sorted[count - 1];
        let sum: f64 = sorted.iter().sum();
        let mean = sum / count as f64;

        let variance: f64 = sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();

        let percentile = |p: f64| -> f64 {
            let idx = ((p / 100.0) * (count - 1) as f64).round() as usize;
            sorted[idx.min(count - 1)]
        };

        Self {
            min,
            max,
            mean,
            std_dev,
            p50: percentile(50.0),
            p90: percentile(90.0),
            p95: percentile(95.0),
            p99: percentile(99.0),
            count,
        }
    }
}

/// Result of a single benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub name: String,
    pub size: String,
    pub iterations: u32,
    pub total_bytes: u64,
    pub duration_secs: f64,
    pub throughput_mbps: f64,
    pub iops: Option<f64>,
    pub latency_stats: Statistics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<u32>,
}

/// System resource usage during benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_mb: u64,
}

/// Complete benchmark report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub version: String,
    pub timestamp: DateTime<Utc>,
    pub environment: EnvironmentInfo,
    pub config: ReportConfig,
    pub results: HashMap<String, Vec<BenchmarkResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_usage: Option<ResourceUsage>,
}

/// Subset of config for the report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportConfig {
    pub path: String,
    pub iterations: u32,
    pub sizes: Vec<String>,
    pub concurrency: Vec<u32>,
}

/// Generate a benchmark report from results
pub fn generate(
    results: &HashMap<String, Vec<BenchmarkResult>>,
    env_info: &EnvironmentInfo,
    config: &Config,
) -> Result<BenchmarkReport> {
    Ok(BenchmarkReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: Utc::now(),
        environment: env_info.clone(),
        config: ReportConfig {
            path: config.path.display().to_string(),
            iterations: config.iterations,
            sizes: config.sizes.iter().map(|s| s.name().to_string()).collect(),
            concurrency: config.concurrency.clone(),
        },
        results: results.clone(),
        resource_usage: None,
    })
}

/// Save report as JSON to a file
pub fn save_json(report: &BenchmarkReport, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

/// Format a human-readable summary of the report
pub fn format_summary(report: &BenchmarkReport) -> String {
    let mut output = String::new();

    output.push_str("═══════════════════════════════════════════════════════════════\n");
    output.push_str("                    NFS Benchmark Results\n");
    output.push_str("═══════════════════════════════════════════════════════════════\n\n");

    output.push_str(&format!("Timestamp: {}\n", report.timestamp));
    output.push_str(&format!("Runtime: {}\n", report.environment.runtime));
    output.push_str(&format!("Storage: {}\n", report.environment.storage_type));
    output.push_str(&format!("Path: {}\n\n", report.config.path));

    for (benchmark_name, results) in &report.results {
        output.push_str(&format!("── {} ──\n", benchmark_name.to_uppercase()));

        for result in results {
            output.push_str(&format!(
                "  {} ({}):\n",
                result.name, result.size
            ));
            output.push_str(&format!(
                "    Throughput: {:.2} MB/s\n",
                result.throughput_mbps
            ));
            if let Some(iops) = result.iops {
                output.push_str(&format!("    IOPS: {:.0}\n", iops));
            }
            output.push_str(&format!(
                "    Latency: p50={:.3}ms p95={:.3}ms p99={:.3}ms\n",
                result.latency_stats.p50 * 1000.0,
                result.latency_stats.p95 * 1000.0,
                result.latency_stats.p99 * 1000.0,
            ));
            if let Some(concurrency) = result.concurrency {
                output.push_str(&format!("    Concurrency: {}\n", concurrency));
            }
            output.push('\n');
        }
    }

    output.push_str("═══════════════════════════════════════════════════════════════\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statistics() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let stats = Statistics::from_values(&values);

        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 10.0);
        assert_eq!(stats.mean, 5.5);
        assert_eq!(stats.count, 10);
    }

    #[test]
    fn test_empty_statistics() {
        let values: Vec<f64> = vec![];
        let stats = Statistics::from_values(&values);

        assert_eq!(stats.count, 0);
        assert_eq!(stats.mean, 0.0);
    }
}
