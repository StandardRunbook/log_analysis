use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use std::env;
use std::time::Instant;

/// Generate a variety of mock log entries for testing
fn generate_mock_logs(count: usize) -> Vec<String> {
    let mut logs = Vec::with_capacity(count);

    // Define various log patterns that should match our templates
    let patterns = vec![
        (
            "cpu_usage: {:.1}% - Server load {}",
            vec!["normal", "high", "critical", "moderate"],
        ),
        (
            "memory_usage: {:.1}GB - Memory consumption {}",
            vec!["stable", "increasing", "high", "normal"],
        ),
        (
            "disk_io: {}MB/s - Disk activity {}",
            vec!["low", "moderate", "high", "normal"],
        ),
        (
            "network_traffic: {}Mbps - Network load {}",
            vec!["light", "moderate", "heavy", "normal"],
        ),
        (
            "error_rate: {:.2}% - System status {}",
            vec!["healthy", "degraded", "critical", "recovering"],
        ),
        (
            "request_latency: {}ms - Response time {}",
            vec!["optimal", "acceptable", "slow", "fast"],
        ),
        (
            "database_connections: {} - Pool status {}",
            vec!["available", "limited", "exhausted", "healthy"],
        ),
    ];

    for i in 0..count {
        // Cycle through different patterns
        let pattern_idx = i % patterns.len();
        let (_template, variants) = &patterns[pattern_idx];

        let log = match pattern_idx {
            0 => {
                // cpu_usage pattern
                let value = 10.0 + (i % 90) as f64;
                let variant = variants[i % variants.len()];
                format!("cpu_usage: {:.1}% - Server load {}", value, variant)
            }
            1 => {
                // memory_usage pattern
                let value = 0.5 + (i % 30) as f64 * 0.1;
                let variant = variants[i % variants.len()];
                format!(
                    "memory_usage: {:.1}GB - Memory consumption {}",
                    value, variant
                )
            }
            2 => {
                // disk_io pattern
                let value = 10 + (i % 500);
                let variant = variants[i % variants.len()];
                format!("disk_io: {}MB/s - Disk activity {}", value, variant)
            }
            3 => {
                // network_traffic pattern
                let value = 1 + (i % 1000);
                let variant = variants[i % variants.len()];
                format!("network_traffic: {}Mbps - Network load {}", value, variant)
            }
            4 => {
                // error_rate pattern
                let value = (i % 100) as f64 * 0.01;
                let variant = variants[i % variants.len()];
                format!("error_rate: {:.2}% - System status {}", value, variant)
            }
            5 => {
                // request_latency pattern
                let value = 10 + (i % 500);
                let variant = variants[i % variants.len()];
                format!("request_latency: {}ms - Response time {}", value, variant)
            }
            6 => {
                // database_connections pattern
                let value = 1 + (i % 100);
                let variant = variants[i % variants.len()];
                format!("database_connections: {} - Pool status {}", value, variant)
            }
            _ => unreachable!(),
        };

        logs.push(log);
    }

    logs
}

/// Generate some unmatched logs to test LLM template generation
fn generate_unmatched_logs(count: usize) -> Vec<String> {
    let mut logs = Vec::new();

    let patterns = vec![
        "New user registration: user_id={} email={} from ip={}",
        "Payment processed: transaction_id={} amount=${:.2} status={}",
        "Cache miss for key '{}' - fetching from database (took {}ms)",
        "API rate limit exceeded for client_id={} - {} requests in {} seconds",
        "Background job completed: job_id={} duration={:.2}s status={}",
    ];

    for i in 0..count {
        let pattern_idx = i % patterns.len();

        let log = match pattern_idx {
            0 => format!(
                "New user registration: user_id={} email=user{}@example.com from ip=192.168.{}.{}",
                1000 + i,
                i,
                (i % 255),
                ((i * 7) % 255)
            ),
            1 => format!(
                "Payment processed: transaction_id={} amount=${:.2} status={}",
                format!("txn_{}", i),
                10.0 + (i % 1000) as f64 * 0.1,
                if i % 10 == 0 { "failed" } else { "success" }
            ),
            2 => format!(
                "Cache miss for key 'user_session_{}' - fetching from database (took {}ms)",
                i,
                10 + (i % 200)
            ),
            3 => format!(
                "API rate limit exceeded for client_id={} - {} requests in {} seconds",
                format!("client_{}", i % 100),
                100 + (i % 500),
                60
            ),
            4 => format!(
                "Background job completed: job_id={} duration={:.2}s status={}",
                format!("job_{}", i),
                1.0 + (i % 60) as f64 * 0.5,
                if i % 15 == 0 { "error" } else { "completed" }
            ),
            _ => unreachable!(),
        };

        logs.push(log);
    }

    logs
}

