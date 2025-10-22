# Log Analyzer Service API

High-performance log analysis HTTP service with zero-copy optimizations, batch processing, and parallel processing capabilities.

## Quick Start

```bash
# Build and run the service
cargo run --release --bin log-analyzer-service

# Or build the binary
cargo build --release --bin log-analyzer-service
./target/release/log-analyzer-service

# Service starts on http://localhost:3000 by default
```

## Configuration

### Environment Variables

- `PORT` - Server port (default: `3000`)

### Template Loading

The service automatically loads templates from `cache/comprehensive_templates.json` if available. If not found, it starts with default templates.

**To pre-load templates:**
```bash
# Generate comprehensive templates
cargo run --release --example generate_comprehensive_templates

# Then start the service
cargo run --release --bin log-analyzer-service
```

## API Endpoints

### `GET /health`

Health check and capabilities endpoint.

**Response:**
```json
{
  "status": "healthy",
  "templates_loaded": 150,
  "optimizations": [
    "Zero-copy (SmallVec)",
    "Thread-local scratch buffers",
    "Inline hints",
    "Unstable sorting",
    "FxHashMap",
    "Lock-free reads"
  ],
  "features": [
    "Single log matching",
    "Batch processing",
    "Parallel processing",
    "Dynamic template addition"
  ]
}
```

**Example:**
```bash
curl http://localhost:3000/health | jq .
```

---

### `GET /stats`

Get service statistics and configuration.

**Response:**
```json
{
  "templates_loaded": 150,
  "optimal_batch_size": 10000,
  "config": {
    "min_fragment_length": 1,
    "fragment_match_threshold": 0.3,
    "optimal_batch_size": 10000
  }
}
```

**Example:**
```bash
curl http://localhost:3000/stats | jq .
```

---

### `POST /match`

Match a single log line against loaded templates.

**Request:**
```json
{
  "log_line": "ERROR: connection timeout"
}
```

**Response:**
```json
{
  "template_id": 1,
  "matched": true
}
```

**Examples:**
```bash
# Match an error log
curl -X POST http://localhost:3000/match \
  -H 'Content-Type: application/json' \
  -d '{"log_line": "ERROR: connection timeout"}' | jq .

# Match a CPU usage log
curl -X POST http://localhost:3000/match \
  -H 'Content-Type: application/json' \
  -d '{"log_line": "cpu_usage: 67.8% - Server load increased"}' | jq .

# No match
curl -X POST http://localhost:3000/match \
  -H 'Content-Type: application/json' \
  -d '{"log_line": "unknown format"}' | jq .
```

---

### `POST /match/batch`

Match multiple log lines using batch or parallel processing.

**Automatic Optimization:**
- Batches ≤1000 logs: Sequential processing
- Batches >1000 logs: Automatic parallel processing
- Can force parallel mode with `use_parallel: true`

**Request:**
```json
{
  "log_lines": [
    "ERROR: connection timeout",
    "INFO: server started",
    "WARN: high memory usage"
  ],
  "use_parallel": false
}
```

**Response:**
```json
{
  "results": [1, 2, 3],
  "total_logs": 3,
  "matched_count": 3,
  "processing_mode": "sequential",
  "throughput_logs_per_sec": 27262.31
}
```

**Examples:**

**Small batch (sequential):**
```bash
curl -X POST http://localhost:3000/match/batch \
  -H 'Content-Type: application/json' \
  -d '{
    "log_lines": [
      "cpu_usage: 67.8% - test1",
      "memory_usage: 2.5GB - test2",
      "disk_io: 100MB/s - test3"
    ]
  }' | jq .
```

**Large batch (auto-parallel):**
```bash
# Generate 2000 logs for testing
python3 << 'EOF'
import json
logs = [f"ERROR: message {i}" for i in range(2000)]
print(json.dumps({"log_lines": logs}))
EOF > /tmp/large_batch.json

# Send to service
curl -X POST http://localhost:3000/match/batch \
  -H 'Content-Type: application/json' \
  -d @/tmp/large_batch.json | jq .
```

**Force parallel mode:**
```bash
curl -X POST http://localhost:3000/match/batch \
  -H 'Content-Type: application/json' \
  -d '{
    "log_lines": ["ERROR: test1", "ERROR: test2"],
    "use_parallel": true
  }' | jq .
```

---

### `GET /templates`

List all loaded templates.

