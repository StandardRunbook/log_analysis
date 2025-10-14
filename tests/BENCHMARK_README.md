# Radix Trie Benchmark Tests

This directory contains comprehensive benchmark tests for the log analysis radix trie implementation.

## Available Benchmarks

### 1. Basic Performance Benchmarks (`radix_trie_benchmark.rs`)

Tests the radix trie matching performance with various log volumes:

```bash
# Run individual benchmarks
cargo test --test radix_trie_benchmark benchmark_1k_logs -- --nocapture
cargo test --test radix_trie_benchmark benchmark_10k_logs -- --nocapture
cargo test --test radix_trie_benchmark benchmark_100k_logs -- --nocapture
cargo test --test radix_trie_benchmark benchmark_1m_logs -- --nocapture

# Run all scales sequentially
cargo test --test radix_trie_benchmark benchmark_all_scales -- --nocapture --test-threads=1
```

**Expected Performance:**
- Throughput: ~7,800-8,000 logs/sec
- Average latency: ~125-130μs per log
- 100% match rate for generated test logs

### 2. LLM-Enhanced Benchmarks (`radix_trie_benchmark_with_llm.rs`)

Tests with both matched and unmatched logs, ready for Ollama integration:

```bash
# Run matched logs only
cargo test --test radix_trie_benchmark_with_llm benchmark_1k_matched_logs -- --nocapture

# Run with unmatched logs (shows what Ollama would process)
cargo test --test radix_trie_benchmark_with_llm benchmark_with_unmatched -- --nocapture
cargo test --test radix_trie_benchmark_with_llm benchmark_mixed_load -- --nocapture

# Show Ollama setup instructions
cargo test --test radix_trie_benchmark_with_llm ollama_instructions -- --ignored --nocapture
```

## Setting Up Ollama

### Installation

**macOS:**
```bash
brew install ollama
```

**Linux:**
```bash
curl -fsSL https://ollama.com/install.sh | sh
```

### Starting Ollama

```bash
# Start the Ollama server
ollama serve
```

### Pull a Model

In another terminal:
```bash
# Pull your preferred model
ollama pull llama2        # General purpose, 7B parameters
ollama pull llama3        # Latest Llama, improved performance
ollama pull mistral       # Fast and efficient
ollama pull codellama     # Optimized for code/patterns
ollama pull phi           # Smaller, faster (2.7B parameters)
```

### Configure Environment Variables

```bash
# Set Ollama configuration
export OLLAMA_ENDPOINT=http://localhost:11434
export OLLAMA_MODEL=llama2  # Or your preferred model
```

### Run Benchmarks with Ollama

```bash
# Run with environment variables set
OLLAMA_ENDPOINT=http://localhost:11434 OLLAMA_MODEL=llama2 \
  cargo test --test radix_trie_benchmark_with_llm -- --nocapture
```

## Benchmark Output Explained

```
============================================================
📊 Benchmark: 10,000 logs
============================================================
⚙️  Setting up matcher with templates...
   ✓ 7 templates loaded
🤖 Ollama configured:
   Endpoint: http://localhost:11434
   Model: llama2
📝 Generating 10000 mock logs (matched)...
   ✓ Generated in 1.21ms
📝 Generating 500 unmatched logs...
   ✓ Total logs: 10500
🔍 Processing logs through radix trie...

📈 Results:
   Total logs processed:  10500
   Matched:               10000 (95.2%)
   Unmatched:             500 (4.8%)
   Extracted values:      20000

⚡ Performance:
   Total time:            1340.25ms
   Throughput:            7832 logs/sec
   Avg latency:           127.64μs per log
   Avg latency:           0.1276ms per log

💾 Memory efficiency:
   Templates:             7
   Avg matches/template:  1429

🔍 Sample unmatched logs:
   1. New user registration: user_id=1000 email=user0@example.com from ip=192.168.0.0
   2. Payment processed: transaction_id=txn_1 amount=$10.10 status=success
   3. Cache miss for key 'user_session_2' - fetching from database (took 12ms)
   ...
```

### Metrics Explained

- **Total logs processed**: Total number of log lines analyzed
- **Matched**: Logs that matched existing templates in the radix trie
- **Unmatched**: Logs that didn't match (candidates for LLM template generation)
- **Extracted values**: Total number of variable values extracted from matched logs
- **Throughput**: How many logs can be processed per second
- **Avg latency**: Average time to match a single log

## Log Patterns in Tests

### Matched Patterns (Pre-configured)
- `cpu_usage: X% - Server load [status]`
- `memory_usage: X.XGB - Memory consumption [status]`
- `disk_io: XMB/s - Disk activity [status]`
- `network_traffic: XMbps - Network load [status]`
- `error_rate: X.XX% - System status [status]`
- `request_latency: Xms - Response time [status]`
- `database_connections: X - Pool status [status]`

### Unmatched Patterns (For LLM Testing)
- `New user registration: user_id=X email=X from ip=X.X.X.X`
- `Payment processed: transaction_id=X amount=$X.XX status=X`
- `Cache miss for key 'X' - fetching from database (took Xms)`
- `API rate limit exceeded for client_id=X - X requests in X seconds`
- `Background job completed: job_id=X duration=X.XXs status=X`

## Performance Tips

1. **Use release builds for accurate benchmarks:**
   ```bash
   cargo test --release --test radix_trie_benchmark -- --nocapture
   ```

2. **Run single-threaded for consistent timing:**
   ```bash
   cargo test --test radix_trie_benchmark -- --nocapture --test-threads=1
   ```

3. **For million-log tests, increase timeout:**
   ```bash
   RUST_TEST_TIME_UNIT=60000 cargo test benchmark_1m_logs -- --nocapture
   ```

## Ollama Model Recommendations

| Model | Size | Speed | Use Case |
|-------|------|-------|----------|
| `phi` | 2.7B | ⚡⚡⚡ Fast | Quick testing, development |
| `mistral` | 7B | ⚡⚡ Medium | Production, good balance |
| `llama2` | 7B | ⚡⚡ Medium | General purpose, reliable |
| `llama3` | 8B | ⚡⚡ Medium | Latest, improved quality |
| `codellama` | 7B | ⚡⚡ Medium | Code/pattern focused |

## Troubleshooting

### Ollama not responding
```bash
# Check if Ollama is running
curl http://localhost:11434/api/version

# Restart Ollama
killall ollama
ollama serve
```

### Model not found
```bash
# List available models
ollama list

# Pull the model you need
ollama pull llama2
```

### Slow performance
- Use a smaller model (`phi` instead of `llama2`)
- Close other applications
- Check if Ollama is using GPU acceleration (if available)

## Contributing

To add new benchmark scenarios:

1. Add your log patterns to `generate_mock_logs()` or `generate_unmatched_logs()`
2. Create corresponding templates in `setup_matcher_with_templates()`
3. Add a new test function with `#[test]` attribute
4. Document expected performance characteristics

## Further Reading

- [Ollama Documentation](https://ollama.com)
- [Radix Trie Performance Analysis](../ARCHITECTURE.md)
- [LLM Template Generation Guide](../LLM_TEMPLATE_GUIDE.md)
