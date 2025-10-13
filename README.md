# Log Analyzer API

A Rust-based REST API service that accepts POST requests to query logs by metric name and time range, with intelligent log template matching, automatic template generation, and Jensen-Shannon Divergence (JSD) analysis for anomaly detection.

## 📚 Documentation

- **[README.md](./README.md)** (this file) - Overview, API documentation, quick start
- **[QUICKSTART.md](./QUICKSTART.md)** - Get started in 3 steps
- **[ARCHITECTURE.md](./ARCHITECTURE.md)** - Detailed system design and architecture
- **[JSD_ANALYSIS_GUIDE.md](./JSD_ANALYSIS_GUIDE.md)** - Complete guide to JSD analysis and anomaly detection
- **[LLM_TEMPLATE_GUIDE.md](./LLM_TEMPLATE_GUIDE.md)** - LLM prompt engineering for template generation

## Features

- Accept POST requests with JSON payload containing:
  - `metric_name`: The name of the metric to filter logs
  - `start_time`: Start of the time range (ISO 8601 format)
  - `end_time`: End of the time range (ISO 8601 format)
- **Three-stage log processing pipeline:**
  1. **Metadata Service Query**: Queries metadata service to identify relevant log streams
  2. **Log Download**: Downloads logs from all identified log streams in parallel
  3. **Template Matching**: Uses a radix trie to match logs against known templates
  4. **LLM Template Generation**: For unmatched logs, calls LLM service to generate new templates
- **Jensen-Shannon Divergence (JSD) Analysis**:
  - Automatically queries baseline logs from 3 hours prior to the requested time range
  - Builds histograms of template distributions for both baseline and current periods
  - Calculates JSD score to measure distribution divergence
  - Identifies top contributing templates to the JSD score
  - Reports relative changes in template frequencies
- Returns processed logs with extracted values and template information
- Built with Axum web framework for high performance
- Async/await with Tokio runtime

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
│  ┌─────────────────────────────────────────────────┐   │
│  │  1. Query Metadata Service                       │   │
│  │     Get log streams for metric                   │   │
│  └───────────────┬─────────────────────────────────┘   │
│                  │                                       │
│  ┌───────────────▼─────────────────────────────────┐   │
│  │  2. Download Logs from Log Streams               │   │
│  │     Fetch logs from each stream                  │   │
│  └───────────────┬─────────────────────────────────┘   │
│                  │                                       │
│  ┌───────────────▼─────────────────────────────────┐   │
│  │  3. Match Logs with Radix Trie                   │   │
│  │     Fast template matching                       │   │
│  └───────────────┬─────────────────────────────────┘   │
│                  │                                       │
│                  │ if no match found                    │
│                  │                                       │
│  ┌───────────────▼─────────────────────────────────┐   │
│  │  4. Generate Template via LLM                    │   │
│  │     Create new template and add to trie          │   │
│  └───────────────┬─────────────────────────────────┘   │
│                  │                                       │
│  ┌───────────────▼─────────────────────────────────┐   │
│  │  5. Extract Values & Return Results              │   │
│  └─────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────┘
```

## Modules

### `metadata_service.rs`
- Client for querying metadata service to discover log streams
- Returns list of log streams relevant to the requested metric

### `log_stream_client.rs`
- Downloads logs from individual log streams
- Supports parallel download from multiple streams

### `log_matcher.rs`
- Implements radix trie-based log template matching
- Fast prefix-based template lookup
- Extracts structured data from logs using regex patterns

### `llm_service.rs`
- Client for LLM service to generate new log templates
- Automatically creates templates for previously unseen log formats

### `histogram.rs`
- Template ID histogram for tracking log template frequencies
- Probability distribution calculation
- Used for JSD analysis

### `jsd.rs`
- Jensen-Shannon Divergence calculation between two distributions
- Template contribution analysis
- Identifies which templates have the highest impact on distribution changes

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run
```

The server will start on `http://127.0.0.1:3000`

## API Endpoint

### POST /query_logs

Request body:
```json
{
  "metric_name": "cpu_usage",
  "start_time": "2025-01-15T10:00:00Z",
  "end_time": "2025-01-15T10:30:00Z"
}
```

