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

FROM debian:bookworm-slim

# install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# copy built binary
COPY --from=builder /app/target/release/nfsb /usr/local/bin/nfsb

RUN groupadd -g 555 appshare && \
    useradd -u 998 -g 555 -m -s /bin/bash appshare

# create workspace directory
RUN mkdir -p /workspace && chown appshare:appshare /workspace

WORKDIR /workspace

# environment
ENV NFS_TEST_PATH=/mnt/nfs
ENV RUST_BACKTRACE=1

# ports
EXPOSE 9090
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

USER appshare

ENTRYPOINT ["nfsb"]
CMD ["serve", "--port", "8080"]
