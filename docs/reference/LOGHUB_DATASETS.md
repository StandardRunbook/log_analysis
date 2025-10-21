# LogHub Datasets - Complete Reference

LogHub-2.0 is a large-scale collection of annotated log datasets for AI-driven log analytics and parsing research. Published at ISSTA'24, it contains **50.4 million annotated log messages** across 14 datasets.

## 📊 Available Datasets

### Distributed Systems

#### 1. **Hadoop**
- **Logs**: 179,993 messages
- **Templates**: 236 unique patterns
- **Source**: Hadoop distributed processing framework
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Hadoop.tar.gz?download=1

#### 2. **HDFS**
- **Logs**: 11,167,740 messages
- **Templates**: 46 unique patterns
- **Source**: Hadoop Distributed File System
- **Format**: ZIP
- **Download**: https://zenodo.org/records/8196385/files/HDFS_v1.zip?download=1

#### 3. **OpenStack**
- **Logs**: 207,632 messages
- **Templates**: 48 unique patterns
- **Source**: OpenStack cloud computing platform
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/OpenStack.tar.gz?download=1
- **Note**: This is the dataset we're currently using (we have OpenStack_2k subset)

#### 4. **Spark**
- **Logs**: 16,075,117 messages
- **Templates**: 236 unique patterns
- **Source**: Apache Spark distributed computing
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Spark.tar.gz?download=1

#### 5. **Zookeeper**
- **Logs**: 74,273 messages
- **Templates**: 89 unique patterns
- **Source**: Apache ZooKeeper coordination service
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Zookeeper.tar.gz?download=1

### Supercomputer Systems

#### 6. **BGL** (Blue Gene/L)
- **Logs**: 4,631,261 messages
- **Templates**: 320 unique patterns
- **Source**: IBM Blue Gene/L supercomputer
- **Format**: ZIP
- **Download**: https://zenodo.org/records/8196385/files/BGL.zip?download=1

#### 7. **HPC**
- **Logs**: 429,987 messages
- **Templates**: 74 unique patterns
- **Source**: High Performance Computing cluster
- **Format**: ZIP
- **Download**: https://zenodo.org/records/8196385/files/HPC.zip?download=1

#### 8. **Thunderbird**
- **Logs**: 16,601,745 messages
- **Templates**: 1,241 unique patterns
- **Source**: Thunderbird supercomputer
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Thunderbird.tar.gz?download=1

### Operating Systems

#### 9. **Linux**
- **Logs**: 23,921 messages
- **Templates**: 338 unique patterns
- **Source**: Linux system logs
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Linux.tar.gz?download=1

#### 10. **Mac**
- **Logs**: 100,314 messages
- **Templates**: 626 unique patterns
- **Source**: macOS system logs
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Mac.tar.gz?download=1

### Server Applications

#### 11. **Apache**
- **Logs**: 51,977 messages
- **Templates**: 29 unique patterns
- **Source**: Apache HTTP Server
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Apache.tar.gz?download=1

#### 12. **OpenSSH**
- **Logs**: 638,946 messages
- **Templates**: 38 unique patterns
- **Source**: OpenSSH server logs
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/OpenSSH.tar.gz?download=1

### Standalone Software

#### 13. **HealthApp**
- **Logs**: 212,394 messages
- **Templates**: 156 unique patterns
- **Source**: Health monitoring application
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/HealthApp.tar.gz?download=1

#### 14. **Proxifier**
- **Logs**: 21,320 messages
- **Templates**: 11 unique patterns
- **Source**: Proxifier network proxy software
- **Format**: TAR.GZ
- **Download**: https://zenodo.org/records/8196385/files/Proxifier.tar.gz?download=1

## 📈 Dataset Statistics Summary

| Category | Datasets | Total Logs | Avg Templates |
|----------|----------|------------|---------------|
| Distributed Systems | 5 | 27,704,755 | 131 |
| Supercomputers | 3 | 21,662,993 | 545 |
| Operating Systems | 2 | 124,235 | 482 |
| Server Applications | 2 | 690,923 | 34 |
| Standalone Software | 2 | 233,714 | 84 |
| **TOTAL** | **14** | **50,416,620** | **249 avg** |

## 📥 How to Download

### Option 1: Direct Download (Recommended for individual datasets)

```bash
# Example: Download Hadoop dataset
wget https://zenodo.org/records/8196385/files/Hadoop.tar.gz?download=1 -O Hadoop.tar.gz
tar -xzf Hadoop.tar.gz

# Example: Download HDFS dataset
wget https://zenodo.org/records/8196385/files/HDFS_v1.zip?download=1 -O HDFS_v1.zip
unzip HDFS_v1.zip
```

### Option 2: Download All Datasets

