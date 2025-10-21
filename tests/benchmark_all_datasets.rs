/// Comprehensive benchmark for all LogHub datasets
///
/// Tests throughput and grouping accuracy for each dataset type
///
/// Run with: cargo test --test benchmark_all_datasets -- --nocapture --test-threads=1

use log_analyzer::benchmark_runner::run_benchmark;
use log_analyzer::implementations::{LLMTemplateGenerator, RegexLogMatcher};
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::traits::BenchmarkConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatasetResult {
    dataset_name: String,
    total_logs: usize,
    templates_generated: usize,
    elapsed_secs: f64,
    throughput: f64,
    avg_latency_ms: f64,
    grouping_accuracy: f64,
    expected_groups: usize,
    actual_groups: usize,
    success: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BenchmarkSummary {
    total_datasets: usize,
    successful_datasets: usize,
    failed_datasets: usize,
    total_logs_processed: usize,
    total_time_secs: f64,
    average_throughput: f64,
    average_accuracy: f64,
    results: Vec<DatasetResult>,
}

/// Get all available LogHub datasets
fn get_available_datasets() -> Vec<String> {
    let mut datasets = Vec::new();

    if let Ok(entries) = fs::read_dir("data/loghub") {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Skip hidden directories
                    if !name.starts_with('.') {
                        datasets.push(name.to_string());
                    }
                }
            }
        }
    }

    datasets.sort();
    datasets
}

