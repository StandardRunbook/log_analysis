/// Benchmark using pre-built cached templates (Aho-Corasick DFA)
///
/// This benchmark loads pre-generated templates from cache/ directory
/// and uses the Aho-Corasick DFA for high-performance matching.
///
/// Run with: cargo test --test benchmark_with_cached_templates -- --nocapture --test-threads=1

use log_analyzer::log_matcher::LogMatcher;
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::traits::DatasetLoader;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

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
struct DatasetBenchmarkResult {
    dataset_name: String,
    cache_file: String,
    templates_loaded: usize,
    total_logs: usize,
    matched_logs: usize,
    unmatched_logs: usize,
    elapsed_secs: f64,
    throughput: f64,
    avg_latency_ms: f64,
    match_rate: f64,
    grouping_accuracy: f64,
    success: bool,
    error: Option<String>,
}

/// Load cached templates for a dataset
fn load_cached_templates(dataset_name: &str) -> anyhow::Result<LogMatcher> {
    // Try to find cached template file
    let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());

    if !std::path::Path::new(&cache_file).exists() {
        anyhow::bail!("No cached templates found: {}", cache_file);
    }

    println!("   📂 Loading templates from: {}", cache_file);

    // Load JSON
    let json_content = fs::read_to_string(&cache_file)?;
    let cached: CachedTemplates = serde_json::from_str(&json_content)?;

    println!("   ✓ Loaded {} templates", cached.templates.len());

    // Create matcher and load templates
    let mut matcher = LogMatcher::new();

    for template in cached.templates {
        matcher.add_template(log_analyzer::log_matcher::LogTemplate {
            template_id: template.template_id,
            pattern: template.pattern,
            variables: template.variables,
            example: template.example,
        });
    }

    let template_count = matcher.get_all_templates().len();
    println!("   ✓ Built Aho-Corasick DFA with {} templates\n", template_count);

    Ok(matcher)
}

/// Benchmark a single dataset with cached templates
async fn benchmark_with_cache(dataset_name: &str, max_logs: Option<usize>) -> DatasetBenchmarkResult {
    println!("{:=<80}", "");
    println!("🔍 Benchmarking: {} (with cached templates)", dataset_name);
    println!("{:=<80}", "");

    // Load cached templates
    let matcher = match load_cached_templates(dataset_name) {
        Ok(m) => m,
        Err(e) => {
            println!("   ❌ Error: {}\n", e);
            return DatasetBenchmarkResult {
                dataset_name: dataset_name.to_string(),
                cache_file: format!("cache/{}_templates.json", dataset_name.to_lowercase()),
                templates_loaded: 0,
                total_logs: 0,
                matched_logs: 0,
                unmatched_logs: 0,
                elapsed_secs: 0.0,
                throughput: 0.0,
                avg_latency_ms: 0.0,
                match_rate: 0.0,
                grouping_accuracy: 0.0,
                success: false,
                error: Some(e.to_string()),
            };
        }
    };

    let templates_loaded = matcher.get_all_templates().len();

    // Load dataset
    let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
    let (logs, ground_truth) = match (dataset.load_raw_logs(), dataset.load_ground_truth()) {
        (Ok(logs), Ok(gt)) => (logs, gt),
        (Err(e), _) | (_, Err(e)) => {
            println!("   ❌ Error loading dataset: {}\n", e);
            return DatasetBenchmarkResult {
                dataset_name: dataset_name.to_string(),
                cache_file: format!("cache/{}_templates.json", dataset_name.to_lowercase()),
                templates_loaded,
                total_logs: 0,
                matched_logs: 0,
                unmatched_logs: 0,
                elapsed_secs: 0.0,
                throughput: 0.0,
                avg_latency_ms: 0.0,
                match_rate: 0.0,
                grouping_accuracy: 0.0,
                success: false,
                error: Some(e.to_string()),
            };
        }
    };

    let test_size = max_logs.unwrap_or(logs.len()).min(logs.len());
    let test_logs = &logs[..test_size];
    let test_gt = &ground_truth[..test_size.min(ground_truth.len())];

    println!("   📊 Testing {} logs\n", test_size);

    // Benchmark matching
    let start = Instant::now();
    let mut matched = 0;
    let mut template_assignments = Vec::new();

    for log_line in test_logs {
        if let Some(template_id) = matcher.match_log(log_line) {
            matched += 1;
            template_assignments.push(Some(template_id));
        } else {
            template_assignments.push(None);
        }
    }

    let elapsed = start.elapsed();
    let unmatched = test_size - matched;
    let match_rate = (matched as f64 / test_size as f64) * 100.0;
    let throughput = test_size as f64 / elapsed.as_secs_f64();
    let avg_latency_ms = (elapsed.as_millis() as f64) / test_size as f64;

    // Calculate grouping accuracy
    let grouping_accuracy = calculate_accuracy(&template_assignments, test_gt);

    println!("   ⚡ Performance:");
    println!("      Throughput: {:.0} logs/sec", throughput);
    println!("      Latency: {:.3} ms/log", avg_latency_ms);
    println!("      Match rate: {:.2}% ({}/{})", match_rate, matched, test_size);
    println!("      Accuracy: {:.2}%\n", grouping_accuracy);

    DatasetBenchmarkResult {
        dataset_name: dataset_name.to_string(),
        cache_file: format!("cache/{}_templates.json", dataset_name.to_lowercase()),
        templates_loaded,
        total_logs: test_size,
        matched_logs: matched,
        unmatched_logs: unmatched,
        elapsed_secs: elapsed.as_secs_f64(),
        throughput,
        avg_latency_ms,
        match_rate,
        grouping_accuracy,
        success: true,
        error: None,
    }
}

