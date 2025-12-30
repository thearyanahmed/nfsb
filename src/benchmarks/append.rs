use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use super::generate_random_data;
use crate::config::Config;
use crate::metrics::Collector;
use crate::report::{BenchmarkResult, Statistics};
use crate::FileSize;

/// Run append benchmarks - continuously append data to files
pub async fn run_append(config: &Config, collector: &Collector) -> Result<Vec<BenchmarkResult>> {
    let mut results = Vec::new();

    for size in &config.sizes {
        let result = run_append_workload(config, collector, *size).await?;
        results.push(result);
    }

    Ok(results)
}

async fn run_append_workload(
    config: &Config,
    collector: &Collector,
    size: FileSize,
) -> Result<BenchmarkResult> {
    let file_path = config.path.join(format!("nfsb_append_{}.dat", size.name()));
    let data = generate_random_data(size.bytes());
    let mut latencies = Vec::with_capacity(config.iterations as usize);

    // create empty file to start
    fs::write(&file_path, &[]).await?;

    info!(
        size = size.name(),
        bytes = size.bytes(),
        iterations = config.iterations,
        "Starting append benchmark"
    );

    let pb = ProgressBar::new(config.iterations as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("append_{}", size.name()));

    let total_start = Instant::now();

    for i in 0..config.iterations {
        let iter_start = Instant::now();

        // open file in append mode
        let mut file = OpenOptions::new()
            .append(true)
            .open(&file_path)
            .await?;

        file.write_all(&data).await?;
        file.sync_all().await?;
        drop(file);

        let latency = iter_start.elapsed().as_secs_f64();
        latencies.push(latency);

        collector.record_write(size.name(), "append", size.bytes() as u64);
        collector.record_operation("append", size.name(), "append");
        collector.record_latency("write", size.name(), latency);

        pb.inc(1);
        debug!(
            iteration = i,
            latency_ms = latency * 1000.0,
            file_size_mb = (i as u64 + 1) * size.bytes() as u64 / 1024 / 1024,
            "Append iteration complete"
        );
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // get final file size for verification
    let final_size = fs::metadata(&file_path).await?.len();
    debug!(final_file_size = final_size, "Final appended file size");

    // clean up
    fs::remove_file(&file_path).await?;

    let total_bytes = size.bytes() as u64 * config.iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_throughput("append", size.name(), "append", throughput_mbps);
    collector.record_duration("append", total_duration);

    info!(
        throughput_mbps = throughput_mbps,
        total_bytes = total_bytes,
        final_file_size = final_size,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Append benchmark complete"
    );

    Ok(BenchmarkResult {
        name: "append".to_string(),
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
