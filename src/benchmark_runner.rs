/// Benchmark runner with dependency injection support
///
/// This module provides a reusable benchmark framework that accepts
/// pluggable implementations of template generators, log matchers, and datasets.
use crate::traits::{
    BenchmarkConfig, BenchmarkResults, DatasetLoader, LogMatcherTrait, TemplateGenerator,
};
use anyhow::Result;
use std::collections::HashMap;
use std::time::Instant;

/// Run a complete benchmark with injected dependencies
///
/// # Arguments
/// * `generator` - Template generator implementation
/// * `matcher` - Log matcher implementation
/// * `dataset` - Dataset loader implementation
/// * `config` - Benchmark configuration
///
/// # Returns
/// Detailed benchmark results including accuracy and performance metrics
pub async fn run_benchmark<G, M, D>(
    generator: &G,
    matcher: &mut M,
    dataset: &D,
    config: &BenchmarkConfig,
) -> Result<BenchmarkResults>
where
    G: TemplateGenerator,
    M: LogMatcherTrait,
    D: DatasetLoader,
{
    if config.verbose {
        println!("\n{}", "=".repeat(80));
        println!("📊 Log Parsing Benchmark");
        println!("   Generator: {}", generator.name());
        println!("   Matcher:   {}", matcher.name());
        println!("   Dataset:   {}", dataset.name());
        println!("{}\n", "=".repeat(80));
    }

    // Load dataset
    if config.verbose {
        println!("📝 Loading dataset...");
    }
    let ground_truth = dataset.load_ground_truth()?;
    let raw_logs = dataset.load_raw_logs()?;

    if config.verbose {
        println!("   ✓ Loaded {} ground truth entries", ground_truth.len());
        println!("   ✓ Loaded {} raw log lines\n", raw_logs.len());
    }

    // Determine test size
    let test_size = config
        .max_logs
        .unwrap_or(raw_logs.len())
        .min(raw_logs.len());
    let test_logs = &raw_logs[..test_size];
    let test_gt = &ground_truth[..test_size.min(ground_truth.len())];

    if config.verbose {
        println!(
            "⚡ Parsing {} logs with template generation...",
            test_logs.len()
        );
        if config.max_logs.is_some() {
            println!("   (Limited to {} logs for quick testing)", test_size);
        }
        println!();
    }

    // Run the benchmark
    let start = Instant::now();
    let mut template_assignments: Vec<Option<u64>> = Vec::new();
    let mut templates_generated = 0;

    for (idx, log_line) in test_logs.iter().enumerate() {
        if config.verbose && idx % 10 == 0 && idx > 0 {
            println!("   Processed {}/{} logs...", idx, test_logs.len());
        }

        // Try to match with existing templates
        let match_result = matcher.match_log(log_line);

        let template_id = if let Some(tid) = match_result {
            // Matched existing template
            Some(tid)
        } else {
            // Generate new template
            match generator.generate_template(log_line).await {
                Ok(new_template) => {
                    let tid = new_template.template_id;
                    matcher.add_template(new_template);
                    templates_generated += 1;
                    Some(tid)
                }
                Err(e) => {
                    if config.verbose {
                        eprintln!("   ⚠️  Failed to generate template for log {}: {}", idx, e);
                    }
                    None
                }
            }
        };

        template_assignments.push(template_id);
    }

    let elapsed = start.elapsed();

    if config.verbose {
        println!("   ✓ Processed all {} logs\n", test_logs.len());
    }

    // Calculate performance metrics
    let throughput = test_logs.len() as f64 / elapsed.as_secs_f64();
    let avg_latency_ms = (elapsed.as_millis() as f64) / (test_logs.len() as f64);

    // Calculate grouping accuracy
    if config.verbose {
        println!("🎯 Calculating grouping accuracy...\n");
    }

    let (correct, incorrect, unmatched) = calculate_accuracy(&template_assignments, test_gt);

    let total = correct + incorrect + unmatched;
    let grouping_accuracy = if total > 0 {
        (correct as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    // Count unique groups
    let expected_groups = test_gt
        .iter()
        .map(|entry| &entry.event_id)
        .collect::<std::collections::HashSet<_>>()
        .len();

    let results = BenchmarkResults {
        total_logs: test_logs.len(),
        templates_generated,
        elapsed_secs: elapsed.as_secs_f64(),
        throughput,
        avg_latency_ms,
        grouping_accuracy,
        correct,
        incorrect,
        unmatched,
        expected_groups,
        actual_groups: templates_generated,
        metadata: config.metadata.clone(),
    };

    if config.verbose {
        results.print("Benchmark Complete");
    }

    Ok(results)
}

/// Calculate accuracy by comparing template assignments to ground truth
fn calculate_accuracy(
    template_assignments: &[Option<u64>],
    ground_truth: &[crate::traits::GroundTruthEntry],
) -> (usize, usize, usize) {
    // Build mapping: ground truth event_id -> assigned template_ids
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

    // For each ground truth group, find the majority template_id
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

    // Calculate correct/incorrect/unmatched
    let mut correct = 0;
    let mut incorrect = 0;
    let mut unmatched = 0;

    for (idx, template_id) in template_assignments.iter().enumerate() {
        if let Some(gt_entry) = ground_truth.get(idx) {
            if let Some(&majority_tid) = gt_to_majority_template.get(&gt_entry.event_id) {
                match template_id {
                    Some(tid) => {
                        if *tid == majority_tid {
                            correct += 1;
                        } else {
                            incorrect += 1;
                        }
                    }
                    None => unmatched += 1,
                }
            } else {
                // No majority template for this ground truth group
                if template_id.is_some() {
                    incorrect += 1;
                } else {
                    unmatched += 1;
                }
            }
        }
    }

    (correct, incorrect, unmatched)
}

/// Run a simple throughput benchmark (no ground truth comparison)
///
/// This is useful for pure performance testing without accuracy evaluation
pub async fn run_throughput_benchmark<G, M>(
    generator: &G,
    matcher: &mut M,
    logs: &[String],
    config: &BenchmarkConfig,
) -> Result<BenchmarkResults>
where
    G: TemplateGenerator,
    M: LogMatcherTrait,
{
    if config.verbose {
        println!("\n{}", "=".repeat(80));
        println!("⚡ Throughput Benchmark");
        println!("   Generator: {}", generator.name());
        println!("   Matcher:   {}", matcher.name());
        if config.use_batch {
            println!("   Mode:      Batch processing");
        } else {
            println!("   Mode:      Sequential processing");
        }
        println!("{}\n", "=".repeat(80));
    }

    let test_size = config.max_logs.unwrap_or(logs.len()).min(logs.len());
    let test_logs = &logs[..test_size];

    if config.verbose {
        println!("⚡ Processing {} logs...\n", test_logs.len());
    }

    let start = Instant::now();
    let mut templates_generated = 0;

    if config.use_batch {
        // Batch processing mode - process in chunks
        let batch_size = 1000; // Process 1000 logs at a time
        for chunk_start in (0..test_logs.len()).step_by(batch_size) {
            let chunk_end = (chunk_start + batch_size).min(test_logs.len());
            let chunk = &test_logs[chunk_start..chunk_end];

            if config.verbose && chunk_start % 5000 == 0 && chunk_start > 0 {
                println!("   Processed {}/{} logs...", chunk_start, test_logs.len());
            }

            let log_refs: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
            let match_results = matcher.match_batch(&log_refs);

            for (idx, match_result) in match_results.iter().enumerate() {
                if match_result.is_none() {
                    if let Ok(new_template) = generator.generate_template(chunk[idx].as_str()).await {
                        matcher.add_template(new_template);
                        templates_generated += 1;
                    }
                }
            }
        }
    } else {
        // Sequential processing mode
        for (idx, log_line) in test_logs.iter().enumerate() {
            if config.verbose && idx % 100 == 0 && idx > 0 {
                println!("   Processed {}/{} logs...", idx, test_logs.len());
            }

            let match_result = matcher.match_log(log_line);

            if match_result.is_none() {
                if let Ok(new_template) = generator.generate_template(log_line).await {
                    matcher.add_template(new_template);
                    templates_generated += 1;
                }
            }
        }
    }

    let elapsed = start.elapsed();

    if config.verbose {
        println!("   ✓ Processed all {} logs\n", test_logs.len());
    }

    let throughput = test_logs.len() as f64 / elapsed.as_secs_f64();
    let avg_latency_ms = (elapsed.as_millis() as f64) / (test_logs.len() as f64);

    let results = BenchmarkResults {
        total_logs: test_logs.len(),
        templates_generated,
        elapsed_secs: elapsed.as_secs_f64(),
        throughput,
        avg_latency_ms,
        grouping_accuracy: 0.0, // N/A for throughput-only benchmark
        correct: 0,
        incorrect: 0,
        unmatched: 0,
        expected_groups: 0,
        actual_groups: templates_generated,
        metadata: config.metadata.clone(),
    };

    if config.verbose {
        println!("📈 Performance Results:");
        println!("   Total logs:          {:>10}", results.total_logs);
        println!(
            "   Templates:           {:>10}",
            results.templates_generated
        );
        println!("   Parse time:          {:>10.2}s", results.elapsed_secs);
        println!(
            "   Throughput:          {:>10.0} logs/sec",
            results.throughput
        );
        println!(
            "   Avg latency:         {:>10.2}ms per log\n",
            results.avg_latency_ms
        );
        println!("{}", "=".repeat(80));
    }

    Ok(results)
}
