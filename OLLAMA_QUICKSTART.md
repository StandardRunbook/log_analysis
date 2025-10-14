# Ollama Quick Start Guide

Quick guide to get started with Ollama for local LLM-powered log template generation.

## 🚀 Quick Setup (5 minutes)

### 1. Install Ollama

```bash
# macOS
brew install ollama

# Linux
curl -fsSL https://ollama.com/install.sh | sh
```

### 2. Start Ollama Server

```bash
ollama serve
```

Leave this terminal running.

### 3. Pull a Model (in new terminal)

```bash
# For quick testing (smaller, faster)
ollama pull phi

# For production (better quality)
ollama pull llama2
```

### 4. Set Environment Variables

```bash
export OLLAMA_ENDPOINT=http://localhost:11434
export OLLAMA_MODEL=phi  # or llama2
```

### 5. Run Benchmarks

```bash
# Basic performance test
cargo test --test radix_trie_benchmark_with_llm benchmark_1k_matched_logs -- --nocapture

# Test with unmatched logs (shows what Ollama would process)
cargo test --test radix_trie_benchmark_with_llm benchmark_with_unmatched -- --nocapture
```

## 📊 Running the Benchmarks

### Without Ollama (basic radix trie performance)
```bash
cargo test --test radix_trie_benchmark benchmark_10k_logs -- --nocapture
```

### With Ollama Configuration (shows unmatched logs)
```bash
OLLAMA_ENDPOINT=http://localhost:11434 OLLAMA_MODEL=phi \
  cargo test --test radix_trie_benchmark_with_llm benchmark_mixed_load -- --nocapture
```

### Show Setup Instructions
```bash
cargo test --test radix_trie_benchmark_with_llm ollama_instructions -- --ignored --nocapture
```

## 🤖 Model Recommendations

| Model | Size | Speed | Best For |
|-------|------|-------|----------|
| `phi` | 2.7B | ⚡⚡⚡ | Quick testing, development |
| `mistral` | 7B | ⚡⚡ | Production, balanced |
| `llama2` | 7B | ⚡⚡ | General purpose |
| `codellama` | 7B | ⚡⚡ | Code/pattern matching |

### Pull Multiple Models
```bash
ollama pull phi
ollama pull llama2
ollama pull mistral
```

### Switch Between Models
```bash
export OLLAMA_MODEL=mistral
# Run tests...

export OLLAMA_MODEL=phi
# Run tests...
```

## 🔧 Configuration Files

### For Production (.env file)
```env
# Add to .env file
LLM_PROVIDER=ollama
OLLAMA_ENDPOINT=http://localhost:11434
OLLAMA_MODEL=llama2
LLM_API_KEY=not-needed-for-ollama
```

### For Testing (environment variables)
```bash
export OLLAMA_ENDPOINT=http://localhost:11434
export OLLAMA_MODEL=llama2
```

## ✅ Verify Setup

```bash
# Check Ollama is running
curl http://localhost:11434/api/version

# List installed models
ollama list

# Test a model directly
ollama run phi "Hello, are you working?"
```

## 📈 Expected Benchmark Results

### Basic Radix Trie (without LLM)
- **Throughput**: ~7,800-8,000 logs/sec
- **Latency**: ~125-130μs per log
- **Match Rate**: 100% for known patterns

### With Unmatched Logs
- **Matched logs**: Same performance as above
- **Unmatched logs**: Would trigger LLM template generation (future feature)
- **Overall throughput**: Depends on matched/unmatched ratio

## 🐛 Troubleshooting

### Ollama not responding
```bash
# Check process
ps aux | grep ollama

# Restart
killall ollama
ollama serve
```

### Model download slow
```bash
# Download smaller model first
ollama pull phi

# Or use a mirror (if available in your region)
```

### Out of memory
```bash
# Use smaller model
ollama pull phi

# Check system resources
ollama ps
```

## 📚 Next Steps

1. **Run basic benchmarks** to understand radix trie performance
2. **Try different Ollama models** to see which works best
3. **Explore the test code** in `tests/radix_trie_benchmark_with_llm.rs`
4. **Read full documentation** in `tests/BENCHMARK_README.md`

## 🔗 Useful Links

- [Ollama GitHub](https://github.com/ollama/ollama)
- [Ollama Models Library](https://ollama.com/library)
- [Benchmark Tests README](tests/BENCHMARK_README.md)
- [Architecture Guide](ARCHITECTURE.md)
