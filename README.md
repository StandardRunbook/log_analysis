# Log Analyzer

A multi-tenant log analysis service that classifies log records into stable
templates, persists them to ClickHouse with content-hashed `template_id`s, and
serves long-horizon distribution-shift analytics (e.g. KL divergence) over
the resulting template counts.

Two ingest paths:
- **OTLP/gRPC** on `4317` — the production protocol; what customer collectors should use.
- **HTTP/JSON** on `3002` — convenient for ad-hoc testing and scripting.

A novel log shape that the parse tree doesn't recognize is forwarded to a
single-consumer LLM worker that synthesizes a regex template; subsequent
records of that shape match in-memory at hot-path speed. No log row is
ever persisted with an empty `template_id` — the LLM consumer owns the
cold-path write.

## Quick start

```bash
# 1. Bring up ClickHouse (parent docker-compose contains the schema)
cd ../ && docker-compose up -d clickhouse && cd log_analysis

# 2. Configure the LLM in .env
#    LLM_PROVIDER=openai
#    LLM_MODEL=gpt-4o
#    LLM_API_KEY=sk-...
#    (or LLM_PROVIDER=ollama and a local model)

# 3. Run the ingest service (HTTP + OTLP/gRPC)
cargo run --release --bin log-ingest-service

# 4. Verify
curl http://localhost:3002/health
# {"status":"healthy","templates_loaded":3,"clickhouse_connected":true}
```

## Ingest paths

### OTLP/gRPC (port 4317)

The service implements `opentelemetry.proto.collector.logs.v1.LogsService`.
Point any OTel-compatible client at `localhost:4317` and export logs.
Resource attributes drive tenant identification:

| Resource attribute    | Maps to              | Default          |
| --------------------- | -------------------- | ---------------- |
| `org_id`              | tenant id            | `"default"`      |
| `log_stream_id`       | per-stream id        | `"default-stream"` |
| `log_stream_name`     | human-readable name  | == log_stream_id |
| `service.name`        | service              | empty            |
| `cloud.region`        | region               | empty            |

The log message comes from `LogRecord.body` (string-typed records only;
non-string bodies are skipped).

### HTTP/JSON (port 3002)

```bash
curl -X POST http://localhost:3002/logs/ingest \
  -H 'Content-Type: application/json' \
  -d '{
    "org_id": "acme",
    "log_stream_id": "api-prod",
    "service": "api-server",
    "region": "us-east-1",
    "log_stream_name": "api-prod-logs",
    "message": "cpu_usage: 45.2% - Server load normal"
  }'
```

Batch form:

```bash
curl -X POST http://localhost:3002/logs/ingest \
  -H 'Content-Type: application/json' \
  -d '{ "logs": [ {…}, {…} ] }'
```

Other HTTP endpoints:

- `GET /health` — liveness, templates loaded, ClickHouse connectivity.
- `GET /stats` — current matcher state.

## Architecture

```
                ┌──────────────────────────────────────────────────────────┐
                │                   ingest service (one process)            │
                │                                                          │
   OTel         │   ┌──────────────┐    ┌─────────────────────────────┐    │
   collector ───┼──▶│ OTLP/gRPC :4317│──┐│                             │    │
                │   └──────────────┘    │ │   LogMatcher (parse tree)  │    │
                │                       ├▶│   - Aho-Corasick DFA       │    │
   ad-hoc       │   ┌──────────────┐    │ │   - ArcSwap snapshot       │    │
   client   ────┼──▶│ HTTP/JSON :3002│──┘│   - per-tenant in v1+        │    │
                │   └──────────────┘     └────┬─────────────────┬──────┘    │
                │                             │                 │           │
                │                          (HIT)             (MISS)         │
                │                             │                 │           │
                │                  ┌──────────▼─────┐  ┌────────▼───────┐   │
                │                  │ writer channel │  │ unmatched mpsc │   │
                │                  │ (Vec<Entry>)   │  │ (LogEntry)     │   │
                │                  └────────┬───────┘  └────────┬───────┘   │
                │                           │                   │           │
                │             ┌─────────────▼──────┐  ┌─────────▼────────┐  │
                │             │  Buffered writer   │  │ LLM consumer     │  │
                │             │  task (decoupled)  │  │ task (single)    │  │
                │             │  - batch + timer   │  │ - rematch dedup  │  │
                │             └────────┬───────────┘  │ - LLM synthesis  │  │
                │                      │              │ - own cold-path  │  │
                │                      │              │   write          │  │
                │                      │              └────────┬─────────┘  │
                └──────────────────────┼───────────────────────┼────────────┘
                                       │                       │
                                       └────────────┬──────────┘
                                                    ▼
                                            ┌──────────────┐
                                            │  ClickHouse  │
                                            │  logs +      │
                                            │  templates + │
                                            │  examples    │
                                            └──────────────┘
```

Load-bearing design properties:

- **Content-hashed `template_id`** — `template_id = blake3(canonicalize(pattern))[..8]`.
  Stable across restarts, deploys, and database migrations. The same parse
  tree always maps to the same ID. Concurrent synthesis is naturally
  idempotent. Necessary for KL-divergence-over-time correctness.
- **No orphan log rows** — a `logs` row is only inserted once its `template_id`
  is known. Misses are queued in full and the LLM consumer writes the row
  after rematching or LLM synthesis. There are never rows with empty
  `template_id`.
- **Decoupled writer** — the hot path (HTTP/OTLP handler) does one channel
  send per *request* (not per record); a dedicated writer task drains the
  channel into ClickHouse on size or time triggers. Hot-path latency is
  independent of ClickHouse round-trip time.
