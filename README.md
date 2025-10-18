# Log Analyzer API

A high-performance Rust-based REST API service for log analysis with intelligent template matching, automatic template generation via LLM, and Jensen-Shannon Divergence (JSD) anomaly detection.

## Quick Start

### 1. Build and Run
```bash
cargo build --release
cargo run
```

Server starts on `http://127.0.0.1:3000`

### 2. Query Logs
```bash
curl -X POST http://127.0.0.1:3000/query_logs \
  -H "Content-Type: application/json" \
  -d '{
    "metric_name": "cpu_usage",
    "start_time": "2025-01-15T10:00:00Z",
    "end_time": "2025-01-15T10:30:00Z"
  }' | jq .
```

### 3. Try with Ollama (Local LLM)
```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama2

# Set environment variable
export LLM_SERVICE_URL="http://localhost:11434"
export USE_OLLAMA=true

# Run the service
cargo run
```

## Features

- **Three-stage log processing pipeline:**
  1. Metadata service query to identify relevant log streams
  2. Parallel log download from multiple streams
  3. Aho-Corasick + regex-based template matching for performance
  4. LLM-powered automatic template generation for unknown log formats

- **Jensen-Shannon Divergence (JSD) Analysis:**
  - Automatic baseline comparison (3 hours prior to query range)
  - Distribution divergence scoring for anomaly detection
  - Top contributing templates with relative change percentages
  - Representative log samples for each template

- **ClickHouse Integration:**
  - Batch log insertion for high throughput
  - Efficient time-range queries
  - Template distribution analysis

- **High Performance:**
  - Aho-Corasick automaton for multi-pattern matching (replaced radix trie)
  - Async/await with Tokio runtime
  - Parallel log stream downloads
  - Compiled regex pattern caching

## API Endpoint

### POST /query_logs

**Request:**
```json
{
  "metric_name": "cpu_usage",
  "start_time": "2025-01-15T10:00:00Z",
  "end_time": "2025-01-15T10:30:00Z"
}
```

**Response:**
```json
{
  "logs": [
    {
      "timestamp": "2025-01-15T10:00:00+00:00",
      "content": "cpu_usage: 45.2% - Server load normal",
      "stream_id": "stream-001",
      "matched_template": "cpu_usage_1",
      "extracted_values": {
        "percentage": "45.2",
        "message": "Server load normal"
      }
    }
  ],
  "count": 14,
  "matched_logs": 14,
  "unmatched_logs": 0,
  "new_templates_generated": 0,
  "jsd_analysis": {
    "jsd_score": 0.111,
    "baseline_period": "2025-01-15 07:00:00 UTC to 2025-01-15 10:00:00 UTC",
    "current_period": "2025-01-15 10:00:00 UTC to 2025-01-15 10:30:00 UTC",
    "baseline_log_count": 2,
    "current_log_count": 14,
    "top_contributors": [
      {
        "template_id": "disk_io_1",
        "baseline_probability": 0.0,
        "current_probability": 0.143,
        "contribution": 0.050,
        "relative_change": 100.0,
        "representative_logs": [
          "disk_io: 250MB/s - Disk activity moderate"
        ]
      }
    ]
  }
}
```

### POST /grafana/search (Grafana Integration)

Returns available metrics for Grafana dropdown.

**Request:**
```json
{
  "target": ""
}
```

**Response:**
```json
["cpu_usage", "memory_usage", "disk_io"]
```

### POST /grafana/query (Grafana Query)

Query logs for Grafana visualization.

**Request:**
```json
{
  "targets": [
    {
      "target": "cpu_usage",
      "refId": "A"
    }
  ],
  "range": {
    "from": "2025-01-15T10:00:00Z",
    "to": "2025-01-15T10:30:00Z"
  }
}
```

## Architecture

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │ POST /query_logs
       │
┌──────▼──────────────────────────────────────────────────┐
│              Log Analyzer API                            │
│                                                           │
│  1. Query Metadata Service                               │
│     └─→ Get log streams for metric                      │
│                                                           │
│  2. Download Logs in Parallel                            │
│     └─→ Fetch from all streams concurrently             │
│                                                           │
│  3. Match with Aho-Corasick + Regex                     │
│     └─→ Fast multi-pattern matching                     │
│                                                           │
│  4. Generate Templates via LLM (if no match)            │
│     └─→ Create and cache new templates                  │
│                                                           │
│  5. JSD Analysis                                         │
│     └─→ Compare baseline vs current distributions       │
│                                                           │
│  6. Store in ClickHouse (batch)                          │
│     └─→ Efficient columnar storage                      │
└──────────────────────────────────────────────────────────┘
```

## Project Structure

```
src/
├── main.rs                  # API server and routing
├── metadata_service.rs      # Discover log streams
├── log_stream_client.rs     # Download logs from streams
├── log_matcher.rs           # Aho-Corasick + regex matching
├── llm_service.rs           # LLM template generation
├── histogram.rs             # Template frequency tracking
├── jsd.rs                   # JSD calculation and analysis
└── clickhouse.rs            # ClickHouse integration
```

## Configuration

### Environment Variables

```bash
# Service endpoints
export METADATA_SERVICE_URL="http://metadata-service:8080"
export LLM_SERVICE_URL="http://llm-service:8081"

