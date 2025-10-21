/// Fragment-based template generation:
/// 1. Tokenize log into fragments using delimiter regex
/// 2. Ask LLM to classify each fragment (timestamp, IP, number, static_text, etc.)
/// 3. Build regex pattern from classified fragments

use regex::Regex;
use serde::{Deserialize, Serialize};

pub struct FragmentClassifier;

impl FragmentClassifier {
    /// Tokenize a log line into fragments using the delimiter regex
    pub fn tokenize(log_line: &str) -> Vec<String> {
        // Regex to split on delimiters:
        // ://  OR  whitespace/quotes/brackets/etc  OR  period followed by space/end  OR  escaped quotes
        let delimiter_pattern = r#"(?:://)|(?:(?:[\s'";=()\[\]{}?@&<>:\n\t\r,])|(?:\.(\s+|$))|(?:\\["\']))"#;

        let delimiter_re = Regex::new(delimiter_pattern).unwrap();

        let mut fragments = Vec::new();
        let mut last_end = 0;

        for mat in delimiter_re.find_iter(log_line) {
            // Add the text before this delimiter as a fragment
            if mat.start() > last_end {
                let fragment = &log_line[last_end..mat.start()];
                if !fragment.is_empty() {
                    fragments.push(fragment.to_string());
                }
            }
            last_end = mat.end();
        }

        // Add remaining text
        if last_end < log_line.len() {
            let fragment = &log_line[last_end..];
            if !fragment.is_empty() {
                fragments.push(fragment.to_string());
            }
        }

        fragments
    }

    /// Build LLM prompt to classify fragments
    pub fn build_classification_prompt(fragments: &[String], full_log: &str) -> String {
        let fragments_str = fragments.iter()
            .enumerate()
            .map(|(i, f)| format!("  {}: \"{}\"", i, f))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"Classify each fragment from this log line as one of: timestamp, hostname, service, pid, number, ip_address, path, hex, uuid, url, static_text

Full log: {}

Fragments:
{}

Respond with ONLY a JSON array of classifications, one per fragment:
["classification1", "classification2", ...]

Valid classifications:
- timestamp: Date/time values (Jun, 14, 15:16:01, 2023-01-15, etc.)
- hostname: Server/host names (combo, server01, etc.)
- service: Service names (sshd, kernel, nginx, etc.)
- pid: Process IDs (numbers in brackets like [19939])
- number: Generic numbers (123, 456, etc.)
- ip_address: IP addresses (192.168.1.1, etc.)
- path: File paths (/var/log, /etc/config, etc.)
- hex: Hexadecimal values (0x1a2b, deadbeef, etc.)
- uuid: UUIDs (550e8400-e29b-41d4-a716-446655440000, etc.)
- url: URLs (http://example.com, etc.)
- static_text: Fixed keywords that don't change (authentication, failure, ERROR, etc.)

Respond with ONLY the JSON array, no explanation."#,
            full_log,
            fragments_str
        )
    }

    /// Parse LLM classification response
    pub fn parse_classifications(response: &str) -> Result<Vec<FragmentType>, String> {
        // Extract JSON array from response
        let json_start = response.find('[').ok_or("No JSON array found")?;
        let json_end = response.rfind(']').ok_or("No JSON array end found")?;
        let json_str = &response[json_start..=json_end];

        let classifications: Vec<String> = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        classifications
            .iter()
            .map(|s| FragmentType::from_str(s))
            .collect()
    }

