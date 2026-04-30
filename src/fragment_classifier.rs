//! Fragment-based template generation:
//! 1. Tokenize log into fragments using delimiter regex
//! 2. Ask LLM to classify each fragment (timestamp, IP, number, static_text, etc.)
//! 3. Build regex pattern from classified fragments

use regex::Regex;
use serde::{Deserialize, Serialize};

pub struct FragmentClassifier;

impl FragmentClassifier {
    /// Tokenize a log line into fragments using the delimiter regex
    pub fn tokenize(log_line: &str) -> Vec<String> {
        // Regex to split on delimiters:
        // ://  OR  whitespace/quotes/brackets/etc  OR  period followed by space/end  OR  escaped quotes
        let delimiter_pattern =
            r#"(?:://)|(?:(?:[\s'";=()\[\]{}?@&<>:\n\t\r,])|(?:\.(\s+|$))|(?:\\["\']))"#;

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
        let fragments_str = fragments
            .iter()
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
            full_log, fragments_str
        )
    }

    /// Parse LLM classification response
    pub fn parse_classifications(response: &str) -> Result<Vec<FragmentType>, String> {
        // Extract JSON array from response
        let json_start = response.find('[').ok_or("No JSON array found")?;
        let json_end = response.rfind(']').ok_or("No JSON array end found")?;
        let json_str = &response[json_start..=json_end];

        let classifications: Vec<String> =
            serde_json::from_str(json_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

        classifications
            .iter()
            .map(|s| s.parse::<FragmentType>())
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
                    pattern.push_str(
                        r"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})",
                    );
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

impl std::str::FromStr for FragmentType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
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

    #[test]
    fn test_tokenize_empty_and_whitespace() {
        assert_eq!(FragmentClassifier::tokenize(""), Vec::<String>::new());
        assert_eq!(FragmentClassifier::tokenize("   "), Vec::<String>::new());
        // Single token, no delimiters: comes back as a single fragment.
        assert_eq!(FragmentClassifier::tokenize("hello"), vec!["hello"]);
    }

    #[test]
    fn test_build_classification_prompt_includes_fragments_and_log() {
        let fragments = vec!["Jun".into(), "14".into()];
        let prompt =
            FragmentClassifier::build_classification_prompt(&fragments, "Jun 14 something");
        // Prompt must include each fragment indexed, the full log line for
        // context, and the canonical classification taxonomy. Otherwise
        // the LLM has nothing to anchor on.
        assert!(prompt.contains("\"Jun\""));
        assert!(prompt.contains("\"14\""));
        assert!(prompt.contains("Jun 14 something"));
        assert!(prompt.contains("timestamp"));
        assert!(prompt.contains("ip_address"));
        assert!(prompt.contains("uuid"));
    }

    #[test]
    fn test_parse_classifications_happy_path() {
        let response = r#"["timestamp", "static_text", "ip_address", "uuid"]"#;
        let parsed = FragmentClassifier::parse_classifications(response).unwrap();
        assert_eq!(parsed.len(), 4);
        assert!(matches!(parsed[0], FragmentType::Timestamp));
        assert!(matches!(parsed[1], FragmentType::StaticText));
        assert!(matches!(parsed[2], FragmentType::IPAddress));
        assert!(matches!(parsed[3], FragmentType::Uuid));
    }

    #[test]
    fn test_parse_classifications_tolerates_surrounding_prose() {
        // Real LLM responses sometimes wrap the JSON in commentary; we
        // only require that the array is anywhere in the response.
        let response =
            r#"Sure! Here's the classification: ["number", "hex"] — let me know if you need more."#;
        let parsed = FragmentClassifier::parse_classifications(response).unwrap();
        assert!(matches!(parsed[0], FragmentType::Number));
        assert!(matches!(parsed[1], FragmentType::Hex));
    }

    #[test]
    fn test_parse_classifications_unknown_falls_back_to_static_text() {
        // The LLM occasionally invents new categories. We default to
        // static_text rather than failing the whole record — the worst
        // case is over-strict matching, not data loss.
        let response = r#"["weird_category"]"#;
        let parsed = FragmentClassifier::parse_classifications(response).unwrap();
        assert!(matches!(parsed[0], FragmentType::StaticText));
    }

    #[test]
    fn test_parse_classifications_rejects_non_array_response() {
        assert!(FragmentClassifier::parse_classifications("not json at all").is_err());
        assert!(FragmentClassifier::parse_classifications("{\"k\": \"v\"}").is_err());
    }

    #[test]
    fn test_build_pattern_each_variable_type_emits_named_capture() {
        // Drive every FragmentType variant through build_pattern and
        // assert the pattern string + variables list both reflect it.
        // This guards the variant-matrix in build_pattern from silently
        // drifting when a new variant is added.
        //
        // FragmentType::Timestamp picks one of {month, time,
        // timestamp_part, timestamp} depending on the fragment shape.
        // Mixed alphanumeric "frag0" hits the catch-all → "timestamp".
        let fragments: Vec<String> = (0..10).map(|i| format!("frag{}", i)).collect();
        let classifications = vec![
            FragmentType::Timestamp,
            FragmentType::Hostname,
            FragmentType::Service,
            FragmentType::Pid,
            FragmentType::Number,
            FragmentType::IPAddress,
            FragmentType::Path,
            FragmentType::Hex,
            FragmentType::Uuid,
            FragmentType::Url,
        ];
        let (pattern, variables) = FragmentClassifier::build_pattern(&fragments, &classifications);
        assert!(pattern.contains("frag2")); // Service is static literal
        for expected in [
            "timestamp",
            "hostname",
            "pid",
            "number",
            "ip_address",
            "path",
            "hex",
            "uuid",
            "url",
        ] {
            assert!(
                variables
                    .iter()
                    .any(|v| v == expected || v.starts_with(&format!("{expected}_"))),
                "expected a variable named {expected:?} in {variables:?}"
            );
        }
    }

    #[test]
    fn test_build_pattern_timestamp_subforms() {
        // The Timestamp branch picks one of four sub-forms by inspecting
        // the fragment text: month-name, time, digit-only, or generic.
        let cases: &[(&str, &str)] = &[
            ("Jun", "month"),
            ("15:30:45", "time"),
            ("2024", "timestamp_part"),
            ("2024-01-15", "timestamp"), // mixed → catch-all
        ];
        for (frag, expected_var) in cases {
            let (_, vars) =
                FragmentClassifier::build_pattern(&[frag.to_string()], &[FragmentType::Timestamp]);
            assert_eq!(vars, vec![expected_var.to_string()], "for {frag:?}");
        }
    }

    #[test]
    fn test_build_pattern_brackets_escape_around_pid() {
        // The bracket-detection special case: '[' and ']' fragments are
        // emitted as escaped literals and the in-between fragment becomes
        // a capture. Common syslog shape: "sshd[19939]".
        let (pattern, variables) = FragmentClassifier::build_pattern(
            &["[".to_string(), "19939".to_string(), "]".to_string()],
            &[
                FragmentType::StaticText,
                FragmentType::Pid,
                FragmentType::StaticText,
            ],
        );
        assert!(pattern.contains(r"\["));
        assert!(pattern.contains(r"\]"));
        assert_eq!(variables, vec!["pid".to_string()]);
    }

    #[test]
    fn test_fragment_type_from_str_round_trip() {
        // Case-insensitive parser — every canonical lowercase name must
        // produce the matching variant. Any unknown string falls back
        // to StaticText.
        let pairs: &[(&str, FragmentType)] = &[
            ("timestamp", FragmentType::Timestamp),
            ("HOSTNAME", FragmentType::Hostname),
            ("Service", FragmentType::Service),
            ("pid", FragmentType::Pid),
            ("number", FragmentType::Number),
            ("ip_address", FragmentType::IPAddress),
            ("path", FragmentType::Path),
            ("hex", FragmentType::Hex),
            ("uuid", FragmentType::Uuid),
            ("url", FragmentType::Url),
            ("static_text", FragmentType::StaticText),
            ("nonsense", FragmentType::StaticText),
        ];
        for (input, expected) in pairs {
            let got: FragmentType = input.parse().unwrap();
            assert!(
                std::mem::discriminant(&got) == std::mem::discriminant(expected),
                "{input:?} parsed to wrong variant"
            );
        }
    }
}
