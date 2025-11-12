/// Regenerate cache files with ClickHouse auto-increment IDs
///
/// Reads template JSON files, uploads to ClickHouse (which assigns unique IDs),
/// then exports back to cache with new IDs

use anyhow::Result;
use chrono::Utc;
use log_analyzer::clickhouse_client::{ClickHouseClient, TemplateRow};
use log_analyzer::log_matcher::LogTemplate;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct TemplateCache {
    templates: Vec<CachedTemplate>,
}

#[derive(Debug, Deserialize)]
struct CachedTemplate {
    #[allow(dead_code)]
    template_id: u64,
    pattern: String,
    variables: Vec<String>,
    example: String,
}

#[derive(Debug, Serialize)]
struct OutputCache {
    templates: Vec<OutputTemplate>,
}

#[derive(Debug, Serialize)]
struct OutputTemplate {
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

    // Clear existing templates
    println!("Clearing existing templates...");
    client.clear_templates().await?;

    let cache_dir = Path::new("cache");
    if !cache_dir.exists() {
        anyhow::bail!("Cache directory not found");
    }

    // Collect datasets to process
    let mut datasets = Vec::new();
    for entry in fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Extract dataset name
        let dataset_name = filename
            .strip_suffix("_templates.json")
            .or_else(|| filename.strip_suffix("_templates_with_anchors.json"))
            .or_else(|| filename.strip_suffix(".json"))
            .unwrap_or(filename);

        datasets.push((dataset_name.to_string(), path.clone()));
    }

    // Sort for consistent ordering
    datasets.sort_by(|a, b| a.0.cmp(&b.0));

    // Phase 1: Upload all templates to ClickHouse (gets auto-increment IDs)
    println!("\n=== Phase 1: Uploading templates to ClickHouse ===\n");
    for (dataset_name, path) in &datasets {
        println!("Uploading templates from {}...", dataset_name);

        let content = fs::read_to_string(path)?;
        let cache: TemplateCache = match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  Skipping {} (incompatible format): {}", dataset_name, e);
                continue;
            }
        };

        for template in cache.templates {
            let row = TemplateRow {
                org_id: org_id.clone(),
                log_stream_id: format!("cache-{}", dataset_name),
                template_id: 0, // Will be auto-assigned
                pattern: template.pattern,
                variables: template.variables,
                example: template.example,
                created_at: Utc::now(),
            };

            match client.insert_template_with_autoid(row).await {
                Ok(new_id) => {
                    println!("  Inserted template with ID {}", new_id);
                }
                Err(e) => {
                    eprintln!("  Failed to insert template: {}", e);
                }
            }
        }
    }

    // Phase 2: Download templates with new IDs and save to cache
    println!("\n=== Phase 2: Downloading templates with new IDs ===\n");
    for (dataset_name, path) in &datasets {
        println!("Regenerating cache for {}...", dataset_name);

        let log_stream_id = format!("cache-{}", dataset_name);
        let templates = client.get_templates_for_stream(&org_id, &log_stream_id).await?;

        if templates.is_empty() {
            println!("  Warning: No templates found for {}", dataset_name);
            continue;
        }

        let output = OutputCache {
            templates: templates.iter().map(|t| OutputTemplate {
                template_id: t.template_id,
                pattern: t.pattern.clone(),
                variables: t.variables.clone(),
                example: t.example.clone(),
            }).collect(),
        };

        let json = serde_json::to_string_pretty(&output)?;
        fs::write(path, json)?;

        println!("  Saved {} templates to {:?}", output.templates.len(), path);
    }

    println!("\n✅ Cache regeneration complete!");
    Ok(())
}
