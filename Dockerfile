# Multi-stage build for optimized Rust application
FROM rust:1.82-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty project
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy hover-schema submodule (needed for schema include)
COPY hover-schema ./hover-schema

# Copy the actual source code (we need all modules for compilation)
COPY src ./src

# Build the application
RUN cargo build --release --bin log-ingest-service

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 appuser

# Copy the binary from builder
COPY --from=builder /app/target/release/log-ingest-service /usr/local/bin/log-ingest-service

# Set ownership
RUN chown appuser:appuser /usr/local/bin/log-ingest-service

# Switch to non-root user
USER appuser

# Expose the service port
EXPOSE 3002

# Set environment variables
ENV RUST_LOG=info \
    INGEST_PORT=3002 \
    CLICKHOUSE_URL=http://localhost:8123

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/log-ingest-service", "--version"] || exit 1

# Run the binary
ENTRYPOINT ["/usr/local/bin/log-ingest-service"]
