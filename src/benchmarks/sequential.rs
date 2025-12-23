use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

use super::generate_random_data;
use crate::config::Config;
use crate::metrics::Collector;
use crate::report::{BenchmarkResult, Statistics};
use crate::FileSize;

/// Run sequential read/write benchmarks
pub async fn run_sequential(config: &Config, collector: &Collector) -> Result<Vec<BenchmarkResult>> {
    let mut results = Vec::new();

    for size in &config.sizes {
        // Sequential Write
        let write_result = run_sequential_write(config, collector, *size).await?;
        results.push(write_result);

        // Sequential Read
        let read_result = run_sequential_read(config, collector, *size).await?;
        results.push(read_result);
    }

    Ok(results)
}

async fn run_sequential_write(
    config: &Config,
    collector: &Collector,
    size: FileSize,
) -> Result<BenchmarkResult> {
    let file_path = config.path.join(format!("nfsb_seq_write_{}.dat", size.name()));
    let data = generate_random_data(size.bytes());
    let mut latencies = Vec::with_capacity(config.iterations as usize);

    info!(
        size = size.name(),
        bytes = size.bytes(),
        iterations = config.iterations,
        "Starting sequential write benchmark"
    );

    let pb = ProgressBar::new(config.iterations as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("seq_write_{}", size.name()));

    let total_start = Instant::now();

    for i in 0..config.iterations {
        let iter_start = Instant::now();

        let mut file = File::create(&file_path).await?;
        file.write_all(&data).await?;
        file.sync_all().await?;
        drop(file);

        let latency = iter_start.elapsed().as_secs_f64();
        latencies.push(latency);

        collector.record_write(size.name(), "sequential", size.bytes() as u64);
        collector.record_operation("write", size.name(), "sequential");
        collector.record_latency("write", size.name(), latency);

        pb.inc(1);
        debug!(iteration = i, latency_ms = latency * 1000.0, "Write iteration complete");
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // Clean up
    fs::remove_file(&file_path).await?;

    let total_bytes = size.bytes() as u64 * config.iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_throughput("write", size.name(), "sequential", throughput_mbps);
    collector.record_duration("sequential_write", total_duration);

    info!(
        throughput_mbps = throughput_mbps,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Sequential write complete"
    );

    Ok(BenchmarkResult {
        name: "sequential_write".to_string(),
        size: size.name().to_string(),
        iterations: config.iterations,
        total_bytes,
        duration_secs: total_duration,
        throughput_mbps,
        iops: None,
        latency_stats,
        concurrency: None,
    })
}

async fn run_sequential_read(
    config: &Config,
    collector: &Collector,
    size: FileSize,
) -> Result<BenchmarkResult> {
    let file_path = config.path.join(format!("nfsb_seq_read_{}.dat", size.name()));
    let data = generate_random_data(size.bytes());
    let mut latencies = Vec::with_capacity(config.iterations as usize);

    // Create test file
    let mut file = File::create(&file_path).await?;
    file.write_all(&data).await?;
    file.sync_all().await?;
    drop(file);

    info!(
        size = size.name(),
        bytes = size.bytes(),
        iterations = config.iterations,
        "Starting sequential read benchmark"
    );

    let pb = ProgressBar::new(config.iterations as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("seq_read_{}", size.name()));

    let total_start = Instant::now();
    let mut buffer = vec![0u8; size.bytes()];

    for i in 0..config.iterations {
        let iter_start = Instant::now();

        let mut file = File::open(&file_path).await?;
        file.read_exact(&mut buffer).await?;
        drop(file);

        let latency = iter_start.elapsed().as_secs_f64();
        latencies.push(latency);

        collector.record_read(size.name(), "sequential", size.bytes() as u64);
        collector.record_operation("read", size.name(), "sequential");
        collector.record_latency("read", size.name(), latency);

        pb.inc(1);
        debug!(iteration = i, latency_ms = latency * 1000.0, "Read iteration complete");
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // Clean up
    fs::remove_file(&file_path).await?;

    let total_bytes = size.bytes() as u64 * config.iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_throughput("read", size.name(), "sequential", throughput_mbps);
    collector.record_duration("sequential_read", total_duration);

    info!(
        throughput_mbps = throughput_mbps,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Sequential read complete"
    );

    Ok(BenchmarkResult {
        name: "sequential_read".to_string(),
        size: size.name().to_string(),
        iterations: config.iterations,
        total_bytes,
        duration_secs: total_duration,
        throughput_mbps,
        iops: None,
        latency_stats,
        concurrency: None,
    })
}
