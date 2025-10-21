/// Performance Comparison: Standard vs Optimized Matcher
///
/// Compares:
/// - LogMatcher (standard): Uses std::HashMap, allocates on every match
/// - FastLogMatcher (optimized): Uses FxHashMap, minimal allocations
///
/// IMPORTANT: Always run with --release for accurate measurements!
///
/// Run with: cargo test --release --test benchmark_optimized -- --nocapture

use log_analyzer::log_matcher::LogMatcher;
use log_analyzer::log_matcher_fast::FastLogMatcher;
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

fn load_fast_matcher(dataset_name: &str) -> anyhow::Result<FastLogMatcher> {
    let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());
    let json_content = fs::read_to_string(&cache_file)?;
    let cached: CachedTemplates = serde_json::from_str(&json_content)?;

    let mut matcher = FastLogMatcher::new();
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
fn benchmark_comparison_apache() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("⚡ PERFORMANCE COMPARISON - Apache");
    println!("{:=<80}\n", "");

    let dataset = LogHubDatasetLoader::new("Apache", "data/loghub");
    let logs = dataset.load_raw_logs()?;
    let test_size = 1000.min(logs.len());
    let test_logs = &logs[..test_size];

    // Standard matcher
    let std_matcher = load_standard_matcher("Apache")?;
    println!("Standard Matcher (std::HashMap):");

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

    // Fast matcher
    let fast_matcher = load_fast_matcher("Apache")?;
    println!("Fast Matcher (FxHashMap + optimizations):");

    let start = Instant::now();
    let mut matched = 0;
    for log in test_logs {
        if fast_matcher.match_log(log).is_some() {
            matched += 1;
        }
    }
    let elapsed = start.elapsed();
    let fast_throughput = test_size as f64 / elapsed.as_secs_f64();
    let fast_latency = (elapsed.as_nanos() as f64) / test_size as f64;
    println!("  Throughput: {:>12.0} logs/sec", fast_throughput);
    println!("  Latency:    {:>12.1} ns/log", fast_latency);
    println!("  Matched:    {:>12}/{}\n", matched, test_size);

    let speedup = fast_throughput / std_throughput;
    let latency_improvement = ((std_latency - fast_latency) / std_latency) * 100.0;

    println!("{:=<80}", "");
    println!("Speedup:            {:.2}x faster ⚡", speedup);
    println!("Latency reduction:  {:.1}% improvement", latency_improvement);
    println!("{:=<80}\n", "");

    Ok(())
}

#[test]
fn benchmark_comparison_all() -> anyhow::Result<()> {
    println!("\n{:=<100}", "");
    println!("⚡ PERFORMANCE COMPARISON - All Datasets");
    println!("{:=<100}\n", "");

    let datasets = vec![
        "Android", "Apache", "Bgl", "Hadoop", "Hdfs", "Healthapp",
        "Hpc", "Linux", "Mac", "Openssh", "Openstack", "Proxifier",
        "Spark", "Thunderbird", "Windows", "Zookeeper"
    ];

    println!("{:<15} {:>12} {:>15} {:>15} {:>12}",
        "Dataset", "Templates", "Standard", "Optimized", "Speedup");
    println!("{:-<100}", "");

    let mut total_speedup = 0.0;
    let mut count = 0;

    for dataset_name in &datasets {
        let std_matcher = match load_standard_matcher(dataset_name) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let fast_matcher = match load_fast_matcher(dataset_name) {
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
        let std_throughput = test_size as f64 / start.elapsed().as_secs_f64();

        // Fast matcher
        let start = Instant::now();
        for log in test_logs {
            let _ = fast_matcher.match_log(log);
        }
        let fast_throughput = test_size as f64 / start.elapsed().as_secs_f64();

        let speedup = fast_throughput / std_throughput;
        total_speedup += speedup;
        count += 1;

        let speedup_symbol = if speedup > 1.5 {
            "⚡⚡"
        } else if speedup > 1.2 {
            "⚡"
        } else {
            ""
        };

        println!("{:<15} {:>12} {:>12.0}/s {:>12.0}/s {:>9.2}x {}",
            dataset_name,
            std_matcher.get_all_templates().len(),
            std_throughput,
            fast_throughput,
            speedup,
            speedup_symbol
        );
    }

    let avg_speedup = total_speedup / count as f64;

    println!("{:-<100}", "");
    println!("\nAverage speedup: {:.2}x faster ⚡\n", avg_speedup);

    Ok(())
}

#[test]
fn benchmark_batch_operations() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("⚡ BATCH MATCHING COMPARISON - Apache (1000 logs)");
    println!("{:=<80}\n", "");

    let dataset = LogHubDatasetLoader::new("Apache", "data/loghub");
    let logs = dataset.load_raw_logs()?;
    let test_size = 1000.min(logs.len());
    let test_logs: Vec<&str> = logs[..test_size].iter().map(|s| s.as_str()).collect();

    // Standard matcher - batch
    let std_matcher = load_standard_matcher("Apache")?;
    println!("Standard Matcher - Batch:");

    let start = Instant::now();
    let results = std_matcher.match_batch(&test_logs);
    let elapsed = start.elapsed();
    let std_throughput = test_size as f64 / elapsed.as_secs_f64();
    let matched = results.iter().filter(|r| r.is_some()).count();
    println!("  Throughput: {:>12.0} logs/sec", std_throughput);
    println!("  Matched:    {:>12}/{}\n", matched, test_size);

    // Fast matcher - batch
    let fast_matcher = load_fast_matcher("Apache")?;
    println!("Fast Matcher - Batch:");

    let start = Instant::now();
    let results = fast_matcher.match_batch(&test_logs);
    let elapsed = start.elapsed();
    let fast_throughput = test_size as f64 / elapsed.as_secs_f64();
    let matched = results.iter().filter(|r| r.is_some()).count();
    println!("  Throughput: {:>12.0} logs/sec", fast_throughput);
    println!("  Matched:    {:>12}/{}\n", matched, test_size);

    let speedup = fast_throughput / std_throughput;
    println!("{:=<80}", "");
    println!("Batch speedup: {:.2}x faster ⚡\n", speedup);

    Ok(())
}
