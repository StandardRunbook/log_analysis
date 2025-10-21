/// Generate Ground Truth Style Templates with LLM
///
/// Improved prompt that preserves log structure:
/// - Keep STATIC parts literal (service names, punctuation, field markers)
/// - Keep PARAMETER values literal (user=root, uid=0)
/// - Replace only EPHEMERAL with <*> (timestamps, PIDs, IPs)
///
use log_analyzer::llm_service::LLMServiceClient;
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
struct GroundTruthTemplate {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    log_type: String,
    template: String,  // Ground truth format with <*>
    parameters: Vec<FieldClassification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    example_log: Option<String>,
    #[serde(default)]
    count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct TemplateCollection {
    total_log_types: usize,
    templates: Vec<GroundTruthTemplate>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    println!("\n🎯 Generating Ground Truth Style Templates with LLM\n");
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

    // Quick identification using service extraction
    println!("🔍 Identifying log types...\n");
    let mut log_type_data: HashMap<String, (usize, String)> = HashMap::new();
    let service_extractor = Regex::new(r"^\w+ \d+ \d+:\d+:\d+ \w+ (\w+)[\[\(:]").unwrap();

    for log in logs.iter() {
        let log_type = if let Some(caps) = service_extractor.captures(log) {
            caps.get(1).unwrap().as_str().to_string()
        } else {
            "unknown".to_string()
        };

        let entry = log_type_data.entry(log_type).or_insert((0, log.to_string()));
        entry.0 += 1;
    }

    let mut sorted_types: Vec<_> = log_type_data.into_iter().collect();
    sorted_types.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

    let top_n = 25.min(sorted_types.len());
    let top_types = &sorted_types[..top_n];

    println!("✅ Found {} log types, using top {}\n", sorted_types.len(), top_n);
    println!("{}", "=".repeat(80));
    println!();

    // Generate templates
    println!("🤖 Generating ground truth templates with LLM...\n");

    let mut templates = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;

    for (idx, (log_type, (count, sample_log))) in top_types.iter().enumerate() {
        println!("📝 Template {}/{}: {} ({} logs)", idx + 1, top_n, log_type, count);
        println!("   Log: {}", truncate(&sample_log, 100));
        println!();

        match generate_ground_truth_template(&llm_client, &sample_log, log_type, *count).await {
            Ok(template) => {
                println!("   ✅ Generated:");
                println!("      Template: {}", truncate(&template.template, 80));
                if !template.parameters.is_empty() {
                    println!("      Parameters: {:?}", template.parameters.iter()
                        .map(|p| format!("{}({})", p.field, p.field_type))
                        .collect::<Vec<_>>());
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
        if idx < top_n - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    // Save results
    println!("📊 Summary:");
    println!("   Success: {} templates", success_count);
    println!("   Failed: {}", fail_count);
    println!();

    let collection = TemplateCollection {
        total_log_types: top_n,
        templates,
    };

    let output_path = "cache/ground_truth_llm_templates.json";
    fs::write(output_path, serde_json::to_string_pretty(&collection)?)?;
    println!("📁 Saved to: {}", output_path);

    Ok(())
}

async fn generate_ground_truth_template(
    llm_client: &LLMServiceClient,
    log_line: &str,
    log_type: &str,
    count: usize,
) -> Result<GroundTruthTemplate> {
    let prompt = format!(r#"
Create a GROUND TRUTH template from this log line by replacing ONLY ephemeral values with <*>.

CRITICAL RULES:
1. **PRESERVE ALL STRUCTURE**: Keep parentheses (), brackets [], colons :, semicolons ;, equals =
2. **KEEP STATIC LITERAL**: Service names, subsystems, actions, field markers
3. **KEEP PARAMETER VALUES LITERAL**: Usernames, constants, specific values
4. **REPLACE ONLY EPHEMERAL**: Timestamps, hostname in header, PIDs, IP addresses

WHAT TO REPLACE WITH <*>:
- Timestamp at start: "Jun 14 15:16:01" → "<*>"
- Hostname after timestamp: "combo" → "<*>"
- PIDs in brackets: "[19939]" → "[<*>]"
- IP addresses: "218.188.2.4" → "<*>"

WHAT TO KEEP LITERAL:
- Service: "sshd" → "sshd"
- Subsystem: "(pam_unix)" → "(pam_unix)"
- Actions: "authentication failure" → "authentication failure"
- Field markers: "user=" → "user="
- Parameter values: "user=root" → "user=root" (NOT user=<*>)
- Constants: "uid=0" → "uid=0"
- Specific values: "tty=NODEVssh" → "tty=NODEVssh"

EXAMPLES:
Input:  "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=218.188.2.4  user=root"
Output: "<*> <*> sshd(pam_unix)[<*>]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=<*>  user=root"

Input:  "Jun 17 07:07:00 combo ftpd[29504]: connection from 24.54.76.216 (host.example.com) at Mon Jun 17"
Output: "<*> <*> ftpd[<*>]: connection from <*> (<*>) at <*>"

RESPOND WITH JSON ONLY (no markdown):
{{
  "log_type": "{}",
  "template": "ground truth template here",
  "parameters": [
    {{"field": "root", "type": "User"}}
  ]
}}

LOG LINE:
{}
"#, log_type, log_line);

    let response = llm_client.call_openai_simple(&prompt).await?;

    // Parse JSON
    let response = response.trim();
    let json_str = if response.starts_with("```json") {
        response.trim_start_matches("```json").trim_end_matches("```").trim()
    } else if response.starts_with("```") {
        response.trim_start_matches("```").trim_end_matches("```").trim()
    } else {
        response
    };

    let mut parsed: GroundTruthTemplate = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("JSON parse error: {}. Response: {}", e, json_str))?;

    // Add metadata
    parsed.example_log = Some(log_line.to_string());
    parsed.description = Some(log_type.to_string());
    parsed.count = count;

    Ok(parsed)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
