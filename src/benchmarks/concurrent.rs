use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Instant;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use tracing::{debug, info};

use super::generate_random_data;
use crate::config::Config;
use crate::metrics::Collector;
use crate::report::{BenchmarkResult, Statistics};
use crate::FileSize;

/// Run concurrent I/O benchmarks
pub async fn run_concurrent(config: &Config, collector: &Collector) -> Result<Vec<BenchmarkResult>> {
    let mut results = Vec::new();

    for &concurrency in &config.concurrency {
        for size in &config.sizes {
            // Concurrent Write (skip in read-only mode)
            if !config.read_only {
                let write_result = run_concurrent_write(config, collector, *size, concurrency).await?;
                results.push(write_result);
            }

            // Concurrent Read
            let read_result = run_concurrent_read(config, collector, *size, concurrency).await?;
            results.push(read_result);
        }
    }

    Ok(results)
}

async fn run_concurrent_write(
    config: &Config,
    collector: &Collector,
    size: FileSize,
    concurrency: u32,
) -> Result<BenchmarkResult> {
    let iterations_per_worker = config.iterations / concurrency;
    let total_iterations = iterations_per_worker * concurrency;
    let data = Arc::new(generate_random_data(size.bytes()));

    info!(
        size = size.name(),
        bytes = size.bytes(),
        concurrency = concurrency,
        iterations_per_worker = iterations_per_worker,
        "Starting concurrent write benchmark"
    );

    let pb = Arc::new(ProgressBar::new(total_iterations as u64));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("conc_write_{}_{}", size.name(), concurrency));

    let semaphore = Arc::new(Semaphore::new(concurrency as usize));
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::with_capacity(total_iterations as usize)));

    let total_start = Instant::now();

    let mut handles = Vec::new();

    for worker_id in 0..concurrency {
        let data = Arc::clone(&data);
        let pb = Arc::clone(&pb);
        let semaphore = Arc::clone(&semaphore);
        let latencies = Arc::clone(&latencies);
        let path = config.path.clone();
        let collector = collector.clone();
        let size_name = size.name().to_string();

        let handle = tokio::spawn(async move {
            for i in 0..iterations_per_worker {
                let _permit = semaphore.acquire().await.unwrap();

                let file_path = path.join(format!(
                    "nfsb_conc_write_{}_{}_{}_{}.dat",
                    size_name, concurrency, worker_id, i
                ));

                let iter_start = Instant::now();

                let mut file = File::create(&file_path).await?;
                file.write_all(&data).await?;
                file.sync_all().await?;
                drop(file);

                let latency = iter_start.elapsed().as_secs_f64();

                {
                    let mut lat = latencies.lock().await;
                    lat.push(latency);
                }

                collector.record_write(&size_name, "concurrent", data.len() as u64);
                collector.record_operation("write", &size_name, "concurrent");
                collector.record_latency("concurrent_write", &size_name, latency);

                pb.inc(1);

                // Clean up
                tokio::fs::remove_file(&file_path).await?;

                debug!(
                    worker = worker_id,
                    iteration = i,
                    latency_ms = latency * 1000.0,
                    "Concurrent write complete"
                );
            }

            Ok::<(), anyhow::Error>(())
        });

        handles.push(handle);
    }

    // Wait for all workers
    for handle in handles {
        handle.await??;
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    let latencies = Arc::try_unwrap(latencies)
        .unwrap()
        .into_inner();

    let total_bytes = size.bytes() as u64 * total_iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_throughput("concurrent_write", size.name(), "concurrent", throughput_mbps);
    collector.record_duration("concurrent_write", total_duration);

    info!(
        throughput_mbps = throughput_mbps,
        concurrency = concurrency,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Concurrent write complete"
    );

    Ok(BenchmarkResult {
        name: "concurrent_write".to_string(),
        size: size.name().to_string(),
        iterations: total_iterations,
        total_bytes,
        duration_secs: total_duration,
        throughput_mbps,
        iops: None,
        latency_stats,
        concurrency: Some(concurrency),
    })
}