```bash
# Create datasets directory
mkdir -p loghub_datasets
cd loghub_datasets

# Download all TAR.GZ datasets
for dataset in Hadoop Spark Zookeeper OpenStack Thunderbird Linux Mac Apache OpenSSH HealthApp Proxifier; do
  wget https://zenodo.org/records/8196385/files/${dataset}.tar.gz?download=1 -O ${dataset}.tar.gz
  tar -xzf ${dataset}.tar.gz
done

# Download all ZIP datasets
for dataset in HDFS_v1 BGL HPC; do
  wget https://zenodo.org/records/8196385/files/${dataset}.zip?download=1 -O ${dataset}.zip
  unzip ${dataset}.zip
done
```

### Option 3: Using the LogHub GitHub Repository

```bash
git clone https://github.com/logpai/loghub-2.0.git
cd loghub-2.0
# Follow repository instructions for dataset access
```

## 📁 Dataset Structure

Each dataset typically contains:

```
Dataset_Name/
├── Dataset_Name.log              # Raw log file
├── Dataset_Name_2k.log           # Sample subset (2000 lines)
├── Dataset_Name_templates.csv    # Ground truth templates
└── Dataset_Name_structured.csv   # Structured logs with template IDs
```

**CSV Format (structured logs):**
- `LineId`: Line number
- `Content`: Raw log message
- `EventId`: Template ID (ground truth)
- `EventTemplate`: The template pattern

**CSV Format (templates):**
- `EventId`: Unique template identifier
- `EventTemplate`: The regex or pattern
- `Occurrences`: Number of times this template appears

## 🔬 Use Cases

1. **Log Parsing**: Evaluate log parsing algorithms
2. **Anomaly Detection**: Train models to detect unusual patterns
3. **Template Generation**: Benchmark LLM-based template generation
4. **Performance Testing**: Test matcher performance with large datasets
5. **Cross-System Analysis**: Compare log patterns across different systems

## 🚀 Quick Start with Our System

```bash
# Download a dataset (e.g., Apache)
cd /path/to/log_analysis/data
mkdir apache
cd apache
wget https://zenodo.org/records/8196385/files/Apache.tar.gz?download=1 -O Apache.tar.gz
tar -xzf Apache.tar.gz

# Generate templates with Ollama
cargo run --example generate_templates_ollama --release
# (modify the example to point to apache dataset)

# Run benchmark
cargo test --test benchmark_with_preloaded --release -- --nocapture
```

## 📚 Citations

If you use LogHub datasets in your research, please cite:

**LogHub-2.0 (ISSTA'24):**
```bibtex
@inproceedings{jiang2024loghub2,
  title={A Large-Scale Evaluation for Log Parsing Techniques: How Far Are We?},
  author={Jiang, Zhihan and others},
  booktitle={Proceedings of the 33rd ACM SIGSOFT International Symposium on Software Testing and Analysis},
  year={2024}
}
```

**Original LogHub (ISSRE'23):**
```bibtex
@inproceedings{zhu2023loghub,
  title={Loghub: A Large Collection of System Log Datasets for AI-driven Log Analytics},
  author={Zhu, Jieming and He, Shilin and others},
  booktitle={IEEE International Symposium on Software Reliability Engineering},
  year={2023}
}
```

## 🔗 Resources

- **LogHub-2.0 GitHub**: https://github.com/logpai/loghub-2.0
- **Original LogHub GitHub**: https://github.com/logpai/loghub
- **Zenodo Repository**: https://zenodo.org/records/8196385
- **ISSTA'24 Paper**: https://zbchern.github.io/papers/issta24.pdf
- **ArXiv**: https://arxiv.org/abs/2308.10828

## 💡 Next Steps for Our Project

### High-Priority Datasets to Test

1. **Apache** (51K logs, 29 templates) - Web server logs, good for quick testing
2. **Linux** (24K logs, 338 templates) - More complex patterns
3. **HDFS** (11M logs, 46 templates) - Large-scale performance testing
4. **Spark** (16M logs, 236 templates) - Largest dataset for stress testing

### Recommended Approach

1. Start with **Apache** or **Linux** (smaller, faster to process)
2. Validate our matcher works across different log formats
3. Generate templates with Ollama and cache them
4. Run performance benchmarks
5. Compare results with other log parsing tools
6. Scale up to larger datasets (HDFS, Spark, Thunderbird)

## 📊 Performance Expectations

Based on our current OpenStack results:

- **Template Generation**: ~2s per template with Ollama
- **Template Loading**: ~5ms for cached templates
- **Matching Throughput**: ~900K logs/sec with Aho-Corasick

**Estimated times for each dataset:**

| Dataset | Templates | Gen Time | Matching Time |
|---------|-----------|----------|---------------|
| Proxifier | 11 | ~22s | <1ms |
| Apache | 29 | ~58s | <1ms |
| OpenSSH | 38 | ~76s | <1ms |
| HDFS | 46 | ~92s | ~12ms |
| Linux | 338 | ~11min | <1ms |
| BGL | 320 | ~10min | ~5ms |
| Spark | 236 | ~8min | ~18ms |
| Thunderbird | 1,241 | ~41min | ~19ms |