/// Benchmark a single dataset
async fn benchmark_dataset(dataset_name: &str, max_logs: Option<usize>) -> DatasetResult {
    println!("\n{:=<80}", "");
    println!("🔍 Benchmarking: {}", dataset_name);
    println!("{:=<80}\n", "");

    let generator = LLMTemplateGenerator::mock();
    let mut matcher = RegexLogMatcher::new();
    let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");

    let config = BenchmarkConfig {
        max_logs,
        verbose: false, // Less verbose for batch processing
        min_accuracy: 0.0, // Don't assert, just measure
        ..Default::default()
    };

    match run_benchmark(&generator, &mut matcher, &dataset, &config).await {
        Ok(results) => {
            println!("✅ {} - {:.2}% accuracy, {:.0} logs/sec",
                dataset_name,
                results.grouping_accuracy,
                results.throughput
            );

            DatasetResult {
                dataset_name: dataset_name.to_string(),
                total_logs: results.total_logs,
                templates_generated: results.templates_generated,
                elapsed_secs: results.elapsed_secs,
                throughput: results.throughput,
                avg_latency_ms: results.avg_latency_ms,
                grouping_accuracy: results.grouping_accuracy,
                expected_groups: results.expected_groups,
                actual_groups: results.actual_groups,
                success: true,
                error: None,
            }
        }
        Err(e) => {
            println!("❌ {} - Error: {}", dataset_name, e);

            DatasetResult {
                dataset_name: dataset_name.to_string(),
                total_logs: 0,
                templates_generated: 0,
                elapsed_secs: 0.0,
                throughput: 0.0,
                avg_latency_ms: 0.0,
                grouping_accuracy: 0.0,
                expected_groups: 0,
                actual_groups: 0,
                success: false,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Benchmark all datasets with a sample size
#[tokio::test]
#[ignore]
async fn benchmark_all_datasets_sample() -> anyhow::Result<()> {
    benchmark_all_datasets_internal(Some(500)).await
}

/// Benchmark all datasets with full data
#[tokio::test]
#[ignore]
async fn benchmark_all_datasets_full() -> anyhow::Result<()> {
    benchmark_all_datasets_internal(None).await
}

/// Quick benchmark (100 logs per dataset)
#[tokio::test]
async fn benchmark_all_datasets_quick() -> anyhow::Result<()> {
    benchmark_all_datasets_internal(Some(100)).await
}

async fn benchmark_all_datasets_internal(max_logs: Option<usize>) -> anyhow::Result<()> {
    let start_time = Instant::now();

    println!("\n{:=<80}", "");
    println!("📊 LOGHUB COMPREHENSIVE BENCHMARK");
    println!("{:=<80}", "");

    let log_limit = max_logs.map(|l| format!("{} logs per dataset", l))
        .unwrap_or_else(|| "all logs".to_string());
    println!("Testing: {}\n", log_limit);

    let datasets = get_available_datasets();

    if datasets.is_empty() {
        println!("⚠️  No datasets found in data/loghub/");
        println!("   Please download LogHub datasets to data/loghub/");
        return Ok(());
    }

    println!("Found {} datasets: {:?}\n", datasets.len(), datasets);

    let mut results = Vec::new();
    let mut total_logs = 0;
    let mut successful = 0;
    let mut failed = 0;

    // Benchmark each dataset
    for dataset in &datasets {
        let result = benchmark_dataset(dataset, max_logs).await;

        if result.success {
            successful += 1;
            total_logs += result.total_logs;
        } else {
            failed += 1;
        }

        results.push(result);
    }

    let total_time = start_time.elapsed().as_secs_f64();

    // Calculate averages (only successful datasets)
    let avg_throughput = if successful > 0 {
        results.iter()
            .filter(|r| r.success)
            .map(|r| r.throughput)
            .sum::<f64>() / successful as f64
    } else {
        0.0
    };

    let avg_accuracy = if successful > 0 {
        results.iter()
            .filter(|r| r.success)
            .map(|r| r.grouping_accuracy)
            .sum::<f64>() / successful as f64
    } else {
        0.0
    };

    let summary = BenchmarkSummary {
        total_datasets: datasets.len(),
        successful_datasets: successful,
        failed_datasets: failed,
        total_logs_processed: total_logs,
        total_time_secs: total_time,
        average_throughput: avg_throughput,
        average_accuracy: avg_accuracy,
        results: results.clone(),
    };

    // Print summary
    print_summary(&summary);

    // Save results to file
    save_results(&summary)?;

    Ok(())
}

fn print_summary(summary: &BenchmarkSummary) {
    println!("\n{:=<80}", "");
    println!("📊 BENCHMARK SUMMARY");
    println!("{:=<80}\n", "");

    println!("Overall Statistics:");
    println!("  Total datasets:        {}", summary.total_datasets);
    println!("  Successful:            {} ✅", summary.successful_datasets);
    println!("  Failed:                {} ❌", summary.failed_datasets);
    println!("  Total logs processed:  {}", summary.total_logs_processed);
    println!("  Total time:            {:.2}s", summary.total_time_secs);
    println!("  Average throughput:    {:.0} logs/sec", summary.average_throughput);
    println!("  Average accuracy:      {:.2}%\n", summary.average_accuracy);

    println!("Dataset Results (sorted by accuracy):");
    println!("{:-<80}", "");
    println!("{:<15} {:>8} {:>10} {:>12} {:>10} {:>12}",
        "Dataset", "Logs", "Templates", "Accuracy", "Throughput", "Status");
    println!("{:-<80}", "");

    let mut sorted_results = summary.results.clone();
    sorted_results.sort_by(|a, b| {
        b.grouping_accuracy.partial_cmp(&a.grouping_accuracy)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for result in &sorted_results {
        if result.success {
            println!("{:<15} {:>8} {:>10} {:>11.2}% {:>9.0}/s {:>12}",
                result.dataset_name,
                result.total_logs,
                result.templates_generated,
                result.grouping_accuracy,
                result.throughput,
                "✅"
            );
        } else {
            println!("{:<15} {:>8} {:>10} {:>12} {:>10} {:>12}",
                result.dataset_name,
                "-",
                "-",
                "ERROR",
                "-",
                "❌"
            );
        }
    }
    println!("{:-<80}", "");

    // Performance rankings
    println!("\n🏆 Top 5 by Accuracy:");
    for (i, result) in sorted_results.iter().filter(|r| r.success).take(5).enumerate() {
        println!("  {}. {} - {:.2}%",
            i + 1,
            result.dataset_name,
            result.grouping_accuracy
        );
    }

    sorted_results.sort_by(|a, b| {
        b.throughput.partial_cmp(&a.throughput)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!("\n⚡ Top 5 by Throughput:");
    for (i, result) in sorted_results.iter().filter(|r| r.success).take(5).enumerate() {
        println!("  {}. {} - {:.0} logs/sec",
            i + 1,
            result.dataset_name,
            result.throughput
        );
    }

    if summary.failed_datasets > 0 {
        println!("\n❌ Failed Datasets:");
        for result in &summary.results {
            if !result.success {
                println!("  - {}: {}",
                    result.dataset_name,
                    result.error.as_ref().unwrap_or(&"Unknown error".to_string())
                );
            }
        }
    }

    println!("\n{:=<80}", "");
}

fn save_results(summary: &BenchmarkSummary) -> anyhow::Result<()> {
    // Create results directory if it doesn't exist
    fs::create_dir_all("benchmark_results")?;

    // Generate filename with timestamp
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("benchmark_results/loghub_benchmark_{}.json", timestamp);

    // Save as JSON
    let json = serde_json::to_string_pretty(&summary)?;
    fs::write(&filename, json)?;

    println!("\n💾 Results saved to: {}", filename);

    // Also save a CSV for easy analysis
    let csv_filename = format!("benchmark_results/loghub_benchmark_{}.csv", timestamp);
    save_results_csv(summary, &csv_filename)?;
    println!("💾 CSV saved to: {}", csv_filename);

    Ok(())
}

fn save_results_csv(summary: &BenchmarkSummary, filename: &str) -> anyhow::Result<()> {
    let mut csv = String::new();

    // Header
    csv.push_str("Dataset,Logs,Templates,Accuracy,Throughput,Latency,ExpectedGroups,ActualGroups,Success\n");

    // Data rows
    for result in &summary.results {
        csv.push_str(&format!(
            "{},{},{},{:.2},{:.0},{:.2},{},{},{}\n",
            result.dataset_name,
            result.total_logs,
            result.templates_generated,
            result.grouping_accuracy,
            result.throughput,
            result.avg_latency_ms,
            result.expected_groups,
            result.actual_groups,
            result.success
        ));
    }

    fs::write(filename, csv)?;
    Ok(())
}

/// Benchmark specific datasets only
#[tokio::test]
#[ignore]
async fn benchmark_selected_datasets() -> anyhow::Result<()> {
    let selected = vec!["Linux", "OpenStack", "HDFS", "Apache"];

    println!("\n{:=<80}", "");
    println!("📊 SELECTED DATASETS BENCHMARK");
    println!("{:=<80}\n", "");
    println!("Testing: {:?}\n", selected);

    let mut results = Vec::new();

    for dataset in &selected {
        let result = benchmark_dataset(dataset, Some(1000)).await;
        results.push(result);
    }

    println!("\n{:=<80}", "");
    println!("Results:");
    for result in &results {
        if result.success {
            println!("  {} - {:.2}% accuracy, {:.0} logs/sec",
                result.dataset_name,
                result.grouping_accuracy,
                result.throughput
            );
        }
    }
    println!("{:=<80}", "");

    Ok(())
}
