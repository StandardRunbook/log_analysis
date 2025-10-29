/// Mega Cache Profiling - All Templates, All Logs
///
/// Loads EVERY template from cache and EVERY log from data directory
/// to test cache behavior with a massive DFA.
///
/// Run with:
/// ```bash
/// cargo build --release --example profile_cache_mega
/// ./target/release/examples/profile_cache_mega
/// ```

use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::matcher_config::MatcherConfig;
use log_analyzer::traits::DatasetLoader;
use std::time::Instant;
use std::fs;

#[derive(serde::Deserialize)]
struct CachedTemplates {
    #[serde(skip_serializing_if = "Option::is_none")]
    next_template_id: Option<u64>,
    templates: Vec<CachedTemplate>,
}

#[derive(serde::Deserialize)]
struct CachedTemplate {
    #[serde(default)]
    template_id: u64,
    pattern: String,
    variables: Vec<String>,
    example: String,
}

fn load_all_templates() -> anyhow::Result<LogMatcher> {
    println!("🔍 Scanning cache directory for templates...");

    let mut all_templates = Vec::new();
    let mut template_count_by_dataset = Vec::new();

    if let Ok(entries) = fs::read_dir("cache") {
        for entry in entries.flatten() {
            let filename = entry.file_name();
            if let Some(name) = filename.to_str() {
                if name.ends_with("_templates.json") && !name.starts_with('.')
                    && !name.contains("comprehensive") && !name.contains("ground_truth") {
                    let path = entry.path();
                    let json_content = fs::read_to_string(&path)?;

                    let cached: CachedTemplates = match serde_json::from_str(&json_content) {
                        Ok(c) => c,
                        Err(e) => {
                            println!("  ⚠️  Skipping {} - parse error: {}", name, e);
                            continue;
                        }
                    };

                    let count = cached.templates.len();
                    template_count_by_dataset.push((name.to_string(), count));

                    for (idx, mut template) in cached.templates.into_iter().enumerate() {
                        // Assign unique IDs if missing
                        if template.template_id == 0 {
                            template.template_id = (all_templates.len() + idx + 1) as u64;
                        }
                        all_templates.push(template);
                    }
                }
            }
        }
    }

    println!("\n📊 Templates by dataset:");
    for (dataset, count) in &template_count_by_dataset {
        println!("  {:<30} {:>5} templates", dataset, count);
    }
    println!("  {:-<40}", "");
    println!("  {:<30} {:>5} templates", "TOTAL", all_templates.len());

    println!("\n🔨 Building unified DFA with {} templates...", all_templates.len());
    let build_start = Instant::now();

    let config = MatcherConfig::batch_processing();
    let mut matcher = LogMatcher::with_config(config);

    for template in all_templates {
        matcher.add_template(LogTemplate {
            template_id: template.template_id,
            pattern: template.pattern,
            variables: template.variables,
            example: template.example,
        });
    }

    let build_time = build_start.elapsed();
    println!("✅ DFA built in {:.2}s", build_time.as_secs_f64());

    Ok(matcher)
}

fn load_all_logs() -> anyhow::Result<Vec<(String, Vec<String>)>> {
    println!("\n🔍 Loading all logs from data/loghub...");

    let mut all_logs = Vec::new();

    // Get all dataset directories
    if let Ok(entries) = fs::read_dir("data/loghub") {
        for entry in entries.flatten() {
            if entry.file_type().ok().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(dataset_name) = entry.file_name().to_str() {
                    let dataset = LogHubDatasetLoader::new(dataset_name, "data/loghub");
                    match dataset.load_raw_logs() {
                        Ok(logs) => {
                            println!("  {:<20} {:>8} logs", dataset_name, logs.len());
                            all_logs.push((dataset_name.to_string(), logs));
                        }
                        Err(e) => {
                            println!("  {:<20} ❌ {}", dataset_name, e);
                        }
                    }
                }
            }
        }
    }

    let total_logs: usize = all_logs.iter().map(|(_, logs)| logs.len()).sum();
    println!("  {:-<30}", "");
    println!("  {:<20} {:>8} logs", "TOTAL", total_logs);

    Ok(all_logs)
}

fn estimate_dfa_size(matcher: &LogMatcher) -> usize {
    let templates = matcher.get_all_templates();
    let mut size = 0;

    for t in &templates {
        size += t.pattern.len();
        size += t.example.len();
        size += t.variables.iter().map(|v| v.len()).sum::<usize>();
        size += std::mem::size_of::<LogTemplate>();
    }

    // Conservative DFA size estimate
    let estimated_dfa = templates.len() * 2048;
    size += estimated_dfa;

    size
}

