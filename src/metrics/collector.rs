use prometheus::{CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry};
use std::sync::Arc;
use tracing::debug;

/// Metrics collector for benchmark measurements
#[derive(Clone)]
pub struct Collector {
    _registry: Arc<Registry>,

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
        let registry = Registry::new();

        // Bytes written counter
        let bytes_written = CounterVec::new(
            Opts::new("nfsb_bytes_written_total", "Total bytes written")
                .namespace("nfsb"),
            &["size", "benchmark"],
        )
        .expect("Failed to create bytes_written counter");

        // Bytes read counter
        let bytes_read = CounterVec::new(
            Opts::new("nfsb_bytes_read_total", "Total bytes read")
                .namespace("nfsb"),
            &["size", "benchmark"],
        )
        .expect("Failed to create bytes_read counter");

        // Operations counter
        let operations_total = CounterVec::new(
            Opts::new("nfsb_operations_total", "Total operations performed")
                .namespace("nfsb"),
            &["operation", "size", "benchmark"],
        )
        .expect("Failed to create operations_total counter");

        // Throughput gauge
        let throughput_mbps = GaugeVec::new(
            Opts::new("nfsb_throughput_mbps", "Current throughput in MB/s")
                .namespace("nfsb"),
            &["operation", "size", "benchmark"],
        )
        .expect("Failed to create throughput_mbps gauge");

        // IOPS gauge
        let iops = GaugeVec::new(
            Opts::new("nfsb_iops", "Current IOPS")
                .namespace("nfsb"),
            &["operation", "size", "benchmark"],
        )
        .expect("Failed to create iops gauge");

        // Operation duration histogram
        let operation_duration = HistogramVec::new(
            HistogramOpts::new(
                "nfsb_operation_duration_seconds",
                "Duration of benchmark operations",
            )
            .namespace("nfsb")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
            &["benchmark"],
        )
        .expect("Failed to create operation_duration histogram");

        // Latency histogram
        let latency = HistogramVec::new(
            HistogramOpts::new("nfsb_latency_seconds", "I/O operation latency")
                .namespace("nfsb")
                .buckets(vec![
                    0.0001, 0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
                ]),
            &["operation", "size"],
        )
        .expect("Failed to create latency histogram");

        // Register all metrics
        registry.register(Box::new(bytes_written.clone())).unwrap();
        registry.register(Box::new(bytes_read.clone())).unwrap();
        registry.register(Box::new(operations_total.clone())).unwrap();
        registry.register(Box::new(throughput_mbps.clone())).unwrap();
        registry.register(Box::new(iops.clone())).unwrap();
        registry.register(Box::new(operation_duration.clone())).unwrap();
        registry.register(Box::new(latency.clone())).unwrap();

        debug!("Metrics collector initialized");

        Self {
            _registry: Arc::new(registry),
            bytes_written,
            bytes_read,
            operations_total,
            throughput_mbps,
            iops,
            operation_duration,
            latency,
        }
    }

    /// Record bytes written
    pub fn record_write(&self, size: &str, benchmark: &str, bytes: u64) {
        self.bytes_written
            .with_label_values(&[size, benchmark])
            .inc_by(bytes as f64);
    }

    /// Record bytes read
    pub fn record_read(&self, size: &str, benchmark: &str, bytes: u64) {
        self.bytes_read
            .with_label_values(&[size, benchmark])
            .inc_by(bytes as f64);
    }

    /// Record an operation
    pub fn record_operation(&self, operation: &str, size: &str, benchmark: &str) {
        self.operations_total
            .with_label_values(&[operation, size, benchmark])
            .inc();
    }

    /// Set throughput measurement
    pub fn set_throughput(&self, operation: &str, size: &str, benchmark: &str, mbps: f64) {
        self.throughput_mbps
            .with_label_values(&[operation, size, benchmark])
            .set(mbps);
    }

    /// Set IOPS measurement
    pub fn set_iops(&self, operation: &str, size: &str, benchmark: &str, value: f64) {
        self.iops
            .with_label_values(&[operation, size, benchmark])
            .set(value);
    }

    /// Record operation duration
    pub fn record_duration(&self, benchmark: &str, duration_secs: f64) {
        self.operation_duration
            .with_label_values(&[benchmark])
            .observe(duration_secs);
    }

    /// Record I/O latency
    pub fn record_latency(&self, operation: &str, size: &str, latency_secs: f64) {
        self.latency
            .with_label_values(&[operation, size])
            .observe(latency_secs);
    }
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}
