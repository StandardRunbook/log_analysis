# LogHub Comprehensive Benchmark Guide

This guide shows you how to benchmark **all LogHub datasets** for throughput and grouping accuracy.

## Quick Start

### 1. Quick Test (100 logs per dataset)
```bash
cargo test --test benchmark_all_datasets benchmark_all_datasets_quick -- --nocapture --test-threads=1
```

**Recommended for:** Quick validation that everything works
**Time:** ~30 seconds for all 16 datasets
**Output:** Throughput and accuracy for each dataset

### 2. Sample Test (500 logs per dataset)
```bash
cargo test --test benchmark_all_datasets benchmark_all_datasets_sample -- --ignored --nocapture --test-threads=1
```

**Recommended for:** Balanced testing
**Time:** ~2-3 minutes for all datasets
**Output:** More accurate metrics with reasonable runtime

### 3. Full Test (all logs, ~2000 per dataset)
```bash
cargo test --test benchmark_all_datasets benchmark_all_datasets_full -- --ignored --nocapture --test-threads=1
```

**Recommended for:** Complete accuracy assessment
**Time:** ~5-10 minutes for all datasets
**Output:** Most accurate benchmark results

### 4. Selected Datasets Only
```bash
cargo test --test benchmark_all_datasets benchmark_selected_datasets -- --ignored --nocapture --test-threads=1
```

Tests only: Linux, OpenStack, HDFS, Apache

## Output

### Console Output

The benchmark provides:

1. **Real-time Progress**
   ```
   ================================================================================
   🔍 Benchmarking: Linux
   ================================================================================

   ✅ Linux - 87.23% accuracy, 245 logs/sec
   ```

2. **Summary Table**
   ```
   Dataset Results (sorted by accuracy):
   --------------------------------------------------------------------------------
   Dataset            Logs  Templates    Accuracy  Throughput       Status
   --------------------------------------------------------------------------------
   Linux               500         42       87.23%      245/s           ✅
   OpenStack           500         38       84.50%      198/s           ✅
   HDFS                500         35       82.10%      312/s           ✅
   ```

3. **Top Performers**
   ```
   🏆 Top 5 by Accuracy:
     1. Linux - 87.23%
     2. OpenStack - 84.50%
     3. HDFS - 82.10%

   ⚡ Top 5 by Throughput:
     1. HDFS - 312 logs/sec
     2. Linux - 245 logs/sec
     3. OpenStack - 198 logs/sec
   ```

### Saved Files

Results are automatically saved to `benchmark_results/`:

1. **JSON File**: `loghub_benchmark_YYYYMMDD_HHMMSS.json`
   - Complete results with all metrics
   - Useful for programmatic analysis

2. **CSV File**: `loghub_benchmark_YYYYMMDD_HHMMSS.csv`
   - Easy to open in Excel/Google Sheets
   - Quick visual analysis

Example JSON structure:
```json
{
  "total_datasets": 16,
  "successful_datasets": 14,
  "failed_datasets": 2,
  "total_logs_processed": 7000,
  "total_time_secs": 28.5,
  "average_throughput": 245.6,
  "average_accuracy": 78.4,
  "results": [
    {
      "dataset_name": "Linux",
      "total_logs": 500,
      "templates_generated": 42,
      "throughput": 245.0,
      "grouping_accuracy": 87.23,
      "expected_groups": 38,
      "actual_groups": 42,
      "success": true
    }
  ]
}
```

## Available Datasets

The benchmark automatically discovers all datasets in `data/loghub/`:

- Android
- Apache
- BGL
- HDFS
- HPC
- Hadoop
- HealthApp
- Linux
- Mac
- OpenSSH
- OpenStack
- Proxifier
- Spark
- Thunderbird
- Windows
- Zookeeper

## Metrics Explained

### Throughput
- **What:** Logs processed per second
- **Higher is better**
- **Typical range:** 150-400 logs/sec (mock generator)

### Grouping Accuracy
- **What:** Percentage of logs correctly grouped by template
- **Calculation:** Compares generated groups with ground truth
- **Higher is better**
- **Typical range:** 60-90%

### Templates Generated
- **What:** Number of unique templates created
- **Compare to:** Expected groups (from ground truth)
- **Ideal:** Close to expected groups (not too many, not too few)

### Latency
- **What:** Average time to process one log
- **Lower is better**
- **Typical range:** 2-6 ms per log

## Customization

### Change Sample Size

Edit the test to use different sample sizes:

