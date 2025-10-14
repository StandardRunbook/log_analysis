// Aho-Corasick DFA benchmark - general-purpose log matching
// Returns only template IDs, no value extraction

mod aho_corasick_matcher;
use aho_corasick_matcher::{AhoCorasickMatcher, LogTemplate};
use rayon::prelude::*;
use std::sync::Arc;
use std::time::Instant;

fn generate_mock_logs(count: usize) -> Vec<String> {
    let mut logs = Vec::with_capacity(count);

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
        let pattern_idx = i % patterns.len();
        let (_template, variants) = &patterns[pattern_idx];

        let log = match pattern_idx {
            0 => {
                let value = 10.0 + (i % 90) as f64;
                let variant = variants[i % variants.len()];
                format!("cpu_usage: {:.1}% - Server load {}", value, variant)
            }
            1 => {
                let value = 0.5 + (i % 30) as f64 * 0.1;
                let variant = variants[i % variants.len()];
                format!(
                    "memory_usage: {:.1}GB - Memory consumption {}",
                    value, variant
                )
            }
            2 => {
                let value = 10 + (i % 500);
                let variant = variants[i % variants.len()];
                format!("disk_io: {}MB/s - Disk activity {}", value, variant)
            }
            3 => {
                let value = 1 + (i % 1000);
                let variant = variants[i % variants.len()];
                format!("network_traffic: {}Mbps - Network load {}", value, variant)
            }
            4 => {
                let value = (i % 100) as f64 * 0.01;
                let variant = variants[i % variants.len()];
                format!("error_rate: {:.2}% - System status {}", value, variant)
            }
            5 => {
                let value = 10 + (i % 500);
                let variant = variants[i % variants.len()];
                format!("request_latency: {}ms - Response time {}", value, variant)
            }
            6 => {
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

fn setup_matcher(cache_size: usize) -> Arc<AhoCorasickMatcher> {
    let matcher = AhoCorasickMatcher::new(cache_size);

    let templates = vec![
        LogTemplate {
            template_id: 4,
            pattern: r"network_traffic: (\d+)Mbps - Network load (.*)".to_string(),
            variables: vec!["throughput".to_string(), "status".to_string()],
            example: "network_traffic: 500Mbps - Network load moderate".to_string(),
            prefix: "network_traffic: ".to_string(),
        },
        LogTemplate {
            template_id: 5,
            pattern: r"error_rate: (\d+\.\d+)% - System status (.*)".to_string(),
            variables: vec!["rate".to_string(), "status".to_string()],
            example: "error_rate: 0.05% - System status healthy".to_string(),
            prefix: "error_rate: ".to_string(),
        },
        LogTemplate {
            template_id: 6,
            pattern: r"request_latency: (\d+)ms - Response time (.*)".to_string(),
            variables: vec!["latency".to_string(), "status".to_string()],
            example: "request_latency: 125ms - Response time acceptable".to_string(),
            prefix: "request_latency: ".to_string(),
        },
        LogTemplate {
            template_id: 7,
            pattern: r"database_connections: (\d+) - Pool status (.*)".to_string(),
            variables: vec!["count".to_string(), "status".to_string()],
            example: "database_connections: 45 - Pool status healthy".to_string(),
            prefix: "database_connections: ".to_string(),
        },
    ];

    for template in templates {
        matcher.add_template(template);
    }

    Arc::new(matcher)
}

fn run_benchmark(name: &str, log_count: usize, cache_size: usize, thread_count: Option<usize>) {
    if let Some(threads) = thread_count {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .ok();
    }

    let actual_threads = rayon::current_num_threads();

    println!("\n{}", "=".repeat(60));
    println!("📊 Benchmark (Aho-Corasick DFA): {}", name);
    println!("   Threads: {}", actual_threads);
    println!("   Cache size: {}", cache_size);
    println!("{}", "=".repeat(60));

    println!("⚙️  Setting up Aho-Corasick DFA matcher...");
    let matcher = setup_matcher(cache_size);
    let template_count = matcher.get_all_templates().len();
    println!("   ✓ {} templates loaded", template_count);
    println!("   ✓ Aho-Corasick DFA (finds ALL patterns in O(n))");
    println!("   ✓ LRU cache enabled (size: {})", cache_size);

    println!("📝 Generating {} mock logs...", log_count);
    let start = Instant::now();
    let logs = generate_mock_logs(log_count);
    let gen_duration = start.elapsed();
    println!(
        "   ✓ Generated in {:.2}ms",
        gen_duration.as_secs_f64() * 1000.0
    );

    println!("🔍 Processing logs (DFA + fast matching + parallel)...");
    let start = Instant::now();

    // Fast matching - just get template IDs
    let results: Vec<_> = logs.par_iter().map(|log| matcher.match_log(log)).collect();

    let duration = start.elapsed();

    let matched = results.iter().filter(|m| m.is_some()).count();
    let unmatched = results.len() - matched;

    let total_ms = duration.as_secs_f64() * 1000.0;
    let logs_per_second = log_count as f64 / duration.as_secs_f64();
    let avg_latency_us = (duration.as_micros() as f64) / log_count as f64;

    let (cache_used, cache_cap) = matcher.cache_stats();

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
    println!("\n⚡ Performance:");
    println!("   Total time:            {:.2}ms", total_ms);
    println!(
        "   Throughput:            {:.0} logs/sec 🚀🚀",
        logs_per_second
    );
    println!("   Avg latency:           {:.2}μs per log", avg_latency_us);
    println!(
        "   Per-thread throughput: {:.0} logs/sec",
        logs_per_second / actual_threads as f64
    );
    println!("   Speedup vs baseline:   {:.2}x", logs_per_second / 7800.0);

    println!("\n💾 Optimization Stack:");
    println!("   ✓ Aho-Corasick DFA (O(n) multi-pattern)");
    println!("   ✓ Regex validation (regex.is_match only)");
    println!("   ✓ No value extraction (template ID only)");
    println!("   ✓ Structural sharing (lock-free)");
    println!("   ✓ Parallel processing ({} threads)", actual_threads);
    println!("   Templates:             {}", template_count);
}

#[test]
fn benchmark_ac_100k() {
    run_benchmark("100K logs, Aho-Corasick", 100_000, 1000, None);
}

#[test]
fn benchmark_ac_1m() {
    run_benchmark("1M logs, Aho-Corasick", 1_000_000, 10000, None);
}

#[test]
fn benchmark_ac_10m() {
    run_benchmark("10M logs, Aho-Corasick", 10_000_000, 10000, None);
}

#[test]
fn benchmark_ac_scaling() {
    println!("\n{}", "█".repeat(60));
    println!("🔥 AHO-CORASICK DFA - ULTIMATE OPTIMIZATION");
    println!("   DFA finds ALL patterns in ONE PASS!");
    println!("{}\n", "█".repeat(60));

    let log_count = 100_000;

    for threads in [1, 2, 4, 8] {
        run_benchmark(
            &format!("100K logs, {} thread(s)", threads),
            log_count,
            1000,
            Some(threads),
        );
        println!();
    }

    println!("{}", "█".repeat(60));
    println!("✅ Aho-Corasick benchmark completed!");
    println!("   DFA + LRU Cache + Structural Sharing + Parallel");
    println!("{}", "█".repeat(60));
}

#[test]
fn benchmark_absolute_maximum() {
    println!("\n{}", "█".repeat(60));
    println!("💥 ABSOLUTE MAXIMUM PERFORMANCE");
    println!("   Aho-Corasick DFA: The ultimate multi-pattern matcher");
    println!("{}\n", "█".repeat(60));

    // 1M logs
    run_benchmark("1M logs - MAXIMUM POWER", 1_000_000, 10000, None);

    println!("\n{}", "█".repeat(60));
    println!("🎯 Comparison:");
    println!("   Baseline:       7,800 logs/sec");
    println!("   SIMD:           420,000 logs/sec (54x)");
    println!("   Aho-Corasick:   ??? logs/sec");
    println!("\n   Goal: Beat 420K logs/sec!");
    println!("{}", "█".repeat(60));
}
