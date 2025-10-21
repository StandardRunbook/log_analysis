# Downloaded LogHub Datasets

## ✅ Successfully Downloaded: 9 Datasets (2.7GB total)

All datasets are located in: `data/loghub/`

### 📊 Dataset Inventory

| Dataset | Lines | Size | Templates | Category | Status |
|---------|-------|------|-----------|----------|--------|
| **Proxifier** | 21,329 | 2.4M | 11 | Standalone Software | ✅ Ready |
| **Linux** | 25,567 | 2.2M | 338 | Operating System | ✅ Ready |
| **Apache** | 56,481 | 4.9M | 29 | Server Application | ✅ Ready |
| **Zookeeper** | 74,380 | 9.9M | 89 | Distributed System | ✅ Ready |
| **Mac** | 117,283 | 16M | 626 | Operating System | ✅ Ready |
| **HealthApp** | 253,395 | 22M | 156 | Standalone Software | ✅ Ready |
| **HPC** | 433,489 | 32M | 74 | Supercomputer | ✅ Ready |
| **BGL** | 4,747,963 | 709M | 320 | Supercomputer | ✅ Ready |
| **HDFS** | 11,175,629 | 1.5G | 46 | Distributed System | ✅ Ready |

**Total**: 16,905,516 log lines across 9 datasets

### 📁 File Structure

```
data/loghub/
├── Apache.log                      # Web server logs
├── BGL.log                         # Blue Gene/L supercomputer
├── HDFS.log                        # Hadoop Distributed File System
├── HealthApp.log                   # Health monitoring app
├── HPC.log                         # High Performance Computing
├── Linux.log                       # Linux system logs
├── Mac.log                         # macOS system logs
├── Proxifier.log                   # Proxy software logs
├── Zookeeper.log                   # ZooKeeper coordination service
├── preprocessed/                   # HDFS preprocessed data
│   ├── HDFS.log_templates.csv
│   ├── Event_occurrence_matrix.csv
│   └── anomaly_label.csv
└── [archives: *.tar.gz, *.zip]    # Original compressed files
```

## 🚀 Quick Start Commands

### Test with Smallest Dataset (Proxifier - 21K lines, 11 templates)

```bash
cd /Volumes/Ankil_SSD/projects/hypothecary-hypothecary-panel/log_analysis

# 1. Create a simple test for Proxifier
cargo run --example generate_templates_ollama --release
# (Modify to use data/loghub/Proxifier.log)

# Expected: ~22 seconds to generate 11 templates
```

### Test with Apache (56K lines, 29 templates)

```bash
# Should take ~58 seconds to generate templates
# Good for testing web server log patterns
```

### Performance Testing with HDFS (11M lines, 46 templates)

```bash
# Large dataset for throughput testing
# Template generation: ~92 seconds
# Matching performance: Our system should handle ~900K logs/sec
```

## 📈 Recommended Testing Order

### Phase 1: Quick Validation (Minutes)
1. **Proxifier** (21K lines, 11 templates) - Simplest dataset
2. **Apache** (56K lines, 29 templates) - Web server patterns

### Phase 2: Moderate Complexity (Hours)
3. **Linux** (25K lines, 338 templates) - Complex OS patterns
4. **Zookeeper** (74K lines, 89 templates) - Distributed coordination
5. **Mac** (117K lines, 626 templates) - Most diverse templates

### Phase 3: Large Scale (Hours to Days)
6. **HealthApp** (253K lines, 156 templates) - Application logs
7. **HPC** (433K lines, 74 templates) - HPC cluster logs
8. **BGL** (4.7M lines, 320 templates) - Supercomputer scale
9. **HDFS** (11M lines, 46 templates) - Massive throughput test

## ⚠️ Datasets NOT Downloaded (Issues)

These datasets had download problems and are **not available**:

- ❌ **Hadoop** - Archive format error
- ❌ **OpenSSH** - Download returned HTML instead of archive
- ❌ **Spark** - Not attempted (16M lines)
- ❌ **Thunderbird** - Not attempted (16M lines)
- ❌ **OpenStack** - Already have separate 2K subset in `data/openstack/`

**Note**: We can retry these later if needed with direct Zenodo access or GitHub repository cloning.

## 🎯 Estimated Performance (Based on OpenStack Results)

Using our current system with Ollama (llama3:latest):