```rust
// Test with 1000 logs per dataset
benchmark_dataset(dataset, Some(1000)).await
```

### Test Specific Datasets

Modify the `benchmark_selected_datasets` function:

```rust
let selected = vec!["Linux", "OpenStack", "MyCustomDataset"];
```

### Add Metadata

Add custom metadata to results:

```rust
let mut metadata = HashMap::new();
metadata.insert("test_run".to_string(), "production".to_string());

let config = BenchmarkConfig {
    metadata,
    ..Default::default()
};
```

## Troubleshooting

### "No datasets found"
```
⚠️  No datasets found in data/loghub/
```

**Solution:** Download LogHub datasets to `data/loghub/`:
```bash
# Each dataset needs these files:
data/loghub/Linux/
├── Linux_2k.log
├── Linux_2k.log_structured.csv
└── Linux_2k.log_templates.csv
```

### Dataset Fails with Error
```
❌ Linux - Error: Failed to read log file
```

**Common causes:**
1. Missing files (need all 3: .log, _structured.csv, _templates.csv)
2. Incorrect file format
3. Permissions issue

**Solution:** Check that all required files exist and are readable

### Low Accuracy
If a dataset shows very low accuracy (< 50%):

1. **Check template quality**: The mock generator may not work well for that log format
2. **Try with real LLM**: Use Ollama or OpenAI instead of mock generator
3. **Check ground truth**: Verify the ground truth files are correct

## Performance Tips

### Parallel Execution
⚠️ **Do NOT use `--test-threads`** > 1 for this benchmark!

The benchmark uses `--test-threads=1` to:
- Avoid memory conflicts
- Get consistent timing
- Prevent resource contention

### Optimization for Speed

1. **Use sample size**: Test with 100-500 logs instead of all
2. **Test selected datasets**: Only benchmark datasets you care about
3. **Build with release**: Add `--release` flag for 2-3x speedup

```bash
cargo test --release --test benchmark_all_datasets benchmark_all_datasets_quick -- --nocapture --test-threads=1
```

## Comparing Results

### Compare Different Runs

```bash
# Run 1: Sample size
cargo test --test benchmark_all_datasets benchmark_all_datasets_sample -- --ignored --nocapture --test-threads=1

# Run 2: Full size
cargo test --test benchmark_all_datasets benchmark_all_datasets_full -- --ignored --nocapture --test-threads=1

# Compare the saved JSON files
diff benchmark_results/loghub_benchmark_*.json
```

### Analyze Results

Use the CSV files for easy analysis:

1. Open in Excel/Google Sheets
2. Sort by accuracy or throughput
3. Create charts
4. Identify patterns

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: Benchmark All Datasets

on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
      - name: Download datasets
        run: ./scripts/download_loghub.sh
      - name: Run benchmark
        run: |
          cargo test --test benchmark_all_datasets benchmark_all_datasets_sample \
            -- --ignored --nocapture --test-threads=1
      - name: Upload results
        uses: actions/upload-artifact@v2
        with:
          name: benchmark-results
          path: benchmark_results/
```

## Example Usage Session

```bash
# 1. Quick validation
$ cargo test --test benchmark_all_datasets benchmark_all_datasets_quick -- --nocapture --test-threads=1

================================================================================
📊 LOGHUB COMPREHENSIVE BENCHMARK
================================================================================
Testing: 100 logs per dataset

Found 16 datasets: ["Android", "Apache", "BGL", ...]

================================================================================
🔍 Benchmarking: Linux
================================================================================

✅ Linux - 85.00% accuracy, 234 logs/sec

[... more datasets ...]

================================================================================
📊 BENCHMARK SUMMARY
================================================================================

Overall Statistics:
  Total datasets:        16
  Successful:            14 ✅
  Failed:                2 ❌
  Total logs processed:  1400
  Total time:            6.23s
  Average throughput:    224 logs/sec
  Average accuracy:      76.45%

💾 Results saved to: benchmark_results/loghub_benchmark_20251021_143022.json
💾 CSV saved to: benchmark_results/loghub_benchmark_20251021_143022.csv
```

## Next Steps

1. **Analyze results** - Open the CSV in your spreadsheet tool
2. **Identify issues** - Focus on datasets with low accuracy
3. **Optimize** - Tune the matching algorithm for problematic datasets
4. **Iterate** - Re-run benchmarks to measure improvements

---

**Need help?** Check the main [BENCHMARK.md](BENCHMARK.md) or create an issue.
