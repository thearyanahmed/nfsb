.PHONY: build build-release run serve info test clean fmt lint help

# default target
all: build

# build debug binary
build:
	cargo build

# build optimized release binary
build-release:
	cargo build --release

# run benchmarks (requires PATH variable)
# usage: make run PATH=/mnt/nfs
run:
	./target/release/nfsb run --path $(PATH)

# run specific benchmark
# usage: make run-benchmark PATH=/mnt/nfs BENCHMARK=sequential
run-benchmark:
	./target/release/nfsb run --path $(PATH) --benchmark $(BENCHMARK)

# start REST API server (default port 8080)
serve:
	./target/release/nfsb serve --port 8080

# start REST API server on custom port
# usage: make serve-port PORT=3000
serve-port:
	./target/release/nfsb serve --port $(PORT)

# show environment info
# usage: make info PATH=/mnt/nfs
info:
	./target/release/nfsb info --path $(PATH)

# run tests
test:
	cargo test

# clean build artifacts
clean:
	cargo clean

# format code
fmt:
	cargo fmt

# lint code
lint:
	cargo clippy

# check code without building
check:
	cargo check

# build and run server in one command
dev: build-release serve

# show help
help:
	@echo "nfsb - NFS Benchmark Tool"
	@echo ""
	@echo "Build commands:"
	@echo "  make build          - Build debug binary"
	@echo "  make build-release  - Build optimized release binary"
	@echo "  make clean          - Clean build artifacts"
	@echo ""
	@echo "Run commands:"
	@echo "  make run PATH=/mnt/nfs              - Run all benchmarks"
	@echo "  make run-benchmark PATH=/mnt/nfs BENCHMARK=sequential"
	@echo "                                      - Run specific benchmark"
	@echo "  make info PATH=/mnt/nfs             - Show environment info"
	@echo ""
	@echo "Server commands:"
	@echo "  make serve                          - Start REST API on port 8080"
	@echo "  make serve-port PORT=3000           - Start REST API on custom port"
	@echo "  make dev                            - Build and start server"
	@echo ""
	@echo "Development commands:"
	@echo "  make test           - Run tests"
	@echo "  make fmt            - Format code"
	@echo "  make lint           - Lint code"
	@echo "  make check          - Check code without building"
	@echo ""
	@echo "REST API endpoints (when server is running):"
	@echo "  POST   /api/v1/benchmarks/run       - Start a benchmark"
	@echo "  GET    /api/v1/benchmarks/:id/status - Get job status"
	@echo "  GET    /api/v1/benchmarks/:id/results - Get job results"
	@echo "  GET    /api/v1/jobs                 - List all jobs"
	@echo "  GET    /api/v1/info?path=<path>     - Get environment info"
	@echo "  GET    /health                      - Health check"
