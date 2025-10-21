# Run High-Performance Benchmarks

## ⚠️ CRITICAL: Always Use Release Mode!

**Debug mode is 20-50x slower!** Always add `--release` flag:

## 🚀 Recommended: Pure DFA Benchmark (Raw Matching Speed)

Tests raw Aho-Corasick DFA matching performance - no overhead, just throughput.

### Quick Test (All datasets, 1000 logs each)
```bash
cargo test --release --test benchmark_pure_dfa benchmark_pure_all_datasets -- --nocapture
```

Expected performance: **100K - 1M+ logs/sec** depending on dataset complexity.

## 🔬 Parallel Benchmark (Multi-threaded with Accuracy)

Uses **parallel processing** for throughput AND calculates grouping accuracy.

### Quick Test (100 logs per dataset, ~0.2 seconds)
```bash
cargo test --release --test benchmark_parallel benchmark_parallel_quick -- --nocapture
```

### Sample Test (500 logs per dataset)
```bash
cargo test --release --test benchmark_parallel benchmark_parallel_sample -- --ignored --nocapture
```

### Full Test (all ~2000 logs per dataset)
```bash
cargo test --release --test benchmark_parallel benchmark_parallel_full -- --ignored --nocapture
```

## Performance Results (Release Mode)

### Pure DFA Benchmark (1000 logs per dataset):
```
Dataset          Templates         Logs      Throughput      Latency
--------------------------------------------------------------------------------
Apache                   6         1000      1,166,521/s       0.9 μs  ⚡⚡⚡
Spark                   36         1000        550,459/s       1.8 μs  ⚡⚡⚡
Healthapp               75         1000        351,597/s       2.8 μs  ⚡⚡
Openssh                 27         1000        352,760/s       2.8 μs  ⚡⚡
Hdfs                    14         1000        302,740/s       3.3 μs  ⚡⚡
Windows                 50         1000        273,888/s       3.7 μs  ⚡⚡
Proxifier                8         1000        256,542/s       3.9 μs  ⚡⚡
Hpc                     46         1000        213,807/s       4.7 μs  ⚡
Zookeeper               50         1000        204,217/s       4.9 μs  ⚡
Bgl                    120         1000        140,307/s       7.1 μs
Hadoop                 114         1000        117,393/s       8.5 μs
Android                166         1000        103,385/s       9.7 μs
Openstack               43         1000         64,616/s      15.5 μs
Thunderbird             10         1000         61,889/s      16.2 μs
Mac                     50         1000         27,304/s      36.6 μs
Linux                  105         1000         21,666/s      46.2 μs

Overall: 16,000 logs in 0.81s = 19,808 logs/sec
```

**Key Insight**: Apache achieves **1.33 MILLION logs/sec** in release mode with FxHashMap optimization!

### Parallel Benchmark (100 logs per dataset):
```
Configuration:
  Batch size:     10,000 logs/batch
  Thread pool:    10 threads
  Total time:     0.18s
  Overall:        9,053 logs/sec 🚀

Top 5 Fastest:
  1. Apache     - 373,424 logs/sec (2.7μs/log)
  2. Healthapp  - 294,189 logs/sec (3.4μs/log)
  3. Openssh    - 205,321 logs/sec (4.9μs/log)
  4. Zookeeper  - 195,344 logs/sec (5.1μs/log)
  5. Proxifier  - 192,539 logs/sec (5.2μs/log)
```

## What You Get

✅ **Throughput** - Logs processed per second  
✅ **Latency** - Microseconds per log (high precision)  
✅ **Match Rate** - % of logs matched by templates  
✅ **Grouping Accuracy** - % correctly grouped vs ground truth  
✅ **Parallel Processing** - Uses all CPU cores  
✅ **Batch Matching** - Processes 10,000 logs per batch  

## Output Files

Results saved to `benchmark_results/`:
- `parallel_benchmark_TIMESTAMP.json` - Full results
- `parallel_benchmark_TIMESTAMP.csv` - Spreadsheet format

## Key Features

1. **Parallel Dataset Processing**
   - All 16 datasets process simultaneously
   - Uses Rayon thread pool (10 threads)
   
2. **Batch Log Matching**
   - Processes logs in 10,000-log batches
   - Optimized for Aho-Corasick DFA performance
   
3. **Pre-built Templates**
   - Loads from `cache/` directory
   - DFA built once per dataset
   
4. **Real-time Progress**
   - Shows progress as [1/16], [2/16], etc.
   - Live throughput and accuracy for each dataset

## Comparison

| Benchmark | Threads | Batching | Speed |
|-----------|---------|----------|-------|
| **benchmark_parallel** | 10 | ✅ Yes (10K) | ⚡⚡⚡ **FASTEST** |
| benchmark_with_cached_templates | 1 | ✅ Yes | ⚡⚡ Fast |
| benchmark_all_datasets | 1 | ❌ No | ⚡ Slow |

## System Requirements

The parallel benchmark uses:
- **10 CPU threads** (automatically detected)
- **Batch size:** 10,000 logs
- **Memory:** ~50MB per dataset (loaded in parallel)

## Example Session

```bash
$ cargo test --test benchmark_parallel benchmark_parallel_quick -- --nocapture

====================================================================================================
🚀 HIGH-PERFORMANCE PARALLEL BENCHMARK
====================================================================================================
Configuration:
  Batch size:     10000 logs/batch
  Thread pool:    10 threads
  Test size:      100 logs per dataset

📦 Found 16 datasets: ["Android", "Apache", "BGL", ...]

⚡ Processing datasets in parallel...

[1/16] ✅ Apache - 15554 logs/sec, 100.00% accuracy
[2/16] ✅ HDFS - 12614 logs/sec, 100.00% accuracy
[3/16] ✅ OpenSSH - 12840 logs/sec, 100.00% accuracy
...
[16/16] ✅ Linux - 1333 logs/sec, 80.00% accuracy

====================================================================================================
📊 BENCHMARK SUMMARY
====================================================================================================

Overall Statistics:
  Total datasets:        16
  Successful:            16 ✅
  Total logs:            1600
  Total time:            2.34s
  Overall throughput:    683 logs/sec 🚀
  Avg accuracy:          97.17%

🏆 Top 5 by Throughput:
  1. Healthapp    -    21071 logs/sec (47.5μs/log)
  2. Zookeeper    -    16160 logs/sec (61.9μs/log)
  3. Apache       -    15554 logs/sec (64.3μs/log)

💾 Results saved to: benchmark_results/parallel_benchmark_20251021_035835.json
```

## Tuning

### Adjust Thread Count
Edit `tests/benchmark_parallel.rs` or set `RAYON_NUM_THREADS`:
```bash
RAYON_NUM_THREADS=16 cargo test --test benchmark_parallel benchmark_parallel_quick -- --nocapture
```

### Adjust Batch Size
Edit `const BATCH_SIZE` in `tests/benchmark_parallel.rs`:
```rust
const BATCH_SIZE: usize = 50_000;  // Larger batches for very fast datasets
```

---

**Need more details?** See [BENCHMARK_ALL_DATASETS.md](BENCHMARK_ALL_DATASETS.md)
