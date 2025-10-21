/// Unified benchmark suite for log analysis
///
/// This benchmark provides comprehensive testing of:
/// - Template generation (using LLM services)
/// - Log matching performance
/// - Accuracy against ground truth datasets
///
/// Run with: cargo test --test benchmark -- --nocapture
///
/// Available benchmarks:
/// - benchmark_openstack: Full OpenStack dataset benchmark
/// - benchmark_linux: Linux dataset benchmark
/// - benchmark_hdfs: HDFS dataset benchmark
/// - benchmark_custom: Custom configuration benchmark

use log_analyzer::benchmark_runner::{run_benchmark, run_throughput_benchmark};
use log_analyzer::implementations::{LLMTemplateGenerator, RegexLogMatcher};
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::traits::{BenchmarkConfig, DatasetLoader};

/// Benchmark OpenStack dataset with LLM template generation
#[tokio::test]
#[ignore] // Use `cargo test benchmark_openstack -- --ignored --nocapture` to run
async fn benchmark_openstack() -> anyhow::Result<()> {
    let generator = LLMTemplateGenerator::mock();
    let mut matcher = RegexLogMatcher::new();
    let dataset = LogHubDatasetLoader::new("OpenStack", "data/loghub");

    let config = BenchmarkConfig {
        max_logs: Some(1000), // Process first 1000 logs
        verbose: true,
        min_accuracy: 70.0,
        ..Default::default()
    };

    let results = run_benchmark(&generator, &mut matcher, &dataset, &config).await?;

    assert!(
        results.grouping_accuracy >= config.min_accuracy,
        "Accuracy {:.2}% below minimum {:.2}%",
        results.grouping_accuracy,
        config.min_accuracy
    );

    Ok(())
}

/// Benchmark Linux dataset
#[tokio::test]
#[ignore]
async fn benchmark_linux() -> anyhow::Result<()> {
    let generator = LLMTemplateGenerator::mock();
    let mut matcher = RegexLogMatcher::new();
    let dataset = LogHubDatasetLoader::new("Linux", "data/loghub");

    let config = BenchmarkConfig {
        max_logs: Some(500),
        verbose: true,
        min_accuracy: 65.0,
        ..Default::default()
    };

    let results = run_benchmark(&generator, &mut matcher, &dataset, &config).await?;

    assert!(
        results.grouping_accuracy >= config.min_accuracy,
        "Accuracy {:.2}% below minimum {:.2}%",
        results.grouping_accuracy,
        config.min_accuracy
    );

    Ok(())
}

/// Benchmark HDFS dataset
#[tokio::test]
#[ignore]
async fn benchmark_hdfs() -> anyhow::Result<()> {
    let generator = LLMTemplateGenerator::mock();
    let mut matcher = RegexLogMatcher::new();
    let dataset = LogHubDatasetLoader::new("HDFS", "data/loghub");

    let config = BenchmarkConfig {
        max_logs: Some(1000),
        verbose: true,
        min_accuracy: 70.0,
        ..Default::default()
    };

    let results = run_benchmark(&generator, &mut matcher, &dataset, &config).await?;

    assert!(
        results.grouping_accuracy >= config.min_accuracy,
        "Accuracy {:.2}% below minimum {:.2}%",
        results.grouping_accuracy,
        config.min_accuracy
    );

    Ok(())
}

/// Benchmark Apache dataset
#[tokio::test]
#[ignore]
async fn benchmark_apache() -> anyhow::Result<()> {
    let generator = LLMTemplateGenerator::mock();
    let mut matcher = RegexLogMatcher::new();
    let dataset = LogHubDatasetLoader::new("Apache", "data/loghub");

    let config = BenchmarkConfig {
        max_logs: Some(500),
        verbose: true,
        min_accuracy: 60.0,
        ..Default::default()
    };

    let results = run_benchmark(&generator, &mut matcher, &dataset, &config).await?;

    assert!(
        results.grouping_accuracy >= config.min_accuracy,
        "Accuracy {:.2}% below minimum {:.2}%",
        results.grouping_accuracy,
        config.min_accuracy
    );

    Ok(())
}