/// Add custom templates to support the benchmark logs
fn setup_matcher_with_templates() -> LogMatcher {
    let mut matcher = LogMatcher::new();

    // Add templates for all our test patterns
    let templates = vec![
        LogTemplate {
            template_id: 0, // Will be auto-assigned
            pattern: r"network_traffic: (\d+)Mbps - Network load (.*)".to_string(),
            variables: vec!["throughput".to_string(), "status".to_string()],
            example: "network_traffic: 500Mbps - Network load moderate".to_string(),
        },
        LogTemplate {
            template_id: 0,
            pattern: r"error_rate: (\d+\.\d+)% - System status (.*)".to_string(),
            variables: vec!["rate".to_string(), "status".to_string()],
            example: "error_rate: 0.05% - System status healthy".to_string(),
        },
        LogTemplate {
            template_id: 0,
            pattern: r"request_latency: (\d+)ms - Response time (.*)".to_string(),
            variables: vec!["latency".to_string(), "status".to_string()],
            example: "request_latency: 125ms - Response time acceptable".to_string(),
        },
        LogTemplate {
            template_id: 0,
            pattern: r"database_connections: (\d+) - Pool status (.*)".to_string(),
            variables: vec!["count".to_string(), "status".to_string()],
            example: "database_connections: 45 - Pool status healthy".to_string(),
        },
    ];

    for template in templates {
        matcher.add_template(template);
    }

    matcher
}

/// Configuration for LLM-enabled benchmarks
#[derive(Debug, Clone)]
struct OllamaConfig {
    endpoint: String,
    model: String,
}

impl OllamaConfig {
    fn from_env() -> Option<Self> {
        let endpoint = env::var("OLLAMA_ENDPOINT").ok()?;
        let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());

        Some(OllamaConfig { endpoint, model })
    }
}

/// Run benchmark with a specific number of logs
fn run_benchmark(name: &str, log_count: usize, unmatched_count: usize) {
    println!("\n{}", "=".repeat(60));
    println!("📊 Benchmark: {}", name);
    println!("{}", "=".repeat(60));

    // Setup
    println!("⚙️  Setting up matcher with templates...");
    let matcher = setup_matcher_with_templates();
    let template_count = matcher.get_all_templates().len();
    println!("   ✓ {} templates loaded", template_count);

    // Check for Ollama configuration
    let ollama_config = OllamaConfig::from_env();
    if let Some(ref config) = ollama_config {
        println!("🤖 Ollama configured:");
        println!("   Endpoint: {}", config.endpoint);
        println!("   Model: {}", config.model);
    } else {
        println!("ℹ️  Ollama not configured (set OLLAMA_ENDPOINT and OLLAMA_MODEL to enable)");
    }

    // Generate matched logs
    println!("📝 Generating {} mock logs (matched)...", log_count);
    let start = Instant::now();
    let mut all_logs = generate_mock_logs(log_count);
    let gen_duration = start.elapsed();
    println!(
        "   ✓ Generated in {:.2}ms",
        gen_duration.as_secs_f64() * 1000.0
    );

    // Generate unmatched logs if requested
    if unmatched_count > 0 {
        println!("📝 Generating {} unmatched logs...", unmatched_count);
        let unmatched_logs = generate_unmatched_logs(unmatched_count);
        all_logs.extend(unmatched_logs);
        println!("   ✓ Total logs: {}", all_logs.len());
    }

    // Process logs
    println!("🔍 Processing logs through radix trie...");
    let start = Instant::now();

    let mut matched = 0;
    let mut unmatched = 0;
    let mut total_extracted_values = 0;
    let mut unmatched_samples = Vec::new();

    for log in &all_logs {
        let result = matcher.match_log(log);
        if result.matched {
            matched += 1;
            total_extracted_values += result.extracted_values.len();
        } else {
            unmatched += 1;
            if unmatched_samples.len() < 5 {
                unmatched_samples.push(log.clone());
            }
        }
    }

    let duration = start.elapsed();

    // Calculate metrics
    let total_ms = duration.as_secs_f64() * 1000.0;
    let logs_per_second = all_logs.len() as f64 / duration.as_secs_f64();
    let avg_latency_us = (duration.as_micros() as f64) / all_logs.len() as f64;

    // Print results
    println!("\n📈 Results:");
    println!("   Total logs processed:  {}", all_logs.len());
    println!(
        "   Matched:               {} ({:.1}%)",
        matched,
        (matched as f64 / all_logs.len() as f64) * 100.0
    );
    println!(
        "   Unmatched:             {} ({:.1}%)",
        unmatched,
        (unmatched as f64 / all_logs.len() as f64) * 100.0
    );
    println!("   Extracted values:      {}", total_extracted_values);
    println!("\n⚡ Performance:");
    println!("   Total time:            {:.2}ms", total_ms);
    println!("   Throughput:            {:.0} logs/sec", logs_per_second);
    println!("   Avg latency:           {:.2}μs per log", avg_latency_us);
    println!(
        "   Avg latency:           {:.4}ms per log",
        avg_latency_us / 1000.0
    );

    if total_ms > 1.0 {
        println!("\n💾 Memory efficiency:");
        println!("   Templates:             {}", template_count);
        println!(
            "   Avg matches/template:  {:.0}",
            matched as f64 / template_count as f64
        );
    }

    // Show unmatched samples
    if !unmatched_samples.is_empty() {
        println!("\n🔍 Sample unmatched logs:");
        for (i, log) in unmatched_samples.iter().enumerate() {
            println!("   {}. {}", i + 1, log);
        }

        if ollama_config.is_some() {
            println!(
                "\n💡 To test template generation with Ollama, use the LLM-enabled tests below"
            );
        }
    }
}

