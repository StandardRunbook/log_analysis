// Benchmark using lock-free LogMatcher for better single-threaded performance

mod lock_free_matcher;
use lock_free_matcher::{LockFreeLogMatcher, LogTemplate};
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

/// Add custom templates to support the benchmark logs
fn setup_matcher_with_templates() -> LockFreeLogMatcher {
    let mut matcher = LockFreeLogMatcher::new();

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

/// Run benchmark with a specific number of logs
fn run_benchmark(name: &str, log_count: usize) {
    println!("\n{}", "=".repeat(60));
    println!("📊 Benchmark (Lock-Free): {}", name);
    println!("{}", "=".repeat(60));

    // Setup
    println!("⚙️  Setting up lock-free matcher with templates...");
    let matcher = setup_matcher_with_templates();
    let template_count = matcher.get_all_templates().len();
    println!(
        "   ✓ {} templates loaded (no RwLock overhead)",
        template_count
    );

    // Generate logs
    println!("📝 Generating {} mock logs...", log_count);
    let start = Instant::now();
    let logs = generate_mock_logs(log_count);
    let gen_duration = start.elapsed();
    println!(
        "   ✓ Generated in {:.2}ms",
        gen_duration.as_secs_f64() * 1000.0
    );

    // Process logs
    println!("🔍 Processing logs through radix trie...");
    let start = Instant::now();

    let mut matched = 0;
    let mut unmatched = 0;
    let mut total_extracted_values = 0;

    for log in &logs {
        let result = matcher.match_log(log);
        if result.matched {
            matched += 1;
            total_extracted_values += result.extracted_values.len();
        } else {
            unmatched += 1;
        }
    }

    let duration = start.elapsed();

    // Calculate metrics
    let total_ms = duration.as_secs_f64() * 1000.0;
    let logs_per_second = log_count as f64 / duration.as_secs_f64();
    let avg_latency_us = (duration.as_micros() as f64) / log_count as f64;

    // Print results
    println!("\n📈 Results:");
    println!("   Total logs processed:  {}", log_count);
    println!(
        "   Matched:               {} ({:.1}%)",
        matched,
        (matched as f64 / log_count as f64) * 100.0
    );
    println!(
        "   Unmatched:             {} ({:.1}%)",
        unmatched,
        (unmatched as f64 / log_count as f64) * 100.0
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
}

#[test]
fn benchmark_lockfree_1k_logs() {
    run_benchmark("1,000 logs", 1_000);
}

#[test]
fn benchmark_lockfree_10k_logs() {
    run_benchmark("10,000 logs", 10_000);
}

#[test]
fn benchmark_lockfree_100k_logs() {
    run_benchmark("100,000 logs", 100_000);
}

#[test]
fn benchmark_lockfree_1m_logs() {
    run_benchmark("1,000,000 logs", 1_000_000);
}

#[test]
fn benchmark_lockfree_all_scales() {
    println!("\n{}", "█".repeat(60));
    println!("🚀 LOCK-FREE RADIX TRIE BENCHMARK");
    println!("   (No Arc/RwLock overhead for single-threaded tests)");
    println!("{}\n", "█".repeat(60));

    let scales = vec![
        ("Small", 1_000),
        ("Medium", 10_000),
        ("Large", 100_000),
        ("Very Large", 1_000_000),
    ];

    for (name, count) in scales {
        run_benchmark(name, count);
        println!("\n");
    }

    println!("{}", "█".repeat(60));
    println!("✅ Lock-free benchmark suite completed!");
    println!("{}", "█".repeat(60));
}