Response:
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
    },
    {
      "timestamp": "2025-01-15T10:05:00+00:00",
      "content": "cpu_usage: 67.8% - Server load increased",
      "stream_id": "stream-001",
      "matched_template": "cpu_usage_1",
      "extracted_values": {
        "percentage": "67.8",
        "message": "Server load increased"
      }
    }
  ],
  "count": 14,
  "matched_logs": 14,
  "unmatched_logs": 0,
  "new_templates_generated": 0,
  "jsd_analysis": {
    "jsd_score": 0.11098152389578031,
    "baseline_period": "2025-01-15 07:00:00 UTC to 2025-01-15 10:00:00 UTC",
    "current_period": "2025-01-15 10:00:00 UTC to 2025-01-15 10:30:00 UTC",
    "baseline_log_count": 2,
    "current_log_count": 14,
    "top_contributors": [
      {
        "template_id": "disk_io_1",
        "baseline_probability": 0.0,
        "current_probability": 0.14285714285714285,
        "contribution": 0.049510512847138956,
        "relative_change": 100.0,
        "representative_logs": [
          "disk_io: 250MB/s - Disk activity moderate",
          "disk_io: 250MB/s - Disk activity moderate"
        ]
      },
      {
        "template_id": "memory_usage_1",
        "baseline_probability": 0.0,
        "current_probability": 0.14285714285714285,
        "contribution": 0.049510512847138956,
        "relative_change": 100.0,
        "representative_logs": [
          "memory_usage: 2.5GB - Memory consumption stable",
          "memory_usage: 2.5GB - Memory consumption stable"
        ]
      },
      {
        "template_id": "cpu_usage_1",
        "baseline_probability": 1.0,
        "current_probability": 0.7142857142857143,
        "contribution": 0.011960498201502398,
        "relative_change": -28.57142857142857,
        "representative_logs": [
          "cpu_usage: 45.2% - Server load normal",
          "cpu_usage: 67.8% - Server load increased",
          "cpu_usage: 89.3% - High server load detected"
        ]
      }
    ]
  }
}
```

### Response Fields

- `logs`: Array of processed log entries
  - `timestamp`: ISO 8601 timestamp
  - `content`: Raw log content
  - `stream_id`: ID of the log stream
  - `matched_template`: ID of the matched template (null if no match)
  - `extracted_values`: Key-value pairs extracted from the log
- `count`: Total number of logs returned
- `matched_logs`: Number of logs matched with existing templates
- `unmatched_logs`: Number of logs that didn't match any template
- `new_templates_generated`: Number of new templates created by LLM
- `jsd_analysis`: JSD analysis comparing current period to baseline (null if insufficient data)
  - `jsd_score`: Jensen-Shannon Divergence score (higher = more divergence)
  - `baseline_period`: Time range of baseline logs (3 hours prior)
  - `current_period`: Time range of current logs (requested period)
  - `baseline_log_count`: Number of logs in baseline period
  - `current_log_count`: Number of logs in current period
  - `top_contributors`: Templates with highest contribution to JSD (sorted by contribution, descending)
    - `template_id`: Template identifier
    - `baseline_probability`: Probability in baseline distribution (0-1)
    - `current_probability`: Probability in current distribution (0-1)
    - `contribution`: Contribution to overall JSD score
    - `relative_change`: Percentage change from baseline to current (%)
    - `representative_logs`: Array of up to 3 example log lines matching this template

## Example Usage

Using curl:
```bash
curl -X POST http://127.0.0.1:3000/query_logs \
  -H "Content-Type: application/json" \
  -d '{
    "metric_name": "cpu_usage",
    "start_time": "2025-01-15T10:00:00Z",
    "end_time": "2025-01-15T10:30:00Z"
  }' | jq .
```

## Available Sample Metrics

The current mock implementation includes sample logs for:
- `cpu_usage` - CPU utilization metrics (2 log streams)
- `memory_usage` - Memory consumption metrics (1 log stream)
- `disk_io` - Disk I/O performance metrics (1 log stream)

## Pre-configured Templates

The system comes with pre-configured templates for:
- CPU usage logs
- Memory usage logs
- Disk I/O logs

New templates are automatically generated and added when encountering unknown log formats.

## Dependencies

- **axum** - Web framework for REST API
- **tokio** - Async runtime
- **serde** / **serde_json** - JSON serialization/deserialization
- **chrono** - Date and time handling
- **reqwest** - HTTP client for external API calls
- **radix_trie** - Efficient prefix-based template matching
- **regex** - Pattern matching for log parsing
- **tracing** / **tracing-subscriber** - Application-level logging
- **anyhow** - Error handling

## Production Deployment Notes

The current implementation uses mock services for:
1. Metadata service responses
2. Log stream downloads
3. LLM template generation

To deploy in production:

### 1. Configure Metadata Service
Uncomment the `query_api` method in `src/metadata_service.rs` and configure your metadata service endpoint.

### 2. Configure Log Storage
Uncomment the `query_log_storage` method in `src/log_stream_client.rs` and integrate with your log storage backend (CloudWatch, Splunk, Elasticsearch, etc.).

### 3. Configure LLM Service
Uncomment the `call_llm_api` method in `src/llm_service.rs` and configure your LLM API endpoint (OpenAI, Anthropic, or self-hosted).

### 4. Environment Configuration
Set environment variables for service endpoints:
```bash
export METADATA_SERVICE_URL="http://metadata-service:8080"
export LLM_SERVICE_URL="http://llm-service:8081"
```

## Testing

Run the test suite:
```bash
cargo test
```

The project includes unit tests for log matching functionality.

## Performance Characteristics

- **Radix Trie Lookup**: O(k) where k is the key length
- **Async Processing**: Parallel log stream downloads
- **Template Caching**: New templates persist for the lifetime of the service
- **Memory Efficient**: Streaming log processing

## Future Enhancements

- [ ] Persistent template storage (database)
- [ ] Template versioning and management API
- [ ] Pagination for large result sets
- [ ] Bulk query support (multiple metrics)
- [ ] Authentication/authorization
- [ ] Log aggregation and statistics
- [ ] Real-time log streaming via WebSockets
- [ ] Template quality metrics and feedback loop
- [ ] Distributed template cache with Redis
