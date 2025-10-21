use log_analyzer::log_matcher::LogTemplate;
use regex::Regex;
use std::collections::HashMap;
use std::fs;

fn main() -> anyhow::Result<()> {
    let datasets = vec![
        ("Linux", "data/loghub/Linux/Linux_2k.log_templates.csv", "data/loghub/Linux/Linux_2k.log_structured.csv"),
        ("Mac", "data/loghub/Mac/Mac_2k.log_templates.csv", "data/loghub/Mac/Mac_2k.log_structured.csv"),
        ("Thunderbird", "data/loghub/Thunderbird/Thunderbird_2k.log_templates.csv", "data/loghub/Thunderbird/Thunderbird_2k.log_structured.csv"),
    ];

    for (dataset_name, template_csv_path, structured_csv_path) in datasets {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📊 Building templates: {}", dataset_name);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let template_csv_content = match fs::read_to_string(template_csv_path) {
            Ok(content) => content,
            Err(_) => {
                println!("   ⚠️  Skipping {} (template file not found)", dataset_name);
                println!();
                continue;
            }
        };

        let structured_csv_content = match fs::read_to_string(structured_csv_path) {
            Ok(content) => content,
            Err(_) => {
                println!("   ⚠️  Skipping {} (structured file not found)", dataset_name);
                println!();
                continue;
            }
        };

        // Build map of EventId -> example log line
        let mut event_examples: HashMap<String, String> = HashMap::new();
        for line in structured_csv_content.lines().skip(1) {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() >= 9 {
                let event_id = fields[8].trim().to_string();
                if !event_examples.contains_key(&event_id) {
                    // Reconstruct the full log line from fields
                    // Format: Month Date Time Component Content
                    let example = format!("{} {} {} {} {}",
                        fields[1], fields[2], fields[3], fields[5], fields[7]);
                    event_examples.insert(event_id, example);
                }
            }
        }

        let mut templates = Vec::new();
        let mut template_id = 1u64;

        for line in template_csv_content.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.splitn(2, ',').collect();
            if parts.len() != 2 {
                continue;
            }

            let event_id = parts[0].trim();
            let drain_template = parts[1].trim();

            // Convert Drain template to regex pattern
            let regex_pattern = drain_template_to_regex(drain_template);

            // Validate regex
            if Regex::new(&regex_pattern).is_err() {
                eprintln!("   ⚠️  Invalid regex for {}: {}", event_id, regex_pattern);
                continue;
            }

            let example = event_examples
                .get(event_id)
                .cloned()
                .unwrap_or_else(|| drain_template.to_string());

            let template = LogTemplate {
                template_id,
                pattern: regex_pattern,
                variables: extract_variable_names(drain_template),
                example,
            };

            templates.push(template);
            template_id += 1;
        }

        println!("   Converted {} templates", templates.len());

        // Save to cache
        let cache_file = format!("cache/{}_templates.json", dataset_name.to_lowercase());

        // Backup old file
        if std::path::Path::new(&cache_file).exists() {
            let backup_file = format!("{}.old", cache_file);
            fs::copy(&cache_file, &backup_file)?;
        }

        let state = serde_json::json!({
            "templates": templates,
            "next_template_id": template_id
        });

        fs::write(&cache_file, serde_json::to_string_pretty(&state)?)?;
        println!("   ✓ Saved to {}", cache_file);
        println!();
    }

    println!("✅ Template conversion complete!");
    println!();
    println!("Run benchmark to test:");
    println!("  cargo test --test benchmark_llm_templates --release -- --nocapture");

    Ok(())
}

/// Convert Drain template format (with <*> wildcards) to regex pattern
/// Includes a flexible header pattern to match log timestamp, host, and service
fn drain_template_to_regex(drain_template: &str) -> String {
    // Flexible log header pattern that matches most syslog-style logs
    // Matches: "Month Day HH:MM:SS hostname service[pid]: "
    // Using .+? to match timestamp/hostname/service flexibly
    let header_pattern = r".+?\s+.+?\s+.+?\s+.+?\s+.+?\[\d+\]:\s+";

    let mut pattern = String::from(header_pattern);
    let mut chars = drain_template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' && chars.peek() == Some(&'*') {
            // Found <*> wildcard
            chars.next(); // consume '*'
            if chars.peek() == Some(&'>') {
                chars.next(); // consume '>'

                // Check if next character is a space or end - if so, use \S+ otherwise use .+?
                if chars.peek().map_or(true, |&c| c.is_whitespace()) {
                    pattern.push_str(r"(\S+)");
                } else {
                    pattern.push_str(r"(.+?)");
                }
            } else {
                // Not a complete <*>, treat literally
                pattern.push_str(&regex::escape("<*"));
            }
        } else {
            // Escape special regex characters
            match ch {
                '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' => {
                    pattern.push('\\');
                    pattern.push(ch);
                }
                _ => pattern.push(ch),
            }
        }
    }

    pattern
}

/// Extract variable names based on position of wildcards
fn extract_variable_names(drain_template: &str) -> Vec<String> {
    let wildcard_count = drain_template.matches("<*>").count();
    (1..=wildcard_count)
        .map(|i| format!("var{}", i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drain_to_regex() {
        assert_eq!(
            drain_template_to_regex("User <*> logged in"),
            "User (.+?) logged in"
        );

        assert_eq!(
            drain_template_to_regex("*** info [mice.c(<*>)]:"),
            "\\*\\*\\* info \\[mice\\.c\\((.+?)\\)\\]:"
        );

        assert_eq!(
            drain_template_to_regex("ACPI disabled because your bios is from <*> and too old"),
            "ACPI disabled because your bios is from (.+?) and too old"
        );
    }
}
