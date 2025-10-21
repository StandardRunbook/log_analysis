use log_analyzer::llm_service::LLMServiceClient;
use log_analyzer::log_matcher::LogTemplate;
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use log_analyzer::traits::DatasetLoader;
use std::collections::HashMap;
use std::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let datasets = vec!["Linux", "Mac", "Thunderbird"];
    let base_path = "data/loghub";

    println!("🔄 Regenerating templates for datasets with improved prompt...");
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
            eprintln!("   Pull model with: ollama pull llama3.2:latest");
            return Err(e);
        }
    }
    println!();

    for dataset in datasets {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📊 Regenerating: {}", dataset);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let loader = LogHubDatasetLoader::new(dataset, base_path);
        let logs = loader.load_raw_logs()?;

        println!("   Loaded {} log lines", logs.len());
        println!("   Generating templates...");

        // Process logs in parallel batches of 5 to avoid overwhelming Ollama
        let mut templates_map: HashMap<String, LogTemplate> = HashMap::new();
        let mut next_id = 1u64;
        let batch_size = 5;
        let total_logs = logs.len();

        for (batch_idx, chunk) in logs.chunks(batch_size).enumerate() {
            let mut tasks = vec![];

            for log_line in chunk {
                let client = LLMServiceClient::new(
                    "ollama".to_string(),
                    "".to_string(),
                    "llama3:latest".to_string(),
                );
                let log = log_line.clone();
                tasks.push(tokio::spawn(async move {
                    client.generate_template(&log).await
                }));
            }

            // Wait for all tasks in this batch
            for (i, task) in tasks.into_iter().enumerate() {
                let log_idx = batch_idx * batch_size + i;
                print!("\r   Progress: {}/{} logs ({:.1}%)", log_idx + 1, total_logs, (log_idx + 1) as f64 / total_logs as f64 * 100.0);
                std::io::Write::flush(&mut std::io::stdout()).ok();

                match task.await {
                    Ok(Ok(mut template)) => {
                        // Check if we already have this pattern
                        if !templates_map.contains_key(&template.pattern) {
                            template.template_id = next_id;
                            next_id += 1;
                            templates_map.insert(template.pattern.clone(), template);
                        }
                    }
                    Ok(Err(e)) => {
                        eprintln!("\n   Warning: Failed to generate template: {}", e);
                    }
                    Err(e) => {
                        eprintln!("\n   Warning: Task failed: {}", e);
                    }
                }
            }
        }

        println!("\r   Progress: {}/{} logs ✓", logs.len(), logs.len());
        println!("   Generated {} unique templates", templates_map.len());

        // Save to cache
        let cache_file = format!("cache/{}_templates.json", dataset.to_lowercase());
        let templates: Vec<LogTemplate> = templates_map.into_values().collect();

        let state = serde_json::json!({
            "templates": templates,
            "next_template_id": next_id
        });

        fs::write(&cache_file, serde_json::to_string_pretty(&state)?)?;
        println!("   ✓ Saved to {}", cache_file);
        println!();
    }

    println!("✅ Template regeneration complete!");
    println!();
    println!("Run benchmark to test:");
    println!("  cargo test --test benchmark_llm_templates --release -- --nocapture");

    Ok(())
}