    /// Build regex pattern from classified fragments
    pub fn build_pattern(
        fragments: &[String],
        classifications: &[FragmentType],
    ) -> (String, Vec<String>) {
        let mut pattern = String::new();
        let mut variables = Vec::new();
        let mut in_bracket_group = false;

        for (i, (fragment, frag_type)) in fragments.iter().zip(classifications.iter()).enumerate() {
            // Check if we're entering/exiting a bracketed section like [pid]
            if fragment == "[" {
                pattern.push_str(r"\[");
                in_bracket_group = true;
                continue;
            } else if fragment == "]" {
                pattern.push_str(r"\]");
                in_bracket_group = false;
                continue;
            }

            match frag_type {
                FragmentType::Timestamp => {
                    // Handle various timestamp formats
                    if fragment.chars().all(|c| c.is_ascii_alphabetic()) {
                        // Month name (Jun, Jul, etc.)
                        pattern.push_str(r"([A-Z][a-z]{2})");
                        variables.push("month".to_string());
                    } else if fragment.contains(':') {
                        // Time (15:16:01)
                        pattern.push_str(r"(\d{2}:\d{2}:\d{2})");
                        variables.push("time".to_string());
                    } else if fragment.chars().all(|c| c.is_ascii_digit()) {
                        // Day or year
                        pattern.push_str(r"(\d+)");
                        variables.push("timestamp_part".to_string());
                    } else {
                        pattern.push_str(r"(.+?)");
                        variables.push("timestamp".to_string());
                    }
                }
                FragmentType::Hostname => {
                    pattern.push_str(r"([\w\.-]+)");
                    variables.push("hostname".to_string());
                }
                FragmentType::Service => {
                    // Keep service name static (important for matching)
                    pattern.push_str(&regex::escape(fragment));
                }
                FragmentType::Pid => {
                    pattern.push_str(r"(\d+)");
                    variables.push("pid".to_string());
                }
                FragmentType::Number => {
                    pattern.push_str(r"(\d+)");
                    variables.push("number".to_string());
                }
                FragmentType::IPAddress => {
                    pattern.push_str(r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})");
                    variables.push("ip_address".to_string());
                }
                FragmentType::Path => {
                    pattern.push_str(r"([\w/\.-]+)");
                    variables.push("path".to_string());
                }
                FragmentType::Hex => {
                    pattern.push_str(r"(0x[0-9a-fA-F]+|[0-9a-fA-F]+)");
                    variables.push("hex".to_string());
                }
                FragmentType::Uuid => {
                    pattern.push_str(r"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})");
                    variables.push("uuid".to_string());
                }
                FragmentType::Url => {
                    pattern.push_str(r"(https?://[^\s]+)");
                    variables.push("url".to_string());
                }
                FragmentType::StaticText => {
                    // Keep static text as-is (escaped)
                    pattern.push_str(&regex::escape(fragment));
                }
            }

            // Add delimiter pattern between fragments (space by default)
            let is_last = i == fragments.len() - 1;
            if !is_last && !in_bracket_group {
                // Look ahead to see if next fragment needs a specific delimiter
                pattern.push_str(r"\s+");
            }
        }

        (pattern, variables)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentType {
    Timestamp,
    Hostname,
    Service,
    Pid,
    Number,
    IPAddress,
    Path,
    Hex,
    Uuid,
    Url,
    StaticText,
}

impl FragmentType {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "timestamp" => Ok(FragmentType::Timestamp),
            "hostname" => Ok(FragmentType::Hostname),
            "service" => Ok(FragmentType::Service),
            "pid" => Ok(FragmentType::Pid),
            "number" => Ok(FragmentType::Number),
            "ip_address" => Ok(FragmentType::IPAddress),
            "path" => Ok(FragmentType::Path),
            "hex" => Ok(FragmentType::Hex),
            "uuid" => Ok(FragmentType::Uuid),
            "url" => Ok(FragmentType::Url),
            "static_text" => Ok(FragmentType::StaticText),
            _ => Ok(FragmentType::StaticText), // Default to static text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let log = "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure";
        let fragments = FragmentClassifier::tokenize(log);

        println!("Fragments: {:?}", fragments);
        assert!(fragments.contains(&"Jun".to_string()));
        assert!(fragments.contains(&"14".to_string()));
        assert!(fragments.contains(&"combo".to_string()));
        assert!(fragments.contains(&"sshd".to_string()));
    }

    #[test]
    fn test_build_pattern() {
        let fragments = vec![
            "Jun".to_string(),
            "14".to_string(),
            "combo".to_string(),
            "sshd".to_string(),
            "authentication".to_string(),
            "failure".to_string(),
        ];

        let classifications = vec![
            FragmentType::Timestamp,
            FragmentType::Timestamp,
            FragmentType::Hostname,
            FragmentType::Service,
            FragmentType::StaticText,
            FragmentType::StaticText,
        ];

        let (pattern, variables) = FragmentClassifier::build_pattern(&fragments, &classifications);

        println!("Pattern: {}", pattern);
        println!("Variables: {:?}", variables);

        assert!(pattern.contains("sshd")); // Service should be static
        assert!(pattern.contains("authentication")); // Static text
        assert!(variables.contains(&"hostname".to_string()));
    }
}
