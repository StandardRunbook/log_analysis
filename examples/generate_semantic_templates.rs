/// Generate semantic templates using LLM
///
/// Philosophy: LLM identifies LOG TYPE/structure, not value-specific patterns
/// - "auth failure with user=root" and "auth failure with user=guest" = SAME template
/// - Parameters extracted using tokenization regex, not LLM
use anyhow::Result;
use log_analyzer::llm_service::LLMServiceClient;
use log_analyzer::loghub_loader::LogHubDatasetLoader;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SemanticTemplate {
    template_id: u64,
    description: String,
    keywords: Vec<String>,
    parameters: Vec<String>,
    example: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let llm_provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string());
    let llm_model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

    println!("🔧 Using LLM: {} / {}", llm_provider, llm_model);

    let llm_client = LLMServiceClient::new(llm_provider, "".to_string(), llm_model);

    // Load Linux dataset
    use log_analyzer::traits::DatasetLoader;
    let loader = LogHubDatasetLoader::new("Linux", "data/loghub");
    let logs = loader.load_raw_logs()?;
    let ground_truth = loader.load_ground_truth()?;
    let event_ids: Vec<String> = ground_truth.iter().map(|e| e.event_id.clone()).collect();

    println!("📊 Loaded {} logs", logs.len());

    // Group by event ID to find representatives
    let mut event_to_logs: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, event_id) in event_ids.iter().enumerate() {
        event_to_logs
            .entry(event_id.clone())
            .or_default()
            .push(idx);
    }

    println!("📋 Found {} unique event types", event_to_logs.len());

    // For each event type, generate a semantic template
    let mut templates = Vec::new();
    let mut processed = HashSet::new();

    for (template_id, (event_id, log_indices)) in event_to_logs.iter().enumerate() {
        if processed.contains(event_id) {
            continue;
        }

        // Pick first log as representative
        let log_idx = log_indices[0];
        let sample_log = &logs[log_idx];

        println!("\n📝 Event {}: {} ({})", event_id, &sample_log[..80.min(sample_log.len())], log_indices.len());

        // Generate semantic template using LLM
        match generate_semantic_template(&llm_client, sample_log).await {
            Ok(template_data) => {
                let template = SemanticTemplate {
                    template_id: template_id as u64,
                    description: template_data.description,
                    keywords: template_data.keywords,
                    parameters: template_data.parameters,
                    example: sample_log.clone(),
                };

                println!("  ✓ Description: {}", template.description);
                println!("  ✓ Keywords: {:?}", template.keywords);
                println!("  ✓ Parameters: {:?}", template.parameters);

                templates.push(template);
                processed.insert(event_id.clone());
            }
            Err(e) => {
                println!("  ✗ Error: {}", e);
            }
        }

        // Rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Save templates
    let output = serde_json::json!({
        "templates": templates,
        "count": templates.len(),
    });

    let output_path = "cache/semantic_templates.json";
    fs::write(output_path, serde_json::to_string_pretty(&output)?)?;

    println!("\n✅ Generated {} semantic templates", templates.len());
    println!("📁 Saved to: {}", output_path);

    Ok(())
}

#[derive(Deserialize)]
struct LLMResponse {
    description: String,
    keywords: Vec<String>,
    parameters: Vec<String>,
}

async fn generate_semantic_template(
    llm_client: &LLMServiceClient,
    log_line: &str,
) -> Result<LLMResponse> {
    let prompt = format!(
        r#"Analyze this log and describe its SEMANTIC STRUCTURE (not specific values).

LOG: {log_line}

Your task: Identify what TYPE of log this is.

CRITICAL RULES:
1. Focus on STRUCTURE, not VALUES
   - ✅ GOOD: "SSH authentication failure"
   - ❌ BAD: "SSH authentication failure for user root"

2. Extract KEYWORDS that identify this log type
   - Include: service names, action words, field names
   - Example: ["sshd", "pam_unix", "authentication", "failure"]
   - Do NOT include: specific values like "root", "192.168.1.1"

3. Identify PARAMETER TYPES (not values!)
   - ✅ GOOD: "username", "ip_address", "pid"
   - ❌ BAD: "root", "192.168.1.1", "12345"

EXAMPLE:
Input: "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; user=root rhost=192.168.1.1"

Output:
{{
  "description": "SSH PAM authentication failure",
  "keywords": ["sshd", "pam_unix", "authentication", "failure"],
  "parameters": ["timestamp", "hostname", "pid", "username", "ip_address"]
}}

Now analyze this log:
{log_line}

Respond with ONLY JSON (no explanation):
{{"description": "...", "keywords": [...], "parameters": [...]}}
"#,
        log_line = log_line
    );

    let response = llm_client.call_openai_simple(&prompt).await?;

    // Parse JSON from response
    let json_start = response.find('{').unwrap_or(0);
    let json_end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
    let json_str = &response[json_start..json_end];

    let parsed: LLMResponse = serde_json::from_str(json_str)?;

    Ok(parsed)
}
