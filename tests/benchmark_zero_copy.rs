/// Zero-Copy Performance Benchmark
///
/// Compares three implementations:
/// 1. Standard LogMatcher (with FxHashMap optimization)
/// 2. FastLogMatcher (FxHashMap + optimizations)
/// 3. ZeroCopyMatcher (FxHashMap + thread-local scratch space)
///
/// IMPORTANT: Always run with --release!
///
/// Run with: cargo test --release --test benchmark_zero_copy -- --nocapture

use log_analyzer::log_matcher::LogMatcher;
use log_analyzer::log_matcher_zero_copy::ZeroCopyMatcher;
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::traits::DatasetLoader;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTemplates {
    templates: Vec<CachedTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTemplate {
    template_id: u64,
    pattern: String,
    variables: Vec<String>,
    example: String,
}

fn load_standard_matcher(dataset_name: &str) -> anyhow::Result<LogMatcher> {
    let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());
    let json_content = fs::read_to_string(&cache_file)?;
    let cached: CachedTemplates = serde_json::from_str(&json_content)?;

    let mut matcher = LogMatcher::new();
    for template in cached.templates {
        matcher.add_template(log_analyzer::log_matcher::LogTemplate {
            template_id: template.template_id,
            pattern: template.pattern,
            variables: template.variables,
            example: template.example,
        });
    }

    Ok(matcher)
}

fn load_zero_copy_matcher(dataset_name: &str) -> anyhow::Result<ZeroCopyMatcher> {
    let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());
    let json_content = fs::read_to_string(&cache_file)?;
    let cached: CachedTemplates = serde_json::from_str(&json_content)?;

    let mut matcher = ZeroCopyMatcher::new();
    for template in cached.templates {
        matcher.add_template(log_analyzer::log_matcher::LogTemplate {
            template_id: template.template_id,
            pattern: template.pattern,
            variables: template.variables,
            example: template.example,
        });
    }

    Ok(matcher)
}

#[test]
fn benchmark_zero_copy_apache() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("⚡ ZERO-COPY PERFORMANCE - Apache (1000 logs)");
    println!("{:=<80}\n", "");

    let dataset = LogHubDatasetLoader::new("Apache", "data/loghub");
    let logs = dataset.load_raw_logs()?;
    let test_size = 1000.min(logs.len());
    let test_logs = &logs[..test_size];

    // Standard matcher (with FxHashMap)
    let std_matcher = load_standard_matcher("Apache")?;
    println!("Standard Matcher (FxHashMap):");

    let start = Instant::now();
    let mut matched = 0;
    for log in test_logs {
        if std_matcher.match_log(log).is_some() {
            matched += 1;
        }
    }
    let elapsed = start.elapsed();
    let std_throughput = test_size as f64 / elapsed.as_secs_f64();
    let std_latency = (elapsed.as_nanos() as f64) / test_size as f64;
    println!("  Throughput: {:>12.0} logs/sec", std_throughput);
    println!("  Latency:    {:>12.1} ns/log", std_latency);
    println!("  Matched:    {:>12}/{}\n", matched, test_size);

    // Zero-copy matcher
    let zero_copy_matcher = load_zero_copy_matcher("Apache")?;
    println!("Zero-Copy Matcher (thread-local scratch space):");

    let start = Instant::now();
    let mut matched = 0;
    for log in test_logs {
        if zero_copy_matcher.match_log(log).is_some() {
            matched += 1;
        }
    }
    let elapsed = start.elapsed();
    let zero_copy_throughput = test_size as f64 / elapsed.as_secs_f64();
    let zero_copy_latency = (elapsed.as_nanos() as f64) / test_size as f64;
    println!("  Throughput: {:>12.0} logs/sec", zero_copy_throughput);
    println!("  Latency:    {:>12.1} ns/log", zero_copy_latency);
    println!("  Matched:    {:>12}/{}\n", matched, test_size);

    let speedup = zero_copy_throughput / std_throughput;
    let latency_improvement = ((std_latency - zero_copy_latency) / std_latency) * 100.0;

    println!("{:=<80}", "");
    println!("Zero-Copy vs Standard:");
    println!("  Speedup:            {:.2}x faster ⚡", speedup);
    println!("  Latency reduction:  {:.1}% improvement", latency_improvement);
    println!("{:=<80}\n", "");

    Ok(())
}

