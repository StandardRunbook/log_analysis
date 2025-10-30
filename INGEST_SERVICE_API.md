# Log Ingestion Service API

High-performance log ingestion service with intelligent template matching, LLM-powered template generation, and buffered ClickHouse writes.

## Quick Start

```bash
# Build and run the service
cargo run --release --bin log-ingest-service

# With environment configuration
CLICKHOUSE_URL=http://localhost:8123 \
LLM_PROVIDER=openai \
LLM_API_KEY=sk-... \
LLM_MODEL=gpt-4 \
cargo run --release --bin log-ingest-service

# Service starts on http://0.0.0.0:3002 by default
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `INGEST_PORT` | `3002` | Server port |
| `CLICKHOUSE_URL` | `http://localhost:8123` | ClickHouse connection URL |
| `LLM_PROVIDER` | `ollama` | LLM provider (`openai` or `ollama`) |
| `LLM_API_KEY` | `""` | API key for OpenAI (not needed for Ollama) |
| `LLM_MODEL` | `llama3` | Model name (`gpt-4`, `gpt-3.5-turbo`, `llama3`, etc.) |

### Performance Tuning Constants

Defined in source code ([log-ingest-service.rs:29-36](src/bin/log-ingest-service.rs#L29-L36)):

```rust
const CLICKHOUSE_BUFFER_SIZE: usize = 1000;        // Flush after 1000 logs
const CLICKHOUSE_FLUSH_INTERVAL_SECS: u64 = 5;     // Or flush every 5 seconds
const LLM_BATCH_SIZE: usize = 10;                  // Process 10 logs per batch
const LLM_BATCH_TIMEOUT_SECS: u64 = 2;             // Or process after 2 seconds
const LLM_MAX_CONCURRENT_BATCHES: usize = 5;       // Max 5 batches in parallel
const LLM_MAX_RETRIES: u32 = 3;                    // Retry failed LLM calls 3 times
const LLM_INITIAL_BACKOFF_MS: u64 = 1000;          // Start with 1s backoff
```

## Architecture

```
Log Ingestion → DFA Matching → Matched?
                                ├─ Yes → Buffered ClickHouse Writer → Flush (1000 logs or 5s)
                                └─ No  → LLM Queue → Batch Processor → Generate Template → Add to DFA
```

### Key Features

- ✅ **Unified API**: Single endpoint accepts both single logs and batches
- ✅ **Smart Batching**: ClickHouse writes batched (1000 logs or 5s)
- ✅ **LLM Integration**: Auto-generates templates for unmatched logs
- ✅ **Thread Pool**: Parallel LLM processing with exponential backoff
- ✅ **Lock-Free Matching**: Concurrent template matching via ArcSwap
- ✅ **Non-Blocking**: Log ingestion never waits for LLM or DB writes

## API Endpoints

### `GET /health`

Health check endpoint.

**Response:**
```json
{
  "status": "healthy",
  "templates_loaded": 150,
  "clickhouse_connected": true
}
```

**Example:**
```bash
curl http://localhost:3002/health | jq .
```

---

### `GET /stats`

Get service statistics and configuration.

**Response:**
```json
{
  "templates_loaded": 150,
  "optimal_batch_size": 10000
}
```

**Example:**
```bash
curl http://localhost:3002/stats | jq .
```

---

### `POST /logs/ingest`

**Unified endpoint** - automatically detects single log or batch format.

#### Single Log Format

**Request:**
```json
{
  "timestamp": "2025-01-15T10:30:45Z",
  "org": "acme",
  "dashboard": "production",
  "service": "api-server",
  "host": "web-01",
  "level": "ERROR",
  "message": "Connection timeout after 30s",
  "metadata": {"request_id": "req_123"}
}
```

**Required fields:**
- `org` (string)
- `message` (string)

**Optional fields:**
- `timestamp` (ISO 8601 string, defaults to now)
- `dashboard` (string)
- `service` (string)
- `host` (string)
- `level` (string, defaults to "INFO")
- `metadata` (JSON object)

#### Batch Format

**Request:**
```json
{
  "logs": [
    {
      "org": "acme",
      "message": "ERROR: Connection failed",
      "level": "ERROR"
    },
    {
      "org": "acme",
      "message": "WARN: Retrying connection",
      "level": "WARN"
    }
  ]
}
```

#### Response (same for both formats)

```json
{
  "accepted": 100,
  "matched": 95,
  "failed": 0
}
```

- `accepted`: Total logs received
- `matched`: Logs matched to existing templates
- `failed`: Logs that failed to process (always 0 in current implementation)

**Note:** Unmatched logs (5 in this example) are queued for LLM template generation in the background.

#### Examples

**Single log:**
```bash
curl -X POST http://localhost:3002/logs/ingest \
  -H 'Content-Type: application/json' \
  -d '{
    "org": "acme",
    "service": "api",
    "level": "ERROR",
    "message": "Connection timeout after 30s"
  }' | jq .
```

**Batch:**
```bash
curl -X POST http://localhost:3002/logs/ingest \
  -H 'Content-Type: application/json' \
  -d '{
    "logs": [
      {"org": "acme", "message": "ERROR: test1", "level": "ERROR"},
      {"org": "acme", "message": "WARN: test2", "level": "WARN"},
      {"org": "acme", "message": "INFO: test3", "level": "INFO"}
    ]
  }' | jq .
```

**Large batch (auto-parallel matching for >1000 logs):**
```bash
# Generate 2000 logs
python3 << 'EOF' > /tmp/large_batch.json
import json
logs = [{"org": "acme", "message": f"ERROR: message {i}"} for i in range(2000)]
print(json.dumps({"logs": logs}))
EOF

# Send to service
curl -X POST http://localhost:3002/logs/ingest \
  -H 'Content-Type: application/json' \
  -d @/tmp/large_batch.json | jq .
```

---

## Performance Characteristics

### Throughput

| Scenario | Throughput |
|----------|-----------|
| Single log (matched) | ~50K logs/sec |
| Batch 100 logs (matched) | ~150K logs/sec |
| Batch 1000+ logs (matched, parallel) | ~370K logs/sec |
| Peak (Spark logs) | ~876K logs/sec |

### Latency

| Operation | Latency |
|-----------|---------|
| Single log ingestion | 1-5 ms |
| Batch 100 logs | 5-10 ms |
| Batch 1000 logs (parallel) | 20-50 ms |
| ClickHouse flush | <100 ms |
| LLM template generation | 1-3 seconds |

### Memory Usage

- Base service: ~10MB
- Per template: ~500 bytes
- With 1000 templates: ~20MB
- ClickHouse buffer: ~5MB (at 1000 logs)
- LLM queue: Variable (depends on unmatched rate)

## Background Processing

### ClickHouse Buffered Writer

Logs are buffered in memory and flushed based on:
- **Size trigger**: 1000 logs accumulated
- **Time trigger**: 5 seconds elapsed since last flush

Benefits:
- 1000x fewer database writes
- Reduced network overhead
- Better ClickHouse performance

### LLM Template Generation Pipeline

Unmatched logs are processed in the background:

```
Unmatched logs → Queue → Batch (10 logs) → Process in parallel → Retry on failure
                                                ↓
                                    Generate templates via LLM
                                                ↓
                                    Add to DFA + Save to ClickHouse
```

**Configuration:**
- Batch size: 10 logs
- Timeout: 2 seconds (process partial batch)
- Max concurrent batches: 5 (max 50 LLM calls)
- Retry: 3 attempts with exponential backoff

**Retry Schedule:**
- Attempt 1: Immediate
- Attempt 2: Wait 1s ± 10% jitter
- Attempt 3: Wait 2s ± 10% jitter
- Give up: Log error and discard

## Deployment

### Docker

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin log-ingest-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/log-ingest-service /usr/local/bin/
EXPOSE 3002
ENV CLICKHOUSE_URL=http://clickhouse:8123
ENV LLM_PROVIDER=ollama
ENV LLM_MODEL=llama3
CMD ["log-ingest-service"]
```

Build and run:
```bash
docker build -t log-ingest-service .
docker run -p 3002:3002 \
  -e CLICKHOUSE_URL=http://clickhouse:8123 \
  -e LLM_PROVIDER=openai \
  -e LLM_API_KEY=sk-... \
  -e LLM_MODEL=gpt-4 \
  log-ingest-service
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: log-ingest-service
spec:
  replicas: 3
  selector:
    matchLabels:
      app: log-ingest
  template:
    metadata:
      labels:
        app: log-ingest
    spec:
      containers:
      - name: service
        image: log-ingest-service:latest
        ports:
        - containerPort: 3002
        env:
        - name: INGEST_PORT
          value: "3002"
        - name: CLICKHOUSE_URL
          value: "http://clickhouse:8123"
        - name: LLM_PROVIDER
          value: "openai"
        - name: LLM_API_KEY
          valueFrom:
            secretKeyRef:
              name: llm-secret
              key: api-key
        - name: LLM_MODEL
          value: "gpt-4"
        resources:
          requests:
            memory: "256Mi"
            cpu: "500m"
          limits:
            memory: "1Gi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 3002
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health
            port: 3002
          initialDelaySeconds: 5
          periodSeconds: 10
---
apiVersion: v1
kind: Service
metadata:
  name: log-ingest-service
spec:
  selector:
    app: log-ingest
  ports:
  - port: 80
    targetPort: 3002
  type: LoadBalancer
```

## Monitoring

### Key Metrics

1. **Ingestion Metrics:**
   - Requests per second
   - Logs per second
   - Match rate (matched/total)
   - Batch size distribution

2. **LLM Metrics:**
   - Queue depth (unmatched logs)
   - Template generation rate
   - Retry rate
   - Success/failure rate

3. **ClickHouse Metrics:**
   - Flush frequency
   - Buffer utilization
   - Write latency
   - Write errors

4. **Resource Metrics:**
   - CPU usage
   - Memory usage
   - Thread pool utilization

### Health Check Monitoring

```bash
# Simple uptime monitoring
while true; do
  curl -f http://localhost:3002/health || echo "Service down!"
  sleep 10
done
```

## Client Examples

### Python Client

```python
import requests
from typing import List, Dict, Optional

class LogIngestClient:
    def __init__(self, base_url="http://localhost:3002"):
        self.base_url = base_url

    def ingest_single(
        self,
        org: str,
        message: str,
        level: str = "INFO",
        service: Optional[str] = None,
        **kwargs
    ) -> Dict:
        """Ingest a single log"""
        payload = {
            "org": org,
            "message": message,
            "level": level,
            **kwargs
        }
        if service:
            payload["service"] = service

        response = requests.post(
            f"{self.base_url}/logs/ingest",
            json=payload
        )
        return response.json()

    def ingest_batch(self, logs: List[Dict]) -> Dict:
        """Ingest multiple logs"""
        response = requests.post(
            f"{self.base_url}/logs/ingest",
            json={"logs": logs}
        )
        return response.json()

    def health(self) -> Dict:
        """Check service health"""
        response = requests.get(f"{self.base_url}/health")
        return response.json()

# Usage
client = LogIngestClient()

# Single log
result = client.ingest_single(
    org="acme",
    service="api-server",
    level="ERROR",
    message="Connection timeout after 30s"
)
print(f"Ingested: {result['accepted']}, Matched: {result['matched']}")

# Batch
logs = [
    {"org": "acme", "message": "ERROR: test1", "level": "ERROR"},
    {"org": "acme", "message": "WARN: test2", "level": "WARN"},
    {"org": "acme", "message": "INFO: test3", "level": "INFO"}
]
result = client.ingest_batch(logs)
print(f"Ingested: {result['accepted']}, Matched: {result['matched']}")
```

### Node.js Client

```javascript
const axios = require('axios');

class LogIngestClient {
  constructor(baseUrl = 'http://localhost:3002') {
    this.baseUrl = baseUrl;
  }

  async ingestSingle({ org, message, level = 'INFO', ...rest }) {
    const response = await axios.post(`${this.baseUrl}/logs/ingest`, {
      org,
      message,
      level,
      ...rest
    });
    return response.data;
  }

  async ingestBatch(logs) {
    const response = await axios.post(`${this.baseUrl}/logs/ingest`, {
      logs
    });
    return response.data;
  }

  async health() {
    const response = await axios.get(`${this.baseUrl}/health`);
    return response.data;
  }
}

// Usage
const client = new LogIngestClient();

(async () => {
  // Single log
  const result = await client.ingestSingle({
    org: 'acme',
    service: 'api-server',
    level: 'ERROR',
    message: 'Connection timeout after 30s'
  });
  console.log(`Ingested: ${result.accepted}, Matched: ${result.matched}`);

  // Batch
  const logs = [
    { org: 'acme', message: 'ERROR: test1', level: 'ERROR' },
    { org: 'acme', message: 'WARN: test2', level: 'WARN' },
    { org: 'acme', message: 'INFO: test3', level: 'INFO' }
  ];
  const batchResult = await client.ingestBatch(logs);
  console.log(`Ingested: ${batchResult.accepted}, Matched: ${batchResult.matched}`);
})();
```

## Troubleshooting

### Service Won't Start

```bash
# Check if port is already in use
lsof -i :3002

# Try different port
INGEST_PORT=8080 cargo run --release --bin log-ingest-service
```

### ClickHouse Connection Issues

```bash
# Test ClickHouse connection
curl http://localhost:8123/ping

# Check ClickHouse URL
echo $CLICKHOUSE_URL
```

### LLM Template Generation Not Working

```bash
# Check LLM configuration
echo "Provider: $LLM_PROVIDER"
echo "Model: $LLM_MODEL"

# Test Ollama (if using)
curl http://localhost:11434/api/tags

# Test OpenAI (if using)
curl https://api.openai.com/v1/models \
  -H "Authorization: Bearer $LLM_API_KEY"
```

### High Memory Usage

- Check ClickHouse buffer size (default 1000 logs)
- Check LLM queue depth (high unmatched rate)
- Monitor template count: `curl http://localhost:3002/stats`
- Reduce batch sizes if needed

### Low Match Rate

If most logs are unmatched:
- Check if templates are loaded from ClickHouse
- Review LLM-generated templates
- Consider pre-generating templates for common patterns
- Adjust fragment matching thresholds

## License

See main project LICENSE.
