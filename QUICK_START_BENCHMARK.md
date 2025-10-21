# Quick Start: Benchmark All LogHub Datasets

## TL;DR

Run this command to benchmark all 16 LogHub datasets:

```bash
cargo test --test benchmark_all_datasets benchmark_all_datasets_quick -- --nocapture --test-threads=1
```

**Time:** ~30 seconds  
**Output:** Console table + saved JSON/CSV files

## What It Does

✅ Tests **all 16 datasets** in `data/loghub/`  
✅ Measures **throughput** (logs/sec)  
✅ Measures **grouping accuracy** (vs ground truth)  
✅ Saves results to `benchmark_results/`  
✅ Shows rankings (top 5 by accuracy & throughput)

## Example Output

```
================================================================================
📊 LOGHUB COMPREHENSIVE BENCHMARK
================================================================================
Testing: 100 logs per dataset

Found 16 datasets: ["Android", "Apache", "BGL", "HDFS", ...]

✅ Android - 74.00% accuracy, 44 logs/sec
✅ Apache - 97.00% accuracy, 1289 logs/sec
✅ BGL - 100.00% accuracy, 31 logs/sec
...

================================================================================
📊 BENCHMARK SUMMARY
================================================================================

Overall Statistics:
  Total datasets:        16
  Successful:            16 ✅
  Failed:                0 ❌
  Total logs processed:  1600
  Total time:            28.5s
  Average throughput:    285 logs/sec
  Average accuracy:      82.3%

Dataset Results (sorted by accuracy):
--------------------------------------------------------------------------------
Dataset            Logs  Templates    Accuracy  Throughput       Status
--------------------------------------------------------------------------------
BGL                 100          1      100.00%       31/s           ✅
Apache              100          5       97.00%     1289/s           ✅
HPC                 100         14       94.00%      512/s           ✅
...

🏆 Top 5 by Accuracy:
  1. BGL - 100.00%
  2. Apache - 97.00%
  3. HPC - 94.00%

⚡ Top 5 by Throughput:
  1. Apache - 1289 logs/sec
  2. HPC - 512 logs/sec
  3. Linux - 245 logs/sec

💾 Results saved to: benchmark_results/loghub_benchmark_20251021_143022.json
💾 CSV saved to: benchmark_results/loghub_benchmark_20251021_143022.csv
```

## Other Commands

### More Logs (500 per dataset, ~2 minutes)
```bash
cargo test --test benchmark_all_datasets benchmark_all_datasets_sample -- --ignored --nocapture --test-threads=1
```

### All Logs (~2000 per dataset, ~5-10 minutes)
```bash
cargo test --test benchmark_all_datasets benchmark_all_datasets_full -- --ignored --nocapture --test-threads=1
```

### Selected Datasets Only
```bash
cargo test --test benchmark_all_datasets benchmark_selected_datasets -- --ignored --nocapture --test-threads=1
```

## Results Files

Automatically saved to `benchmark_results/`:

- **JSON:** `loghub_benchmark_TIMESTAMP.json` - Complete data
- **CSV:** `loghub_benchmark_TIMESTAMP.csv` - Open in Excel/Sheets

## Next Steps

1. **View results:** Open the CSV file in a spreadsheet
2. **Analyze:** Compare accuracy and throughput across datasets
3. **Optimize:** Focus on datasets with low accuracy
4. **Iterate:** Make changes and re-benchmark

---

📖 **Full guide:** [BENCHMARK_ALL_DATASETS.md](BENCHMARK_ALL_DATASETS.md)
