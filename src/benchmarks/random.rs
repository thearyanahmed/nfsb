use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use std::time::Instant;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tracing::{debug, info};

use super::generate_random_data;
use crate::config::Config;
use crate::metrics::Collector;
use crate::report::{BenchmarkResult, Statistics};
use crate::FileSize;

const BLOCK_SIZE: usize = 4096; // 4KB blocks for random I/O
const RANDOM_ITERATIONS: u32 = 1000; // More iterations for random I/O

/// Run random I/O benchmarks
pub async fn run_random(config: &Config, collector: &Collector) -> Result<Vec<BenchmarkResult>> {
    let mut results = Vec::new();

    for size in &config.sizes {
        // Random Write
        let write_result = run_random_write(config, collector, *size).await?;
        results.push(write_result);

        // Random Read
        let read_result = run_random_read(config, collector, *size).await?;
        results.push(read_result);
    }

    Ok(results)
}

async fn run_random_write(
    config: &Config,
    collector: &Collector,
    size: FileSize,
) -> Result<BenchmarkResult> {
    let file_path = config.path.join(format!("nfsb_rand_write_{}.dat", size.name()));
    let file_size = size.bytes();
    let iterations = RANDOM_ITERATIONS;
    let block_data = generate_random_data(BLOCK_SIZE);
    let mut latencies = Vec::with_capacity(iterations as usize);

    // Pre-create file with target size
    let data = generate_random_data(file_size);
    let mut file = File::create(&file_path).await?;
    file.write_all(&data).await?;
    file.sync_all().await?;
    drop(file);

    info!(
        size = size.name(),
        file_size = file_size,
        block_size = BLOCK_SIZE,
        iterations = iterations,
        "Starting random write benchmark"
    );

    let pb = ProgressBar::new(iterations as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("rand_write_{}", size.name()));

    let mut file = File::options()
        .write(true)
        .open(&file_path)
        .await?;

    let max_offset = file_size - BLOCK_SIZE;

    // pre-generate all random offsets to avoid holding RNG across await points
    let offsets: Vec<u64> = {
        let mut rng = rand::thread_rng();
        (0..iterations)
            .map(|_| rng.gen_range(0..=max_offset) as u64)
            .collect()
    };

    let total_start = Instant::now();

    for (i, &offset) in offsets.iter().enumerate() {

        let iter_start = Instant::now();
        file.seek(SeekFrom::Start(offset)).await?;
        file.write_all(&block_data).await?;
        file.sync_all().await?;

        let latency = iter_start.elapsed().as_secs_f64();
        latencies.push(latency);

        collector.record_write(size.name(), "random", BLOCK_SIZE as u64);
        collector.record_operation("write", size.name(), "random");
        collector.record_latency("random_write", size.name(), latency);

        pb.inc(1);
        debug!(iteration = i, offset = offset, latency_ms = latency * 1000.0, "Random write complete");
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");
    drop(file);

    // Clean up
    tokio::fs::remove_file(&file_path).await?;

    let total_bytes = BLOCK_SIZE as u64 * iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let iops = iterations as f64 / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_throughput("random_write", size.name(), "random", throughput_mbps);
    collector.set_iops("random_write", size.name(), "random", iops);
    collector.record_duration("random_write", total_duration);

    info!(
        throughput_mbps = throughput_mbps,
        iops = iops,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Random write complete"
    );

    Ok(BenchmarkResult {
        name: "random_write".to_string(),
        size: size.name().to_string(),
        iterations,
        total_bytes,
        duration_secs: total_duration,
        throughput_mbps,
        iops: Some(iops),
        latency_stats,
        concurrency: None,
    })
}

async fn run_random_read(
    config: &Config,
    collector: &Collector,
    size: FileSize,
) -> Result<BenchmarkResult> {
    let file_path = config.path.join(format!("nfsb_rand_read_{}.dat", size.name()));
    let file_size = size.bytes();
    let iterations = RANDOM_ITERATIONS;
    let mut latencies = Vec::with_capacity(iterations as usize);

    // Pre-create file with target size
    let data = generate_random_data(file_size);
    let mut file = File::create(&file_path).await?;
    file.write_all(&data).await?;
    file.sync_all().await?;
    drop(file);

    info!(
        size = size.name(),
        file_size = file_size,
        block_size = BLOCK_SIZE,
        iterations = iterations,
        "Starting random read benchmark"
    );

    let pb = ProgressBar::new(iterations as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("rand_read_{}", size.name()));

    let mut file = File::open(&file_path).await?;
    let mut buffer = vec![0u8; BLOCK_SIZE];
    let max_offset = file_size - BLOCK_SIZE;

    // pre-generate all random offsets to avoid holding RNG across await points
    let offsets: Vec<u64> = {
        let mut rng = rand::thread_rng();
        (0..iterations)
            .map(|_| rng.gen_range(0..=max_offset) as u64)
            .collect()
    };

    let total_start = Instant::now();

    for (i, &offset) in offsets.iter().enumerate() {

        let iter_start = Instant::now();
        file.seek(SeekFrom::Start(offset)).await?;
        file.read_exact(&mut buffer).await?;

        let latency = iter_start.elapsed().as_secs_f64();
        latencies.push(latency);

        collector.record_read(size.name(), "random", BLOCK_SIZE as u64);
        collector.record_operation("read", size.name(), "random");
        collector.record_latency("random_read", size.name(), latency);

        pb.inc(1);
        debug!(iteration = i, offset = offset, latency_ms = latency * 1000.0, "Random read complete");
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");
    drop(file);

    // Clean up
    tokio::fs::remove_file(&file_path).await?;

    let total_bytes = BLOCK_SIZE as u64 * iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let iops = iterations as f64 / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_throughput("random_read", size.name(), "random", throughput_mbps);
    collector.set_iops("random_read", size.name(), "random", iops);
    collector.record_duration("random_read", total_duration);

    info!(
        throughput_mbps = throughput_mbps,
        iops = iops,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Random read complete"
    );

    Ok(BenchmarkResult {
        name: "random_read".to_string(),
        size: size.name().to_string(),
        iterations,
        total_bytes,
        duration_secs: total_duration,
        throughput_mbps,
        iops: Some(iops),
        latency_stats,
        concurrency: None,
    })
}
