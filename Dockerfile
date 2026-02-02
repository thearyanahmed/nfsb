# Multi-stage build for nfsb with test support
# Test users for NFS group permission testing:
#   appnfs  (uid=1000, gid=1000) - default user
#   appnfs2 (uid=1001, gid=1000) - same group as appnfs
#   appother (uid=2000, gid=1234) - different group

FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

# install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# copy manifests
COPY Cargo.toml Cargo.lock ./

# create dummy src to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# copy actual source
COPY src ./src

# build release binary
RUN touch src/main.rs && cargo build --release

# ============================================================================
# Runtime image with test support
# ============================================================================
FROM rust:1.83-slim-bookworm

# install runtime dependencies and test tools
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    nfs-common \
    procps \
    gosu \
    && rm -rf /var/lib/apt/lists/*

# copy built binary
COPY --from=builder /app/target/release/nfsb /usr/local/bin/nfsb

# create workspace directory with open permissions for testing
RUN mkdir -p /workspace && chmod 777 /workspace

# ============================================================================
# Create test users for NFS group permission testing
# ============================================================================
# group 1000: shared group for NFS (matches /data/gid1000 export anongid=1000)
# group 1234: different group (should NOT have access to gid1000 export)
RUN groupadd -g 1000 appnfs && \
    groupadd -g 1234 othergroup && \
    useradd -u 1000 -g 1000 -m -s /bin/bash appnfs && \
    useradd -u 1001 -g 1000 -m -s /bin/bash appnfs2 && \
    useradd -u 2000 -g 1234 -m -s /bin/bash appother

# create cargo home directories
RUN mkdir -p /root/.cargo /home/appnfs/.cargo /home/appnfs2/.cargo /home/appother/.cargo && \
    chown -R appnfs:appnfs /home/appnfs && \
    chown -R appnfs2:appnfs /home/appnfs2 && \
    chown -R appother:othergroup /home/appother

# ============================================================================
# Copy source code for running tests inside container
# ============================================================================
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests

# set permissions so both root and appnfs can build/test
RUN chmod -R 777 /app

# pre-compile dependencies for faster test runs (as root)
RUN cargo fetch

# pre-compile test dependencies (creates target dir with proper perms)
RUN cargo build --tests 2>/dev/null || true && \
    chmod -R 777 /app/target 2>/dev/null || true

# ============================================================================
# Environment setup
# ============================================================================

# default NFS test path (override with NFS_TEST_PATH env var)
ENV NFS_TEST_PATH=/mnt/nfs
ENV CARGO_HOME=/root/.cargo
ENV RUST_BACKTRACE=1

# prometheus metrics port
EXPOSE 9090
# REST API port
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# ============================================================================
# Run as root to allow user switching with gosu for testing
# Usage: gosu appnfs <cmd> | gosu appnfs2 <cmd> | gosu appother <cmd>
# ============================================================================
ENV CARGO_HOME=/root/.cargo

ENTRYPOINT ["nfsb"]
CMD ["serve", "--port", "8080"]