/// Quick throughput benchmark (no ground truth comparison)
#[tokio::test]
async fn benchmark_throughput() -> anyhow::Result<()> {
    let generator = LLMTemplateGenerator::mock();
    let mut matcher = RegexLogMatcher::new();
    let dataset = LogHubDatasetLoader::new("Linux", "data/loghub");

    let mut logs = dataset.load_raw_logs()?;
    logs.truncate(100);

    let config = BenchmarkConfig {
        max_logs: Some(100),
        verbose: true,
        ..Default::default()
    };

    let results = run_throughput_benchmark(&generator, &mut matcher, &logs, &config).await?;

    // Just verify it completes
    assert!(results.throughput > 0.0, "Throughput should be positive");

    Ok(())
}

/// Full OpenStack benchmark (all 2000 logs)
#[tokio::test]
#[ignore]
async fn benchmark_openstack_full() -> anyhow::Result<()> {
    let generator = LLMTemplateGenerator::mock();
    let mut matcher = RegexLogMatcher::new();
    let dataset = LogHubDatasetLoader::new("OpenStack", "data/loghub");

    let config = BenchmarkConfig {
        max_logs: None, // Process all logs
        verbose: true,
        min_accuracy: 70.0,
        ..Default::default()
    };

    let results = run_benchmark(&generator, &mut matcher, &dataset, &config).await?;

    assert!(
        results.grouping_accuracy >= config.min_accuracy,
        "Accuracy {:.2}% below minimum {:.2}%",
        results.grouping_accuracy,
        config.min_accuracy
    );

    Ok(())
}

/// Multiple datasets comparison
#[tokio::test]
#[ignore]
async fn benchmark_comparison() -> anyhow::Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("📊 Multi-Dataset Comparison Benchmark");
    println!("{}\n", "=".repeat(80));

    let datasets = vec![
        ("Linux", 500),
        ("OpenStack", 500),
        ("HDFS", 500),
        ("Apache", 500),
    ];

    for (dataset_name, max_logs) in datasets {
        println!("\n🔍 Testing {} dataset...\n", dataset_name);

        let generator = LLMTemplateGenerator::mock();
        let mut matcher = RegexLogMatcher::new();
        let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");

        let config = BenchmarkConfig {
            max_logs: Some(max_logs),
            verbose: false, // Less verbose for comparison
            ..Default::default()
        };

        match run_benchmark(&generator, &mut matcher, &dataset, &config).await {
            Ok(results) => {
                println!("  ✅ {}: {:.2}% accuracy, {:.0} logs/sec",
                    dataset_name,
                    results.grouping_accuracy,
                    results.throughput
                );
            }
            Err(e) => {
                println!("  ❌ {}: Error - {}", dataset_name, e);
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    Ok(())
}

#[cfg(test)]
mod custom_benchmarks {
    use super::*;

    /// Custom benchmark with specific configuration
    #[tokio::test]
    #[ignore]
    async fn benchmark_custom_config() -> anyhow::Result<()> {
        let generator = LLMTemplateGenerator::mock();
        let mut matcher = RegexLogMatcher::new();
        let dataset = LogHubDatasetLoader::new("OpenStack", "data/loghub");

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("test_type".to_string(), "custom".to_string());
        metadata.insert("environment".to_string(), "testing".to_string());

        let config = BenchmarkConfig {
            max_logs: Some(250),
            use_batch: true,
            verbose: true,
            min_accuracy: 60.0,
            metadata,
        };

        let results = run_benchmark(&generator, &mut matcher, &dataset, &config).await?;

        println!("\n📋 Custom Configuration Results:");
        println!("   Accuracy: {:.2}%", results.grouping_accuracy);
        println!("   Throughput: {:.0} logs/sec", results.throughput);

        Ok(())
    }
}