#[test]
fn benchmark_1k_matched_logs() {
    run_benchmark("1,000 matched logs", 1_000, 0);
}

#[test]
fn benchmark_10k_matched_logs() {
    run_benchmark("10,000 matched logs", 10_000, 0);
}

#[test]
fn benchmark_with_unmatched() {
    run_benchmark("1,000 matched + 100 unmatched", 1_000, 100);
}

#[test]
fn benchmark_mixed_load() {
    run_benchmark("10,000 matched + 500 unmatched", 10_000, 500);
}

/// Instructions for running with Ollama
#[test]
#[ignore] // Run with: cargo test ollama_instructions -- --ignored --nocapture
fn ollama_instructions() {
    println!("\n{}", "█".repeat(60));
    println!("🦙 OLLAMA INTEGRATION INSTRUCTIONS");
    println!("{}\n", "█".repeat(60));

    println!("To use Ollama with the benchmarks, follow these steps:\n");

    println!("1️⃣  Install and start Ollama:");
    println!("   brew install ollama              # macOS");
    println!("   ollama serve                     # Start the server\n");

    println!("2️⃣  Pull a model (in another terminal):");
    println!("   ollama pull llama2               # Or llama3, mistral, codellama, etc.\n");

    println!("3️⃣  Set environment variables:");
    println!("   export OLLAMA_ENDPOINT=http://localhost:11434");
    println!("   export OLLAMA_MODEL=llama2       # Or your preferred model\n");

    println!("4️⃣  Run the benchmarks:");
    println!("   cargo test --test radix_trie_benchmark_with_llm -- --nocapture\n");

    println!("📚 Available models to try:");
    println!("   - llama2        (Good general purpose)");
    println!("   - llama3        (Latest, improved performance)");
    println!("   - mistral       (Fast and efficient)");
    println!("   - codellama     (Optimized for code/patterns)");
    println!("   - phi           (Smaller, faster model)\n");

    println!("💡 Current configuration:");
    if let Some(config) = OllamaConfig::from_env() {
        println!("   ✅ Ollama is configured!");
        println!("   Endpoint: {}", config.endpoint);
        println!("   Model: {}", config.model);
    } else {
        println!("   ⚠️  Ollama is NOT configured");
        println!("   Set OLLAMA_ENDPOINT and OLLAMA_MODEL environment variables");
    }

    println!("\n{}", "█".repeat(60));
}
