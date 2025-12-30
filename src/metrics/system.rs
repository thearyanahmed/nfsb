use prometheus::{Gauge, Opts};
use sysinfo::System;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// System metrics collector for CPU and memory
pub struct SystemMetrics {
    system: Arc<RwLock<System>>,
    cpu_usage: Gauge,
    memory_used_bytes: Gauge,
    memory_total_bytes: Gauge,
    memory_usage_percent: Gauge,
}

impl SystemMetrics {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();

        let cpu_usage = Gauge::with_opts(
            Opts::new("nfsb_cpu_usage_percent", "Current CPU usage percentage")
        ).expect("Failed to create cpu_usage gauge");

        let memory_used_bytes = Gauge::with_opts(
            Opts::new("nfsb_memory_used_bytes", "Memory used in bytes")
        ).expect("Failed to create memory_used_bytes gauge");

        let memory_total_bytes = Gauge::with_opts(
            Opts::new("nfsb_memory_total_bytes", "Total memory in bytes")
        ).expect("Failed to create memory_total_bytes gauge");

        let memory_usage_percent = Gauge::with_opts(
            Opts::new("nfsb_memory_usage_percent", "Memory usage percentage")
        ).expect("Failed to create memory_usage_percent gauge");

        // register with default registry
        prometheus::default_registry()
            .register(Box::new(cpu_usage.clone()))
            .expect("Failed to register cpu_usage");
        prometheus::default_registry()
            .register(Box::new(memory_used_bytes.clone()))
            .expect("Failed to register memory_used_bytes");
        prometheus::default_registry()
            .register(Box::new(memory_total_bytes.clone()))
            .expect("Failed to register memory_total_bytes");
        prometheus::default_registry()
            .register(Box::new(memory_usage_percent.clone()))
            .expect("Failed to register memory_usage_percent");

        debug!("System metrics initialized");

        Self {
            system: Arc::new(RwLock::new(system)),
            cpu_usage,
            memory_used_bytes,
            memory_total_bytes,
            memory_usage_percent,
        }
    }

    /// update all system metrics
    pub async fn refresh(&self) {
        let mut system = self.system.write().await;
        system.refresh_cpu_usage();
        system.refresh_memory();

        // cpu usage (average across all cores)
        let cpu_usage: f64 = system.cpus().iter().map(|c| c.cpu_usage() as f64).sum::<f64>()
            / system.cpus().len().max(1) as f64;
        self.cpu_usage.set(cpu_usage);

        // memory
        let total = system.total_memory();
        let used = system.used_memory();
        let percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        self.memory_total_bytes.set(total as f64);
        self.memory_used_bytes.set(used as f64);
        self.memory_usage_percent.set(percent);
    }

    /// start background task that refreshes metrics every interval
    pub fn start_background_refresh(self: Arc<Self>, interval_secs: u64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                self.refresh().await;
            }
        });
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}
