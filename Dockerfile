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
    nfs-common \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/nfsb /usr/local/bin/nfsb

# Create directory for benchmark data
RUN mkdir -p /data

WORKDIR /data

EXPOSE 9090

ENTRYPOINT ["nfsb"]
CMD ["--help"]
