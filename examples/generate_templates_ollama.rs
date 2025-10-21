/// Generate all templates using Ollama and save to disk
/// This creates a pre-populated matcher that can be loaded instantly
///
/// Run with:
///   cargo run --example generate_templates_ollama --release
use log_analyzer::implementations::OpenStackDatasetLoader;
use log_analyzer::llm_service::LLMServiceClient;
use log_analyzer::log_matcher::LogMatcher;
use log_analyzer::traits::DatasetLoader;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n🚀 Ollama Template Generation\n");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Check if Ollama is running
    println!("1️⃣  Checking Ollama status...");
    let check_ollama = tokio::process::Command::new("curl")
        .args(&["-s", "http://localhost:11434/api/tags"])
        .output()
        .await?;

    if !check_ollama.status.success() {
        eprintln!("❌ Ollama is not running!");
        eprintln!("   Start it with: ollama serve");
        std::process::exit(1);
    }
    println!("   ✅ Ollama is running\n");

    // Load dataset
    println!("2️⃣  Loading OpenStack dataset...");
    let dataset = OpenStackDatasetLoader::new("data/openstack");
    let all_logs = dataset.load_raw_logs()?;
    println!("   ✅ Loaded {} logs\n", all_logs.len());

    // Get unique log samples (one per template)
    println!("3️⃣  Finding unique log patterns...");
    let ground_truth = dataset.load_ground_truth()?;
    let mut template_samples: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for entry in ground_truth.iter() {
        if !template_samples.contains_key(&entry.event_id) {
            template_samples.insert(entry.event_id.clone(), entry.log_line.clone());
        }
    }

    let unique_logs: Vec<String> = template_samples.values().cloned().collect();
    println!("   ✅ Found {} unique templates\n", unique_logs.len());

    // Create LLM client
    println!("4️⃣  Initializing Ollama client...");
    let llm_client = LLMServiceClient::new(
        "ollama".to_string(),
        "".to_string(),
        "llama3:latest".to_string(),
    );
    println!("   ✅ Using model: llama3:latest\n");

    // Generate templates
    println!("5️⃣  Generating templates with Ollama...");
    println!("   This will take a while (~3-5 seconds per template)...\n");

    let mut matcher = LogMatcher::new();
    let total_start = Instant::now();

    for (idx, log) in unique_logs.iter().enumerate() {
        let start = Instant::now();
        print!("   [{}/{}] Generating... ", idx + 1, unique_logs.len());
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match llm_client.generate_template(log).await {
            Ok(template) => {
                let elapsed = start.elapsed();
                println!(
                    "✅ ({:.1}s) Pattern: {}",
                    elapsed.as_secs_f64(),
                    &template.pattern[..template.pattern.len().min(60)]
                );
                matcher.add_template(template);
            }
            Err(e) => {
                println!("❌ Failed: {}", e);
            }
        }
    }

    let total_elapsed = total_start.elapsed();
    println!(
        "\n   ✅ Generated {} templates in {:.1}s (avg: {:.1}s per template)\n",
        unique_logs.len(),
        total_elapsed.as_secs_f64(),
        total_elapsed.as_secs_f64() / unique_logs.len() as f64
    );

    // Save to disk
    println!("6️⃣  Saving templates to disk...");
    let bin_path = "openstack_templates.bin";
    let json_path = "openstack_templates.json";

    matcher.save_to_file(bin_path)?;
    matcher.save_to_json(json_path)?;

    let bin_size = std::fs::metadata(bin_path)?.len();
    let json_size = std::fs::metadata(json_path)?.len();

    println!("   ✅ Binary: {} ({} bytes)", bin_path, bin_size);
    println!("   ✅ JSON: {} ({} bytes)\n", json_path, json_size);

    // Quick validation test
    println!("7️⃣  Validating matcher...");
    let test_logs = vec![
        all_logs.get(0).map(|s| s.as_str()).unwrap_or(""),
        all_logs.get(100).map(|s| s.as_str()).unwrap_or(""),
        all_logs.get(500).map(|s| s.as_str()).unwrap_or(""),
    ];

    for log in &test_logs {
        if !log.is_empty() {
            match matcher.match_log(log) {
                Some(id) => println!("   ✅ Matched template {}", id),
                None => println!("   ⚠️  No match"),
            }
        }
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Template generation complete!");
    println!("\n💡 Next steps:");
    println!("   - Run benchmark with pre-loaded templates:");
    println!("     cargo test --test benchmark_with_preloaded --release -- --nocapture");
    println!("   - Compare with cold start benchmark");
    println!("   - Templates can be loaded instantly from {}\n", bin_path);

    Ok(())
}
