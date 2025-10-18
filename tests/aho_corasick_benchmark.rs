// Aho-Corasick DFA benchmark - general-purpose log matching
// Uses the actual LogMatcher implementation from src/log_matcher.rs

use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
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

fn setup_matcher() -> LogMatcher {
    let mut matcher = LogMatcher::new();

    // Add additional templates beyond the 3 default ones
    let templates = vec![
        LogTemplate {
            template_id: 0, // Auto-assigned
            pattern: r"network_traffic: (\d+)Mbps - Network load (.*)".to_string(),
            variables: vec!["bandwidth".to_string(), "status".to_string()],
            example: "network_traffic: 100Mbps - Network load moderate".to_string(),
        },
        LogTemplate {
            template_id: 0,
            pattern: r"error_rate: (\d+\.\d+)% - System status (.*)".to_string(),
            variables: vec!["rate".to_string(), "status".to_string()],
            example: "error_rate: 0.50% - System status healthy".to_string(),
        },
        LogTemplate {
            template_id: 0,
            pattern: r"request_latency: (\d+)ms - Response time (.*)".to_string(),
            variables: vec!["latency".to_string(), "status".to_string()],
            example: "request_latency: 50ms - Response time optimal".to_string(),
        },
        LogTemplate {
            template_id: 0,
            pattern: r"database_connections: (\d+) - Pool status (.*)".to_string(),
            variables: vec!["connections".to_string(), "status".to_string()],
            example: "database_connections: 50 - Pool status healthy".to_string(),
        },
    ];

    for template in templates {
        matcher.add_template(template);
    }

    matcher
}

#[test]
fn benchmark_ac_100k() {
    let matcher = setup_matcher();
    let logs = generate_mock_logs(100_000);

    println!("\n============================================================");
    println!("📊 Aho-Corasick Benchmark: 100K logs");
    println!("============================================================");

    let log_refs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();

    let start = Instant::now();
    let results = matcher.match_batch(&log_refs);
    let elapsed = start.elapsed();

    let matched = results.iter().filter(|r| r.is_some()).count();
    let unmatched = results.len() - matched;

    let throughput = (logs.len() as f64 / elapsed.as_secs_f64()) as u64;
    let avg_latency_us = (elapsed.as_micros() as f64) / (logs.len() as f64);

    println!("📈 Results:");
    println!("   Total logs:            {:>10}", logs.len());
    println!(
        "   Matched:               {:>10} ({:.1}%)",
        matched,
        (matched as f64 / logs.len() as f64) * 100.0
    );
    println!("   Unmatched:             {:>10}", unmatched);
    println!();
    println!("⚡ Performance:");
    println!("   Total time:            {:>10.2}ms", elapsed.as_millis());
    println!("   Throughput:            {:>10} logs/sec", throughput);
    println!(
        "   Avg latency:           {:>10.2}μs per log",
        avg_latency_us
    );
    println!("============================================================\n");

    assert!(matched > logs.len() * 90 / 100); // At least 90% match rate
}

#[test]
fn benchmark_ac_1m() {
    let matcher = setup_matcher();
    let logs = generate_mock_logs(1_000_000);

    println!("\n============================================================");
    println!("📊 Aho-Corasick Benchmark: 1M logs");
    println!("============================================================");

    let log_refs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();

    let start = Instant::now();
    let results = matcher.match_batch(&log_refs);
    let elapsed = start.elapsed();

    let matched = results.iter().filter(|r| r.is_some()).count();
    let unmatched = results.len() - matched;

    let throughput = (logs.len() as f64 / elapsed.as_secs_f64()) as u64;
    let avg_latency_us = (elapsed.as_micros() as f64) / (logs.len() as f64);

    println!("📈 Results:");
    println!("   Total logs:            {:>10}", logs.len());
    println!(
        "   Matched:               {:>10} ({:.1}%)",
        matched,
        (matched as f64 / logs.len() as f64) * 100.0
    );
    println!("   Unmatched:             {:>10}", unmatched);
    println!();
    println!("⚡ Performance:");
    println!("   Total time:            {:>10.2}ms", elapsed.as_millis());
    println!("   Throughput:            {:>10} logs/sec", throughput);
    println!(
        "   Avg latency:           {:>10.2}μs per log",
        avg_latency_us
    );
    println!("============================================================\n");

    assert!(matched > logs.len() * 90 / 100);
}

