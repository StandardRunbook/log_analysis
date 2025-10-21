/// Build DFAs offline for all LogHub datasets
///
/// Uses pre-generated templates from LogHub
///
/// Usage:
///   cargo run --example build_all_dfas --release
use log_analyzer::log_matcher::{LogMatcher, LogTemplate};
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug)]
struct DatasetResult {
    name: String,
    templates: usize,
    build_time: std::time::Duration,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("\n🏗️  Building DFAs for All LogHub Datasets\n");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Loading pre-generated templates from LogHub\n");

    // All LogHub datasets
    let datasets = vec![
        "Android",
        "Apache",
        "BGL",
        "Hadoop",
        "HDFS",
        "HealthApp",
        "HPC",
        "Linux",
        "Mac",
        "OpenSSH",
        "OpenStack",
        "Proxifier",
        "Spark",
        "Thunderbird",
        "Windows",
        "Zookeeper",
    ];

    let base_path = "data/loghub";
    fs::create_dir_all("cache")?;

    let mut results = Vec::new();
    let mut skipped = Vec::new();

    for (idx, dataset_name) in datasets.iter().enumerate() {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "📦 Dataset {}/{}: {}",
            idx + 1,
            datasets.len(),
            dataset_name
        );
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        let cache_path = format!("cache/{}_templates.bin", dataset_name.to_lowercase());
        let json_path = format!("cache/{}_templates.json", dataset_name.to_lowercase());

        if Path::new(&cache_path).exists() {
            println!("   ℹ️  Cache already exists: {}", cache_path);
            println!("   Skipping (delete to rebuild)\n");
            skipped.push(dataset_name.to_string());
            continue;
        }

        let start = Instant::now();

        println!("1️⃣  Loading templates...");
        let loader = LogHubDatasetLoader::new(dataset_name, base_path);

        let templates = match loader.load_templates() {
            Ok(t) => t,
            Err(e) => {
                println!("   ❌ Failed: {}", e);
                println!("   Skipping...\n");
                skipped.push(dataset_name.to_string());
                continue;
            }
        };

        println!("   ✅ Loaded {} templates\n", templates.len());

        println!("2️⃣  Building DFA...");
        let mut matcher = LogMatcher::new();

        for (event_id, regex) in templates {
            let template_id = if event_id.starts_with('E') {
                event_id[1..].parse::<u64>().unwrap_or(0)
            } else {
                event_id.parse::<u64>().unwrap_or(0)
            };

            matcher.add_template(LogTemplate {
                template_id,
                pattern: regex,
                variables: Vec::new(),
                example: String::new(),
            });
        }

        let template_count = matcher.get_all_templates().len();
        println!("   ✅ Built DFA with {} templates\n", template_count);

        println!("3️⃣  Saving to cache...");

        if let Err(e) = matcher.save_to_file(&cache_path) {
            println!("   ❌ Failed to save binary: {}", e);
        } else {
            let size = fs::metadata(&cache_path)?.len();
            println!("   ✅ Binary: {} ({} bytes)", cache_path, size);
        }

        if let Err(e) = matcher.save_to_json(&json_path) {
            println!("   ❌ Failed to save JSON: {}", e);
        } else {
            let size = fs::metadata(&json_path)?.len();
            println!("   ✅ JSON: {} ({} bytes)", json_path, size);
        }

        let build_time = start.elapsed();
        println!("   ⏱️  Total: {:.2}s\n", build_time.as_secs_f64());

        results.push(DatasetResult {
            name: dataset_name.to_string(),
            templates: template_count,
            build_time,
        });
    }

    // Summary
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ DFA Building Complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    if !results.is_empty() {
        println!("📊 Built {} DFAs:\n", results.len());
        println!("{:<15} {:>10} {:>12}", "Dataset", "Templates", "Build Time");
        println!("{:<15} {:>10} {:>12}", "-------", "---------", "----------");

        for result in &results {
            println!(
                "{:<15} {:>10} {:>12.2}s",
                result.name,
                result.templates,
                result.build_time.as_secs_f64()
            );
        }
        println!();
    }

    if !skipped.is_empty() {
        println!("⏭️  Skipped: {}\n", skipped.join(", "));
    }

    println!("💾 Caches: cache/");
    println!("🚀 Next: cargo test --test offline_dfa_benchmark --release -- --nocapture\n");

    Ok(())
}