- **Single-consumer LLM worker with rematch-before-LLM dedup** — a burst of
  N records of the same novel shape produces 1 LLM call, not N. By the time
  the second record is pulled from the queue, the matcher has been updated
  and the rematch returns a hit.

## Performance

End-to-end ingest, on a single MacBook with local Docker ClickHouse, batch
size 1000, concurrency 32:

| Path             | Throughput            | p50    | p99    | p99.9   |
| ---------------- | --------------------- | ------ | ------ | ------- |
| HTTP/JSON, single-log/req | ~30K logs/sec         | 1.9ms  | 5.5ms  | 11.0ms  |
| HTTP/JSON, batched        | ~530K logs/sec        | 24ms   | 287ms  | 287ms   |
| **OTLP/gRPC, batched**    | **~5.7M logs/sec**    | 4.9ms  | 8.7ms  | 10.1ms  |

The 5.7M figure is **ingest throughput** — what the channel can absorb in
burst. Sustained durable-write throughput is bounded by ClickHouse capacity;
plan ClickHouse sizing against the steady-state log rate, not the burst
ceiling.

LLM cold-path latency depends on the configured provider/model. Recent
measurements:
- `gpt-4o`: ~250 ms per novel shape.
- `o1`: ~3–5 s per novel shape; produces structurally tighter patterns.
- `llama3:8b` (Ollama): ~1 s, but produces over-generic patterns; not
  recommended for production.

The LLM is on the cold path only — it never blocks the request that triggered
it (the row is queued and persisted once the consumer determines the
template).

## Configuration

`.env` (loaded via `dotenvy`):

```
CLICKHOUSE_URL=http://localhost:8123
INGEST_PORT=3002
OTLP_GRPC_PORT=4317

LLM_PROVIDER=openai            # openai | anthropic | ollama
LLM_MODEL=gpt-4o
LLM_API_KEY=sk-...

# Or for multi-LLM consensus:
# LLM_CONFIG_FILE=./llm-config.json
```

See [LLM_CONFIG.md](LLM_CONFIG.md) for the full provider/consensus
configuration shape.

## Testing

```bash
# Unit + integration tests (lib + serialization + benchmarks)
cargo test --lib --release

# OpenStack grouping accuracy against a configured LLM
cargo test --test openstack_accuracy_test --release -- --nocapture

# LLM connectivity smoke check (one round-trip, prints the result)
cargo run --release --example llm_smoke_test

# LLM template-distinctness diagnostic (one log per ground-truth event)
cargo run --release --example llm_distinctness_test

# End-to-end perf tests
cargo run --release --example perf_test       # JSON HTTP path
cargo run --release --example perf_test_otlp  # OTLP/gRPC path

# Tunables for the perf tests:
#   TOTAL_LOGS=200000 BATCH_SIZE=1000 CONCURRENCY=32 ORG_ID=mytest
```

## Library usage

The matcher is usable standalone:

```rust
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::template_id::template_id_from_pattern;

let matcher = LogMatcher::new();

// IDs are content-derived hashes — the same pattern always produces
// the same ID across processes and restarts.
let pattern = r"User (\w+) logged in from (\d+\.\d+\.\d+\.\d+)";
matcher.add_template(LogTemplate {
    template_id: template_id_from_pattern(pattern),
    pattern: pattern.to_string(),
    variables: vec!["user".into(), "ip".into()],
    example: "User alice logged in from 10.0.0.1".into(),
});

let result = matcher.match_log("User bob logged in from 10.0.0.2");

// Batched matching for higher throughput
let logs = vec!["...", "..."];
let results = matcher.match_batch(&logs);

// Parallel matching for very large batches (>1000)
let results = matcher.match_batch_parallel(&logs);
```

## Layout

```
log_analysis/
├── src/
│   ├── log_matcher.rs       # Aho-Corasick parse-tree matcher
│   ├── template_id.rs       # Content-hash IDs + canonicalization
│   ├── otlp_server.rs       # OTLP/gRPC LogsService
│   ├── buffered_writer.rs   # Decoupled ClickHouse writer
│   ├── llm_service.rs       # Multi-provider LLM client (OpenAI/Anthropic/Ollama)
│   ├── llm_config.rs        # Provider / consensus configuration
│   ├── clickhouse_client.rs # ClickHouse persistence
│   ├── matcher_config.rs    # Tunables (fragment threshold, batch size, …)
│   └── bin/
│       ├── log-ingest-service.rs   # Main ingest binary
│       ├── regenerate-cache.rs     # Rebuild benchmark caches with current ID scheme
│       └── sync-templates.rs       # Push cached templates into ClickHouse
├── examples/
│   ├── perf_test.rs               # JSON HTTP perf test
│   ├── perf_test_otlp.rs          # OTLP/gRPC perf test
│   ├── llm_smoke_test.rs          # LLM connectivity check
│   ├── llm_distinctness_test.rs   # Model-vs-matcher diagnostic
│   └── multi_llm_test.rs          # Multi-provider consensus example
├── tests/
│   ├── benchmarks.rs              # Throughput / parallel matcher benchmarks
│   ├── matcher_serialization_test.rs
│   └── openstack_accuracy_test.rs # End-to-end LLM template-generation accuracy
├── hover-schema/                  # ClickHouse schema (submodule)
└── docs:
    ├── SERVICE_API.md             # HTTP API
    ├── INGEST_SERVICE_API.md      # Ingest payload shape
    ├── BENCHMARKS.md              # Benchmark guide
    ├── BENCHMARK_ALL_DATASETS.md  # LogHub dataset benchmarks
    └── LLM_CONFIG.md              # LLM provider configuration
```
