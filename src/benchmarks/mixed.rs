use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::time::Instant;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

use super::generate_random_data;
use crate::config::Config;
use crate::metrics::Collector;
use crate::report::{BenchmarkResult, Statistics};
use crate::FileSize;

/// default read ratio (70% reads, 30% writes)
const DEFAULT_READ_RATIO: f64 = 0.7;

/// Run mixed read/write benchmarks
pub async fn run_mixed(config: &Config, collector: &Collector) -> Result<Vec<BenchmarkResult>> {
    let mut results = Vec::new();

    for size in &config.sizes {
        let result = run_mixed_workload(config, collector, *size, DEFAULT_READ_RATIO).await?;
        results.push(result);
    }

    Ok(results)
}

async fn run_mixed_workload(
    config: &Config,
    collector: &Collector,
    size: FileSize,
    read_ratio: f64,
) -> Result<BenchmarkResult> {
    let file_path = config.path.join(format!("nfsb_mixed_{}.dat", size.name()));
    let data = generate_random_data(size.bytes());
    let mut latencies = Vec::with_capacity(config.iterations as usize);
    let mut read_latencies = Vec::new();
    let mut write_latencies = Vec::new();
    let mut read_count = 0u64;
    let mut write_count = 0u64;

    // create initial file for reading
    let mut file = File::create(&file_path).await?;
    file.write_all(&data).await?;
    file.sync_all().await?;
    drop(file);

    info!(
        size = size.name(),
        bytes = size.bytes(),
        iterations = config.iterations,
        read_ratio = read_ratio,
        "Starting mixed read/write benchmark"
    );

    let pb = ProgressBar::new(config.iterations as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("mixed_{}", size.name()));

    let total_start = Instant::now();
    let mut buffer = vec![0u8; size.bytes()];
    let mut rng = StdRng::from_entropy();

    for i in 0..config.iterations {
        let iter_start = Instant::now();
        let is_read = rng.gen::<f64>() < read_ratio;

        if is_read {
            // read operation
            let mut file = File::open(&file_path).await?;
            file.read_exact(&mut buffer).await?;
            drop(file);

            let latency = iter_start.elapsed().as_secs_f64();
            latencies.push(latency);
            read_latencies.push(latency);
            read_count += 1;

            collector.record_read(size.name(), "mixed", size.bytes() as u64);
            collector.record_operation("read", size.name(), "mixed");
            collector.record_latency("read", size.name(), latency);
        } else {
            // write operation
            let mut file = File::create(&file_path).await?;
            file.write_all(&data).await?;
            file.sync_all().await?;
            drop(file);

            let latency = iter_start.elapsed().as_secs_f64();
            latencies.push(latency);
            write_latencies.push(latency);
            write_count += 1;

            collector.record_write(size.name(), "mixed", size.bytes() as u64);
            collector.record_operation("write", size.name(), "mixed");
            collector.record_latency("write", size.name(), latency);
        }

        pb.inc(1);
        debug!(
            iteration = i,
            is_read = is_read,
            latency_ms = iter_start.elapsed().as_secs_f64() * 1000.0,
            "Mixed iteration complete"
        );
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // clean up
    fs::remove_file(&file_path).await?;

    let total_bytes = size.bytes() as u64 * config.iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    // calculate separate read/write throughput
    let read_throughput = if !read_latencies.is_empty() {
        let read_bytes = size.bytes() as f64 * read_count as f64;
        let read_duration: f64 = read_latencies.iter().sum();
        (read_bytes / 1024.0 / 1024.0) / read_duration
    } else {
        0.0
    };

    let write_throughput = if !write_latencies.is_empty() {
        let write_bytes = size.bytes() as f64 * write_count as f64;
        let write_duration: f64 = write_latencies.iter().sum();
        (write_bytes / 1024.0 / 1024.0) / write_duration
    } else {
        0.0
    };

    collector.set_throughput("mixed", size.name(), "mixed", throughput_mbps);
    collector.set_throughput("read", size.name(), "mixed", read_throughput);
    collector.set_throughput("write", size.name(), "mixed", write_throughput);
    collector.record_duration("mixed", total_duration);

    let actual_read_ratio = read_count as f64 / config.iterations as f64;

    info!(
        throughput_mbps = throughput_mbps,
        read_throughput_mbps = read_throughput,
        write_throughput_mbps = write_throughput,
        read_count = read_count,
        write_count = write_count,
        actual_read_ratio = actual_read_ratio,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Mixed workload complete"
    );

    Ok(BenchmarkResult {
        name: format!("mixed_{}_{}", (read_ratio * 100.0) as u32, ((1.0 - read_ratio) * 100.0) as u32),
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
