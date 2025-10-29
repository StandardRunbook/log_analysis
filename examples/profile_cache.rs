/// Cache Profiling for Aho-Corasick Log Matcher
///
/// This benchmark measures cache performance characteristics:
/// - Cache line utilization
/// - Memory access patterns
/// - Potential cache thrashing
/// - Memory bandwidth utilization
///
/// Run with:
/// ```bash
/// cargo build --release --example profile_cache
/// time ./target/release/examples/profile_cache
/// ```
///
/// For detailed cache analysis on macOS, use Instruments:
/// ```bash
/// xcrun xctrace record --template 'Time Profiler' --launch ./target/release/examples/profile_cache
/// ```

use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::matcher_config::MatcherConfig;
use log_analyzer::traits::DatasetLoader;
use std::time::Instant;

#[derive(Debug)]
struct CacheMetrics {
    total_iterations: usize,
    total_logs_processed: usize,
    total_time_secs: f64,
    throughput: f64,
    avg_latency_ns: f64,
    // Estimated cache metrics
    working_set_size_bytes: usize,
    estimated_cache_misses: usize,
    memory_bandwidth_mbps: f64,
}

/// Load templates from cache
fn load_matcher(dataset_name: &str) -> anyhow::Result<LogMatcher> {
    let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());

    if !std::path::Path::new(&cache_file).exists() {
        anyhow::bail!("No cached templates: {}", cache_file);
    }

    let json_content = std::fs::read_to_string(&cache_file)?;

    #[derive(serde::Deserialize)]
    struct CachedTemplates {
        templates: Vec<CachedTemplate>,
    }

    #[derive(serde::Deserialize)]
    struct CachedTemplate {
        template_id: u64,
        pattern: String,
        variables: Vec<String>,
        example: String,
    }

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

/// Estimate working set size (templates + patterns + DFA)
fn estimate_working_set_size(matcher: &LogMatcher) -> usize {
    let templates = matcher.get_all_templates();
    let mut size = 0;

    // Template data
    for t in &templates {
        size += t.pattern.len();
        size += t.example.len();
        size += t.variables.iter().map(|v| v.len()).sum::<usize>();
        size += std::mem::size_of::<LogTemplate>();
    }

    // Rough estimate for Aho-Corasick DFA (can be large)
    // DFA size is typically O(alphabet_size * num_states)
    // For text patterns, this can be 256 * num_fragments * branching_factor
    let estimated_dfa_size = templates.len() * 1024; // Conservative estimate
    size += estimated_dfa_size;

    size
}

