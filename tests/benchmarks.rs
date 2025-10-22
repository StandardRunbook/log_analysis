/// Consolidated Log Analyzer Benchmark Suite
///
/// This is the CANONICAL way to benchmark the log analyzer.
/// All benchmarks use the optimized LogMatcher with zero-copy optimizations.
///
/// ## Benchmark Modes:
///
/// 1. **Quick** - Fast smoke tests (100 logs per dataset)
///    ```bash
///    cargo test --release --test benchmarks quick -- --nocapture
///    ```
///
/// 2. **Throughput** - Pure matching performance with cached templates
///    ```bash
///    cargo test --release --test benchmarks throughput -- --nocapture
///    ```
///
/// 3. **Parallel** - Multi-threaded benchmark across all datasets
///    ```bash
///    cargo test --release --test benchmarks parallel -- --nocapture
///    ```
///
/// 4. **Accuracy** - Template generation + accuracy measurement
///    ```bash
///    cargo test --release --test benchmarks accuracy -- --nocapture
///    ```
///
/// 5. **Full** - Comprehensive benchmark (all datasets, all logs)
///    ```bash
///    cargo test --release --test benchmarks full -- --nocapture --ignored
///    ```
///
/// ## Performance Tips:
/// - ALWAYS use `--release` flag for accurate measurements
/// - Debug mode is 20-50x slower than release mode
/// - Use `--test-threads=1` to avoid contention (except for parallel tests)
///
/// ## Output:
/// - Results are saved to `benchmark_results/` directory
/// - JSON format for programmatic analysis
/// - CSV format for spreadsheets

use log_analyzer::benchmark_runner::run_benchmark;
use log_analyzer::implementations::{LLMTemplateGenerator, RegexLogMatcher};
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::matcher_config::MatcherConfig;
use log_analyzer::traits::{BenchmarkConfig, DatasetLoader};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTemplates {
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Clone, Serialize)]
struct DatasetResult {
    dataset_name: String,
    templates_loaded: usize,
    total_logs: usize,
    matched_logs: usize,
    elapsed_secs: f64,
    throughput: f64,
    avg_latency_us: f64,
    match_rate: f64,
    grouping_accuracy: f64,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    benchmark_type: String,
    total_datasets: usize,
    successful_datasets: usize,
    total_logs: usize,
    total_time_secs: f64,
    overall_throughput: f64,
    avg_accuracy: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_threads: Option<usize>,
    results: Vec<DatasetResult>,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get all datasets with cached templates
fn get_cached_datasets() -> Vec<String> {
    let mut datasets = Vec::new();

    if let Ok(entries) = fs::read_dir("cache") {
        for entry in entries.flatten() {
            let filename = entry.file_name();
            if let Some(name) = filename.to_str() {
                if name.ends_with("_templates.json") && !name.starts_with('.') {
                    if let Some(dataset) = name.strip_suffix("_templates.json") {
                        let dataset_name = capitalize(dataset);
                        datasets.push(dataset_name);
                    }
                }
            }
        }
    }

    datasets.sort();
    datasets
}

/// Capitalize first letter
fn capitalize(s: &str) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| if i == 0 { c.to_uppercase().to_string() } else { c.to_string() })
        .collect()
}

/// Load cached templates and build optimized matcher
fn load_cached_matcher(dataset_name: &str) -> anyhow::Result<LogMatcher> {
    let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());

    if !std::path::Path::new(&cache_file).exists() {
        anyhow::bail!("No cached templates: {}", cache_file);
    }

    let json_content = fs::read_to_string(&cache_file)?;
    let cached: CachedTemplates = serde_json::from_str(&json_content)?;

    let config = MatcherConfig::batch_processing();
    let mut matcher = LogMatcher::with_config(config);

    for template in cached.templates {
        matcher.add_template(LogTemplate {
            template_id: template.template_id,
            pattern: template.pattern,
            variables: template.variables,
            example: template.example,
        });
    }

    Ok(matcher)
}

