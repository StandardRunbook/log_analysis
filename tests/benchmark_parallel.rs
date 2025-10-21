/// High-Performance Parallel Benchmark with Multi-Level Parallelism
///
/// FEATURES:
/// - Parallel dataset processing (processes multiple datasets concurrently)
/// - Parallel log matching within each dataset (par_chunks for batch processing)
/// - Uses pre-built Aho-Corasick DFA from cache/
/// - Optimized for maximum throughput with nested parallelism
///
/// IMPORTANT: Always run with --release for accurate performance measurements!
///
/// Run with: cargo test --release --test benchmark_parallel -- --nocapture

use log_analyzer::log_matcher::LogMatcher;
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::matcher_config::MatcherConfig;
use log_analyzer::traits::DatasetLoader;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

const BATCH_SIZE: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTemplates {
    next_template_id: Option<u64>,
    templates: Vec<CachedTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTemplate {
    template_id: u64,
    pattern: String,
    variables: Vec<String>,
    example: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatasetResult {
    dataset_name: String,
    templates_loaded: usize,
    total_logs: usize,
    matched_logs: usize,
    elapsed_secs: f64,
    throughput: f64,
    avg_latency_us: f64, // microseconds for better precision
    match_rate: f64,
    grouping_accuracy: f64,
    batches_processed: usize,
    batch_size: usize,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    total_datasets: usize,
    successful_datasets: usize,
    total_logs: usize,
    total_time_secs: f64,
    overall_throughput: f64,
    avg_accuracy: f64,
    parallel_threads: usize,
    batch_size: usize,
    results: Vec<DatasetResult>,
}

/// Load cached templates and build matcher with optimized config
fn load_matcher(dataset_name: &str) -> anyhow::Result<LogMatcher> {
    let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());

    if !std::path::Path::new(&cache_file).exists() {
        anyhow::bail!("No cached templates: {}", cache_file);
    }

    let json_content = fs::read_to_string(&cache_file)?;
    let cached: CachedTemplates = serde_json::from_str(&json_content)?;

    // Use batch processing config for maximum throughput
    let config = MatcherConfig::batch_processing();
    let mut matcher = LogMatcher::with_config(config);

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

/// Benchmark a single dataset with batch processing
fn benchmark_dataset(dataset_name: &str, max_logs: Option<usize>) -> anyhow::Result<DatasetResult> {
    // Load matcher
    let matcher = load_matcher(dataset_name)?;
    let templates_loaded = matcher.get_all_templates().len();

    // Load dataset
    let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
    let logs = dataset.load_raw_logs()?;
    let ground_truth = dataset.load_ground_truth()?;

    let test_size = max_logs.unwrap_or(logs.len()).min(logs.len());
    let test_logs = &logs[..test_size];
    let test_gt = &ground_truth[..test_size.min(ground_truth.len())];

    // Process logs - use parallelism only for larger datasets
    let start = Instant::now();

    let template_assignments: Vec<Option<u64>> = if test_size >= 1000 {
        // Parallel processing for large datasets
        let log_refs: Vec<&str> = test_logs.iter().map(|s| s.as_str()).collect();
        log_refs
            .par_chunks(BATCH_SIZE)
            .flat_map(|batch| {
                batch.iter().map(|log_line| matcher.match_log(log_line)).collect::<Vec<_>>()
            })
            .collect()
    } else {
        // Sequential processing for small datasets (avoid parallelism overhead)
        test_logs.iter().map(|log_line| matcher.match_log(log_line.as_str())).collect()
    };

    let matched_count = template_assignments.iter().filter(|t| t.is_some()).count();

    let elapsed = start.elapsed();

    // Calculate metrics
    let throughput = test_size as f64 / elapsed.as_secs_f64();
    let avg_latency_us = (elapsed.as_micros() as f64) / test_size as f64;
    let match_rate = (matched_count as f64 / test_size as f64) * 100.0;

    // Calculate grouping accuracy
    let grouping_accuracy = calculate_accuracy(&template_assignments, test_gt);

    let num_batches = (test_size + BATCH_SIZE - 1) / BATCH_SIZE;

    Ok(DatasetResult {
        dataset_name: dataset_name.to_string(),
        templates_loaded,
        total_logs: test_size,
        matched_logs: matched_count,
        elapsed_secs: elapsed.as_secs_f64(),
        throughput,
        avg_latency_us,
        match_rate,
        grouping_accuracy,
        batches_processed: num_batches,
        batch_size: BATCH_SIZE,
    })
}

fn calculate_accuracy(
    assignments: &[Option<u64>],
    ground_truth: &[log_analyzer::traits::GroundTruthEntry],
) -> f64 {
    let mut gt_to_predicted: HashMap<String, Vec<u64>> = HashMap::new();

    for (idx, template_id) in assignments.iter().enumerate() {
        if let (Some(gt_entry), Some(tid)) = (ground_truth.get(idx), template_id) {
            gt_to_predicted
                .entry(gt_entry.event_id.clone())
                .or_default()
                .push(*tid);
        }
    }

    let mut gt_to_majority: HashMap<String, u64> = HashMap::new();
    for (gt_event, template_ids) in &gt_to_predicted {
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for tid in template_ids {
            *counts.entry(*tid).or_insert(0) += 1;
        }
        if let Some((&majority_tid, _)) = counts.iter().max_by_key(|(_, &count)| count) {
            gt_to_majority.insert(gt_event.clone(), majority_tid);
        }
    }

    let mut correct = 0;
    let mut total = 0;

    for (idx, template_id) in assignments.iter().enumerate() {
        if let Some(gt_entry) = ground_truth.get(idx) {
            if let Some(&majority_tid) = gt_to_majority.get(&gt_entry.event_id) {
                total += 1;
                if let Some(tid) = template_id {
                    if *tid == majority_tid {
                        correct += 1;
                    }
                }
            }
        }
    }

    if total > 0 {
        (correct as f64 / total as f64) * 100.0
    } else {
        0.0
    }
}

/// Get datasets with cached templates
fn get_cached_datasets() -> Vec<String> {
    let mut datasets = Vec::new();

    if let Ok(entries) = fs::read_dir("cache") {
        for entry in entries.flatten() {
            let filename = entry.file_name();
            if let Some(name) = filename.to_str() {
                if name.ends_with("_templates.json")
                    && !name.starts_with('.')
                    && !name.contains("comprehensive")
                    && !name.contains("ground_truth")
                    && !name.contains("tough")
                    && !name.contains("_top")
                    && !name.contains("_with_") {
                    if let Some(dataset) = name.strip_suffix("_templates.json") {
                        let dataset_name = dataset.chars().enumerate()
                            .map(|(i, c)| if i == 0 { c.to_uppercase().to_string() } else { c.to_string() })
                            .collect::<String>();
                        datasets.push(dataset_name);
                    }
                }
            }
        }
    }

    datasets.sort();
    datasets
}

/// Quick parallel benchmark (100 logs per dataset)
#[tokio::test]
async fn benchmark_parallel_quick() -> anyhow::Result<()> {
    run_parallel_benchmark(Some(100)).await
}

/// Sample parallel benchmark (500 logs per dataset)
#[tokio::test]
#[ignore]
async fn benchmark_parallel_sample() -> anyhow::Result<()> {
    run_parallel_benchmark(Some(500)).await
}

/// Full parallel benchmark (all logs)
#[tokio::test]
#[ignore]
async fn benchmark_parallel_full() -> anyhow::Result<()> {
    run_parallel_benchmark(None).await
}

async fn run_parallel_benchmark(max_logs: Option<usize>) -> anyhow::Result<()> {
    let overall_start = Instant::now();

    println!("\n{:=<100}", "");
    println!("🚀 HIGH-PERFORMANCE PARALLEL BENCHMARK");
    println!("{:=<100}", "");
    println!("Configuration:");
    println!("  Batch size:     {} logs/batch", BATCH_SIZE);
    println!("  Thread pool:    {} threads", rayon::current_num_threads());

    let log_limit = max_logs
        .map(|l| format!("{} logs per dataset", l))
        .unwrap_or_else(|| "all logs".to_string());
    println!("  Test size:      {}", log_limit);
    println!("{:=<100}\n", "");

    let datasets = get_cached_datasets();

    if datasets.is_empty() {
        println!("⚠️  No cached templates found in cache/");
        return Ok(());
    }

    println!("📦 Found {} datasets: {:?}\n", datasets.len(), datasets);

    // For small test sizes, process datasets in parallel (better for quick benchmarks)
    // For large test sizes, process sequentially with parallel log matching
    let use_dataset_parallelism = max_logs.map(|l| l < 1000).unwrap_or(false);

    if use_dataset_parallelism {
        println!("⚡ Processing datasets in parallel...\n");
    } else {
        println!("⚡ Processing datasets with parallel log matching...\n");
    }

    let results: Vec<_> = if use_dataset_parallelism {
        datasets
            .par_iter()
            .map(|dataset| {
                let result = benchmark_dataset(dataset, max_logs);
                match &result {
                    Ok(r) => {
                        println!("✅ {} - {:.0} logs/sec, {:.2}% accuracy",
                            dataset, r.throughput, r.grouping_accuracy);
                    }
                    Err(e) => {
                        println!("❌ {} - Error: {}", dataset, e);
                    }
                }
                result
            })
            .collect()
    } else {
        datasets.iter().map(|dataset| {
            let result = benchmark_dataset(dataset, max_logs);
            match &result {
                Ok(r) => {
                    println!("✅ {} - {:.0} logs/sec, {:.2}% accuracy",
                        dataset, r.throughput, r.grouping_accuracy);
                }
                Err(e) => {
                    println!("❌ {} - Error: {}", dataset, e);
                }
            }
            result
        })
        .collect()
    };

    let total_time = overall_start.elapsed().as_secs_f64();

    // Process results
    let mut successful_results = Vec::new();
    let mut total_logs = 0;

    for result in results {
        if let Ok(r) = result {
            total_logs += r.total_logs;
            successful_results.push(r);
        }
    }

    print_summary(&successful_results, total_logs, total_time, datasets.len());
    save_results(&successful_results, total_time)?;

    Ok(())
}

fn print_summary(results: &[DatasetResult], total_logs: usize, total_time: f64, total_datasets: usize) {
    println!("\n{:=<100}", "");
    println!("📊 BENCHMARK SUMMARY");
    println!("{:=<100}\n", "");

    let successful = results.len();
    let failed = total_datasets - successful;

    let avg_throughput = if !results.is_empty() {
        results.iter().map(|r| r.throughput).sum::<f64>() / results.len() as f64
    } else {
        0.0
    };

    let avg_accuracy = if !results.is_empty() {
        results.iter().map(|r| r.grouping_accuracy).sum::<f64>() / results.len() as f64
    } else {
        0.0
    };

    let overall_throughput = total_logs as f64 / total_time;

    println!("Overall Statistics:");
    println!("  Total datasets:        {}", total_datasets);
    println!("  Successful:            {} ✅", successful);
    println!("  Failed:                {} ❌", failed);
    println!("  Total logs:            {}", total_logs);
    println!("  Total time:            {:.2}s", total_time);
    println!("  Overall throughput:    {:.0} logs/sec 🚀", overall_throughput);
    println!("  Avg dataset throughput:{:.0} logs/sec", avg_throughput);
    println!("  Avg accuracy:          {:.2}%\n", avg_accuracy);

    // Sort by throughput
    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| b.throughput.partial_cmp(&a.throughput).unwrap_or(std::cmp::Ordering::Equal));

    println!("Dataset Results (sorted by throughput):");
    println!("{:-<100}", "");
    println!("{:<12} {:>10} {:>10} {:>12} {:>15} {:>12} {:>10}",
        "Dataset", "Templates", "Logs", "Match Rate", "Throughput", "Latency", "Accuracy");
    println!("{:-<100}", "");

    for r in &sorted {
        println!("{:<12} {:>10} {:>10} {:>11.1}% {:>12.0}/s {:>9.1}μs {:>9.2}%",
            r.dataset_name,
            r.templates_loaded,
            r.total_logs,
            r.match_rate,
            r.throughput,
            r.avg_latency_us,
            r.grouping_accuracy
        );
    }
    println!("{:-<100}", "");

    // Top performers
    println!("\n🏆 Top 5 by Throughput:");
    for (i, r) in sorted.iter().take(5).enumerate() {
        println!("  {}. {:<12} - {:>8.0} logs/sec ({:.1}μs/log)",
            i + 1, r.dataset_name, r.throughput, r.avg_latency_us);
    }

    sorted.sort_by(|a, b| b.grouping_accuracy.partial_cmp(&a.grouping_accuracy).unwrap_or(std::cmp::Ordering::Equal));
    println!("\n🎯 Top 5 by Accuracy:");
    for (i, r) in sorted.iter().take(5).enumerate() {
        println!("  {}. {:<12} - {:>6.2}% ({}/{} matched)",
            i + 1, r.dataset_name, r.grouping_accuracy, r.matched_logs, r.total_logs);
    }

    // Performance stats
    if let Some(fastest) = sorted.iter().max_by(|a, b| a.throughput.partial_cmp(&b.throughput).unwrap()) {
        println!("\n⚡ Performance Highlights:");
        println!("  Fastest:        {} at {:.0} logs/sec", fastest.dataset_name, fastest.throughput);
        println!("  Batch size:     {} logs", BATCH_SIZE);
        println!("  Parallel:       {} threads", rayon::current_num_threads());
        println!("  Total batches:  {}", results.iter().map(|r| r.batches_processed).sum::<usize>());
    }
}

fn save_results(results: &[DatasetResult], total_time: f64) -> anyhow::Result<()> {
    fs::create_dir_all("benchmark_results")?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let json_file = format!("benchmark_results/parallel_benchmark_{}.json", timestamp);
    let csv_file = format!("benchmark_results/parallel_benchmark_{}.csv", timestamp);

    let summary = BenchmarkSummary {
        total_datasets: results.len(),
        successful_datasets: results.len(),
        total_logs: results.iter().map(|r| r.total_logs).sum(),
        total_time_secs: total_time,
        overall_throughput: results.iter().map(|r| r.total_logs).sum::<usize>() as f64 / total_time,
        avg_accuracy: results.iter().map(|r| r.grouping_accuracy).sum::<f64>() / results.len() as f64,
        parallel_threads: rayon::current_num_threads(),
        batch_size: BATCH_SIZE,
        results: results.to_vec(),
    };

    fs::write(&json_file, serde_json::to_string_pretty(&summary)?)?;
    println!("\n💾 Results saved to: {}", json_file);

    // CSV
    let mut csv = String::from("Dataset,Templates,Logs,Matched,MatchRate,Throughput,LatencyUs,Accuracy,Batches,BatchSize\n");
    for r in results {
        csv.push_str(&format!(
            "{},{},{},{},{:.2},{:.0},{:.1},{:.2},{},{}\n",
            r.dataset_name, r.templates_loaded, r.total_logs, r.matched_logs,
            r.match_rate, r.throughput, r.avg_latency_us, r.grouping_accuracy,
            r.batches_processed, r.batch_size
        ));
    }
    fs::write(&csv_file, csv)?;
    println!("💾 CSV saved to: {}", csv_file);

    Ok(())
}
