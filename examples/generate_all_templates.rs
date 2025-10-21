/// Generate LLM-based hierarchical templates for ALL log types in Linux dataset
///
/// Strategy:
/// 1. Scan through dataset and identify all unique log types
/// 2. For each log type, pick a representative sample log
/// 3. Use LLM to generate hierarchical template with field classifications
/// 4. Save all templates to cache/linux_templates.json
///
use log_analyzer::llm_service::LLMServiceClient;
use log_analyzer::token_classifier::{classify_token, extract_log_type_signature, TokenClass};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use anyhow::Result;
use dotenvy::dotenv;

#[derive(Debug, Serialize, Deserialize)]
struct FieldClassification {
    field: String,
    #[serde(rename = "type")]
    field_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HierarchicalTemplate {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    log_type: String,
    template: String,
    regex: String,
    parameters: Vec<FieldClassification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    example_log: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TemplateCollection {
    total_log_types: usize,
    templates: Vec<HierarchicalTemplate>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    println!("🎯 Generating Hierarchical Templates for ALL Linux Log Types\n");
    println!("{}", "=".repeat(80));
    println!();

    // Initialize LLM client
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("LLM_API_KEY"))?;
    let llm_client = LLMServiceClient::new(
        "openai".to_string(),
        api_key,
        "gpt-4o-mini".to_string(),
    );

    // Load logs
    let content = fs::read_to_string("data/loghub/Linux/Linux_2k.log")?;
    let logs: Vec<&str> = content.lines().collect();
    println!("📂 Loaded {} logs\n", logs.len());

    // Step 1: Identify all unique log types with sample logs
    println!("🔍 Step 1: Identifying unique log types...\n");
    let mut log_type_samples: HashMap<String, String> = HashMap::new();

    for log in logs.iter() {
        // Tokenize and classify
        let tokens = tokenize(log);
        let classified: Vec<(String, TokenClass)> = tokens
            .iter()
            .enumerate()
            .map(|(i, token)| {
                let context = if i > 0 { Some(tokens[i - 1].as_str()) } else { None };
                (token.clone(), classify_token(token, context))
            })
            .collect();

        // Extract log type (Level 1)
        let log_type = extract_log_type_signature(
            &classified.iter().map(|(t, c)| (t.as_str(), c.clone())).collect::<Vec<_>>()
        );

        // Store first occurrence as sample
        if !log_type.is_empty() && !log_type_samples.contains_key(&log_type) {
            log_type_samples.insert(log_type.clone(), log.to_string());
        }
    }

    println!("✅ Found {} unique log types\n", log_type_samples.len());
    println!("{}", "=".repeat(80));
    println!();

    // Step 2: Generate templates for each log type
    println!("🤖 Step 2: Generating templates with LLM...\n");

    let mut templates = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;

    for (idx, (log_type, sample_log)) in log_type_samples.iter().enumerate() {
        println!("📝 Template {}/{}: {}", idx + 1, log_type_samples.len(), log_type);
        println!("   Log: {}", truncate(&sample_log, 100));
        println!();

        match generate_template(&llm_client, &sample_log, log_type).await {
            Ok(template) => {
                println!("   ✅ Generated successfully");
                println!("   Template: \"{}\"", template.template);
                if !template.parameters.is_empty() {
                    println!("   Parameters:");
                    for param in &template.parameters {
                        println!("     - {} ({})", param.field, param.field_type);
                    }
                }
                templates.push(template);
                success_count += 1;
            }
            Err(e) => {
                println!("   ❌ Failed: {}", e);
                fail_count += 1;
            }
        }
        println!("{}", "=".repeat(80));
        println!();

        // Rate limiting
        if idx < log_type_samples.len() - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    // Step 3: Save results
    println!("📊 Summary:");
    println!("   Total: {} log types", log_type_samples.len());
    println!("   Success: {} templates", success_count);
    println!("   Failed: {}", fail_count);
    println!();

    let collection = TemplateCollection {
        total_log_types: log_type_samples.len(),
        templates,
    };

    let output_path = "cache/linux_templates.json";
    fs::write(output_path, serde_json::to_string_pretty(&collection)?)?;
    println!("📁 Saved to: {}", output_path);

    Ok(())
}

async fn generate_template(
    llm_client: &LLMServiceClient,
    log_line: &str,
    log_type: &str,
) -> Result<HierarchicalTemplate> {
    let prompt = format!(r#"
Analyze this log line and classify each field as STATIC, EPHEMERAL, or PARAMETER.

CLASSIFICATION RULES:
1. STATIC - Keywords defining structure (service names, actions, field markers like "user=", "rhost=")
2. EPHEMERAL - Always-changing values (timestamps, PIDs, IPs, UUIDs) - IGNORE for template
3. PARAMETER - Business values to track (usernames, hostnames NOT IPs, resources, actions)

PARAMETER TYPES:
- User: Usernames, email addresses
- Location: Hostnames (not IPs), domain names
- Resource: File paths, table names, database names
- Action: Status codes, operations, error types
- Generic: Other trackable values

TASK:
1. Identify the log type signature (STATIC tokens only): "{}"
2. Create a template signature with STATIC + PARAMETER types: "service action param=<Type>"
3. Generate a regex pattern with named capture groups like (?P<username>\\w+) for each PARAMETER
4. List all PARAMETER fields with their types

RESPOND WITH VALID JSON ONLY (no markdown, no code blocks):
{{
  "log_type": "service action keywords",
  "template": "service action param=<ParameterType>",
  "regex": "regex with (?P<name>pattern) for parameters",
  "parameters": [
    {{"field": "example_value", "type": "User"}}
  ]
}}

LOG LINE:
{}
"#, log_type, log_line);

    let response = llm_client.call_openai_simple(&prompt).await?;

    // Try to parse JSON response
    let response = response.trim();
    let json_str = if response.starts_with("```json") {
        response.trim_start_matches("```json").trim_end_matches("```").trim()
    } else if response.starts_with("```") {
        response.trim_start_matches("```").trim_end_matches("```").trim()
    } else {
        response
    };

    let mut parsed: HierarchicalTemplate = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("JSON parse error: {}. Response: {}", e, json_str))?;

    // Add example log and generate description
    parsed.example_log = Some(log_line.to_string());
    parsed.description = Some(log_type.to_string());

    Ok(parsed)
}

fn tokenize(text: &str) -> Vec<String> {
    let tokenizer = Regex::new(r"[\w./@-]+|[=:\[\]\(\)]").unwrap();
    tokenizer
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
