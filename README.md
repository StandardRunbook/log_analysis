# Log Analysis System

A high-performance log parsing and analysis system using Aho-Corasick pattern matching and LLM-based template generation.

## Quick Start

### Run Benchmarks

```bash
# Quick benchmark test (100 logs)
cargo test --test benchmark benchmark_throughput -- --nocapture

# Full benchmark with accuracy metrics
cargo test --test benchmark benchmark_openstack -- --ignored --nocapture

# Compare multiple datasets
cargo test --test benchmark benchmark_comparison -- --ignored --nocapture
```

### Run Tests

```bash
# Run all unit tests
cargo test --lib

# Run configuration tests
cargo test --test test_config

# Run serialization tests
cargo test --test matcher_serialization_test
```

## Features

### Core Engine
- **High-performance matching** - Aho-Corasick algorithm + regex validation
- **Configurable** - Streaming, batch, or bulk processing modes
- **Persistent** - Save/load matcher state in binary or JSON

### Template Generation
- **LLM-based** - OpenAI, Anthropic, Ollama support
- **Format-aware** - Automatic detection of log formats
- **Semantic** - Template generation based on log structure

### Benchmarking
- **Comprehensive metrics** - Throughput, latency, accuracy
- **Multiple datasets** - OpenStack, Linux, HDFS, Apache
- **Easy to extend** - Dependency injection pattern

## Architecture

```
src/
├── Core Matching
│   ├── log_matcher.rs          # Aho-Corasick + regex engine
│   ├── matcher_config.rs       # Configuration
│   └── log_format_detector.rs  # Format detection
│
├── LLM Integration
│   └── llm_service.rs          # LLM API client
│
├── Template Generation
│   ├── smart_template_generator.rs
│   ├── semantic_template_generator.rs
│   ├── token_classifier.rs
│   ├── pattern_learner.rs
│   └── fragment_classifier.rs
│
└── Benchmarking
    ├── benchmark_runner.rs     # Execution framework
    ├── traits.rs               # DI traits
    ├── implementations.rs      # DI implementations
    ├── loghub_loader.rs       # Dataset loading
    └── dataset_splitter.rs    # Data splitting
```

## Performance

- **~30K logs/sec** - Batch processing mode
- **~28K logs/sec** - Streaming mode (lower latency)
- **70-90% accuracy** - Template grouping accuracy vs ground truth

## Configuration

### Streaming Mode (Low Latency)
```rust
use log_analyzer::matcher_config::MatcherConfig;
use log_analyzer::log_matcher::LogMatcher;

let config = MatcherConfig::streaming();
let matcher = LogMatcher::with_config(config);
```

### Batch Processing Mode (High Throughput)
```rust
let config = MatcherConfig::batch_processing();
let matcher = LogMatcher::with_config(config);
```

### Custom Configuration
```rust
let config = MatcherConfig::new()
    .with_match_kind(MatchKind::LeftmostLongest)
    .with_min_fragment_length(3)
    .with_batch_size(5_000);
```

## Datasets

Supports LogHub-2.0 datasets:
- OpenStack
- Linux
- HDFS
- Apache
- And more...

Place datasets in `data/loghub/<dataset>/`:
```
data/loghub/Linux/
├── Linux_2k.log
├── Linux_2k.log_structured.csv
└── Linux_2k.log_templates.csv
```

## Documentation

- **[BENCHMARK.md](BENCHMARK.md)** - Complete benchmarking guide
- **[CLEANUP.md](CLEANUP.md)** - Codebase cleanup summary
- **[docs/README.md](docs/README.md)** - Additional documentation

## Development

### Build
```bash
cargo build --release
```

### Test
```bash
cargo test
```

### Benchmark
```bash
cargo test --test benchmark -- --ignored --nocapture
```

## Examples

See `examples/` directory for:
- Template generation from datasets
- Building DFAs from templates
- LLM-based template generation
- Semantic template generation

```bash
# Generate templates using Ollama
cargo run --example generate_templates_ollama

# Build templates from CSV
cargo run --example build_templates_from_csv
```

## License

See LICENSE file for details.

## Acknowledgments

- LogHub-2.0 dataset (ISSTA'24)
- Aho-Corasick algorithm implementation
- OpenAI/Anthropic/Ollama for LLM support
