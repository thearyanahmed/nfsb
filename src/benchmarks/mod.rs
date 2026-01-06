mod append;
mod concurrent;
mod metadata;
mod mixed;
mod random;
mod sequential;

use anyhow::Result;
use std::collections::HashMap;
use tracing::info;

use crate::config::Config;
use crate::metrics::Collector;
use crate::report::BenchmarkResult;
use crate::storage::EnvironmentInfo;
use crate::BenchmarkType;

pub use append::run_append;
pub use concurrent::run_concurrent;
pub use metadata::run_metadata;
pub use mixed::run_mixed;
pub use random::run_random;
pub use sequential::run_sequential;

/// Run all configured benchmarks
pub async fn run_all(
    config: &Config,
    collector: &Collector,
    _env_info: &EnvironmentInfo,
) -> Result<HashMap<String, Vec<BenchmarkResult>>> {
    let mut results: HashMap<String, Vec<BenchmarkResult>> = HashMap::new();

    if config.read_only {
        info!("Running in READ-ONLY mode - skipping all write benchmarks");
    }

    // Warmup phase (skip in read-only mode since warmup writes)
    if config.warmup && !config.read_only {
        info!("Running warmup phase...");
        warmup(config).await?;
    }

    match config.benchmark {
        BenchmarkType::Sequential => {
            info!("Running sequential benchmarks");
            let seq_results = run_sequential(config, collector).await?;
            results.insert("sequential".to_string(), seq_results);
        }
        BenchmarkType::Random => {
            info!("Running random I/O benchmarks");
            let rand_results = run_random(config, collector).await?;
            results.insert("random".to_string(), rand_results);
        }
        BenchmarkType::Concurrent => {
            info!("Running concurrent I/O benchmarks");
            let conc_results = run_concurrent(config, collector).await?;
            results.insert("concurrent".to_string(), conc_results);
        }
        BenchmarkType::Metadata => {
            info!("Running metadata benchmarks");
            let meta_results = run_metadata(config, collector).await?;
            results.insert("metadata".to_string(), meta_results);
        }
        BenchmarkType::Mixed => {
            if config.read_only {
                info!("Skipping mixed benchmarks (requires writes)");
            } else {
                info!("Running mixed read/write benchmarks");
                let mixed_results = run_mixed(config, collector).await?;
                results.insert("mixed".to_string(), mixed_results);
            }
        }
        BenchmarkType::Append => {
            if config.read_only {
                info!("Skipping append benchmarks (requires writes)");
            } else {
                info!("Running append benchmarks");
                let append_results = run_append(config, collector).await?;
                results.insert("append".to_string(), append_results);
            }
        }
        BenchmarkType::All => {
            info!("Running all benchmarks");

            info!("Running sequential benchmarks");
            let seq_results = run_sequential(config, collector).await?;
            results.insert("sequential".to_string(), seq_results);

            info!("Running random I/O benchmarks");
            let rand_results = run_random(config, collector).await?;
            results.insert("random".to_string(), rand_results);

            info!("Running concurrent I/O benchmarks");
            let conc_results = run_concurrent(config, collector).await?;
            results.insert("concurrent".to_string(), conc_results);

            info!("Running metadata benchmarks");
            let meta_results = run_metadata(config, collector).await?;
            results.insert("metadata".to_string(), meta_results);

            if config.read_only {
                info!("Skipping mixed benchmarks (requires writes)");
                info!("Skipping append benchmarks (requires writes)");
            } else {
                info!("Running mixed read/write benchmarks");
                let mixed_results = run_mixed(config, collector).await?;
                results.insert("mixed".to_string(), mixed_results);

                info!("Running append benchmarks");
                let append_results = run_append(config, collector).await?;
                results.insert("append".to_string(), append_results);
            }
        }
    }

    Ok(results)
}

/// Warmup phase to prime caches and establish connections
async fn warmup(config: &Config) -> Result<()> {
    use rand::Rng;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;

    let warmup_path = config.path.join(".nfsb_warmup");

    // Write some data
    let data: Vec<u8> = (0..1024 * 1024).map(|_| rand::thread_rng().gen()).collect();

    let mut file = fs::File::create(&warmup_path).await?;
    for _ in 0..10 {
        file.write_all(&data).await?;
    }
    file.sync_all().await?;
    drop(file);

    // Read it back
    let _ = fs::read(&warmup_path).await?;

    // Clean up
    fs::remove_file(&warmup_path).await?;

    info!("Warmup complete");
    Ok(())
}

/// Generate random data for benchmarks
pub fn generate_random_data(size: usize) -> Vec<u8> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..size).map(|_| rng.gen()).collect()
}
