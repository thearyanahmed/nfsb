use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use tokio::fs;
use tracing::{debug, info};

use crate::config::Config;
use crate::metrics::Collector;
use crate::report::{BenchmarkResult, Statistics};

const METADATA_ITERATIONS: u32 = 1000;

/// Run metadata operation benchmarks
pub async fn run_metadata(config: &Config, collector: &Collector) -> Result<Vec<BenchmarkResult>> {
    let mut results = Vec::new();

    // File create/delete benchmark
    let create_delete_result = run_create_delete(config, collector).await?;
    results.push(create_delete_result);

    // Directory operations benchmark
    let dir_result = run_directory_ops(config, collector).await?;
    results.push(dir_result);

    // Stat operations benchmark
    let stat_result = run_stat_ops(config, collector).await?;
    results.push(stat_result);

    Ok(results)
}

async fn run_create_delete(config: &Config, collector: &Collector) -> Result<BenchmarkResult> {
    let iterations = METADATA_ITERATIONS;
    let base_path = config.path.join("nfsb_metadata_test");
    let mut latencies = Vec::with_capacity(iterations as usize * 2);

    // Create test directory
    fs::create_dir_all(&base_path).await?;

    info!(
        iterations = iterations,
        "Starting file create/delete benchmark"
    );

    let pb = ProgressBar::new(iterations as u64 * 2);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message("create_delete");

    let total_start = Instant::now();

    // Create files
    for i in 0..iterations {
        let file_path = base_path.join(format!("file_{}.txt", i));

        let iter_start = Instant::now();
        let file = fs::File::create(&file_path).await?;
        drop(file);
        let latency = iter_start.elapsed().as_secs_f64();

        latencies.push(latency);
        collector.record_operation("create", "metadata", "metadata");
        collector.record_latency("create", "metadata", latency);

        pb.inc(1);
        debug!(iteration = i, latency_ms = latency * 1000.0, "File create complete");
    }

    // Delete files
    for i in 0..iterations {
        let file_path = base_path.join(format!("file_{}.txt", i));

        let iter_start = Instant::now();
        fs::remove_file(&file_path).await?;
        let latency = iter_start.elapsed().as_secs_f64();

        latencies.push(latency);
        collector.record_operation("delete", "metadata", "metadata");
        collector.record_latency("delete", "metadata", latency);

        pb.inc(1);
        debug!(iteration = i, latency_ms = latency * 1000.0, "File delete complete");
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // Clean up
    fs::remove_dir(&base_path).await?;

    let total_ops = iterations * 2;
    let ops_per_sec = total_ops as f64 / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_iops("create_delete", "metadata", "metadata", ops_per_sec);
    collector.record_duration("create_delete", total_duration);

    info!(
        ops_per_sec = ops_per_sec,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Create/delete benchmark complete"
    );

    Ok(BenchmarkResult {
        name: "create_delete".to_string(),
        size: "metadata".to_string(),
        iterations: total_ops,
        total_bytes: 0,
        duration_secs: total_duration,
        throughput_mbps: 0.0,
        iops: Some(ops_per_sec),
        latency_stats,
        concurrency: None,
    })
}

async fn run_directory_ops(config: &Config, collector: &Collector) -> Result<BenchmarkResult> {
    let iterations = METADATA_ITERATIONS / 10; // Fewer iterations for dir ops
    let base_path = config.path.join("nfsb_dir_test");
    let mut latencies = Vec::with_capacity(iterations as usize * 2);

    info!(iterations = iterations, "Starting directory operations benchmark");

    let pb = ProgressBar::new(iterations as u64 * 2);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message("dir_ops");

    let total_start = Instant::now();

    // Create directories
    for i in 0..iterations {
        let dir_path = base_path.join(format!("dir_{}", i));

        let iter_start = Instant::now();
        fs::create_dir_all(&dir_path).await?;
        let latency = iter_start.elapsed().as_secs_f64();

        latencies.push(latency);
        collector.record_operation("mkdir", "metadata", "metadata");
        collector.record_latency("mkdir", "metadata", latency);

        pb.inc(1);
        debug!(iteration = i, latency_ms = latency * 1000.0, "Mkdir complete");
    }

    // Remove directories
    for i in 0..iterations {
        let dir_path = base_path.join(format!("dir_{}", i));

        let iter_start = Instant::now();
        fs::remove_dir(&dir_path).await?;
        let latency = iter_start.elapsed().as_secs_f64();

        latencies.push(latency);
        collector.record_operation("rmdir", "metadata", "metadata");
        collector.record_latency("rmdir", "metadata", latency);

        pb.inc(1);
        debug!(iteration = i, latency_ms = latency * 1000.0, "Rmdir complete");
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // Clean up base directory
    let _ = fs::remove_dir(&base_path).await;

    let total_ops = iterations * 2;
    let ops_per_sec = total_ops as f64 / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_iops("dir_ops", "metadata", "metadata", ops_per_sec);
    collector.record_duration("dir_ops", total_duration);

    info!(
        ops_per_sec = ops_per_sec,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Directory operations benchmark complete"
    );

    Ok(BenchmarkResult {
        name: "dir_ops".to_string(),
        size: "metadata".to_string(),
        iterations: total_ops,
        total_bytes: 0,
        duration_secs: total_duration,
        throughput_mbps: 0.0,
        iops: Some(ops_per_sec),
        latency_stats,
        concurrency: None,
    })
}

async fn run_stat_ops(config: &Config, collector: &Collector) -> Result<BenchmarkResult> {
    let iterations = METADATA_ITERATIONS;
    let file_path = config.path.join("nfsb_stat_test.dat");
    let mut latencies = Vec::with_capacity(iterations as usize);

    // Create test file
    fs::write(&file_path, b"test data for stat operations").await?;

    info!(iterations = iterations, "Starting stat operations benchmark");

    let pb = ProgressBar::new(iterations as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message("stat_ops");

    let total_start = Instant::now();

    for i in 0..iterations {
        let iter_start = Instant::now();
        let _metadata = fs::metadata(&file_path).await?;
        let latency = iter_start.elapsed().as_secs_f64();

        latencies.push(latency);
        collector.record_operation("stat", "metadata", "metadata");
        collector.record_latency("stat", "metadata", latency);

        pb.inc(1);
        debug!(iteration = i, latency_ms = latency * 1000.0, "Stat complete");
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // Clean up
    fs::remove_file(&file_path).await?;

    let ops_per_sec = iterations as f64 / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_iops("stat", "metadata", "metadata", ops_per_sec);
    collector.record_duration("stat_ops", total_duration);

    info!(
        ops_per_sec = ops_per_sec,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Stat operations benchmark complete"
    );

    Ok(BenchmarkResult {
        name: "stat_ops".to_string(),
        size: "metadata".to_string(),
        iterations,
        total_bytes: 0,
        duration_secs: total_duration,
        throughput_mbps: 0.0,
        iops: Some(ops_per_sec),
        latency_stats,
        concurrency: None,
    })
}