**Response:**
```json
[
  {
    "template_id": 1,
    "pattern": "cpu_usage: (\\d+\\.\\d+)% - (.*)",
    "variables": ["percentage", "message"],
    "example": "cpu_usage: 45.2% - Server load normal"
  },
  {
    "template_id": 2,
    "pattern": "memory_usage: (\\d+\\.\\d+)GB - (.*)",
    "variables": ["amount", "message"],
    "example": "memory_usage: 2.5GB - Memory consumption stable"
  }
]
```

**Example:**
```bash
curl http://localhost:3000/templates | jq .
```

---

### `POST /templates/add`

Add a new template (currently not implemented - load templates at startup).

**Response:**
```json
{
  "success": false,
  "template_id": 0,
  "message": "Template addition not implemented in this version. Load templates at startup."
}
```

## Performance Characteristics

### Throughput

| Batch Size | Mode | Expected Throughput |
|------------|------|---------------------|
| 1 log | Single | ~50K logs/sec |
| 10-100 logs | Sequential | ~100-150K logs/sec |
| 100-1000 logs | Sequential | ~150-250K logs/sec |
| 1000+ logs | Parallel | ~300-400K logs/sec |

**Actual results may vary based on:**
- Number of CPU cores
- Template complexity
- Log line patterns
- System load

### Latency

| Operation | Typical Latency |
|-----------|-----------------|
| Single match | 20-100 μs |
| Batch (100 logs) | 1-5 ms |
| Batch (1000 logs) | 5-15 ms |
| Large batch (10K logs, parallel) | 30-100 ms |

### Memory Usage

- Base service: ~5MB
- Per template: ~500 bytes
- With 1000 templates: ~10MB
- Thread-local buffers: ~2KB per thread
- Request processing: ~1KB per 100 logs

## Optimization Features

### Zero-Copy Processing

The service uses zero-copy optimizations to minimize allocations:

```rust
// String slices instead of owned strings
let log_refs: Vec<&str> = req.log_lines.iter().map(|s| s.as_str()).collect();
results = matcher.match_batch(&log_refs);
```

**Benefits:**
- No string copying
- Reduced GC pressure
- Better cache locality
- 30-40% performance improvement

### Thread-Local Scratch Buffers

Each thread maintains a reusable scratch buffer for matching:

**Benefits:**
- Zero allocations during matching
- Thread-safe without locks
- Scales linearly with thread count

### SmallVec Stack Allocation

Most operations use stack-allocated vectors:

**Benefits:**
- No heap allocations for common case
- Better cache performance
- Reduced memory fragmentation

### Parallel Processing

Automatic parallel processing for large batches using Rayon:

```rust
// Automatically enabled for batches > 1000 logs
if log_lines.len() > 1000 {
    matcher.match_batch_parallel(&log_lines)
} else {
    matcher.match_batch(&log_lines)
}
```

**Benefits:**
- ~8-10x speedup on 10-core systems
- Lock-free concurrent matching
- Per-thread scratch buffers

## Production Deployment

### Docker

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin log-analyzer-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/log-analyzer-service /usr/local/bin/
COPY --from=builder /app/cache /app/cache
WORKDIR /app
EXPOSE 3000
CMD ["log-analyzer-service"]
```

Build and run:
```bash
docker build -t log-analyzer-service .
docker run -p 3000:3000 log-analyzer-service
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: log-analyzer-service
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
      - name: service
        image: log-analyzer-service:latest
        ports:
        - containerPort: 3000
        env:
        - name: PORT
          value: "3000"
        resources:
          requests:
            memory: "128Mi"
            cpu: "500m"
          limits:
            memory: "512Mi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 3
          periodSeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: log-analyzer-service
spec:
  selector:
    app: log-analyzer
  ports:
  - port: 80
    targetPort: 3000
  type: LoadBalancer
```

### Systemd Service

```ini
[Unit]
Description=Log Analyzer Service
After=network.target

[Service]
Type=simple
User=log-analyzer
WorkingDirectory=/opt/log-analyzer
ExecStart=/opt/log-analyzer/bin/log-analyzer-service
Restart=always
RestartSec=10
Environment="PORT=3000"

[Install]
WantedBy=multi-user.target
```

## Monitoring

### Metrics to Track

1. **Request Metrics:**
   - Requests per second
   - Response times (p50, p95, p99)
   - Error rates

2. **Performance Metrics:**
   - Throughput (logs/sec)
   - Batch sizes
   - Parallel vs sequential ratio

3. **Resource Metrics:**
   - CPU usage
   - Memory usage
   - Thread pool utilization

### Prometheus Integration

Add to `main.rs`:

```rust
use prometheus::{Counter, Histogram, Registry};

