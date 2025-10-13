# Log Analyzer Architecture

## Overview

The Log Analyzer is a sophisticated log processing system that intelligently retrieves, matches, and analyzes logs using a three-stage pipeline with automatic template generation.

## Request Flow

```
User Request
    ↓
┌─────────────────────────────────────────────────────────────┐
│  POST /query_logs                                            │
│  {metric_name, start_time, end_time}                        │
└───────────────────────┬─────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────────────┐
│  Stage 1: Metadata Service Query                            │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ MetadataServiceClient::get_log_streams()            │   │
│  │ • Query metadata service API                         │   │
│  │ • Receive list of relevant log streams              │   │
│  │   - stream_id                                        │   │
│  │   - stream_name                                      │   │
│  │   - source                                           │   │
│  └─────────────────────────────────────────────────────┘   │
└───────────────────────┬─────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────────────┐
│  Stage 2: Parallel Log Download                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ For each log stream in parallel:                    │   │
│  │   LogStreamClient::download_logs()                  │   │
│  │   • Fetch logs from log storage                     │   │
│  │   • Filter by time range                            │   │
│  │   • Return LogEntry objects                         │   │
│  └─────────────────────────────────────────────────────┘   │
└───────────────────────┬─────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────────────┐
│  Stage 3: Log Matching & Template Generation                │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ For each log entry:                                  │   │
│  │                                                       │   │
│  │  1. LogMatcher::match_log()                         │   │
│  │     ├─ Search radix trie for prefix match          │   │
│  │     ├─ Find candidate templates                     │   │
│  │     └─ Try regex match on each candidate           │   │
│  │                                                       │   │
│  │  2. If matched:                                      │   │
│  │     └─ Extract values using regex groups            │   │
│  │                                                       │   │
│  │  3. If NOT matched:                                  │   │
│  │     ├─ LLMServiceClient::generate_template()       │   │
│  │     ├─ Create new LogTemplate                       │   │
│  │     ├─ Add template to radix trie                   │   │
│  │     └─ Re-match log with new template              │   │
│  └─────────────────────────────────────────────────────┘   │
└───────────────────────┬─────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────────────────┐
│  Response                                                    │
│  {logs, count, matched_logs, unmatched_logs,                │
│   new_templates_generated}                                   │
└─────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Main API (`main.rs`)

**Purpose**: HTTP server and request orchestration

**Key Responsibilities**:
- Accept and validate incoming POST requests
- Orchestrate the three-stage pipeline
- Manage shared application state
- Return processed results

**State Management**:
```rust
struct AppState {
    metadata_client: MetadataServiceClient,
    log_stream_client: LogStreamClient,
    log_matcher: Arc<tokio::sync::RwLock<LogMatcher>>,
    llm_client: LLMServiceClient,
}
```

### 2. Metadata Service Client (`metadata_service.rs`)

**Purpose**: Discover relevant log streams for a metric

**Key Types**:
```rust
struct MetadataQuery {
    metric_name: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

struct LogStream {
    stream_id: String,
    stream_name: String,
    source: String,
}
```

**API Contract**:
- Input: metric name + time range
- Output: List of log stream identifiers
- Mock: Returns predefined streams based on metric name
- Production: Should query actual metadata service API

### 3. Log Stream Client (`log_stream_client.rs`)

**Purpose**: Download logs from storage backends

**Key Types**:
```rust
struct LogEntry {
    timestamp: DateTime<Utc>,
    content: String,
    stream_id: String,
}
```

**API Contract**:
- Input: log stream + time range
- Output: List of log entries
- Mock: Returns sample log data
- Production: Should integrate with CloudWatch/Splunk/Elasticsearch

### 4. Log Matcher (`log_matcher.rs`)

**Purpose**: Fast template matching using radix trie

**Data Structures**:
- **Radix Trie**: Prefix-based template lookup (O(k) complexity)
- **Regex Patterns**: HashMap of compiled regex for each template
- **Templates**: Log format patterns with variable extraction

**Key Types**:
```rust
struct LogTemplate {
    template_id: String,
    pattern: String,           // Regex pattern
    variables: Vec<String>,    // Variable names
    example: String,
}

struct MatchResult {
    matched: bool,
    template_id: Option<String>,
    extracted_values: HashMap<String, String>,
}
```

**Matching Algorithm**:
1. Extract prefix from log line
2. Search radix trie for candidate templates
3. Try regex match against each candidate
4. Extract variable values on successful match
5. Return match result with extracted data

**Default Templates**:
- `cpu_usage_1`: Matches CPU percentage logs
- `memory_usage_1`: Matches memory consumption logs
- `disk_io_1`: Matches disk I/O throughput logs

### 5. LLM Service Client (`llm_service.rs`)

**Purpose**: Generate new templates for unknown log formats

**Key Types**:
```rust
struct TemplateGenerationRequest {
    log_line: String,
    context: Option<Vec<String>>,
}
```

**Template Generation**:
- Input: Unmatched log line
- Process: Send to LLM API for analysis
- Output: New LogTemplate with regex pattern
- Mock: Uses heuristic pattern extraction
- Production: Should call OpenAI/Anthropic/custom LLM

**Pattern Generation Heuristics** (Mock):
- Replace decimal numbers with `(\d+\.\d+)`
- Replace integers with `(\d+)`
- Escape special regex characters
- Create capture groups for dynamic values

## Concurrency Model

### Async/Await with Tokio
- All I/O operations are async
- Non-blocking log stream downloads
- Concurrent template matching

### Thread-Safe State
- `LogMatcher` wrapped in `Arc<RwLock<>>`
- Multiple readers for matching (read lock)
- Single writer for adding templates (write lock)
- No blocking during normal operation

## Performance Characteristics

### Time Complexity
- Metadata query: O(1) - hash map lookup
- Log download: O(n) - n = number of streams
- Template matching: O(k + m) - k = key length, m = candidate templates
- Regex matching: O(l) - l = log line length

### Space Complexity
- Radix trie: O(t × k) - t = templates, k = average prefix length
- Regex cache: O(t) - compiled patterns
- Log buffer: O(n × l) - n = logs, l = line length

### Scalability
- Horizontal: Can run multiple API instances
- Vertical: Async I/O handles many concurrent requests
- Bottleneck: LLM API calls (can be cached/batched)

## Error Handling

### Graceful Degradation
- Metadata service failure → return error to client
- Log stream download failure → log warning, continue with other streams
- LLM service failure → log warning, return unmatched log

### Error Types
- Validation errors: 400 Bad Request
- Service unavailable: 500 Internal Server Error
- Timeout errors: Configurable per service

## Extensibility Points

### 1. Custom Template Storage
Replace in-memory trie with:
- PostgreSQL for persistence
- Redis for distributed caching
- S3 for template versioning

### 2. Custom Log Sources
Implement `LogStreamClient` for:
- AWS CloudWatch Logs
- Splunk HEC
- Elasticsearch
- Kafka topics
- S3 log buckets

### 3. Custom LLM Providers
Implement `LLMServiceClient` for:
- OpenAI GPT-4
- Anthropic Claude
- Self-hosted models (Ollama)
- Fine-tuned domain-specific models

### 4. Additional Processing
Add middleware for:
- Log enrichment
- Anomaly detection
- Statistical analysis
- Real-time alerting

## Configuration

### Service Endpoints
```rust
// In main.rs - replace with environment variables
let metadata_client = MetadataServiceClient::new(
    env::var("METADATA_SERVICE_URL")
        .unwrap_or("http://metadata-service:8080".to_string())
);

let llm_client = LLMServiceClient::new(
    env::var("LLM_SERVICE_URL")
        .unwrap_or("http://llm-service:8081".to_string())
);
```

### Server Configuration
```rust
let addr = SocketAddr::from((
    [127, 0, 0, 1], 
    env::var("PORT").unwrap_or("3000".to_string()).parse()?
));
```

## Monitoring & Observability

### Tracing
- Request/response logging
- Service call tracing
- Performance metrics
- Error tracking

### Metrics to Track
- Requests per second
- Average response time
- Template match rate
- LLM invocation count
- Cache hit rate
- Error rate by service

### Logging Levels
- `INFO`: Request lifecycle
- `DEBUG`: Template matching details
- `WARN`: Service failures
- `ERROR`: Critical failures

## Security Considerations

### Input Validation
- Time range validation (start < end)
- Metric name sanitization
- Request size limits

### Future Security Features
- API authentication (JWT/API keys)
- Rate limiting
- Request signing
- Audit logging

## Deployment

### Docker
```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/log_analyzer /usr/local/bin/
CMD ["log_analyzer"]
```

### Kubernetes
- Multiple replicas for HA
- HPA for auto-scaling
- Service mesh for observability
- ConfigMaps for configuration

## Testing Strategy

### Unit Tests
- Template matching logic (`log_matcher.rs`)
- Pattern generation heuristics
- Time range validation

### Integration Tests
- End-to-end API tests
- Mock service integration
- Error handling scenarios

### Performance Tests
- Load testing with k6/Locust
- Template cache efficiency
- Concurrent request handling
