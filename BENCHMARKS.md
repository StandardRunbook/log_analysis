# Log Analyzer Benchmarks

This document describes how to run benchmarks for the log analyzer system.

## Quick Start

All benchmarks are consolidated in `tests/benchmarks.rs`. This is the **canonical way** to benchmark the log analyzer.

### Prerequisites

1. **Generate cached templates** (one-time setup):
   ```bash
   # This creates template cache files in cache/ directory
   # Run this before benchmarking for best performance
   cargo run --release --example regenerate_templates
   ```

2. **Download LogHub datasets** (optional):
   ```bash
   # Place datasets in data/loghub/
   # Each dataset should be in its own directory
   ```

## Available Benchmarks

### 1. Quick Benchmark (Recommended for CI/Testing)

Fast smoke test using 100 logs per dataset with pre-cached templates.

```bash
cargo test --release --test benchmarks quick -- --nocapture
```

**Output:**
- Overall throughput: ~80,000-150,000 logs/sec
- Average accuracy: ~98%
- Execution time: < 1 second

### 2. Throughput Benchmark

Tests pure matching performance with different dataset sizes.

```bash
cargo test --release --test benchmarks throughput -- --nocapture
```

**Tests:**
- Multiple log sizes: 100, 500, 1000, 5000 logs
- Pure matching performance (no template generation)
- Microsecond-level latency measurements

### 3. Parallel Benchmark

Multi-threaded benchmark across all datasets using Rayon.

```bash
cargo test --release --test benchmarks parallel -- --nocapture
```

**Features:**
- Uses all available CPU cores
- Processes 500 logs per dataset
- Demonstrates concurrent matching performance

### 4. Accuracy Benchmark

Template generation + accuracy measurement (slower but comprehensive).

```bash
cargo test --release --test benchmarks accuracy -- --nocapture --ignored
```

**Tests:**
- LLM-based template generation
- Grouping accuracy against ground truth
- End-to-end system performance

### 5. Full Benchmark

Comprehensive benchmark using all logs from all datasets.

```bash
cargo test --release --test benchmarks full -- --nocapture --ignored
```

**Warning:** This can take several minutes depending on dataset sizes.

## Performance Tips

### Always Use Release Mode

**Debug mode is 20-50x slower than release mode!**

```bash
# ❌ WRONG - Debug mode
cargo test --test benchmarks quick

# ✅ CORRECT - Release mode
cargo test --release --test benchmarks quick
```

### Thread Configuration

```bash
# Single-threaded (good for throughput tests)
cargo test --release --test benchmarks throughput -- --nocapture --test-threads=1

# Multi-threaded (default, good for parallel tests)
cargo test --release --test benchmarks parallel -- --nocapture
```

## Results

Benchmark results are automatically saved to `benchmark_results/` directory:

```
benchmark_results/
├── quick_20251021_160202.json      # JSON format
├── quick_20251021_160202.csv       # CSV format
├── parallel_20251021_160305.json
└── parallel_20251021_160305.csv
```

### JSON Format

```json
{
  "benchmark_type": "quick",
  "total_datasets": 16,
  "successful_datasets": 16,
  "total_logs": 1600,
  "total_time_secs": 0.019,
  "overall_throughput": 84441.0,
  "avg_accuracy": 98.46,
  "results": [...]
}
```

### CSV Format

```csv
Dataset,Templates,Logs,Matched,MatchRate,Throughput,LatencyUs,Accuracy
Apache,6,100,100,100.00,356719,2.8,100.00
Linux,105,100,68,68.00,24346,41.1,80.00
...
```

## Expected Performance

With the optimized zero-copy LogMatcher on modern hardware:

| Benchmark | Throughput | Latency | Accuracy |
|-----------|-----------|---------|----------|
| Quick | 80K-150K logs/sec | 6-12 μs | 98%+ |
| Throughput | 200K-700K logs/sec | 1.5-5 μs | N/A |
| Parallel | 150K-250K logs/sec | 4-10 μs | 98%+ |
| Accuracy | 300-500 logs/sec | 2-5 ms | 70-95% |

**Note:** Accuracy benchmark is slower due to template generation overhead.

## Architecture

The consolidated benchmark system:

1. **Zero-Copy Optimizations**:
   - SmallVec for stack allocation
   - Thread-local scratch buffers
   - Inline hints for hot paths
   - Unstable sorting (no allocation)

2. **Batch Processing**:
   - 1,000-10,000 log chunks
   - Amortizes Arc load overhead
   - Better cache locality

3. **Parallel Processing**:
   - Rayon for data parallelism
   - Per-thread scratch spaces
   - Lock-free shared matchers

## Troubleshooting

### No Cached Templates

```
⚠️  No cached templates found. Run template generation first.
```

**Solution:**
```bash
cargo run --release --example regenerate_templates
```

### Low Performance

**Check you're using `--release`:**
```bash
# This shows which profile was used
cargo test --release --test benchmarks quick -- --nocapture 2>&1 | head
```

**Expected:** `Finished release profile [optimized]`

### Dataset Not Found

```
❌ Linux - Error: No cached templates: cache/linux_templates.json
```

**Solutions:**
1. Generate templates: `cargo run --release --example regenerate_templates`
2. Check cache directory exists: `ls cache/`
3. Verify dataset name matches: `ls data/loghub/`

## Comparing with Previous Results

```bash
# Run quick benchmark and save output
cargo test --release --test benchmarks quick -- --nocapture > results_$(date +%Y%m%d).txt

# Compare two runs
diff results_20251020.txt results_20251021.txt
```

Or use the JSON/CSV files for programmatic comparison:

```python
import json

with open('benchmark_results/quick_20251021_160202.json') as f:
    results = json.load(f)

print(f"Overall throughput: {results['overall_throughput']:.0f} logs/sec")
print(f"Average accuracy: {results['avg_accuracy']:.2f}%")
```

## Continuous Integration

Recommended CI configuration:

```yaml
# .github/workflows/benchmark.yml
name: Benchmark
on: [push, pull_request]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run quick benchmark
        run: |
          cd log_analysis
          cargo test --release --test benchmarks quick -- --nocapture

      - name: Upload results
        uses: actions/upload-artifact@v2
        with:
          name: benchmark-results
          path: benchmark_results/
```

## Development

To add a new benchmark mode:

1. Add a new test function in `tests/benchmarks.rs`
2. Use consistent naming: `#[tokio::test]` or `#[test]`
3. Call `save_results()` to save output
4. Update this README

Example:

```rust
#[tokio::test]
async fn my_custom_benchmark() -> anyhow::Result<()> {
    println!("🚀 MY CUSTOM BENCHMARK");

    let datasets = get_cached_datasets();
    let results = benchmark_datasets_with_cache(&datasets, Some(250), true).await?;
    print_summary("custom", &results);

    Ok(())
}
```

## License

See main project LICENSE.
