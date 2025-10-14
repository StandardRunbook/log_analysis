// Benchmark with radix trie pre-populated with random templates at various depths

mod lock_free_matcher;
use lock_free_matcher::{LockFreeLogMatcher, LogTemplate};
use std::time::Instant;

/// Generate random log templates at different depths
fn generate_random_templates(depth: usize) -> Vec<LogTemplate> {
    let mut templates = Vec::new();

    // Common prefixes for each depth level
    let depth_prefixes = vec![
        vec!["app:", "sys:", "db:", "net:", "api:"],
        vec!["error", "warn", "info", "debug", "trace"],
        vec!["user", "admin", "system", "service", "worker"],
        vec!["request", "response", "query", "update", "delete"],
        vec!["success", "failure", "timeout", "pending", "complete"],
    ];

    let mut id = 1;

    // Generate templates for each depth level (1 to depth)
    for d in 1..=depth.min(5) {
        let mut prefix_combinations = vec![String::new()];

        // Build combinations up to current depth
        for level in 0..d {
            let mut new_combinations = Vec::new();
            for prefix in &prefix_combinations {
                for suffix in &depth_prefixes[level] {
                    let new_prefix = if prefix.is_empty() {
                        suffix.to_string()
                    } else {
                        format!("{} {}", prefix, suffix)
                    };
                    new_combinations.push(new_prefix);
                }
            }
            prefix_combinations = new_combinations;
        }

        // Create templates from combinations (limit to avoid explosion)
        let sample_size = prefix_combinations.len().min(50);
        for i in 0..sample_size {
            let idx = (i * prefix_combinations.len()) / sample_size;
            let prefix = &prefix_combinations[idx];

            templates.push(LogTemplate {
                template_id: id,
                pattern: format!(r"{}: (\d+) - (.*)", regex::escape(prefix)),
                variables: vec!["id".to_string(), "message".to_string()],
                example: format!("{}: 123 - sample message", prefix),
            });
            id += 1;
        }
    }

    templates
}

/// Generate logs that match various template depths
fn generate_logs_for_depth(count: usize, max_depth: usize) -> Vec<String> {
    let mut logs = Vec::with_capacity(count);

    let depth_patterns = vec![
        vec!["app:", "sys:", "db:", "net:", "api:"],
        vec!["error", "warn", "info", "debug", "trace"],
        vec!["user", "admin", "system", "service", "worker"],
        vec!["request", "response", "query", "update", "delete"],
        vec!["success", "failure", "timeout", "pending", "complete"],
    ];

    for i in 0..count {
        let depth = (i % max_depth) + 1;
        let mut prefix_parts = Vec::new();

        for level in 0..depth.min(5) {
            let idx = (i + level) % depth_patterns[level].len();
            prefix_parts.push(depth_patterns[level][idx]);
        }

        let prefix = prefix_parts.join(" ");
        let log = format!("{}: {} - Log message {}", prefix, 100 + i, i);
        logs.push(log);
    }

    logs
}

/// Setup matcher with random templates up to specified depth
fn setup_matcher_with_depth(depth: usize) -> LockFreeLogMatcher {
    let mut matcher = LockFreeLogMatcher::new();

    let templates = generate_random_templates(depth);

    for template in templates {
        matcher.add_template(template);
    }

    matcher
}

/// Run benchmark with a specific depth and log count
fn run_depth_benchmark(name: &str, depth: usize, log_count: usize) {
    println!("\n{}", "=".repeat(60));
    println!("📊 Benchmark: {} (Depth: {})", name, depth);
    println!("{}", "=".repeat(60));

    // Setup
    println!("⚙️  Pre-populating radix trie with templates...");
    let start = Instant::now();
    let matcher = setup_matcher_with_depth(depth);
    let setup_duration = start.elapsed();
    let template_count = matcher.get_all_templates().len();
    println!(
        "   ✓ {} templates loaded in {:.2}ms",
        template_count,
        setup_duration.as_secs_f64() * 1000.0
    );
    println!("   ✓ Trie depth: {}", depth);

    // Generate logs
    println!("📝 Generating {} logs for depth {}...", log_count, depth);
    let start = Instant::now();
    let logs = generate_logs_for_depth(log_count, depth);
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
        println!("\n💾 Trie Statistics:");
        println!("   Templates:             {}", template_count);
        println!("   Trie depth:            {}", depth);
        println!(
            "   Avg matches/template:  {:.2}",
            matched as f64 / template_count as f64
        );
    }
}

#[test]
fn benchmark_depth_1() {
    run_depth_benchmark("10K logs, depth 1", 1, 10_000);
}

#[test]
fn benchmark_depth_2() {
    run_depth_benchmark("10K logs, depth 2", 2, 10_000);
}

#[test]
fn benchmark_depth_3() {
    run_depth_benchmark("10K logs, depth 3", 3, 10_000);
}

#[test]
fn benchmark_depth_4() {
    run_depth_benchmark("10K logs, depth 4", 4, 10_000);
}

#[test]
fn benchmark_depth_5() {
    run_depth_benchmark("10K logs, depth 5", 5, 10_000);
}

#[test]
fn benchmark_depth_5_large() {
    run_depth_benchmark("100K logs, depth 5", 5, 100_000);
}

#[test]
fn benchmark_all_depths() {
    println!("\n{}", "█".repeat(60));
    println!("🌲 RADIX TRIE DEPTH BENCHMARK");
    println!("   Testing performance with varying trie depths");
    println!("{}\n", "█".repeat(60));

    for depth in 1..=5 {
        run_depth_benchmark(&format!("Depth {}", depth), depth, 10_000);
        println!();
    }

    println!("{}", "█".repeat(60));
    println!("✅ Depth benchmark suite completed!");
    println!("{}", "█".repeat(60));
}
