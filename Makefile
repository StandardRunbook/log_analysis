.PHONY: bench-parallel bench-compile profile test clean clickhouse-start clickhouse-stop clickhouse-logs test-integration service-start service-stop help

BENCH_EXEC := target/release/benchmark_parallel
CLICKHOUSE_CONTAINER := clickhouse-log-analyzer

# ============================================================================
# Help
# ============================================================================

help:
	@echo "📚 Log Analysis Service - Makefile Commands"
	@echo ""
	@echo "🚀 Quick Start:"
	@echo "  make clickhouse-start    Start ClickHouse (no auth required)"
	@echo "  make service-start       Build and start log-ingest-service"
	@echo "  make test-integration    Run full integration test"
	@echo ""
	@echo "🗄️  ClickHouse:"
	@echo "  make clickhouse-start    Start ClickHouse in Docker"
	@echo "  make clickhouse-stop     Stop ClickHouse"
	@echo "  make clickhouse-logs     View ClickHouse logs"
	@echo "  make clickhouse-clean    Remove ClickHouse container"
	@echo ""
	@echo "🔧 Service:"
	@echo "  make service-build       Build log-ingest-service"
	@echo "  make service-start       Start service (auto-starts ClickHouse)"
	@echo ""
	@echo "🧪 Testing:"
	@echo "  make test                Run unit tests"
	@echo "  make test-integration    Run integration test with ClickHouse"
	@echo ""
	@echo "⚡ Benchmarking:"
	@echo "  make bench-parallel      Run parallel benchmark"
	@echo "  make bench-compile       Compile benchmark for profiling"
	@echo "  make profile             Profile with Instruments"
	@echo ""
	@echo "🧹 Cleanup:"
	@echo "  make clean               Clean build artifacts"
	@echo "  make clean-all           Clean everything including Docker"
	@echo ""
	@echo "📝 Configuration:"
	@echo "  Edit .env file or set environment variables:"
	@echo "  - CLICKHOUSE_URL         (default: http://localhost:8123)"
	@echo "  - LLM_PROVIDER           (openai or ollama)"
	@echo "  - LLM_API_KEY            (required for openai)"
	@echo "  - LLM_MODEL              (e.g., gpt-4, llama3)"
	@echo ""
	@echo "🔐 ClickHouse Authentication (optional):"
	@echo "  For local dev: No authentication needed"
	@echo "  For production: Set CLICKHOUSE_USER and CLICKHOUSE_PASSWORD"
	@echo ""

# ============================================================================
# Benchmarking
# ============================================================================

# Compile and run parallel benchmark
bench-parallel:
	cargo test --release --test benchmarks parallel -- --nocapture

# Just compile the benchmark (for profiling)
bench-compile:
	@cargo test --release --test benchmarks parallel --no-run
	@find target/release/deps -name 'benchmarks-*' -type f -perm +111 -exec cp {} $(BENCH_EXEC) \;
	@echo "Benchmark compiled to: $(BENCH_EXEC)"

# Profile with cpu_cache template
profile: bench-compile
	xcrun xctrace record --template 'CPU Counters' --launch -- $(BENCH_EXEC) parallel --nocapture

# Run all tests
test:
	cargo test --release

# ============================================================================
# ClickHouse Management
# ============================================================================

# Start ClickHouse in Docker
clickhouse-start:
	@echo "🚀 Starting ClickHouse..."
	@if docker ps -a --format '{{.Names}}' | grep -q "^$(CLICKHOUSE_CONTAINER)$$"; then \
		echo "📦 Container exists, starting..."; \
		docker start $(CLICKHOUSE_CONTAINER); \
	else \
		echo "📦 Creating new container (no auth for local dev)..."; \
		docker run -d \
			--name $(CLICKHOUSE_CONTAINER) \
			-p 8123:8123 \
			-p 9000:9000 \
			-v $(PWD)/clickhouse-users.xml:/etc/clickhouse-server/users.d/no-auth.xml \
			--ulimit nofile=262144:262144 \
			clickhouse/clickhouse-server; \
	fi
	@echo "⏳ Waiting for ClickHouse to be ready..."
	@for i in 1 2 3 4 5 6 7 8 9 10; do \
		if curl -s http://localhost:8123/ping > /dev/null 2>&1; then \
			echo "✅ ClickHouse is ready at http://localhost:8123"; \
			exit 0; \
		fi; \
		sleep 1; \
	done; \
	echo "⚠️  ClickHouse may still be starting..."; \

# Stop ClickHouse
clickhouse-stop:
	@echo "🛑 Stopping ClickHouse..."
	@docker stop $(CLICKHOUSE_CONTAINER) || true
	@echo "✅ ClickHouse stopped"

# View ClickHouse logs
clickhouse-logs:
	docker logs -f $(CLICKHOUSE_CONTAINER)

# Remove ClickHouse container and data
clickhouse-clean:
	@echo "🗑️  Removing ClickHouse container..."
	@docker rm -f $(CLICKHOUSE_CONTAINER) || true
	@echo "✅ ClickHouse removed"

# ============================================================================
# Log Ingest Service
# ============================================================================

# Build log-ingest-service
service-build:
	cargo build --release --bin log-ingest-service

# Start log-ingest-service
service-start: clickhouse-start service-build
	@echo "🚀 Starting log-ingest-service..."
	CLICKHOUSE_URL=http://localhost:8123 \
	LLM_PROVIDER=ollama \
	LLM_MODEL=llama3 \
	./target/release/log-ingest-service

# ============================================================================
# Integration Testing
# ============================================================================

# Run integration test with ClickHouse
test-integration: clickhouse-start
	@echo "🧪 Running ClickHouse integration test..."
	@sleep 2
	./test_clickhouse_integration.sh

# ============================================================================
# Cleanup
# ============================================================================

# Clean build artifacts
clean:
	cargo clean
	rm -f $(BENCH_EXEC)
	rm -rf *.trace
	rm -rf benchmark_results/

# Clean everything including Docker containers
clean-all: clean clickhouse-clean
	@echo "✅ All cleaned up"