/// Benchmark with different access patterns to detect cache thrashing
fn benchmark_access_patterns(
    matcher: &LogMatcher,
    logs: &[String],
    pattern_name: &str,
) -> CacheMetrics {
    let iterations = 10;
    let start = Instant::now();
    let mut total_matches = 0usize;

    for _ in 0..iterations {
        for log in logs {
            if matcher.match_log(log).is_some() {
                total_matches += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    let total_logs = logs.len() * iterations;
    let throughput = total_logs as f64 / elapsed.as_secs_f64();
    let avg_latency_ns = elapsed.as_nanos() as f64 / total_logs as f64;

    // Estimate cache misses based on timing
    // On modern CPUs, L1 cache hit ~4 cycles (~1ns at 4GHz)
    // L2 cache hit ~12 cycles (~3ns)
    // L3 cache hit ~40 cycles (~10ns)
    // RAM access ~200 cycles (~50ns+)
    let estimated_cache_misses = if avg_latency_ns > 50.0 {
        // If average latency is high, likely RAM access
        (total_logs as f64 * 0.3) as usize
    } else {
        (total_logs as f64 * 0.1) as usize
    };

    // Estimate memory bandwidth (very rough)
    let avg_log_size = logs.iter().map(|l| l.len()).sum::<usize>() / logs.len();
    let bytes_processed = total_logs * avg_log_size;
    let memory_bandwidth_mbps = (bytes_processed as f64 / elapsed.as_secs_f64()) / 1_000_000.0;

    println!("\n📊 Pattern: {}", pattern_name);
    println!("  Iterations:         {}", iterations);
    println!("  Total logs:         {}", total_logs);
    println!("  Total matches:      {}", total_matches);
    println!("  Elapsed:            {:.2}s", elapsed.as_secs_f64());
    println!("  Throughput:         {:.0} logs/sec", throughput);
    println!("  Avg latency:        {:.1} ns/log", avg_latency_ns);
    println!("  Est. cache misses:  {}", estimated_cache_misses);
    println!("  Est. bandwidth:     {:.2} MB/s", memory_bandwidth_mbps);

    CacheMetrics {
        total_iterations: iterations,
        total_logs_processed: total_logs,
        total_time_secs: elapsed.as_secs_f64(),
        throughput,
        avg_latency_ns,
        working_set_size_bytes: estimate_working_set_size(matcher),
        estimated_cache_misses,
        memory_bandwidth_mbps,
    }
}

/// Test with different log set sizes to see cache scaling
fn test_cache_scaling(matcher: &LogMatcher, logs: &[String]) {
    println!("\n{:=<80}", "");
    println!("🔬 CACHE SCALING TEST");
    println!("{:=<80}\n", "");

    let sizes = [10, 50, 100, 500, 1000, 5000, 10000];

    println!("{:>8} {:>15} {:>15} {:>15}", "Logs", "Throughput", "Latency(ns)", "MB/s");
    println!("{:-<60}", "");

    for &size in &sizes {
        if size > logs.len() {
            break;
        }

        let test_logs = &logs[..size];
        let start = Instant::now();

        // Run multiple times to get stable measurements
        let iterations = 100;
        for _ in 0..iterations {
            for log in test_logs {
                matcher.match_log(log);
            }
        }

        let elapsed = start.elapsed();
        let total_logs = size * iterations;
        let throughput = total_logs as f64 / elapsed.as_secs_f64();
        let avg_latency_ns = elapsed.as_nanos() as f64 / total_logs as f64;

        let avg_log_size = test_logs.iter().map(|l| l.len()).sum::<usize>() / test_logs.len();
        let bytes_processed = total_logs * avg_log_size;
        let memory_bandwidth = (bytes_processed as f64 / elapsed.as_secs_f64()) / 1_000_000.0;

        println!("{:>8} {:>12.0}/s {:>14.1}ns {:>14.2}",
                 size, throughput, avg_latency_ns, memory_bandwidth);
    }
}

/// Test random vs sequential access patterns
fn test_access_patterns(matcher: &LogMatcher, logs: &[String]) {
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    println!("\n{:=<80}", "");
    println!("🔀 ACCESS PATTERN COMPARISON");
    println!("{:=<80}\n", "");

    let test_size = 500.min(logs.len());
    let test_logs = &logs[..test_size];

    // Sequential access
    let sequential_metrics = benchmark_access_patterns(matcher, test_logs, "Sequential");

    // Random access (can cause cache thrashing)
    let mut random_logs: Vec<String> = test_logs.to_vec();
    random_logs.shuffle(&mut thread_rng());
    let random_metrics = benchmark_access_patterns(matcher, &random_logs, "Random");

    // Strided access (worst case for cache)
    let stride = 7; // Prime number to avoid patterns
    let mut strided_logs = Vec::new();
    for i in (0..test_logs.len()).step_by(stride) {
        strided_logs.push(test_logs[i].clone());
    }
    let strided_metrics = benchmark_access_patterns(matcher, &strided_logs, "Strided");

    println!("\n{:=<80}", "");
    println!("📈 CACHE THRASHING ANALYSIS");
    println!("{:=<80}\n", "");

    let seq_to_rand_ratio = sequential_metrics.throughput / random_metrics.throughput;
    let seq_to_stride_ratio = sequential_metrics.throughput / strided_metrics.throughput;

    println!("Throughput Ratios:");
    println!("  Sequential/Random:  {:.2}x", seq_to_rand_ratio);
    println!("  Sequential/Strided: {:.2}x", seq_to_stride_ratio);

    if seq_to_rand_ratio > 1.5 {
        println!("\n⚠️  CACHE THRASHING DETECTED!");
        println!("   Random access is {:.1}x slower than sequential.", seq_to_rand_ratio);
        println!("   This suggests the working set doesn't fit in cache.");
    } else {
        println!("\n✅ Good cache locality - minimal thrashing");
    }

    println!("\nWorking Set Analysis:");
    println!("  Estimated size: {} KB", sequential_metrics.working_set_size_bytes / 1024);
    println!("  Typical L1:     32-64 KB per core");
    println!("  Typical L2:     256-512 KB per core");
    println!("  Typical L3:     8-32 MB shared");
}

fn main() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("🔬 CACHE PROFILING - Aho-Corasick Log Matcher");
    println!("{:=<80}\n", "");

    // Test with different datasets
    let datasets = vec!["Linux", "Apache", "Hdfs", "OpenStack"];

    for dataset_name in datasets {
        println!("\n{:=<80}", "");
        println!("📂 Dataset: {}", dataset_name);
        println!("{:=<80}", "");

        let matcher = match load_matcher(dataset_name) {
            Ok(m) => m,
            Err(e) => {
                println!("❌ Skipping {} - {}", dataset_name, e);
                continue;
            }
        };

        let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
        let logs = match dataset.load_raw_logs() {
            Ok(l) => l,
            Err(e) => {
                println!("❌ Failed to load logs: {}", e);
                continue;
            }
        };

        println!("\nConfiguration:");
        println!("  Templates:      {}", matcher.get_all_templates().len());
        println!("  Total logs:     {}", logs.len());
        println!("  Working set:    ~{} KB", estimate_working_set_size(&matcher) / 1024);

        // Run tests
        test_cache_scaling(&matcher, &logs);
        test_access_patterns(&matcher, &logs);
    }

    println!("\n{:=<80}", "");
    println!("✅ PROFILING COMPLETE");
    println!("{:=<80}", "");

    println!("For detailed cache analysis, use macOS Instruments:");
    println!("  xcrun xctrace record --template 'Time Profiler' \\");
    println!("    --launch ./target/release/examples/profile_cache");
    println!("\nOr use the Allocations instrument to see memory patterns:");
    println!("  xcrun xctrace record --template 'Allocations' \\");
    println!("    --launch ./target/release/examples/profile_cache");

    Ok(())
}
