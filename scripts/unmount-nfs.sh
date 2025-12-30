#!/bin/bash

# unmount-nfs.sh - unmount NFS share from a droplet
# usage: ./unmount-nfs.sh --mount <mount-point> [--remove-fstab]

set -euo pipefail

MOUNT_POINT=""
REMOVE_FSTAB=false

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Unmount a DigitalOcean NFS share from this droplet.

Required:
    -m, --mount <path>      Mount point to unmount (e.g., /mnt/nfs-0)

Optional:
    -r, --remove-fstab      Remove entry from /etc/fstab
    -h, --help              Show this help message

Examples:
    # unmount only (keeps fstab entry)
    $0 --mount /mnt/nfs-0

    # unmount and remove from fstab
    $0 --mount /mnt/nfs-0 --remove-fstab
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
        -m|--mount)
            MOUNT_POINT="$2"
            shift 2
            ;;
        -r|--remove-fstab)
            REMOVE_FSTAB=true
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
if [[ -z "$MOUNT_POINT" ]]; then
    error "Mount point is required. Use --mount <path>"
fi

# check if mounted
if ! mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
    log "$MOUNT_POINT is not mounted"
else
    log "Unmounting $MOUNT_POINT..."
    sudo umount "$MOUNT_POINT"
    log "Successfully unmounted $MOUNT_POINT"
fi

# remove from fstab if requested
if [[ "$REMOVE_FSTAB" == "true" ]]; then
    if grep -q "$MOUNT_POINT" /etc/fstab; then
        log "Removing $MOUNT_POINT from /etc/fstab..."
        sudo sed -i "\|$MOUNT_POINT|d" /etc/fstab
        log "Removed from fstab"
    else
        log "No entry for $MOUNT_POINT found in /etc/fstab"
    fi
fi

log "Done!"
