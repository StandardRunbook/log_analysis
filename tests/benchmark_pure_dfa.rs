/// Pure DFA Matching Benchmark
///
/// Tests ONLY the raw matching performance using pre-built Aho-Corasick DFA.
/// No parallelism, no accuracy calculation, no overhead - just pure throughput.
///
/// IMPORTANT: Always run with --release for accurate performance measurements!
/// Debug mode is ~20-50x slower than release mode.
///
/// Run with: cargo test --release --test benchmark_pure_dfa -- --nocapture

use log_analyzer::log_matcher::LogMatcher;
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

fn load_matcher(dataset_name: &str) -> anyhow::Result<LogMatcher> {
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

#[test]
fn benchmark_pure_apache() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("⚡ PURE DFA BENCHMARK - Apache");
    println!("{:=<80}\n", "");

    let matcher = load_matcher("Apache")?;
    let dataset = LogHubDatasetLoader::new("Apache", "data/loghub");
    let logs = dataset.load_raw_logs()?;

    println!("Templates: {}", matcher.get_all_templates().len());
    println!("Total logs: {}\n", logs.len());

    // Warm-up
    for log in logs.iter().take(10) {
        matcher.match_log(log);
    }

    // Benchmark different sizes
    for test_size in [100, 500, 1000, 5000, logs.len().min(10000)] {
        let test_logs = &logs[..test_size];

        let start = Instant::now();
        let mut matched = 0;

        for log in test_logs {
            if matcher.match_log(log).is_some() {
                matched += 1;
            }
        }

        let elapsed = start.elapsed();
        let throughput = test_size as f64 / elapsed.as_secs_f64();
        let latency_us = (elapsed.as_micros() as f64) / test_size as f64;

        println!("{:>6} logs: {:>8.0} logs/sec  ({:>6.1} μs/log)  [matched: {}]",
            test_size, throughput, latency_us, matched);
    }

    Ok(())
}

#[test]
fn benchmark_pure_all_datasets() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("⚡ PURE DFA BENCHMARK - All Datasets (1000 logs each)");
    println!("{:=<80}\n", "");

    let datasets = vec![
        "Android", "Apache", "Bgl", "Hadoop", "Hdfs", "Healthapp",
        "Hpc", "Linux", "Mac", "Openssh", "Openstack", "Proxifier",
        "Spark", "Thunderbird", "Windows", "Zookeeper"
    ];

    println!("{:<15} {:>10} {:>12} {:>15} {:>12}",
        "Dataset", "Templates", "Logs", "Throughput", "Latency");
    println!("{:-<80}", "");

    let mut total_logs = 0;
    let overall_start = Instant::now();

    for dataset_name in &datasets {
        let matcher = match load_matcher(dataset_name) {
            Ok(m) => m,
            Err(_) => {
                println!("{:<15} {:>10} {:>12} {:>15} {:>12}",
                    dataset_name, "-", "-", "ERROR", "-");
                continue;
            }
        };

        let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
        let logs = match dataset.load_raw_logs() {
            Ok(l) => l,
            Err(_) => continue,
        };

        let test_size = 1000.min(logs.len());
        let test_logs = &logs[..test_size];

        let start = Instant::now();
        let mut matched = 0;

        for log in test_logs {
            if matcher.match_log(log).is_some() {
                matched += 1;
            }
        }

        let elapsed = start.elapsed();
        let throughput = test_size as f64 / elapsed.as_secs_f64();
        let latency_us = (elapsed.as_micros() as f64) / test_size as f64;

        println!("{:<15} {:>10} {:>12} {:>12.0}/s {:>9.1} μs",
            dataset_name,
            matcher.get_all_templates().len(),
            test_size,
            throughput,
            latency_us
        );

        total_logs += test_size;
    }

    let total_time = overall_start.elapsed();
    let overall_throughput = total_logs as f64 / total_time.as_secs_f64();

    println!("{:-<80}", "");
    println!("\nOverall: {} logs in {:.2}s = {:.0} logs/sec\n",
        total_logs, total_time.as_secs_f64(), overall_throughput);

    Ok(())
}
