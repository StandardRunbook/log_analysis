/// Sync cached templates to ClickHouse database
///
/// Reads template JSON files from cache/ directory and inserts them into the templates table

use anyhow::Result;
use chrono::Utc;
use log_analyzer::clickhouse_client::{ClickHouseClient, TemplateRow};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct TemplateCache {
    templates: Vec<CachedTemplate>,
}

#[derive(Debug, Deserialize)]
struct CachedTemplate {
    template_id: u64,
    pattern: String,
    variables: Vec<String>,
    example: String,
}

#[tokio::main]
async fn main() -> Result<()> {

    let clickhouse_url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123".to_string());

    let org_id = std::env::var("ORG_ID")
        .unwrap_or_else(|_| "default".to_string());

    println!("Connecting to ClickHouse at {}", clickhouse_url);
    let client = ClickHouseClient::new(&clickhouse_url)?;

    let cache_dir = Path::new("cache");
    if !cache_dir.exists() {
        anyhow::bail!("Cache directory not found");
    }

    let mut total_inserted = 0;

    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        println!("Processing {}...", filename);

        let content = fs::read_to_string(&path)?;
        let cache: TemplateCache = match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  Skipping {} (incompatible format): {}", filename, e);
                continue;
            }
        };

        // Extract dataset name from filename (e.g., "android_templates.json" -> "android")
        let dataset_name = filename
            .strip_suffix("_templates.json")
            .or_else(|| filename.strip_suffix("_templates_with_anchors.json"))
            .or_else(|| filename.strip_suffix(".json"))
            .unwrap_or(filename);

        let template_count = cache.templates.len();
        let mut inserted_count = 0;

        for template in cache.templates {
            let row = TemplateRow {
                org_id: org_id.clone(),
                log_stream_id: format!("cache-{}", dataset_name),
                template_id: template.template_id,
                pattern: template.pattern,
                variables: template.variables,
                example: template.example,
                created_at: Utc::now(),
            };

            match client.insert_template(row).await {
                Ok(_) => {
                    total_inserted += 1;
                    inserted_count += 1;
                }
                Err(e) => eprintln!("  Failed to insert template {}: {}", template.template_id, e),
            }
        }

        println!("  Inserted {}/{} templates from {}", inserted_count, template_count, filename);
    }

    println!("\nTotal templates inserted: {}", total_inserted);
    Ok(())
}
