use log_analyzer::llm_service::LLMServiceClient;
use log_analyzer::log_matcher::LogTemplate;
use std::collections::HashMap;
use std::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let datasets = vec!["Linux", "Mac", "Thunderbird"];
    let base_path = "data/loghub";

    println!("🔄 Generating templates from CSV samples with LLM...");
    println!("⚠️  Make sure Ollama is running: ollama serve");
    println!();

    // Check if Ollama is accessible
    let test_client = LLMServiceClient::new(
        "ollama".to_string(),
        "".to_string(),
        "llama3:latest".to_string(),
    );

    println!("Testing Ollama connection...");
    match test_client.generate_template("test").await {
        Ok(_) => println!("✓ Ollama is running and accessible"),
        Err(e) => {
            eprintln!("❌ Cannot connect to Ollama: {}", e);
            eprintln!("   Start Ollama with: ollama serve");
            return Err(e);
        }
    }
    println!();

    for dataset in datasets {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📊 Generating: {}", dataset);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let structured_csv_path = format!("{}/{}/{}_2k.log_structured.csv", base_path, dataset, dataset);
        let structured_csv_content = match fs::read_to_string(&structured_csv_path) {
            Ok(content) => content,
            Err(_) => {
                println!("   ⚠️  Skipping {} (structured file not found)", dataset);
                println!();
                continue;
            }
        };

        // Group logs by EventId and take 3 samples of each
        let mut event_samples: HashMap<String, Vec<String>> = HashMap::new();
        for line in structured_csv_content.lines().skip(1) {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() >= 9 {
                let event_id = fields[8].trim().to_string();
                // Reconstruct full log line
                let full_log = format!("{} {} {} {} {}: {}",
                    fields[1], fields[2], fields[3], fields[4], fields[5], fields[7]);

                let samples = event_samples.entry(event_id).or_insert_with(Vec::new);
                if samples.len() < 3 {
                    samples.push(full_log);
                }
            }
        }

        println!("   Found {} unique event types", event_samples.len());
        println!("   Generating templates from 3 samples each...");

        let llm_client = LLMServiceClient::new(
            "ollama".to_string(),
            "".to_string(),
            "llama3:latest".to_string(),
        );

        let mut templates_map: HashMap<String, LogTemplate> = HashMap::new();
        let mut next_id = 1u64;
        let total_events = event_samples.len();

        for (idx, (_event_id, samples)) in event_samples.iter().enumerate() {
            print!("\r   Progress: {}/{} event types ({:.1}%)", idx + 1, total_events, (idx + 1) as f64 / total_events as f64 * 100.0);
            std::io::Write::flush(&mut std::io::stdout()).ok();

            // Use the first sample to generate template
            if let Some(sample_log) = samples.first() {
                match llm_client.generate_template(sample_log).await {
                    Ok(mut template) => {
                        if !templates_map.contains_key(&template.pattern) {
                            template.template_id = next_id;
                            template.example = sample_log.clone();
                            next_id += 1;
                            templates_map.insert(template.pattern.clone(), template);
                        }
                    }
                    Err(_e) => {
                        // Silently skip errors to avoid spam
                    }
                }
            }
        }

        println!("\r   Progress: {}/{} event types ✓", total_events, total_events);
        println!("   Generated {} unique templates", templates_map.len());

        // Save to cache
        let cache_file = format!("cache/{}_templates.json", dataset.to_lowercase());

        // Backup old file
        if std::path::Path::new(&cache_file).exists() {
            let backup_file = format!("{}.old2", cache_file);
            fs::copy(&cache_file, &backup_file)?;
        }

        let templates: Vec<LogTemplate> = templates_map.into_values().collect();
        let state = serde_json::json!({
            "templates": templates,
            "next_template_id": next_id
        });

        fs::write(&cache_file, serde_json::to_string_pretty(&state)?)?;
        println!("   ✓ Saved to {}", cache_file);
        println!();
    }

    println!("✅ Template generation complete!");
    println!();
    println!("Run benchmark to test:");
    println!("  cargo test --test benchmark_llm_templates --release -- --nocapture");

    Ok(())
}
