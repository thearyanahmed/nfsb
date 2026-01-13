FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy src to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source
COPY src ./src

# Build release binary
RUN touch src/main.rs && cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    nfs-common \
    && rm -rf /var/lib/apt/lists/*

# create non-root user
RUN groupadd --gid 1000 nfsb && \
    useradd --uid 1000 --gid 1000 --shell /bin/bash --create-home nfsb

COPY --from=builder /app/target/release/nfsb /usr/local/bin/nfsb

# create directories and set ownership
RUN mkdir -p /workspace && \
    chown -R nfsb:nfsb /workspace

WORKDIR /workspace

# switch to non-root user
USER nfsb

# prometheus metrics port
EXPOSE 9090
# REST API port
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

ENTRYPOINT ["nfsb"]
CMD ["serve", "--port", "8080"]