# LLM configuration
export USE_OLLAMA=true                    # Use Ollama instead of external LLM
export OLLAMA_MODEL="llama2"              # Model to use
export OLLAMA_ENDPOINT="http://localhost:11434"

# ClickHouse configuration
export CLICKHOUSE_URL="http://localhost:8123"
export CLICKHOUSE_USER="default"
export CLICKHOUSE_PASSWORD=""
export CLICKHOUSE_DATABASE="logs"

# Server configuration
export PORT=3000
export RUST_LOG=info                      # Log level: debug, info, warn, error
```

### ClickHouse Setup

```sql
CREATE TABLE logs (
    timestamp DateTime64(3),
    content String,
    stream_id String,
    template_id Nullable(String),
    extracted_values String
) ENGINE = MergeTree()
ORDER BY (template_id, timestamp);
```

## Batch Processing

Process large volumes of logs efficiently:

```bash
# Process logs in batches
cargo run --bin batch_processor -- \
  --metric cpu_usage \
  --start "2025-01-01T00:00:00Z" \
  --end "2025-01-31T23:59:59Z" \
  --batch-size 10000
```

**Features:**
- Configurable batch size for memory management
- Progress tracking and resumption
- Parallel template matching
- Automatic template caching
- ClickHouse bulk insertion

## Performance Optimizations

### Recent Improvements

1. **Aho-Corasick Automaton** (replaced radix trie)
   - Multi-pattern matching in single pass
   - O(n) time complexity for n-character input
   - 10x faster for multiple templates

2. **Batch Processing**
   - Process millions of logs without memory issues
   - Configurable batch sizes
   - Parallel processing within batches

3. **ClickHouse Integration**
   - Columnar storage for analytics
   - Fast time-range queries
   - Efficient aggregations

4. **Compiled Regex Caching**
   - Pre-compile all regex patterns
   - Reuse across requests
   - Significant CPU savings

### Scaling to Millions of Logs

The system has been optimized to handle:
- 10M+ logs per batch
- 100+ concurrent requests
- 1000+ unique templates
- Sub-second JSD analysis

See performance benchmarks in `tests/performance_tests.rs`

## Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test log_matcher
cargo test jsd
cargo test batch_processing

# Run with output
cargo test -- --nocapture

# Run performance tests
cargo test --release performance
```

## Production Deployment

### 1. Replace Mock Services

**Metadata Service** (`src/metadata_service.rs`):
```rust
// Uncomment query_api() method
// Configure your metadata service endpoint
```

**Log Storage** (`src/log_stream_client.rs`):
```rust
// Uncomment query_log_storage() method
// Integrate with CloudWatch/Splunk/Elasticsearch
```

**LLM Service** (`src/llm_service.rs`):
```rust
// Use Ollama (local) or configure OpenAI/Anthropic
```

### 2. Docker Deployment

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/log_analyzer /usr/local/bin/
EXPOSE 3000
CMD ["log_analyzer"]
```

```bash
docker build -t log-analyzer .
docker run -p 3000:3000 \
  -e METADATA_SERVICE_URL=http://metadata:8080 \
  -e CLICKHOUSE_URL=http://clickhouse:8123 \
  log-analyzer
```

### 3. Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: log-analyzer
spec:
  replicas: 3
  selector:
    matchLabels:
      app: log-analyzer
  template:
    metadata:
      labels:
        app: log-analyzer
    spec:
      containers:
      - name: log-analyzer
        image: log-analyzer:latest
        ports:
        - containerPort: 3000
        env:
        - name: CLICKHOUSE_URL
          value: "http://clickhouse:8123"
        - name: USE_OLLAMA
          value: "true"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
```

## Monitoring

### Metrics to Track

- Requests per second
- Average response time
- Template match rate
- LLM invocation frequency
- JSD score distribution
- ClickHouse write throughput
- Error rate by service

### Logging

```bash
# Debug level for development
RUST_LOG=debug cargo run

# Info level for production
RUST_LOG=info cargo run

# Structured JSON logs
RUST_LOG=info RUST_LOG_FORMAT=json cargo run
```

## Dependencies

- **axum** - Web framework
- **tokio** - Async runtime
- **serde** / **serde_json** - Serialization
- **chrono** - Date/time handling
- **reqwest** - HTTP client
- **aho-corasick** - Multi-pattern matching
- **regex** - Pattern matching
- **clickhouse** - ClickHouse client
- **tracing** - Logging
- **anyhow** - Error handling

## Available Sample Metrics

The mock implementation includes:
- `cpu_usage` - CPU utilization (2 streams)
- `memory_usage` - Memory consumption (1 stream)
- `disk_io` - Disk I/O performance (1 stream)

## Troubleshooting

### Port Already in Use
```bash
PORT=8080 cargo run
```

### Build Errors
```bash
rustup update
cargo clean
cargo build
```

### ClickHouse Connection Failed
```bash
# Verify ClickHouse is running
docker ps | grep clickhouse

# Test connection
curl http://localhost:8123/ping
```

### Ollama Not Responding
```bash
# Check if Ollama is running
ollama list

# Start Ollama service
ollama serve
```

## License

MIT

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request