/// Calculate grouping accuracy
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

/// Save benchmark results
fn save_results(summary: &BenchmarkSummary) -> anyhow::Result<()> {
    fs::create_dir_all("benchmark_results")?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let json_file = format!(
        "benchmark_results/{}_{}.json",
        summary.benchmark_type, timestamp
    );
    let csv_file = format!(
        "benchmark_results/{}_{}.csv",
        summary.benchmark_type, timestamp
    );

    // Save JSON
    fs::write(&json_file, serde_json::to_string_pretty(&summary)?)?;
    println!("\n💾 Results saved to: {}", json_file);

    // Save CSV
    let mut csv = String::from(
        "Dataset,Templates,Logs,Matched,MatchRate,Throughput,LatencyUs,Accuracy\n",
    );
    for r in &summary.results {
        csv.push_str(&format!(
            "{},{},{},{},{:.2},{:.0},{:.1},{:.2}\n",
            r.dataset_name,
            r.templates_loaded,
            r.total_logs,
            r.matched_logs,
            r.match_rate,
            r.throughput,
            r.avg_latency_us,
            r.grouping_accuracy
        ));
    }
    fs::write(&csv_file, csv)?;
    println!("💾 CSV saved to: {}", csv_file);

    Ok(())
}

// ============================================================================
// Benchmark: Quick (100 logs per dataset)
// ============================================================================

#[tokio::test]
async fn quick() -> anyhow::Result<()> {
    println!("\n{:=<100}", "");
    println!("⚡ QUICK BENCHMARK (100 logs per dataset)");
    println!("{:=<100}\n", "");

    let datasets = get_cached_datasets();
    if datasets.is_empty() {
        println!("⚠️  No cached templates found. Run template generation first.");
        return Ok(());
    }

    let results = benchmark_datasets_with_cache(&datasets, Some(100), true).await?;
    print_summary("quick", &results);
    Ok(())
}

// ============================================================================
// Benchmark: Throughput (pure matching speed)
// ============================================================================

#[tokio::test]
async fn throughput() -> anyhow::Result<()> {
    println!("\n{:=<100}", "");
    println!("🚀 THROUGHPUT BENCHMARK (pure matching performance)");
    println!("{:=<100}\n", "");

    let datasets = vec!["Apache", "Linux", "Hdfs", "OpenStack"];
    let sizes = vec![100, 500, 1000, 5000];

    for dataset_name in datasets {
        let matcher = match load_cached_matcher(dataset_name) {
            Ok(m) => m,
            Err(_) => {
                println!("❌ {} - no cached templates", dataset_name);
                continue;
            }
        };

        let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
        let logs = dataset.load_raw_logs()?;

        println!("\n{} ({}templates):", dataset_name, matcher.get_all_templates().len());
        println!("  {:>8} {:>15} {:>12}", "Logs", "Throughput", "Latency");
        println!("  {:-<40}", "");

        for &size in &sizes {
            if size > logs.len() {
                continue;
            }

            let test_logs = &logs[..size];
            let start = Instant::now();

            for log in test_logs {
                matcher.match_log(log);
            }

            let elapsed = start.elapsed();
            let throughput = size as f64 / elapsed.as_secs_f64();
            let latency_us = (elapsed.as_micros() as f64) / size as f64;

            println!("  {:>8} {:>12.0}/s {:>9.1}μs", size, throughput, latency_us);
        }
    }

    Ok(())
}

// ============================================================================
// Benchmark: Parallel (multi-threaded across all datasets)
// ============================================================================

