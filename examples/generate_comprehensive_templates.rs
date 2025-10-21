/// Generate templates from EVERY unique log pattern
///
/// Strategy:
/// 1. Group logs by removing ephemeral values (timestamps, IPs, PIDs)
/// 2. For each unique pattern, pick a representative sample
/// 3. Generate LLM template for each pattern
/// 4. Target ~100-150 templates to match ground truth coverage
///
use log_analyzer::llm_service::LLMServiceClient;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use anyhow::Result;
use dotenvy::dotenv;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FieldClassification {
    field: String,
    #[serde(rename = "type")]
    field_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroundTruthTemplate {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    log_type: String,
    template: String,
    parameters: Vec<FieldClassification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    example_log: Option<String>,
    #[serde(default)]
    count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct TemplateCollection {
    total_patterns: usize,
    templates: Vec<GroundTruthTemplate>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    println!("\n🎯 Generating Comprehensive Templates from ALL Log Patterns\n");
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

    // Step 1: Group logs by pattern (normalize ephemeral values)
    println!("🔍 Step 1: Grouping logs by pattern...\n");

    let timestamp_re = Regex::new(r"^\w+ \d+ \d+:\d+:\d+")?;
    let hostname_re = Regex::new(r"^\w+ \d+ \d+:\d+:\d+ (\w+)")?;
    let pid_re = Regex::new(r"\[\d+\]")?;
    let ip_re = Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")?;
    let ip_hostname_re = Regex::new(r"\b\d{1,3}-\d{1,3}-\d{1,3}-\d{1,3}\.[a-z0-9.-]+\b")?;

    let mut patterns: HashMap<String, (usize, String)> = HashMap::new();

    for log in &logs {
        // Normalize log by replacing ephemeral values
        let mut normalized = log.to_string();

        // Replace timestamp
        normalized = timestamp_re.replace(&normalized, "<TIMESTAMP>").to_string();

        // Replace hostname after timestamp
        if let Some(caps) = hostname_re.captures(log) {
            if let Some(hostname) = caps.get(1) {
                normalized = normalized.replacen(hostname.as_str(), "<HOSTNAME>", 1);
            }
        }

        // Replace PIDs
        normalized = pid_re.replace_all(&normalized, "[<PID>]").to_string();

        // Replace IP-based hostnames
        normalized = ip_hostname_re.replace_all(&normalized, "<HOSTNAME>").to_string();

        // Replace standalone IPs
        normalized = ip_re.replace_all(&normalized, "<IP>").to_string();

        // Track pattern
        let entry = patterns.entry(normalized).or_insert((0, log.to_string()));
        entry.0 += 1;
    }

    let mut sorted_patterns: Vec<_> = patterns.into_iter().collect();
    sorted_patterns.sort_by(|a, b| b.1.0.cmp(&a.1.0));

    println!("   ✅ Found {} unique patterns", sorted_patterns.len());
    println!("   📊 Top 10 patterns:");
    for (i, (pattern, (count, _))) in sorted_patterns.iter().take(10).enumerate() {
        println!("      {}. {} logs - {}", i + 1, count, truncate(&pattern, 70));
    }
    println!();
    println!("{}", "=".repeat(80));
    println!();

    // Step 2: Generate templates for all patterns
    println!("🤖 Step 2: Generating LLM templates...\n");
    println!("   Targeting ALL {} unique patterns\n", sorted_patterns.len());

    let mut templates = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;

    for (idx, (pattern, (count, sample_log))) in sorted_patterns.iter().enumerate() {
        // Extract log type from pattern
        let log_type = extract_log_type(&pattern);

        println!("📝 Template {}/{}: {}", idx + 1, sorted_patterns.len(), log_type);
        println!("   Pattern: {}", truncate(&pattern, 90));
        println!("   Logs: {}", count);

        match generate_ground_truth_template(&llm_client, &sample_log, &log_type, *count).await {
            Ok(template) => {
                println!("   ✅ Generated: {}", truncate(&template.template, 80));
                templates.push(template);
                success_count += 1;
            }
            Err(e) => {
                println!("   ❌ Failed: {}", e);
                fail_count += 1;
            }
        }
        println!();

        // Rate limiting - be aggressive with OpenAI
        if idx < sorted_patterns.len() - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        }

        // Save intermediate results every 50 templates
        if (idx + 1) % 50 == 0 {
            save_templates(&templates, sorted_patterns.len())?;
            println!("   💾 Saved intermediate results ({} templates)\n", templates.len());
        }
    }

    // Final save
    save_templates(&templates, sorted_patterns.len())?;

    println!("{}", "=".repeat(80));
    println!();
    println!("📊 Final Summary:");
    println!("   Total patterns: {}", sorted_patterns.len());
    println!("   Success: {} templates", success_count);
    println!("   Failed: {}", fail_count);
    println!("   Coverage: {}/{} logs ({:.1}%)",
        templates.iter().map(|t| t.count).sum::<usize>(),
        logs.len(),
        (templates.iter().map(|t| t.count).sum::<usize>() as f64 / logs.len() as f64) * 100.0
    );
    println!();
    println!("📁 Saved to: cache/comprehensive_templates.json");

    Ok(())
}

async fn generate_ground_truth_template(
    llm_client: &LLMServiceClient,
    log_line: &str,
    log_type: &str,
    count: usize,
) -> Result<GroundTruthTemplate> {
    let prompt = format!(r#"
Create a GROUND TRUTH template by replacing ONLY ephemeral values with <*>.

PRESERVE STRUCTURE: Keep (), [], :, ;, = and ALL text exactly as-is except ephemeral values.

REPLACE WITH <*>:
- Timestamps: "Jun 14 15:16:01" → "<*>"
- Hostname after timestamp → "<*>"
- PIDs: "[19939]" → "[<*>]"
- IP addresses: "218.188.2.4" or "220-135-151-1.hinet-ip.hinet.net" → "<*>"

KEEP LITERAL:
- Service names: "sshd", "ftpd"
- Subsystems: "(pam_unix)"
- Messages: "authentication failure"
- Field markers: "user=", "uid="
- Parameter VALUES: "user=root", "uid=0" (keep the value!)
- Everything else

EXAMPLES:
IN:  "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=218.188.2.4  user=root"
OUT: "<*> <*> sshd(pam_unix)[<*>]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=<*>  user=root"

RESPOND JSON ONLY:
{{
  "log_type": "{}",
  "template": "template here",
  "parameters": [{{"field": "root", "type": "User"}}]
}}

LOG:
{}
"#, log_type, log_line);

    let response = llm_client.call_openai_simple(&prompt).await?;

    let response = response.trim();
    let json_str = if response.starts_with("```") {
        response.trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim()
    } else {
        response
    };

    let mut parsed: GroundTruthTemplate = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("JSON parse: {}", e))?;

    parsed.example_log = Some(log_line.to_string());
    parsed.description = Some(log_type.to_string());
    parsed.count = count;

    Ok(parsed)
}

fn extract_log_type(pattern: &str) -> String {
    // Extract meaningful service/action from pattern
    let parts: Vec<&str> = pattern.split_whitespace().collect();

    // Skip timestamp and hostname placeholders
    let meaningful = parts.iter()
        .skip(2)
        .take(3)
        .map(|s| *s)
        .collect::<Vec<_>>()
        .join(" ");

    meaningful.chars().take(40).collect()
}

fn save_templates(templates: &[GroundTruthTemplate], total: usize) -> Result<()> {
    let collection = TemplateCollection {
        total_patterns: total,
        templates: templates.to_vec(),
    };

    let output_path = "cache/comprehensive_templates.json";
    fs::write(output_path, serde_json::to_string_pretty(&collection)?)?;
    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