#[test]
fn benchmark_absolute_maximum() {
    let matcher = Arc::new(setup_matcher());
    let total_logs = 10_000_000;
    let batch_size = 100_000;

    println!("\n============================================================");
    println!("📊 Absolute Maximum Benchmark: 10M logs (parallel batches)");
    println!("============================================================");

    let start = Instant::now();

    let num_batches = total_logs / batch_size;
    let total_matched: usize = (0..num_batches)
        .into_par_iter()
        .map(|_| {
            let logs = generate_mock_logs(batch_size);
            let log_refs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();
            let results = matcher.match_batch(&log_refs);
            results.iter().filter(|r| r.is_some()).count()
        })
        .sum();

    let elapsed = start.elapsed();

    let throughput = (total_logs as f64 / elapsed.as_secs_f64()) as u64;
    let avg_latency_us = (elapsed.as_micros() as f64) / (total_logs as f64);

    println!("📈 Results:");
    println!("   Total logs:            {:>10}", total_logs);
    println!(
        "   Matched:               {:>10} ({:.1}%)",
        total_matched,
        (total_matched as f64 / total_logs as f64) * 100.0
    );
    println!("   Batch size:            {:>10}", batch_size);
    println!();
    println!("⚡ Performance:");
    println!("   Total time:            {:>10.2}s", elapsed.as_secs_f64());
    println!("   Throughput:            {:>10} logs/sec", throughput);
    println!(
        "   Avg latency:           {:>10.2}μs per log",
        avg_latency_us
    );
    println!("============================================================\n");

    assert!(total_matched > total_logs * 90 / 100);
}

#[test]
fn benchmark_ac_10m() {
    let matcher = Arc::new(setup_matcher());
    let total_logs = 10_000_000;
    let batch_size = 50_000;

    println!("\n============================================================");
    println!("📊 Aho-Corasick Benchmark: 10M logs (sequential batches)");
    println!("============================================================");

    let start = Instant::now();

    let mut total_matched = 0;
    for _ in 0..(total_logs / batch_size) {
        let logs = generate_mock_logs(batch_size);
        let log_refs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();
        let results = matcher.match_batch(&log_refs);
        total_matched += results.iter().filter(|r| r.is_some()).count();
    }

    let elapsed = start.elapsed();

    let throughput = (total_logs as f64 / elapsed.as_secs_f64()) as u64;
    let avg_latency_us = (elapsed.as_micros() as f64) / (total_logs as f64);

    println!("📈 Results:");
    println!("   Total logs:            {:>10}", total_logs);
    println!(
        "   Matched:               {:>10} ({:.1}%)",
        total_matched,
        (total_matched as f64 / total_logs as f64) * 100.0
    );
    println!("   Batch size:            {:>10}", batch_size);
    println!();
    println!("⚡ Performance:");
    println!("   Total time:            {:>10.2}s", elapsed.as_secs_f64());
    println!("   Throughput:            {:>10} logs/sec", throughput);
    println!(
        "   Avg latency:           {:>10.2}μs per log",
        avg_latency_us
    );
    println!("============================================================\n");

    assert!(total_matched > total_logs * 90 / 100);
}

#[test]
fn benchmark_ac_scaling() {
    println!("\n============================================================");
    println!("📊 Aho-Corasick Scaling Analysis");
    println!("============================================================\n");

    let scales = vec![1_000, 10_000, 100_000, 1_000_000];

    for &count in &scales {
        let matcher = setup_matcher();
        let logs = generate_mock_logs(count);
        let log_refs: Vec<&str> = logs.iter().map(|s| s.as_str()).collect();

        let start = Instant::now();
        let results = matcher.match_batch(&log_refs);
        let elapsed = start.elapsed();

        let _matched = results.iter().filter(|r| r.is_some()).count();
        let throughput = (count as f64 / elapsed.as_secs_f64()) as u64;
        let avg_latency_us = (elapsed.as_micros() as f64) / (count as f64);

        println!(
            "{:>10} logs: {:>8} logs/sec, {:>6.2}μs/log",
            count, throughput, avg_latency_us
        );
    }

    println!("============================================================\n");
}