fn run_cache_stress_test(matcher: &LogMatcher, all_logs: &[(String, Vec<String>)]) {
    println!("\n{:=<80}", "");
    println!("🔥 CACHE STRESS TEST - Mega DFA");
    println!("{:=<80}\n", "");

    let working_set = estimate_dfa_size(matcher);
    let template_count = matcher.get_all_templates().len();

    println!("Configuration:");
    println!("  Templates:           {}", template_count);
    println!("  Estimated DFA size:  {:.2} MB", working_set as f64 / 1_000_000.0);
    println!("  L1 cache (typical):  32-64 KB");
    println!("  L2 cache (typical):  256-512 KB");
    println!("  L3 cache (typical):  8-32 MB");

    if working_set > 512 * 1024 {
        println!("\n⚠️  WARNING: Working set exceeds typical L2 cache!");
        println!("   Expect cache thrashing if DFA doesn't fit in L3.");
    }

    println!("\n{:=<80}", "");
    println!("🚀 RUNNING BENCHMARKS");
    println!("{:=<80}\n", "");

    println!("{:<20} {:>8} {:>15} {:>15} {:>10}",
             "Dataset", "Logs", "Throughput", "Latency(μs)", "Matched");
    println!("{:-<70}", "");

    let mut total_logs = 0;
    let mut total_matched = 0;
    let mut total_time = 0.0;

    for (dataset_name, logs) in all_logs {
        let start = Instant::now();
        let mut matched = 0;

        for log in logs {
            if matcher.match_log(log).is_some() {
                matched += 1;
            }
        }

        let elapsed = start.elapsed();
        let throughput = logs.len() as f64 / elapsed.as_secs_f64();
        let latency_us = (elapsed.as_micros() as f64) / logs.len() as f64;

        println!("{:<20} {:>8} {:>12.0}/s {:>14.2}μs {:>9.1}%",
                 dataset_name, logs.len(), throughput, latency_us,
                 (matched as f64 / logs.len() as f64) * 100.0);

        total_logs += logs.len();
        total_matched += matched;
        total_time += elapsed.as_secs_f64();
    }

    println!("{:-<70}", "");
    println!("{:<20} {:>8} {:>12.0}/s {:>14.2}μs {:>9.1}%",
             "TOTAL", total_logs,
             total_logs as f64 / total_time,
             (total_time * 1_000_000.0) / total_logs as f64,
             (total_matched as f64 / total_logs as f64) * 100.0);
}

fn run_access_pattern_test(matcher: &LogMatcher, all_logs: &[(String, Vec<String>)]) {
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    println!("\n{:=<80}", "");
    println!("🔀 ACCESS PATTERN TEST - Cache Thrashing Detection");
    println!("{:=<80}\n", "");

    // Flatten all logs into one big vector
    let mut flat_logs: Vec<String> = Vec::new();
    for (_, logs) in all_logs {
        flat_logs.extend_from_slice(logs);
    }

    // Take a sample if too large
    let sample_size = 5000.min(flat_logs.len());
    let test_logs: Vec<String> = flat_logs[..sample_size].to_vec();

    println!("Testing with {} logs\n", test_logs.len());

    // Sequential access
    println!("📊 Sequential access...");
    let start = Instant::now();
    let iterations = 10;
    let mut matches = 0;

    for _ in 0..iterations {
        for log in &test_logs {
            if matcher.match_log(log).is_some() {
                matches += 1;
            }
        }
    }

    let seq_elapsed = start.elapsed();
    let seq_throughput = (test_logs.len() * iterations) as f64 / seq_elapsed.as_secs_f64();
    println!("  Throughput: {:.0} logs/sec", seq_throughput);
    println!("  Latency:    {:.1} ns/log",
             seq_elapsed.as_nanos() as f64 / (test_logs.len() * iterations) as f64);

    // Random access
    println!("\n📊 Random access...");
    let mut random_logs = test_logs.clone();
    random_logs.shuffle(&mut thread_rng());

    let start = Instant::now();
    let mut matches = 0;

    for _ in 0..iterations {
        for log in &random_logs {
            if matcher.match_log(log).is_some() {
                matches += 1;
            }
        }
    }

    let rand_elapsed = start.elapsed();
    let rand_throughput = (random_logs.len() * iterations) as f64 / rand_elapsed.as_secs_f64();
    println!("  Throughput: {:.0} logs/sec", rand_throughput);
    println!("  Latency:    {:.1} ns/log",
             rand_elapsed.as_nanos() as f64 / (random_logs.len() * iterations) as f64);

    // Analysis
    println!("\n{:=<80}", "");
    println!("📈 CACHE THRASHING ANALYSIS");
    println!("{:=<80}\n", "");

    let ratio = seq_throughput / rand_throughput;
    println!("Sequential/Random ratio: {:.2}x", ratio);

    if ratio > 1.5 {
        println!("\n⚠️  SIGNIFICANT CACHE THRASHING DETECTED!");
        println!("   Random access is {:.1}x slower than sequential.", ratio);
        println!("   The DFA working set likely exceeds available cache.");
    } else if ratio > 1.2 {
        println!("\n⚠️  MODERATE cache effects observed");
        println!("   Random access is {:.1}x slower than sequential.", ratio);
    } else {
        println!("\n✅ Excellent cache locality!");
        println!("   Random and sequential performance are similar.");
    }
}

fn main() -> anyhow::Result<()> {
    println!("\n{:=<80}", "");
    println!("🔬 MEGA CACHE PROFILING - ALL TEMPLATES × ALL LOGS");
    println!("{:=<80}\n", "");

    let matcher = load_all_templates()?;
    let all_logs = load_all_logs()?;

    run_cache_stress_test(&matcher, &all_logs);
    run_access_pattern_test(&matcher, &all_logs);

    println!("\n{:=<80}", "");
    println!("✅ PROFILING COMPLETE");
    println!("{:=<80}\n", "");

    Ok(())
}
