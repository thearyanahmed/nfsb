use prometheus::{CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts};
use std::sync::{Arc, RwLock};
use tracing::debug;

/// environment configuration for metric labels
#[derive(Clone, Debug)]
pub struct EnvironmentLabels {
    /// runtime environment: gvisor, native, droplet
    pub runtime: String,
    /// storage type being benchmarked: nfs, ephemeral, block
    pub storage_type: String,
    /// optional run identifier for grouping results
    pub run_id: Option<String>,
}

impl Default for EnvironmentLabels {
    fn default() -> Self {
        Self {
            runtime: "unknown".to_string(),
            storage_type: "unknown".to_string(),
            run_id: None,
        }
    }
}

impl EnvironmentLabels {
    pub fn new(runtime: impl Into<String>, storage_type: impl Into<String>) -> Self {
        Self {
            runtime: runtime.into(),
            storage_type: storage_type.into(),
            run_id: None,
        }
    }

    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }
}

/// Metrics collector for benchmark measurements
#[derive(Clone)]
pub struct Collector {
    // environment labels applied to all metrics (interior mutability for API use case)
    env_labels: Arc<RwLock<EnvironmentLabels>>,

    // Counters
    pub bytes_written: CounterVec,
    pub bytes_read: CounterVec,
    pub operations_total: CounterVec,

    // Gauges
    pub throughput_mbps: GaugeVec,
    pub iops: GaugeVec,

    // Histograms
    pub operation_duration: HistogramVec,
    pub latency: HistogramVec,
}

impl Collector {
    pub fn new() -> Self {
        Self::with_env(EnvironmentLabels::default())
    }