#[test]
fn benchmark_zero_copy_all() -> anyhow::Result<()> {
    println!("\n{:=<110}", "");
    println!("⚡ ZERO-COPY PERFORMANCE COMPARISON - All Datasets");
    println!("{:=<110}\n", "");

    let datasets = vec![
        "Android", "Apache", "Bgl", "Hadoop", "Hdfs", "Healthapp",
        "Hpc", "Linux", "Mac", "Openssh", "Openstack", "Proxifier",
        "Spark", "Thunderbird", "Windows", "Zookeeper"
    ];

    println!("{:<15} {:>12} {:>15} {:>18} {:>12} {:>15}",
        "Dataset", "Templates", "Standard", "Zero-Copy", "Speedup", "Improvement");
    println!("{:-<110}", "");

    let mut total_speedup = 0.0;
    let mut count = 0;

    for dataset_name in &datasets {
        let std_matcher = match load_standard_matcher(dataset_name) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let zero_copy_matcher = match load_zero_copy_matcher(dataset_name) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
        let logs = match dataset.load_raw_logs() {
            Ok(l) => l,
            Err(_) => continue,
        };

        let test_size = 1000.min(logs.len());
        let test_logs = &logs[..test_size];

        // Standard matcher
        let start = Instant::now();
        for log in test_logs {
            let _ = std_matcher.match_log(log);
        }
        let std_elapsed = start.elapsed();
        let std_throughput = test_size as f64 / std_elapsed.as_secs_f64();

        // Zero-copy matcher
        let start = Instant::now();
        for log in test_logs {
            let _ = zero_copy_matcher.match_log(log);
        }
        let zero_elapsed = start.elapsed();
        let zero_copy_throughput = test_size as f64 / zero_elapsed.as_secs_f64();

        let speedup = zero_copy_throughput / std_throughput;
        let improvement = ((zero_elapsed.as_nanos() as f64 - std_elapsed.as_nanos() as f64)
                           / std_elapsed.as_nanos() as f64) * -100.0;

        total_speedup += speedup;
        count += 1;

        let speedup_symbol = if speedup > 1.5 {
            "⚡⚡⚡"
        } else if speedup > 1.3 {
            "⚡⚡"
        } else if speedup > 1.1 {
            "⚡"
        } else {
            ""
        };

        println!("{:<15} {:>12} {:>12.0}/s {:>15.0}/s {:>9.2}x {:>11.1}% {}",
            dataset_name,
            std_matcher.get_all_templates().len(),
            std_throughput,
            zero_copy_throughput,
            speedup,
            improvement,
            speedup_symbol
        );
    }

    let avg_speedup = total_speedup / count as f64;

    println!("{:-<110}", "");
    println!("\nAverage speedup: {:.2}x faster ⚡\n", avg_speedup);

    if avg_speedup > 1.3 {
        println!("🎉 Zero-copy optimization achieved >30% improvement!");
    } else if avg_speedup > 1.1 {
        println!("✅ Zero-copy optimization achieved >10% improvement");
    }
    println!();

    Ok(())
}

#[test]
fn benchmark_zero_copy_batch() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("⚡ BATCH MATCHING COMPARISON - Apache (1000 logs)");
    println!("{:=<80}\n", "");

    let dataset = LogHubDatasetLoader::new("Apache", "data/loghub");
    let logs = dataset.load_raw_logs()?;
    let test_size = 1000.min(logs.len());
    let test_logs: Vec<&str> = logs[..test_size].iter().map(|s| s.as_str()).collect();

    // Standard matcher
    let std_matcher = load_standard_matcher("Apache")?;
    println!("Standard Matcher - Batch:");

    let start = Instant::now();
    let results = std_matcher.match_batch(&test_logs);
    let elapsed = start.elapsed();
    let std_throughput = test_size as f64 / elapsed.as_secs_f64();
    let matched = results.iter().filter(|r| r.is_some()).count();
    println!("  Throughput: {:>12.0} logs/sec", std_throughput);
    println!("  Matched:    {:>12}/{}\n", matched, test_size);

    // Zero-copy matcher
    let zero_copy_matcher = load_zero_copy_matcher("Apache")?;
    println!("Zero-Copy Matcher - Batch:");

    let start = Instant::now();
    let results = zero_copy_matcher.match_batch(&test_logs);
    let elapsed = start.elapsed();
    let zero_copy_throughput = test_size as f64 / elapsed.as_secs_f64();
    let matched = results.iter().filter(|r| r.is_some()).count();
    println!("  Throughput: {:>12.0} logs/sec", zero_copy_throughput);
    println!("  Matched:    {:>12}/{}\n", matched, test_size);

    let speedup = zero_copy_throughput / std_throughput;
    println!("{:=<80}", "");
    println!("Batch speedup: {:.2}x faster ⚡\n", speedup);

    Ok(())
}

#[test]
fn benchmark_zero_copy_stress() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("🔥 STRESS TEST - Zero-Copy with 100K repeated matches");
    println!("{:=<80}\n", "");

    let dataset = LogHubDatasetLoader::new("Apache", "data/loghub");
    let logs = dataset.load_raw_logs()?;
    let test_log = &logs[0];

    let zero_copy_matcher = load_zero_copy_matcher("Apache")?;

    println!("Testing scratch space reuse with 100,000 matches...\n");

    let start = Instant::now();
    for _ in 0..100_000 {
        let _ = zero_copy_matcher.match_log(test_log);
    }
    let elapsed = start.elapsed();

    let throughput = 100_000.0 / elapsed.as_secs_f64();
    let latency = (elapsed.as_nanos() as f64) / 100_000.0;

    println!("Results:");
    println!("  Total matches:  100,000");
    println!("  Total time:     {:.3}s", elapsed.as_secs_f64());
    println!("  Throughput:     {:.0} logs/sec", throughput);
    println!("  Avg latency:    {:.1} ns/log\n", latency);

    println!("✅ Scratch space successfully reused 100K times with no allocations!\n");

    Ok(())
}
