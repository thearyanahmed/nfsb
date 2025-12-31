use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod api;
mod benchmarks;
mod config;
mod metrics;
mod report;
mod storage;

use config::Config;

#[derive(Parser)]
#[command(name = "nfsb")]
#[command(author = "DigitalOcean App Platform Team")]
#[command(version = "0.3.5")]
#[command(about = "NFS benchmark tool for measuring file I/O performance")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", global = true)]
    log_level: LogLevel,

    /// Output format for results
    #[arg(long, default_value = "text", global = true)]
    format: OutputFormat,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => Level::TRACE,
            LogLevel::Debug => Level::DEBUG,
            LogLevel::Info => Level::INFO,
            LogLevel::Warn => Level::WARN,
            LogLevel::Error => Level::ERROR,
        }
    }
}

#[derive(Clone, Copy, ValueEnum, Debug, Default)]
pub enum OutputFormat {
    /// Human-readable text output
    #[default]
    Text,
    /// JSON structured output
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Run benchmarks
    Run {
        /// Path to the directory to benchmark (e.g., /mnt/nfs)
        #[arg(short, long)]
        path: PathBuf,

        /// Create a test subdirectory within path (e.g., --test-dir nfsb-test creates /mnt/nfs/nfsb-test)
        #[arg(short = 'd', long)]
        test_dir: Option<String>,

        /// Clean up test directory after benchmarks complete
        #[arg(long)]
        cleanup: bool,

        /// Output file for JSON results
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Specific benchmark to run (default: all)
        #[arg(short, long)]
        benchmark: Option<BenchmarkType>,

        /// File sizes to test
        #[arg(short, long, value_delimiter = ',', default_value = "small,medium,large")]
        sizes: Vec<FileSize>,

        /// Number of iterations per benchmark
        #[arg(short, long, default_value = "100")]
        iterations: u32,

        /// Concurrency levels to test
        #[arg(short, long, value_delimiter = ',', default_value = "1,4,8,16")]
        concurrency: Vec<u32>,

        /// Port for Prometheus metrics HTTP server (0 to disable)
        #[arg(long, default_value = "9090")]
        prometheus_port: u16,

        /// Skip warmup phase
        #[arg(long)]
        no_warmup: bool,
    },
    /// Show detected environment info
    Info {
        /// Path to check
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
    /// Start REST API server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

#[derive(Clone, Copy, ValueEnum, Debug, PartialEq, Eq)]
pub enum BenchmarkType {
    /// Sequential read/write operations
    Sequential,
    /// Random read/write operations
    Random,
    /// Concurrent I/O operations
    Concurrent,
    /// File metadata operations (create/delete)
    Metadata,
    /// Mixed read/write operations (configurable ratio)
    Mixed,
    /// Append operations (continuously append to files)
    Append,
    /// All benchmarks
    All,
}

#[derive(Clone, Copy, ValueEnum, Debug, PartialEq, Eq, Hash)]
pub enum FileSize {
    /// 4KB files
    Small,
    /// 1MB files
    Medium,
    /// 100MB files
    Large,
}

impl FileSize {
    pub fn bytes(&self) -> usize {
        match self {
            FileSize::Small => 4 * 1024,           // 4KB
            FileSize::Medium => 1024 * 1024,       // 1MB
            FileSize::Large => 100 * 1024 * 1024,  // 100MB
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            FileSize::Small => "small",
            FileSize::Medium => "medium",
            FileSize::Large => "large",
        }
    }
}

fn init_logging(level: LogLevel) {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::from(level))
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_logging(cli.log_level);

    match cli.command {
        Commands::Run {
            path,
            test_dir,
            cleanup,
            output,
            benchmark,
            sizes,
            iterations,
            concurrency,
            prometheus_port,
            no_warmup,
        } => {
            // resolve the actual test path
            let test_path = if let Some(ref dir_name) = test_dir {
                let full_path = path.join(dir_name);
                info!(path = %full_path.display(), "Creating test directory");
                tokio::fs::create_dir_all(&full_path).await?;
                full_path
            } else {
                path.clone()
            };

            let config = Config {
                path: test_path.clone(),
                output,
                benchmark: benchmark.unwrap_or(BenchmarkType::All),
                sizes,
                iterations,
                concurrency,
                prometheus_port,
                warmup: !no_warmup,
                format: cli.format,
            };

            let result = run_benchmarks(config).await;

            // cleanup test directory if requested
            if cleanup && test_dir.is_some() {
                info!(path = %test_path.display(), "Cleaning up test directory");
                if let Err(e) = tokio::fs::remove_dir_all(&test_path).await {
                    tracing::warn!(error = %e, "Failed to cleanup test directory");
                }
            }

            result?;
        }
        Commands::Info { path } => {
            show_info(&path, cli.format).await?;
        }
        Commands::Serve { port } => {
            api::serve(port).await?;
        }
    }

    Ok(())
}

async fn run_benchmarks(config: Config) -> Result<()> {
    info!("nfsb - NFS Benchmark Tool v0.3.5");

    // Detect environment
    let env_info = storage::detect_environment(&config.path).await?;
    info!(
        runtime = %env_info.runtime,
        storage = %env_info.storage_type,
        path = %config.path.display(),
        "Detected environment"
    );

    // Start Prometheus server if enabled
    let _metrics_handle = if config.prometheus_port > 0 {
        info!(port = config.prometheus_port, "Starting Prometheus metrics server");
        Some(metrics::start_server(config.prometheus_port).await?)
    } else {
        None
    };

    // Start system metrics collection (refresh every 1 second during benchmarks)
    let system_metrics = std::sync::Arc::new(metrics::SystemMetrics::new());
    system_metrics.clone().start_background_refresh(1);

    // Initialize metrics collector with environment labels from detected environment
    let env_labels = metrics::EnvironmentLabels::new(
        env_info.runtime.to_string(),
        env_info.storage_type.to_string(),
    );
    let collector = metrics::Collector::with_env(env_labels);

    // Run benchmarks
    let results = benchmarks::run_all(&config, &collector, &env_info).await?;

    // Generate report
    let report = report::generate(&results, &env_info, &config)?;

    // Output results
    match config.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&report)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            println!("{}", report::format_summary(&report));
        }
    }

    if let Some(output_path) = &config.output {
        report::save_json(&report, output_path)?;
        info!(path = %output_path.display(), "Results saved");
    }

    Ok(())
}

async fn show_info(path: &Path, format: OutputFormat) -> Result<()> {
    let env_info = storage::detect_environment(path).await?;

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&env_info)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            println!("Environment Information:");
            println!("  Runtime: {}", env_info.runtime);
            println!("  Storage Type: {}", env_info.storage_type);
            println!("  Mount Point: {}", env_info.mount_point.as_deref().unwrap_or("N/A"));
            println!("  Filesystem: {}", env_info.filesystem.as_deref().unwrap_or("N/A"));

            if let Some(sys) = &env_info.system {
                println!("\nSystem Information:");
                println!("  CPU Cores: {}", sys.cpu_cores);
                println!("  Total Memory: {} MB", sys.total_memory_mb);
                println!("  Available Memory: {} MB", sys.available_memory_mb);
            }
        }
    }

    Ok(())
}