    pub fn with_env(env_labels: EnvironmentLabels) -> Self {
        // Bytes written counter - includes runtime and storage_type labels
        let bytes_written = CounterVec::new(
            Opts::new("nfsb_bytes_written_total", "Total bytes written"),
            &["size", "benchmark", "runtime", "storage_type"],
        )
        .expect("Failed to create bytes_written counter");

        // Bytes read counter
        let bytes_read = CounterVec::new(
            Opts::new("nfsb_bytes_read_total", "Total bytes read"),
            &["size", "benchmark", "runtime", "storage_type"],
        )
        .expect("Failed to create bytes_read counter");

        // Operations counter
        let operations_total = CounterVec::new(
            Opts::new("nfsb_operations_total", "Total operations performed"),
            &["operation", "size", "benchmark", "runtime", "storage_type"],
        )
        .expect("Failed to create operations_total counter");

        // Throughput gauge
        let throughput_mbps = GaugeVec::new(
            Opts::new("nfsb_throughput_mbps", "Current throughput in MB/s"),
            &["operation", "size", "benchmark", "runtime", "storage_type"],
        )
        .expect("Failed to create throughput_mbps gauge");

        // IOPS gauge
        let iops = GaugeVec::new(
            Opts::new("nfsb_iops", "Current IOPS"),
            &["operation", "size", "benchmark", "runtime", "storage_type"],
        )
        .expect("Failed to create iops gauge");

        // Operation duration histogram
        let operation_duration = HistogramVec::new(
            HistogramOpts::new(
                "nfsb_operation_duration_seconds",
                "Duration of benchmark operations",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
            &["benchmark", "runtime", "storage_type"],
        )
        .expect("Failed to create operation_duration histogram");

        // Latency histogram
        let latency = HistogramVec::new(
            HistogramOpts::new("nfsb_latency_seconds", "I/O operation latency")
                .buckets(vec![
                    0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
                ]),
            &["operation", "size", "runtime", "storage_type"],
        )
        .expect("Failed to create latency histogram");

        // Register all metrics with default registry
        let registry = prometheus::default_registry();
        registry.register(Box::new(bytes_written.clone())).expect("Failed to register bytes_written");
        registry.register(Box::new(bytes_read.clone())).expect("Failed to register bytes_read");
        registry.register(Box::new(operations_total.clone())).expect("Failed to register operations_total");
        registry.register(Box::new(throughput_mbps.clone())).expect("Failed to register throughput_mbps");
        registry.register(Box::new(iops.clone())).expect("Failed to register iops");
        registry.register(Box::new(operation_duration.clone())).expect("Failed to register operation_duration");
        registry.register(Box::new(latency.clone())).expect("Failed to register latency");

        debug!(
            runtime = %env_labels.runtime,
            storage_type = %env_labels.storage_type,
            "Metrics collector initialized with environment labels"
        );

        Self {
            env_labels: Arc::new(RwLock::new(env_labels)),
            bytes_written,
            bytes_read,
            operations_total,
            throughput_mbps,
            iops,
            operation_duration,
            latency,
        }
    }

    /// Update the environment labels for subsequent metric recordings.
    /// Useful when the same collector is reused for different benchmark runs
    /// (e.g., in the API server context).
    pub fn set_environment(&self, labels: EnvironmentLabels) {
        let mut env = self.env_labels.write().expect("RwLock poisoned");
        debug!(
            old_runtime = %env.runtime,
            old_storage = %env.storage_type,
            new_runtime = %labels.runtime,
            new_storage = %labels.storage_type,
            "Updating collector environment labels"
        );
        *env = labels;
    }

    /// Get the current environment labels
    pub fn get_environment(&self) -> EnvironmentLabels {
        self.env_labels.read().expect("RwLock poisoned").clone()
    }

    /// Record bytes written
    pub fn record_write(&self, size: &str, benchmark: &str, bytes: u64) {
        let env = self.env_labels.read().expect("RwLock poisoned");
        self.bytes_written
            .with_label_values(&[size, benchmark, &env.runtime, &env.storage_type])
            .inc_by(bytes as f64);
    }

    /// Record bytes read
    pub fn record_read(&self, size: &str, benchmark: &str, bytes: u64) {
        let env = self.env_labels.read().expect("RwLock poisoned");
        self.bytes_read
            .with_label_values(&[size, benchmark, &env.runtime, &env.storage_type])
            .inc_by(bytes as f64);
    }

    /// Record an operation
    pub fn record_operation(&self, operation: &str, size: &str, benchmark: &str) {
        let env = self.env_labels.read().expect("RwLock poisoned");
        self.operations_total
            .with_label_values(&[operation, size, benchmark, &env.runtime, &env.storage_type])
            .inc();
    }

    /// Set throughput measurement
    pub fn set_throughput(&self, operation: &str, size: &str, benchmark: &str, mbps: f64) {
        let env = self.env_labels.read().expect("RwLock poisoned");
        self.throughput_mbps
            .with_label_values(&[operation, size, benchmark, &env.runtime, &env.storage_type])
            .set(mbps);
    }

    /// Set IOPS measurement
    pub fn set_iops(&self, operation: &str, size: &str, benchmark: &str, value: f64) {
        let env = self.env_labels.read().expect("RwLock poisoned");
        self.iops
            .with_label_values(&[operation, size, benchmark, &env.runtime, &env.storage_type])
            .set(value);
    }

    /// Record operation duration
    pub fn record_duration(&self, benchmark: &str, duration_secs: f64) {
        let env = self.env_labels.read().expect("RwLock poisoned");
        self.operation_duration
            .with_label_values(&[benchmark, &env.runtime, &env.storage_type])
            .observe(duration_secs);
    }

    /// Record I/O latency
    pub fn record_latency(&self, operation: &str, size: &str, latency_secs: f64) {
        let env = self.env_labels.read().expect("RwLock poisoned");
        self.latency
            .with_label_values(&[operation, size, &env.runtime, &env.storage_type])
            .observe(latency_secs);
    }

    /// Reset gauges when benchmark stops
    pub fn reset_gauges(&self) {
        self.throughput_mbps.reset();
        self.iops.reset();
        debug!("Gauges reset to 0");
    }
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}
