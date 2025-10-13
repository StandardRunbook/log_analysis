# Quick Start Guide

## Get Started in 3 Steps

### 1. Build the Project
```bash
cargo build --release
```

### 2. Run the Server
```bash
cargo run
```

Server starts on `http://127.0.0.1:3000`

### 3. Query Logs
```bash
curl -X POST http://127.0.0.1:3000/query_logs \
  -H "Content-Type: application/json" \
  -d '{
    "metric_name": "cpu_usage",
    "start_time": "2025-01-15T10:00:00Z",
    "end_time": "2025-01-15T10:30:00Z"
  }' | jq .
```

## Example Response

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
  "new_templates_generated": 0
}
```

## What's Happening?

For each query, the system:

1. **Queries Metadata Service** → Finds 2 log streams for `cpu_usage`
   - `stream-001`: system-metrics-primary
   - `stream-002`: system-metrics-secondary

2. **Downloads Logs** → Fetches logs from both streams in parallel
   - 7 logs from stream-001
   - 7 logs from stream-002

3. **Matches Templates** → Uses radix trie to match each log
   - Pre-loaded templates: `cpu_usage_1`, `memory_usage_1`, `disk_io_1`
   - Extracts structured data (percentage, message)

4. **Generates Templates** → For unmatched logs, calls LLM service
   - Creates new template
   - Adds to radix trie for future matches

## Try Different Metrics

### Memory Usage
```bash
curl -X POST http://127.0.0.1:3000/query_logs \
  -H "Content-Type: application/json" \
  -d '{
    "metric_name": "memory_usage",
    "start_time": "2025-01-15T10:00:00Z",
    "end_time": "2025-01-15T10:30:00Z"
  }' | jq .
```

### Disk I/O
```bash
curl -X POST http://127.0.0.1:3000/query_logs \
  -H "Content-Type: application/json" \
  -d '{
    "metric_name": "disk_io",
    "start_time": "2025-01-15T10:00:00Z",
    "end_time": "2025-01-15T10:30:00Z"
  }' | jq .
```

## Testing the LLM Template Generation

The mock data includes an "unknown_metric" log that doesn't match any template. In a production environment with LLM integration, this would trigger automatic template generation.

## Run Tests
```bash
cargo test
```

## View Logs
The application uses structured logging. Run with debug level:
```bash
RUST_LOG=debug cargo run
```

## Next Steps

- Read [README.md](./README.md) for full feature list
- Read [ARCHITECTURE.md](./ARCHITECTURE.md) for system design
- Configure production services in each module
- Deploy with Docker/Kubernetes

## Production Configuration

Replace mock implementations with real services:

### 1. Metadata Service
Edit `src/metadata_service.rs`:
```rust
// Uncomment query_api() method
// Set METADATA_SERVICE_URL environment variable
```

### 2. Log Storage
Edit `src/log_stream_client.rs`:
```rust
// Uncomment query_log_storage() method
// Configure CloudWatch/Splunk/Elasticsearch
```

### 3. LLM Service
Edit `src/llm_service.rs`:
```rust
// Uncomment call_llm_api() method
// Set LLM_SERVICE_URL environment variable
// Configure OpenAI/Anthropic API keys
```

## Troubleshooting

### Port Already in Use
```bash
# Change port in src/main.rs or use environment variable
PORT=8080 cargo run
```

### Build Errors
```bash
# Update Rust toolchain
rustup update

# Clean and rebuild
cargo clean
cargo build
```

### No Logs Returned
- Check metric_name matches available metrics
- Verify time range is within sample data range
- Check server logs for errors

## Performance Tips

- Use `--release` build for production
- Configure connection pooling for HTTP clients
- Enable template persistence with database
- Add Redis cache for frequently accessed templates