async fn run_concurrent_read(
    config: &Config,
    collector: &Collector,
    size: FileSize,
    concurrency: u32,
) -> Result<BenchmarkResult> {
    let iterations_per_worker = config.iterations / concurrency;
    let total_iterations = iterations_per_worker * concurrency;

    // Create test files for each worker (skip in read-only mode)
    let mut test_files = Vec::new();

    if !config.read_only {
        let data = generate_random_data(size.bytes());
        for worker_id in 0..concurrency {
            let file_path = config.path.join(format!(
                "nfsb_conc_read_{}_{}.dat",
                size.name(),
                worker_id
            ));
            let mut file = File::create(&file_path).await?;
            file.write_all(&data).await?;
            file.sync_all().await?;
            test_files.push(file_path);
        }
    } else {
        // Read-only mode: check that files exist
        for worker_id in 0..concurrency {
            let file_path = config.path.join(format!(
                "nfsb_conc_read_{}_{}.dat",
                size.name(),
                worker_id
            ));
            if !file_path.exists() {
                anyhow::bail!(
                    "Read-only mode: test file does not exist: {}. Run benchmark without read_only first.",
                    file_path.display()
                );
            }
            test_files.push(file_path);
        }
    }

    // Get actual file size from first file
    let file_size = tokio::fs::metadata(&test_files[0]).await?.len() as usize;

    info!(
        size = size.name(),
        bytes = file_size,
        concurrency = concurrency,
        iterations_per_worker = iterations_per_worker,
        read_only = config.read_only,
        "Starting concurrent read benchmark"
    );

    let pb = Arc::new(ProgressBar::new(total_iterations as u64));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("conc_read_{}_{}", size.name(), concurrency));

    let semaphore = Arc::new(Semaphore::new(concurrency as usize));
    let latencies = Arc::new(tokio::sync::Mutex::new(Vec::with_capacity(total_iterations as usize)));

    let total_start = Instant::now();

    let mut handles = Vec::new();

    for (worker_id, file_path) in test_files.iter().enumerate() {
        let pb = Arc::clone(&pb);
        let semaphore = Arc::clone(&semaphore);
        let latencies = Arc::clone(&latencies);
        let file_path = file_path.clone();
        let collector = collector.clone();
        let size_name = size.name().to_string();

        let handle = tokio::spawn(async move {
            let mut buffer = vec![0u8; file_size];

            for i in 0..iterations_per_worker {
                let _permit = semaphore.acquire().await.unwrap();

                let iter_start = Instant::now();

                let mut file = File::open(&file_path).await?;
                file.read_exact(&mut buffer).await?;
                drop(file);

                let latency = iter_start.elapsed().as_secs_f64();

                {
                    let mut lat = latencies.lock().await;
                    lat.push(latency);
                }

                collector.record_read(&size_name, "concurrent", file_size as u64);
                collector.record_operation("read", &size_name, "concurrent");
                collector.record_latency("concurrent_read", &size_name, latency);

                pb.inc(1);

                debug!(
                    worker = worker_id,
                    iteration = i,
                    latency_ms = latency * 1000.0,
                    "Concurrent read complete"
                );
            }

            Ok::<(), anyhow::Error>(())
        });

        handles.push(handle);
    }

    // Wait for all workers
    for handle in handles {
        handle.await??;
    }

    let total_duration = total_start.elapsed().as_secs_f64();
    pb.finish_with_message("done");

    // Clean up test files (skip in read-only mode)
    if !config.read_only {
        for file_path in &test_files {
            tokio::fs::remove_file(&file_path).await?;
        }
    }

    let latencies = Arc::try_unwrap(latencies)
        .unwrap()
        .into_inner();

    let total_bytes = file_size as u64 * total_iterations as u64;
    let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / total_duration;
    let latency_stats = Statistics::from_values(&latencies);

    collector.set_throughput("concurrent_read", size.name(), "concurrent", throughput_mbps);
    collector.record_duration("concurrent_read", total_duration);

    info!(
        throughput_mbps = throughput_mbps,
        concurrency = concurrency,
        p50_ms = latency_stats.p50 * 1000.0,
        p99_ms = latency_stats.p99 * 1000.0,
        "Concurrent read complete"
    );

    Ok(BenchmarkResult {
        name: "concurrent_read".to_string(),
        size: size.name().to_string(),
        iterations: total_iterations,
        total_bytes,
        duration_secs: total_duration,
        throughput_mbps,
        iops: None,
        latency_stats,
        concurrency: Some(concurrency),
    })
}