#[tokio::test]
async fn parallel() -> anyhow::Result<()> {
    println!("\n{:=<100}", "");
    println!("🚀 PARALLEL BENCHMARK");
    println!("{:=<100}", "");
    println!("Configuration:");
    println!("  Threads:   {} threads", rayon::current_num_threads());
    println!("  Test size: 500 logs per dataset");
    println!("{:=<100}\n", "");

    let datasets = get_cached_datasets();
    if datasets.is_empty() {
        println!("⚠️  No cached templates found.");
        return Ok(());
    }

    let start = Instant::now();

    let results: Vec<DatasetResult> = datasets
        .par_iter()
        .filter_map(|dataset| {
            match benchmark_single_dataset_cached(dataset, Some(500)) {
                Ok(r) => {
                    println!(
                        "✅ {} - {:.0} logs/sec, {:.2}% accuracy",
                        dataset, r.throughput, r.grouping_accuracy
                    );
                    Some(r)
                }
                Err(e) => {
                    println!("❌ {} - Error: {}", dataset, e);
                    None
                }
            }
        })
        .collect();

    let total_time = start.elapsed().as_secs_f64();
    print_summary_with_time("parallel", &results, total_time, Some(rayon::current_num_threads()));

    Ok(())
}

// ============================================================================
// Benchmark: Accuracy (with template generation)
// ============================================================================

#[tokio::test]
#[ignore]
async fn accuracy() -> anyhow::Result<()> {
    println!("\n{:=<100}", "");
    println!("🎯 ACCURACY BENCHMARK (with template generation)");
    println!("{:=<100}\n", "");

    let datasets = vec!["Linux", "Apache", "OpenStack"];

    for dataset_name in datasets {
        println!("\n{:=<80}", "");
        println!("📊 Testing: {}", dataset_name);
        println!("{:=<80}\n", "");

        let generator = LLMTemplateGenerator::mock();
        let mut matcher = RegexLogMatcher::new();
        let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");

        let config = BenchmarkConfig {
            max_logs: Some(500),
            use_batch: true,
            verbose: true,
            min_accuracy: 70.0,
            ..Default::default()
        };

        let results = run_benchmark(&generator, &mut matcher, &dataset, &config).await?;
        println!(
            "\n✅ {} - {:.2}% accuracy, {:.0} logs/sec\n",
            dataset_name, results.grouping_accuracy, results.throughput
        );
    }

    Ok(())
}

// ============================================================================
// Benchmark: Full (all datasets, all logs)
// ============================================================================

#[tokio::test]
#[ignore]
async fn full() -> anyhow::Result<()> {
    println!("\n{:=<100}", "");
    println!("🔥 FULL BENCHMARK (all datasets, all logs)");
    println!("{:=<100}\n", "");

    let datasets = get_cached_datasets();
    if datasets.is_empty() {
        println!("⚠️  No cached templates found.");
        return Ok(());
    }

    let results = benchmark_datasets_with_cache(&datasets, None, false).await?;
    print_summary("full", &results);
    Ok(())
}

// ============================================================================
// Core Benchmark Functions
// ============================================================================

async fn benchmark_datasets_with_cache(
    datasets: &[String],
    max_logs: Option<usize>,
    parallel: bool,
) -> anyhow::Result<Vec<DatasetResult>> {
    let results: Vec<_> = if parallel {
        datasets
            .par_iter()
            .filter_map(|dataset| benchmark_single_dataset_cached(dataset, max_logs).ok())
            .collect()
    } else {
        datasets
            .iter()
            .filter_map(|dataset| {
                let result = benchmark_single_dataset_cached(dataset, max_logs);
                match &result {
                    Ok(r) => {
                        println!(
                            "✅ {} - {:.0} logs/sec, {:.2}% accuracy",
                            dataset, r.throughput, r.grouping_accuracy
                        );
                    }
                    Err(e) => {
                        println!("❌ {} - Error: {}", dataset, e);
                    }
                }
                result.ok()
            })
            .collect()
    };

    Ok(results)
}