| Dataset | Template Gen Time | Cache Load Time | Match Throughput |
|---------|-------------------|-----------------|------------------|
| Proxifier | ~22 sec | ~5ms | ~900K logs/sec |
| Apache | ~58 sec | ~5ms | ~900K logs/sec |
| Linux | ~11 min | ~10ms | ~900K logs/sec |
| Zookeeper | ~3 min | ~5ms | ~900K logs/sec |
| Mac | ~31 min | ~15ms | ~900K logs/sec |
| HealthApp | ~5 min | ~8ms | ~900K logs/sec |
| HPC | ~2.5 min | ~5ms | ~900K logs/sec |
| BGL | ~10 min | ~10ms | ~900K logs/sec |
| HDFS | ~92 sec | ~5ms | ~900K logs/sec |

**Key Insight**: Template generation is one-time cost. After caching, we get:
- **Load time**: ~5-15ms (instant)
- **Matching**: ~900K logs/second consistently

## 🧪 Next Steps

### 1. Create Generic Dataset Loader

Modify `src/implementations.rs` to support any LogHub dataset:

```rust
pub struct LogHubDatasetLoader {
    log_file: String,
    dataset_name: String,
}

impl LogHubDatasetLoader {
    pub fn new(dataset_name: &str) -> Self {
        Self {
            log_file: format!("data/loghub/{}.log", dataset_name),
            dataset_name: dataset_name.to_string(),
        }
    }
}
```

### 2. Update Template Generator

Modify `examples/generate_templates_ollama.rs` to accept dataset name as argument:

```bash
cargo run --example generate_templates_ollama --release -- Apache
cargo run --example generate_templates_ollama --release -- Proxifier
```

### 3. Run Benchmarks on All Datasets

Create a script to:
1. Generate templates for each dataset
2. Cache them
3. Run performance benchmarks
4. Compare results across datasets

### 4. Cross-Dataset Analysis

- Compare template complexity across different system types
- Identify common log patterns
- Test matcher robustness with diverse formats

## 📝 Usage Examples

### Read a dataset

```rust
use std::fs::File;
use std::io::{BufRead, BufReader};

let file = File::open("data/loghub/Apache.log")?;
let reader = BufReader::new(file);

for (i, line) in reader.lines().enumerate() {
    let log_line = line?;
    println!("Log {}: {}", i, log_line);
    if i >= 10 { break; } // Show first 10 lines
}
```

### Generate and cache templates

```bash
# For Proxifier (quickest test)
cd data/loghub
head -100 Proxifier.log > Proxifier_100.log  # Create sample

# Generate templates
cargo run --example generate_templates_ollama --release
# Output: proxifier_templates.bin, proxifier_templates.json
```

## 📚 Dataset Details

### Proxifier (Simplest - Start Here!)
- **Size**: 21K lines, 2.4MB
- **Templates**: 11 unique patterns
- **Type**: Network proxy client logs
- **Best for**: Initial testing, validation

### Apache (Web Servers)
- **Size**: 56K lines, 4.9MB
- **Templates**: 29 unique patterns
- **Type**: Apache HTTP Server access logs
- **Best for**: Web server log analysis

### Linux (Operating System)
- **Size**: 25K lines, 2.2MB
- **Templates**: 338 unique patterns (most complex!)
- **Type**: Linux system logs (syslog, kern.log, etc.)
- **Best for**: Testing template diversity

### BGL (Supercomputer)
- **Size**: 4.7M lines, 709MB
- **Templates**: 320 unique patterns
- **Type**: IBM Blue Gene/L supercomputer logs
- **Best for**: Large-scale performance testing

### HDFS (Largest Dataset)
- **Size**: 11M lines, 1.5GB
- **Templates**: 46 unique patterns
- **Type**: Hadoop Distributed File System logs
- **Best for**: Maximum throughput testing
- **Note**: Includes preprocessed templates in CSV format

## 💾 Disk Space

Total space used: **2.7GB**

Breakdown:
- Raw logs: 2.3GB
- Compressed archives: 400MB

## 🔄 Re-downloading

If you need to re-download any dataset:

```bash
cd data/loghub

# Example: Re-download Apache
rm Apache.log Apache.tar.gz
curl -L "https://zenodo.org/records/8196385/files/Apache.tar.gz?download=1" -o Apache.tar.gz
tar -xzf Apache.tar.gz
```

## ✨ Success!

All 9 datasets are ready for:
- ✅ Template generation with Ollama
- ✅ Performance benchmarking
- ✅ Cross-dataset validation
- ✅ Matcher robustness testing

Start with **Proxifier** or **Apache** for quick wins! 🚀
