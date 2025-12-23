use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use sysinfo::System;
use tracing::{debug, warn};

/// Detected runtime environment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeType {
    /// gVisor sandbox runtime
    GVisor,
    /// Standard runc/containerd runtime
    Runc,
    /// Native/bare metal (no containerization)
    Native,
    /// Unknown runtime
    Unknown,
}

impl fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeType::GVisor => write!(f, "gvisor"),
            RuntimeType::Runc => write!(f, "runc"),
            RuntimeType::Native => write!(f, "native"),
            RuntimeType::Unknown => write!(f, "unknown"),
        }
    }
}

/// Detected storage type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    /// NFS mount
    Nfs,
    /// Ephemeral/local storage
    Ephemeral,
    /// Unknown storage type
    Unknown,
}

impl fmt::Display for StorageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageType::Nfs => write!(f, "nfs"),
            StorageType::Ephemeral => write!(f, "ephemeral"),
            StorageType::Unknown => write!(f, "unknown"),
        }
    }
}

/// System information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_cores: usize,
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
    pub os_name: String,
    pub kernel_version: Option<String>,
}

/// Complete environment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    pub runtime: RuntimeType,
    pub storage_type: StorageType,
    pub mount_point: Option<String>,
    pub filesystem: Option<String>,
    pub system: Option<SystemInfo>,
}

/// Detect the current environment
pub async fn detect_environment(path: &Path) -> Result<EnvironmentInfo> {
    let runtime = detect_runtime().await;
    let (storage_type, mount_point, filesystem) = detect_storage(path).await;
    let system = detect_system_info();

    Ok(EnvironmentInfo {
        runtime,
        storage_type,
        mount_point,
        filesystem,
        system: Some(system),
    })
}

/// Detect if running under gVisor or other container runtime
async fn detect_runtime() -> RuntimeType {
    // Check /proc/version for gVisor signatures
    if let Ok(version) = tokio::fs::read_to_string("/proc/version").await {
        debug!(version = %version, "Read /proc/version");

        if version.contains("gvisor") || version.contains("runsc") {
            return RuntimeType::GVisor;
        }
    }

    // Check for container runtime via cgroup
    if let Ok(cgroup) = tokio::fs::read_to_string("/proc/1/cgroup").await {
        debug!("Checking cgroup for container runtime");

        if cgroup.contains("docker") || cgroup.contains("containerd") || cgroup.contains("kubepods") {
            // Inside a container, but not gVisor
            return RuntimeType::Runc;
        }
    }

    // Check if running in a container by looking for /.dockerenv
    if tokio::fs::metadata("/.dockerenv").await.is_ok() {
        return RuntimeType::Runc;
    }

    // Check for kubernetes service account
    if tokio::fs::metadata("/var/run/secrets/kubernetes.io").await.is_ok() {
        return RuntimeType::Runc;
    }

    // Check /sys/kernel/security/apparmor for gVisor
    if let Ok(profiles) = tokio::fs::read_to_string("/sys/kernel/security/apparmor/profiles").await {
        if profiles.contains("gvisor") {
            return RuntimeType::GVisor;
        }
    }

    // Default to native if no container detected
    RuntimeType::Native
}

/// Detect storage type for the given path
async fn detect_storage(path: &Path) -> (StorageType, Option<String>, Option<String>) {
    // Try to read mount info
    if let Ok(mounts) = tokio::fs::read_to_string("/proc/mounts").await {
        let path_str = path.to_string_lossy();

        // Find the mount point for our path
        let mut best_match: Option<(&str, &str, &str)> = None;
        let mut best_match_len = 0;

        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let mount_point = parts[1];
                let fs_type = parts[2];

                // Check if this mount point is a prefix of our path
                if path_str.starts_with(mount_point) && mount_point.len() > best_match_len {
                    best_match = Some((parts[0], mount_point, fs_type));
                    best_match_len = mount_point.len();
                }
            }
        }

        if let Some((device, mount_point, fs_type)) = best_match {
            debug!(
                device = device,
                mount_point = mount_point,
                fs_type = fs_type,
                "Detected mount"
            );

            let storage_type = match fs_type {
                "nfs" | "nfs4" | "nfs3" => StorageType::Nfs,
                "overlay" | "ext4" | "xfs" | "tmpfs" | "vfat" => StorageType::Ephemeral,
                _ => StorageType::Unknown,
            };

            return (
                storage_type,
                Some(mount_point.to_string()),
                Some(fs_type.to_string()),
            );
        }
    }

    warn!("Could not determine storage type for path");
    (StorageType::Unknown, None, None)
}

/// Collect system information
fn detect_system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    SystemInfo {
        cpu_cores: sys.cpus().len(),
        total_memory_mb: sys.total_memory() / 1024 / 1024,
        available_memory_mb: sys.available_memory() / 1024 / 1024,
        os_name: System::name().unwrap_or_else(|| "unknown".to_string()),
        kernel_version: System::kernel_version(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_detect_environment() {
        let path = PathBuf::from(".");
        let env = detect_environment(&path).await.unwrap();

        // Should detect something
        assert!(env.system.is_some());
    }
}