fn calculate_accuracy(
    template_assignments: &[Option<u64>],
    ground_truth: &[log_analyzer::traits::GroundTruthEntry],
) -> f64 {
    let mut gt_to_predicted: HashMap<String, Vec<u64>> = HashMap::new();

    for (idx, template_id) in template_assignments.iter().enumerate() {
        if let Some(gt_entry) = ground_truth.get(idx) {
            if let Some(tid) = template_id {
                gt_to_predicted
                    .entry(gt_entry.event_id.clone())
                    .or_insert_with(Vec::new)
                    .push(*tid);
            }
        }
    }

    let mut gt_to_majority_template: HashMap<String, u64> = HashMap::new();
    for (gt_event, template_ids) in &gt_to_predicted {
        let mut counts: HashMap<u64, usize> = HashMap::new();
        for tid in template_ids {
            *counts.entry(*tid).or_insert(0) += 1;
        }
        if let Some((&majority_tid, _)) = counts.iter().max_by_key(|&(_, count)| count) {
            gt_to_majority_template.insert(gt_event.clone(), majority_tid);
        }
    }

    let mut correct = 0;
    let mut total = 0;

    for (idx, template_id) in template_assignments.iter().enumerate() {
        if let Some(gt_entry) = ground_truth.get(idx) {
            if let Some(&majority_tid) = gt_to_majority_template.get(&gt_entry.event_id) {
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

/// Get datasets that have cached templates
fn get_cached_datasets() -> Vec<String> {
    let mut datasets = Vec::new();

    if let Ok(entries) = fs::read_dir("cache") {
        for entry in entries.flatten() {
            let filename = entry.file_name();
            if let Some(name) = filename.to_str() {
                if name.ends_with("_templates.json") && !name.starts_with('.') {
                    // Extract dataset name
                    if let Some(dataset) = name.strip_suffix("_templates.json") {
                        // Capitalize first letter
                        let dataset_name = dataset.chars().enumerate().map(|(i, c)| {
                            if i == 0 { c.to_uppercase().to_string() } else { c.to_string() }
                        }).collect::<String>();
                        datasets.push(dataset_name);
                    }
                }
            }
        }
    }

    datasets.sort();
    datasets
}

/// Benchmark all datasets with cached templates
#[tokio::test]
async fn benchmark_all_cached_quick() -> anyhow::Result<()> {
    benchmark_all_cached_internal(Some(100)).await
}

#[tokio::test]
#[ignore]
async fn benchmark_all_cached_sample() -> anyhow::Result<()> {
    benchmark_all_cached_internal(Some(500)).await
}

#[tokio::test]
#[ignore]
async fn benchmark_all_cached_full() -> anyhow::Result<()> {
    benchmark_all_cached_internal(None).await
}

async fn benchmark_all_cached_internal(max_logs: Option<usize>) -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("📊 CACHED TEMPLATES BENCHMARK (Aho-Corasick DFA)");
    println!("{:=<80}\n", "");

    let log_limit = max_logs.map(|l| format!("{} logs per dataset", l))
        .unwrap_or_else(|| "all logs".to_string());
    println!("Testing: {}\n", log_limit);

    let datasets = get_cached_datasets();

    if datasets.is_empty() {
        println!("⚠️  No cached templates found in cache/ directory");
        println!("   Generate templates first using the examples");
        return Ok(());
    }

    println!("Found {} datasets with cached templates: {:?}\n", datasets.len(), datasets);

    let start_time = Instant::now();
    let mut results = Vec::new();
    let mut total_logs = 0;
    let mut total_matched = 0;
    let mut _successful = 0;

    for dataset in &datasets {
        let result = benchmark_with_cache(dataset, max_logs).await;

        if result.success {
            _successful += 1;
            total_logs += result.total_logs;
            total_matched += result.matched_logs;
        }

        results.push(result);
    }

    let total_time = start_time.elapsed().as_secs_f64();

    print_summary(&results, total_logs, total_matched, total_time);
    save_results(&results, total_time)?;

    Ok(())
}

fn print_summary(results: &[DatasetBenchmarkResult], total_logs: usize, total_matched: usize, total_time: f64) {
    println!("\n{:=<80}", "");
    println!("📊 BENCHMARK SUMMARY");
    println!("{:=<80}\n", "");

    let successful = results.iter().filter(|r| r.success).count();
    let failed = results.len() - successful;

    let avg_throughput = if successful > 0 {
        results.iter().filter(|r| r.success).map(|r| r.throughput).sum::<f64>() / successful as f64
    } else {
        0.0
    };

    let avg_accuracy = if successful > 0 {
        results.iter().filter(|r| r.success).map(|r| r.grouping_accuracy).sum::<f64>() / successful as f64
    } else {
        0.0
    };

    let overall_match_rate = if total_logs > 0 {
        (total_matched as f64 / total_logs as f64) * 100.0
    } else {
        0.0
    };

    println!("Overall Statistics:");
    println!("  Total datasets:        {}", results.len());
    println!("  Successful:            {} ✅", successful);
    println!("  Failed:                {} ❌", failed);
    println!("  Total logs processed:  {}", total_logs);
    println!("  Total matched:         {} ({:.1}%)", total_matched, overall_match_rate);
    println!("  Total time:            {:.2}s", total_time);
    println!("  Average throughput:    {:.0} logs/sec", avg_throughput);
    println!("  Average accuracy:      {:.2}%\n", avg_accuracy);

    println!("Dataset Results (sorted by throughput):");
    println!("{:-<95}", "");
    println!("{:<12} {:>10} {:>10} {:>12} {:>12} {:>12} {:>10}",
        "Dataset", "Templates", "Logs", "Matched", "Throughput", "Accuracy", "Status");
    println!("{:-<95}", "");

    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| b.throughput.partial_cmp(&a.throughput).unwrap_or(std::cmp::Ordering::Equal));

    for r in &sorted {
        if r.success {
            println!("{:<12} {:>10} {:>10} {:>11.1}% {:>11.0}/s {:>11.2}% {:>10}",
                r.dataset_name,
                r.templates_loaded,
                r.total_logs,
                r.match_rate,
                r.throughput,
                r.grouping_accuracy,
                "✅"
            );
        } else {
            println!("{:<12} {:>10} {:>10} {:>12} {:>12} {:>12} {:>10}",
                r.dataset_name,
                "-",
                "-",
                "ERROR",
                "-",
                "-",
                "❌"
            );
        }
    }
    println!("{:-<95}", "");

    println!("\n🏆 Top 5 by Throughput:");
    for (i, r) in sorted.iter().filter(|r| r.success).take(5).enumerate() {
        println!("  {}. {} - {:.0} logs/sec ({:.3} ms/log)",
            i + 1, r.dataset_name, r.throughput, r.avg_latency_ms);
    }

    sorted.sort_by(|a, b| b.grouping_accuracy.partial_cmp(&a.grouping_accuracy).unwrap_or(std::cmp::Ordering::Equal));
    println!("\n🎯 Top 5 by Accuracy:");
    for (i, r) in sorted.iter().filter(|r| r.success).take(5).enumerate() {
        println!("  {}. {} - {:.2}%", i + 1, r.dataset_name, r.grouping_accuracy);
    }
}

fn save_results(results: &[DatasetBenchmarkResult], total_time: f64) -> anyhow::Result<()> {
    fs::create_dir_all("benchmark_results")?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let json_file = format!("benchmark_results/cached_benchmark_{}.json", timestamp);
    let csv_file = format!("benchmark_results/cached_benchmark_{}.csv", timestamp);

    // Save JSON
    #[derive(Serialize)]
    struct Summary {
        total_time_secs: f64,
        results: Vec<DatasetBenchmarkResult>,
    }

    let summary = Summary {
        total_time_secs: total_time,
        results: results.to_vec(),
    };

    fs::write(&json_file, serde_json::to_string_pretty(&summary)?)?;
    println!("\n💾 Results saved to: {}", json_file);

    // Save CSV
    let mut csv = String::from("Dataset,CacheFile,Templates,Logs,Matched,MatchRate,Throughput,Latency,Accuracy,Success\n");
    for r in results {
        csv.push_str(&format!(
            "{},{},{},{},{},{:.2},{:.0},{:.3},{:.2},{}\n",
            r.dataset_name, r.cache_file, r.templates_loaded, r.total_logs,
            r.matched_logs, r.match_rate, r.throughput, r.avg_latency_ms,
            r.grouping_accuracy, r.success
        ));
    }
    fs::write(&csv_file, csv)?;
    println!("💾 CSV saved to: {}", csv_file);

    Ok(())
}