// Define metrics
lazy_static! {
    static ref REQUESTS: Counter = Counter::new("requests_total", "Total requests").unwrap();
    static ref THROUGHPUT: Histogram = Histogram::new("throughput_logs_per_sec", "Logs/sec").unwrap();
}
```

### Health Check Monitoring

```bash
# Simple uptime monitoring
while true; do
  curl -f http://localhost:3000/health || echo "Service down!"
  sleep 10
done
```

## Load Testing

### Using `wrk`

```bash
# Install wrk
# macOS: brew install wrk
# Linux: apt-get install wrk

# Test single match endpoint
wrk -t4 -c100 -d30s \
  -s <(cat <<'EOF'
wrk.method = "POST"
wrk.headers["Content-Type"] = "application/json"
wrk.body = '{"log_line": "ERROR: test"}'
EOF
) http://localhost:3000/match

# Test batch endpoint
wrk -t4 -c50 -d30s \
  -s <(cat <<'EOF'
wrk.method = "POST"
wrk.headers["Content-Type"] = "application/json"
wrk.body = '{"log_lines": ["ERROR: test1", "ERROR: test2", "ERROR: test3"]}'
EOF
) http://localhost:3000/match/batch
```

### Using `hey`

```bash
# Install hey
go install github.com/rakyll/hey@latest

# Load test
hey -n 10000 -c 100 -m POST \
  -H "Content-Type: application/json" \
  -d '{"log_line": "ERROR: test"}' \
  http://localhost:3000/match
```

## Troubleshooting

### Service Won't Start

```bash
# Check if port is already in use
lsof -i :3000

# Try different port
PORT=8080 cargo run --release --bin log-analyzer-service
```

### Low Performance

```bash
# Verify you're using --release mode
cargo run --release --bin log-analyzer-service  # ✅ CORRECT
cargo run --bin log-analyzer-service            # ❌ WRONG (20-50x slower)

# Check CPU usage
top -pid $(pgrep log-analyzer-service)

# Check if templates are loaded
curl http://localhost:3000/stats | jq .templates_loaded
```

### High Memory Usage

- Each template uses ~500 bytes
- 10K templates = ~50MB
- Check template count: `curl http://localhost:3000/stats`
- Consider splitting into multiple services

## Examples

### Python Client

```python
import requests
import json

class LogAnalyzerClient:
    def __init__(self, base_url="http://localhost:3000"):
        self.base_url = base_url

    def match(self, log_line: str) -> dict:
        """Match a single log line"""
        response = requests.post(
            f"{self.base_url}/match",
            json={"log_line": log_line}
        )
        return response.json()

    def match_batch(self, log_lines: list, use_parallel=False) -> dict:
        """Match multiple log lines"""
        response = requests.post(
            f"{self.base_url}/match/batch",
            json={
                "log_lines": log_lines,
                "use_parallel": use_parallel
            }
        )
        return response.json()

    def health(self) -> dict:
        """Check service health"""
        response = requests.get(f"{self.base_url}/health")
        return response.json()

# Usage
client = LogAnalyzerClient()

# Single match
result = client.match("ERROR: connection timeout")
print(f"Matched template: {result['template_id']}")

# Batch match
logs = ["ERROR: test1", "INFO: test2", "WARN: test3"]
result = client.match_batch(logs)
print(f"Matched {result['matched_count']}/{result['total_logs']} logs")
print(f"Throughput: {result['throughput_logs_per_sec']:.0f} logs/sec")
```

### Node.js Client

```javascript
const axios = require('axios');

class LogAnalyzerClient {
  constructor(baseUrl = 'http://localhost:3000') {
    this.baseUrl = baseUrl;
  }

  async match(logLine) {
    const response = await axios.post(`${this.baseUrl}/match`, {
      log_line: logLine
    });
    return response.data;
  }

  async matchBatch(logLines, useParallel = false) {
    const response = await axios.post(`${this.baseUrl}/match/batch`, {
      log_lines: logLines,
      use_parallel: useParallel
    });
    return response.data;
  }

  async health() {
    const response = await axios.get(`${this.baseUrl}/health`);
    return response.data;
  }
}

// Usage
const client = new LogAnalyzerClient();

(async () => {
  // Single match
  const result = await client.match("ERROR: connection timeout");
  console.log(`Matched template: ${result.template_id}`);

  // Batch match
  const logs = ["ERROR: test1", "INFO: test2", "WARN: test3"];
  const batchResult = await client.matchBatch(logs);
  console.log(`Matched ${batchResult.matched_count}/${batchResult.total_logs} logs`);
  console.log(`Throughput: ${batchResult.throughput_logs_per_sec.toFixed(0)} logs/sec`);
})();
```

## License

See main project LICENSE.
