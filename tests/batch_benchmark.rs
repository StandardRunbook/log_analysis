// Batch processing benchmark - test different batch sizes

use log_analyzer::log_matcher::LogMatcher;
use rayon::prelude::*;
use std::time::Instant;

fn generate_mock_logs(count: usize) -> Vec<String> {
    let mut logs = Vec::with_capacity(count);
    let patterns = vec![
        "cpu_usage: ",
        "memory_usage: ",
        "disk_io: ",
        "network_traffic: ",
        "error_rate: ",
        "request_latency: ",
        "database_connections: ",
    ];

    for i in 0..count {
        let pattern = patterns[i % patterns.len()];
        let log = match pattern {
            "cpu_usage: " => format!(
                "cpu_usage: {:.1}% - Server load normal",
                10.0 + (i % 90) as f64
            ),
            "memory_usage: " => format!(
                "memory_usage: {:.1}GB - Memory stable",
                0.5 + (i % 30) as f64 * 0.1
            ),
            "disk_io: " => format!("disk_io: {}MB/s - Disk activity moderate", 10 + (i % 500)),
            "network_traffic: " => format!(
                "network_traffic: {}Mbps - Network load light",
                1 + (i % 1000)
            ),
            "error_rate: " => format!(
                "error_rate: {:.2}% - System healthy",
                (i % 100) as f64 * 0.01
            ),
            "request_latency: " => format!("request_latency: {}ms - Response fast", 10 + (i % 500)),
            "database_connections: " => {
                format!("database_connections: {} - Pool healthy", 1 + (i % 100))
            }
            _ => unreachable!(),
        };
        logs.push(log);
    }
    logs
}

fn benchmark_single_log(matcher: &LogMatcher, logs: &[String]) -> f64 {
    let start = Instant::now();

    let results: Vec<_> = logs.iter().map(|log| matcher.match_log(log)).collect();

    let duration = start.elapsed();
    let matched = results.iter().filter(|m| m.is_some()).count();

    let throughput = logs.len() as f64 / duration.as_secs_f64();

    println!("Single-log (sequential):");
    println!("  Matched: {}/{}", matched, logs.len());
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  Throughput: {:.2}M logs/sec", throughput / 1_000_000.0);

    throughput
}

fn benchmark_batch(matcher: &LogMatcher, logs: &[String], batch_size: usize) -> f64 {
    let start = Instant::now();

    let log_refs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();
    let mut total_matched = 0;

    for chunk in log_refs.chunks(batch_size) {
        let results = matcher.match_batch(chunk);
        total_matched += results.iter().filter(|m| m.is_some()).count();
    }

    let duration = start.elapsed();
    let throughput = logs.len() as f64 / duration.as_secs_f64();

    println!("Batch (size={}):", batch_size);
    println!("  Matched: {}/{}", total_matched, logs.len());
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  Throughput: {:.2}M logs/sec", throughput / 1_000_000.0);

    throughput
}

fn benchmark_batch_parallel(matcher: &LogMatcher, logs: &[String], batch_size: usize) -> f64 {
    let start = Instant::now();

    let log_refs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();

    let results: Vec<_> = log_refs
        .par_chunks(batch_size)
        .map(|chunk| matcher.match_batch(chunk))
        .collect();

    let duration = start.elapsed();
    let total_matched: usize = results
        .iter()
        .map(|batch| batch.iter().filter(|m| m.is_some()).count())
        .sum();

    let throughput = logs.len() as f64 / duration.as_secs_f64();

    println!(
        "Batch parallel (size={}, {} threads):",
        batch_size,
        rayon::current_num_threads()
    );
    println!("  Matched: {}/{}", total_matched, logs.len());
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  Throughput: {:.2}M logs/sec", throughput / 1_000_000.0);

    throughput
}

#[test]
fn benchmark_batch_sizes() {
    println!("\n{}", "=".repeat(70));
    println!("🚀 BATCH PROCESSING BENCHMARK");
    println!("{}\n", "=".repeat(70));

    let log_count = 1_000_000;
    let matcher = LogMatcher::new();

    println!("📝 Generating {} mock logs...", log_count);
    let logs = generate_mock_logs(log_count);
    println!("   ✓ Generated\n");

    // Baseline: single log processing (sequential)
    println!("🔍 Baseline: Single-log processing (sequential)");
    let baseline = benchmark_single_log(&matcher, &logs);
    println!();

    // Test different batch sizes (sequential)
    println!("🔍 Batch processing (sequential)");
    let batch_sizes = vec![10, 100, 1000, 10000];
    for size in batch_sizes {
        let throughput = benchmark_batch(&matcher, &logs, size);
        let speedup = throughput / baseline;
        println!("  Speedup vs baseline: {:.2}x\n", speedup);
    }

    // Test batch with parallelism
    println!("🔍 Batch processing (parallel)");
    let parallel_batch_sizes = vec![100, 1000, 10000, 100000];
    for size in parallel_batch_sizes {
        let throughput = benchmark_batch_parallel(&matcher, &logs, size);
        let speedup = throughput / baseline;
        println!("  Speedup vs baseline: {:.2}x\n", speedup);
    }

    println!("{}", "=".repeat(70));
    println!("✅ Batch benchmark complete!");
    println!("{}", "=".repeat(70));
}

#[test]
fn benchmark_optimal_batch() {
    println!("\n{}", "=".repeat(70));
    println!("🎯 FINDING OPTIMAL BATCH SIZE");
    println!("{}\n", "=".repeat(70));

    let log_count = 1_000_000;
    let matcher = LogMatcher::new();

    println!("📝 Generating {} mock logs...", log_count);
    let logs = generate_mock_logs(log_count);
    println!("   ✓ Generated\n");

    let batch_sizes = vec![10, 50, 100, 500, 1000, 5000, 10000, 50000, 100000];

    let mut best_throughput = 0.0;
    let mut best_batch_size = 0;

    for size in batch_sizes {
        let throughput = benchmark_batch_parallel(&matcher, &logs, size);
        if throughput > best_throughput {
            best_throughput = throughput;
            best_batch_size = size;
        }
        println!();
    }

    println!("{}", "=".repeat(70));
    println!("🏆 OPTIMAL BATCH SIZE: {}", best_batch_size);
    println!(
        "   Throughput: {:.2}M logs/sec",
        best_throughput / 1_000_000.0
    );
    println!("{}", "=".repeat(70));
}
