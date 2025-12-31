use prometheus::{Gauge, GaugeVec, Opts};
use sysinfo::System;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::debug;

/// cgroup CPU stats for container environments
struct CgroupCpuStats {
    last_usage_usec: u64,
    last_time: Instant,
}

/// System metrics collector for CPU and memory
/// uses cgroup stats in containers (gVisor), falls back to sysinfo
pub struct SystemMetrics {
    system: Arc<RwLock<System>>,
    cgroup_stats: Arc<RwLock<Option<CgroupCpuStats>>>,
    cpu_usage: Gauge,
    memory_used_bytes: Gauge,
    memory_total_bytes: Gauge,
    memory_usage_percent: Gauge,
    mount_info: GaugeVec,
    known_mounts: Arc<RwLock<HashSet<String>>>,
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

        let mount_info = GaugeVec::new(
            Opts::new("nfsb_mount_info", "Mounted filesystems (value=1 indicates mounted)"),
            &["source", "target", "fstype"]
        ).expect("Failed to create mount_info gauge");

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
        prometheus::default_registry()
            .register(Box::new(mount_info.clone()))
            .expect("Failed to register mount_info");

        debug!("System metrics initialized");

        Self {
            system: Arc::new(RwLock::new(system)),
            cgroup_stats: Arc::new(RwLock::new(None)),
            cpu_usage,
            memory_used_bytes,
            memory_total_bytes,
            memory_usage_percent,
            mount_info,
            known_mounts: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// try to read CPU usage from cgroup v2 stats (works in containers/gVisor)
    fn read_cgroup_cpu_usec() -> Option<u64> {
        // cgroup v2 path
        let content = std::fs::read_to_string("/sys/fs/cgroup/cpu.stat").ok()?;
        for line in content.lines() {
            if line.starts_with("usage_usec") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse().ok();
                }
            }
        }
        None
    }

    /// calculate CPU percentage from cgroup stats
    async fn get_cgroup_cpu_percent(&self, num_cpus: usize) -> Option<f64> {
        let current_usec = Self::read_cgroup_cpu_usec()?;
        let now = Instant::now();

        let mut stats = self.cgroup_stats.write().await;

        if let Some(ref prev) = *stats {
            let delta_usec = current_usec.saturating_sub(prev.last_usage_usec);
            let delta_time = now.duration_since(prev.last_time);
            let delta_time_usec = delta_time.as_micros() as u64;

            if delta_time_usec > 0 {
                // CPU usage = (cpu_time_used / wall_time) * 100 / num_cpus
                let percent = (delta_usec as f64 / delta_time_usec as f64) * 100.0 / num_cpus.max(1) as f64;

                // update for next calculation
                *stats = Some(CgroupCpuStats {
                    last_usage_usec: current_usec,
                    last_time: now,
                });

                return Some(percent.min(100.0));
            }
        }

        // first reading - just store for next time
        *stats = Some(CgroupCpuStats {
            last_usage_usec: current_usec,
            last_time: now,
        });

        None
    }

    /// refresh mount information from /proc/mounts
    /// only tracks relevant mounts: nfs, /mnt/*, /data/*, /tmp
    async fn refresh_mounts(&self) {
        let contents = match std::fs::read_to_string("/proc/mounts") {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut current_mounts = HashSet::new();

        for line in contents.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let source = parts[0];
                let target = parts[1];
                let fstype = parts[2];

                // filter to relevant mounts
                let is_relevant = fstype.starts_with("nfs")
                    || target.starts_with("/mnt")
                    || target.starts_with("/data")
                    || target == "/tmp";

                if is_relevant {
                    let key = format!("{}:{}:{}", source, target, fstype);
                    current_mounts.insert(key.clone());

                    self.mount_info
                        .with_label_values(&[source, target, fstype])
                        .set(1.0);
                }
            }
        }

        // remove stale mounts that no longer exist
        let mut known = self.known_mounts.write().await;
        for old_key in known.difference(&current_mounts) {
            let parts: Vec<&str> = old_key.split(':').collect();
            if parts.len() == 3 {
                if let Err(_) = self.mount_info.remove_label_values(&[parts[0], parts[1], parts[2]]) {
                    // ignore removal errors
                }
            }
        }
        *known = current_mounts;
    }

    /// update all system metrics
    pub async fn refresh(&self) {
        let mut system = self.system.write().await;
        system.refresh_cpu_usage();
        system.refresh_memory();

        // refresh mounts (drop the system lock first)
        drop(system);
        self.refresh_mounts().await;
        let system = self.system.read().await;

        let num_cpus = system.cpus().len().max(1);

        // try cgroup CPU first (works in gVisor), fall back to sysinfo
        let cpu_usage = if let Some(cgroup_cpu) = self.get_cgroup_cpu_percent(num_cpus).await {
            cgroup_cpu
        } else {
            // sysinfo fallback (works on native Linux/macOS)
            system.cpus().iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / num_cpus as f64
        };

        self.cpu_usage.set(cpu_usage);

        // memory (sysinfo works fine in containers)
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
            // prime the cgroup stats with an initial reading
            self.refresh().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            self.refresh().await;

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
