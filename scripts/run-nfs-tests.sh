#!/bin/bash
# run NFS permission tests
#
# usage:
#   ./scripts/run-nfs-tests.sh              # run as current user
#   ./scripts/run-nfs-tests.sh --as-root    # run as root (if current user is nfsb)
#   ./scripts/run-nfs-tests.sh --as-nfsb    # run as nfsb user (uid=999)
#   ./scripts/run-nfs-tests.sh --both       # run as both users

set -e

# colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # no color

# default NFS path
NFS_PATH="${NFS_TEST_PATH:-/mnt/nfs}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  NFS Permission Tests (APPS-13266)${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo -e "NFS Path: ${YELLOW}${NFS_PATH}${NC}"
echo ""

# check if NFS path exists
if [ ! -d "$NFS_PATH" ]; then
    echo -e "${RED}ERROR: NFS path does not exist: ${NFS_PATH}${NC}"
    echo "Set NFS_TEST_PATH environment variable to your NFS mount point"
    exit 1
fi

run_tests() {
    local user=$1
    local label=$2

    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  Running tests as: ${YELLOW}${label}${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo ""

    if [ "$user" = "root" ]; then
        cargo test --test nfs_permissions -- --nocapture --test-threads=1 2>&1
    else
        # run as nfsb user
        if [ "$(id -u)" = "0" ]; then
            # we are root, switch to nfsb
            su nfsb -c "cd /app && CARGO_HOME=/home/nfsb/.cargo NFS_TEST_PATH=${NFS_PATH} cargo test --test nfs_permissions -- --nocapture --test-threads=1" 2>&1
        else
            # already non-root
            cargo test --test nfs_permissions -- --nocapture --test-threads=1 2>&1
        fi
    fi
}

case "${1:-}" in
    --as-root)
        if [ "$(id -u)" != "0" ]; then
            echo -e "${RED}ERROR: Not running as root${NC}"
            exit 1
        fi
        run_tests "root" "ROOT (uid=0)"
        ;;
    --as-nfsb)
        run_tests "nfsb" "NFSB (uid=999)"
        ;;
    --both)
        if [ "$(id -u)" != "0" ]; then
            echo -e "${RED}ERROR: Must be root to run --both${NC}"
            exit 1
        fi
        run_tests "root" "ROOT (uid=0)"
        echo ""
        echo -e "${GREEN}========================================${NC}"
        echo ""
        run_tests "nfsb" "NFSB (uid=999)"
        ;;
    *)
        # run as current user
        current_uid=$(id -u)
        if [ "$current_uid" = "0" ]; then
            run_tests "root" "ROOT (uid=0)"
        else
            run_tests "nfsb" "NON-ROOT (uid=${current_uid})"
        fi
        ;;
esac

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Tests completed${NC}"
echo -e "${GREEN}========================================${NC}"
