# High-Performance Log Analyzer

A blazing-fast log analysis system with zero-copy optimizations, batch processing, and parallel processing capabilities.

## 🚀 Quick Start

```bash
# Run the HTTP service
cargo run --release --bin log-analyzer-service

# Service starts on http://localhost:3000
# Test it:
curl http://localhost:3000/health
```

## ⚡ Performance

- **Average throughput:** 370,000 logs/sec
- **Peak throughput:** 876,000 logs/sec (Spark logs)
- **Latency:** 1.1-21.7 μs per log
- **Accuracy:** 99.86% average grouping accuracy

### Optimization Features

✅ **Zero-Copy Processing**
- SmallVec for stack allocation
- Thread-local scratch buffers
- No allocations during matching

✅ **Batch Processing**
- Sequential: ~160K logs/sec
- Parallel: ~370K logs/sec

✅ **Multi-Threading**
- Rayon-based parallelism
- Lock-free concurrent matching
- Auto-scales with CPU cores

✅ **Advanced Algorithms**
- Aho-Corasick multi-pattern DFA
- FxHashMap for fast hashing
- Persistent data structures (copy-on-write)

## 📖 Documentation

- **[SERVICE_API.md](SERVICE_API.md)** - Complete HTTP API documentation
- **[BENCHMARKS.md](BENCHMARKS.md)** - Benchmark guide with 5 modes
- **[OPTIMIZATIONS.md](OPTIMIZATIONS.md)** - Detailed optimization documentation

## 🔧 Usage

### As a Service (HTTP API)

```bash
# Start the service
cargo run --release --bin log-analyzer-service

# Match a single log
curl -X POST http://localhost:3000/match \
  -H 'Content-Type: application/json' \
  -d '{"log_line": "ERROR: connection timeout"}'

# Batch matching
curl -X POST http://localhost:3000/match/batch \
  -H 'Content-Type: application/json' \
  -d '{"log_lines": ["ERROR: test1", "INFO: test2"]}'
```

### As a Library

```rust
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};

// Create matcher
let mut matcher = LogMatcher::new();

// Add template
matcher.add_template(LogTemplate {
    template_id: 1,
    pattern: r"ERROR: (.*)".to_string(),
    variables: vec!["message".to_string()],
    example: "ERROR: connection timeout".to_string(),
});

// Match single log
let result = matcher.match_log("ERROR: connection failed");
assert_eq!(result, Some(1));

// Batch processing
let logs = vec!["ERROR: test1", "ERROR: test2", "ERROR: test3"];
let results = matcher.match_batch(&logs);

// Parallel processing (for large batches)
let large_batch: Vec<&str> = (0..10000)
    .map(|i| "ERROR: test")
    .collect();
let results = matcher.match_batch_parallel(&large_batch);
```

## 📊 Benchmarks

Run benchmarks to see the optimizations in action:

```bash
# Quick benchmark (100 logs per dataset)
cargo test --release --test benchmarks quick -- --nocapture

# Throughput benchmark (pure matching speed)
cargo test --release --test benchmarks throughput -- --nocapture

# Parallel benchmark (multi-threaded)
cargo test --release --test benchmarks parallel -- --nocapture
```

**Expected results:**
```
Overall throughput:    84,441 logs/sec 🚀
Avg dataset throughput:156,025 logs/sec
Avg accuracy:          98.46%
```

## 🏗️ Architecture

### Core Components

```
log_analysis/
├── src/
│   ├── log_matcher.rs          # Optimized matcher with zero-copy
│   ├── main.rs                 # HTTP service
│   ├── traits.rs               # Trait definitions
│   └── implementations.rs      # Service implementations
├── tests/
│   └── benchmarks.rs          # Consolidated benchmark suite
├── SERVICE_API.md             # HTTP API docs
├── BENCHMARKS.md              # Benchmark guide
└── OPTIMIZATIONS.md           # Optimization details
```

### Key Technologies

- **Aho-Corasick** - Multi-pattern DFA for fast fragment matching
- **Rayon** - Data parallelism for batch processing
- **SmallVec** - Stack allocation for small collections
- **FxHashMap** - Fast non-cryptographic hashing
- **Arc-Swap** - Lock-free atomic updates
- **Axum** - High-performance HTTP framework

## 📝 License

See LICENSE file for details.
