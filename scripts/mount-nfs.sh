#!/bin/bash

# mount-nfs.sh - mount DigitalOcean NFS share on a droplet
# usage: ./mount-nfs.sh --server <ip> --share <share-path> --mount <mount-point> [--persistent]

set -euo pipefail

# defaults
NFS_SERVER=""
SHARE_PATH=""
MOUNT_POINT="/mnt/nfs-0"
PERSISTENT=false
INSTALL_DEPS=false

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Mount a DigitalOcean NFS share on this droplet.

Required:
    -s, --server <ip>       NFS server IP (e.g., 10.100.0.2)
    -S, --share <path>      NFS share path (e.g., /30733493/8f661f98-bd24-4144-bbf6-2cd373edd81d)

Optional:
    -m, --mount <path>      Local mount point (default: /mnt/nfs-0)
    -p, --persistent        Add to fstab for persistent mount across reboots
    -i, --install           Install nfs-common if not present
    -h, --help              Show this help message

Examples:
    # session mount (won't persist after reboot)
    $0 --server 10.100.0.2 --share /30733493/8f661f98-bd24-4144-bbf6-2cd373edd81d

    # persistent mount (survives reboot)
    $0 --server 10.100.0.2 --share /30733493/8f661f98-bd24-4144-bbf6-2cd373edd81d --persistent

    # with custom mount point and install deps
    $0 --server 10.100.0.2 --share /30733493/8f661f98-bd24-4144-bbf6-2cd373edd81d -m /mnt/mydata --install
EOF
    exit 1
}

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

error() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] ERROR: $*" >&2
    exit 1
}

# parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -s|--server)
            NFS_SERVER="$2"
            shift 2
            ;;
        -S|--share)
            SHARE_PATH="$2"
            shift 2
            ;;
        -m|--mount)
            MOUNT_POINT="$2"
            shift 2
            ;;
        -p|--persistent)
            PERSISTENT=true
            shift
            ;;
        -i|--install)
            INSTALL_DEPS=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# validate required args
if [[ -z "$NFS_SERVER" ]]; then
    error "NFS server IP is required. Use --server <ip>"
fi

if [[ -z "$SHARE_PATH" ]]; then
    error "NFS share path is required. Use --share <path>"
fi

# install nfs-common if requested
if [[ "$INSTALL_DEPS" == "true" ]]; then
    log "Updating apt and installing nfs-common..."
    sudo apt update && sudo apt install -y nfs-common
fi

# check if nfs client is available
if ! dpkg -l | grep -q nfs-common; then
    error "nfs-common not installed. Run with --install or: sudo apt update && sudo apt install -y nfs-common"
fi

# create mount point
log "Creating mount target: $MOUNT_POINT"
sudo mkdir -p "$MOUNT_POINT"

# check if already mounted
if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
    log "Warning: $MOUNT_POINT is already mounted"
    mount | grep "$MOUNT_POINT"
    exit 0
fi

# construct NFS source
NFS_SOURCE="${NFS_SERVER}:${SHARE_PATH}"

log "Mounting NFS share..."
log "  Source: $NFS_SOURCE"
log "  Target: $MOUNT_POINT"

# mount the NFS share
sudo mount -t nfs "$NFS_SOURCE" "$MOUNT_POINT"

# verify mount
if mountpoint -q "$MOUNT_POINT"; then
    log "Successfully mounted $NFS_SOURCE to $MOUNT_POINT"
else
    error "Mount verification failed"
fi

# add to fstab for persistent mount
if [[ "$PERSISTENT" == "true" ]]; then
    FSTAB_ENTRY="$NFS_SOURCE $MOUNT_POINT nfs _netdev,nofail,x-systemd.automount,x-systemd.idle-timeout=600,vers=4.2 0 0"

    # check if already in fstab
    if grep -q "$NFS_SOURCE" /etc/fstab; then
        log "Entry already exists in /etc/fstab"
    else
        log "Adding persistent mount to /etc/fstab..."
        echo "$FSTAB_ENTRY" | sudo tee -a /etc/fstab
        log "Added to fstab. Mount will persist across reboots."
    fi
fi

# show mount info
log "Mount details:"
mount | grep "$MOUNT_POINT"

# test write access
if touch "$MOUNT_POINT/.nfs_test" 2>/dev/null; then
    rm -f "$MOUNT_POINT/.nfs_test"
    log "Write access: OK"
else
    log "Write access: FAILED (read-only or permission denied)"
fi

log "Done!"