fn benchmark_single_dataset_cached(
    dataset_name: &str,
    max_logs: Option<usize>,
) -> anyhow::Result<DatasetResult> {
    let matcher = load_cached_matcher(dataset_name)?;
    let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
    let logs = dataset.load_raw_logs()?;
    let ground_truth = dataset.load_ground_truth()?;

    let test_size = max_logs.unwrap_or(logs.len()).min(logs.len());
    let test_logs = &logs[..test_size];
    let test_gt = &ground_truth[..test_size.min(ground_truth.len())];

    let start = Instant::now();
    let template_assignments: Vec<Option<u64>> = test_logs
        .iter()
        .map(|log| matcher.match_log(log))
        .collect();
    let elapsed = start.elapsed();

    let matched_count = template_assignments.iter().filter(|t| t.is_some()).count();
    let throughput = test_size as f64 / elapsed.as_secs_f64();
    let avg_latency_us = (elapsed.as_micros() as f64) / test_size as f64;
    let match_rate = (matched_count as f64 / test_size as f64) * 100.0;
    let grouping_accuracy = calculate_accuracy(&template_assignments, test_gt);

    Ok(DatasetResult {
        dataset_name: dataset_name.to_string(),
        templates_loaded: matcher.get_all_templates().len(),
        total_logs: test_size,
        matched_logs: matched_count,
        elapsed_secs: elapsed.as_secs_f64(),
        throughput,
        avg_latency_us,
        match_rate,
        grouping_accuracy,
    })
}

// ============================================================================
// Output Functions
// ============================================================================

fn print_summary(benchmark_type: &str, results: &[DatasetResult]) {
    let total_time: f64 = results.iter().map(|r| r.elapsed_secs).sum();
    print_summary_with_time(benchmark_type, results, total_time, None);
}

fn print_summary_with_time(
    benchmark_type: &str,
    results: &[DatasetResult],
    total_time: f64,
    threads: Option<usize>,
) {
    if results.is_empty() {
        println!("\n❌ No results to display");
        return;
    }

    let total_logs: usize = results.iter().map(|r| r.total_logs).sum();
    let avg_throughput = results.iter().map(|r| r.throughput).sum::<f64>() / results.len() as f64;
    let avg_accuracy =
        results.iter().map(|r| r.grouping_accuracy).sum::<f64>() / results.len() as f64;
    let overall_throughput = total_logs as f64 / total_time;

    println!("\n{:=<100}", "");
    println!("📊 BENCHMARK SUMMARY");
    println!("{:=<100}\n", "");
    println!("Overall Statistics:");
    println!("  Total datasets:        {}", results.len());
    println!("  Total logs:            {}", total_logs);
    println!("  Total time:            {:.2}s", total_time);
    println!(
        "  Overall throughput:    {:.0} logs/sec 🚀",
        overall_throughput
    );
    println!("  Avg dataset throughput:{:.0} logs/sec", avg_throughput);
    println!("  Avg accuracy:          {:.2}%", avg_accuracy);
    if let Some(t) = threads {
        println!("  Parallel threads:      {}", t);
    }
    println!();

    // Print table
    println!("{:-<100}", "");
    println!(
        "{:<12} {:>10} {:>10} {:>12} {:>15} {:>12} {:>10}",
        "Dataset", "Templates", "Logs", "Match Rate", "Throughput", "Latency", "Accuracy"
    );
    println!("{:-<100}", "");

    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| {
        b.throughput
            .partial_cmp(&a.throughput)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for r in &sorted {
        println!(
            "{:<12} {:>10} {:>10} {:>11.1}% {:>12.0}/s {:>9.1}μs {:>9.2}%",
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

    // Save results
    let summary = BenchmarkSummary {
        benchmark_type: benchmark_type.to_string(),
        total_datasets: results.len(),
        successful_datasets: results.len(),
        total_logs,
        total_time_secs: total_time,
        overall_throughput,
        avg_accuracy,
        parallel_threads: threads,
        results: results.to_vec(),
    };

    if let Err(e) = save_results(&summary) {
        eprintln!("⚠️  Failed to save results: {}", e);
    }
}
