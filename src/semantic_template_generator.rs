/// Semantic Template Generator - LLM generates structure, regex extracts parameters
///
/// Philosophy:
/// - LLM: Identifies log TYPE/structure (e.g., "authentication failure" vs "session opened")
/// - Tokenization: Extracts PARAMETER values (e.g., username="root", ip="192.168.1.1")
///
/// This hybrid approach:
/// - Avoids value-specific template explosion (user=root vs user=guest = same template)
/// - Enables parameter distribution tracking for KL divergence
/// - Uses LLM for semantic understanding, regex for fast extraction
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A semantic template captures log STRUCTURE, not specific values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticTemplate {
    pub template_id: u64,

    /// The structural pattern - describes what makes this log type unique
    /// Example: "authentication failure for user via ssh"
    pub description: String,

    /// Static keywords that identify this log type
    /// Example: ["authentication", "failure", "sshd", "pam_unix"]
    pub identifying_keywords: Vec<String>,

    /// Named parameters that vary within this log type
    /// Example: ["username", "hostname", "ip_address", "pid"]
    pub parameters: Vec<String>,

    /// Example log for this template
    pub example: String,

    /// Regex pattern for matching (optional, can be generated from keywords + tokenization)
    pub pattern: Option<String>,
}

/// A matched log with extracted parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMatch {
    pub template_id: u64,

    /// Extracted parameter values
    /// Example: {"username": "root", "ip_address": "192.168.1.1", "pid": "12345"}
    pub parameters: HashMap<String, String>,

    /// Match confidence (0.0 to 1.0)
    pub confidence: f64,
}

/// Tokenization regex - splits log into tokens
static TOKENIZER: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
    Regex::new(r#"(?:://)|(?:(?:[\s'`";=()\[\]{}?@&<>:\n\t\r,])|(?:[\.](\s+|$))|(?:\\["']))+"#).unwrap()
});

/// Tokenize a log line into components
pub fn tokenize(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut last_end = 0;

    for mat in TOKENIZER.find_iter(text) {
        if mat.start() > last_end {
            tokens.push(&text[last_end..mat.start()]);
        }
        last_end = mat.end();
    }

    if last_end < text.len() {
        tokens.push(&text[last_end..]);
    }

    tokens
}

/// Classify tokens into static (keywords) vs dynamic (parameters)
pub fn classify_tokens(tokens: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut keywords = Vec::new();
    let mut parameters = Vec::new();

    for token in tokens {
        if is_static_keyword(token) {
            keywords.push(token.to_string());
        } else if is_likely_parameter(token) {
            // Classify what TYPE of parameter this is
            let param_type = infer_parameter_type(token);
            if !parameters.contains(&param_type) {
                parameters.push(param_type);
            }
        }
    }

    (keywords, parameters)
}

/// Check if a token is a static keyword (unlikely to be a parameter)
fn is_static_keyword(token: &str) -> bool {
    // Service names, log levels, field names, common verbs
    let static_patterns = [
        "authentication", "failure", "success", "opened", "closed",
        "sshd", "kernel", "cups", "ftpd", "su", "gpm",
        "pam_unix", "session", "connection", "from", "for",
        "uid", "euid", "tty", "ruser", "rhost", "user", "logname",
        "INFO", "ERROR", "WARN", "DEBUG",
    ];

    static_patterns.iter().any(|&pat| token.contains(pat))
}

/// Check if a token looks like a parameter value
fn is_likely_parameter(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }

    // Numbers, IPs, paths, usernames, etc.
    token.chars().any(|c| c.is_numeric()) ||
    token.contains('.') ||
    token.contains('/') ||
    token.contains('-') ||
    token.len() > 2  // Short tokens like "at", "by" are likely keywords
}

/// Infer what type of parameter this token represents
fn infer_parameter_type(token: &str) -> String {
    // IP address
    if Regex::new(r"^\d+\.\d+\.\d+\.\d+$").unwrap().is_match(token) {
        return "ip_address".to_string();
    }

    // Hostname with dots
    if token.contains('.') && token.chars().any(|c| c.is_alphabetic()) {
        return "hostname".to_string();
    }

    // Pure number (could be PID, UID, port, etc.)
    if token.chars().all(|c| c.is_numeric()) {
        return "number".to_string();
    }

    // Path
    if token.starts_with('/') {
        return "path".to_string();
    }

    // Timestamp patterns
    if Regex::new(r"^\d{2}:\d{2}:\d{2}$").unwrap().is_match(token) {
        return "time".to_string();
    }

    // Month
    if ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        .contains(&token) {
        return "month".to_string();
    }

    // Default: generic value
    "value".to_string()
}

/// Generate a semantic template from a log line using LLM
pub async fn generate_semantic_template(
    log_line: &str,
    _llm_client: &crate::llm_service::LLMServiceClient,
) -> Result<SemanticTemplate> {
    // First, tokenize to understand structure
    let tokens = tokenize(log_line);
    let (keywords, param_types) = classify_tokens(&tokens);

    // Build LLM prompt focused on SEMANTIC STRUCTURE
    let _prompt = format!(
        r#"Analyze this log line and describe its SEMANTIC STRUCTURE (not specific values):

LOG: {log_line}

Tokenized keywords: {keywords:?}
Detected parameter types: {param_types:?}

Your task:
1. Describe what TYPE of log this is in 5-10 words (e.g., "SSH authentication failure attempt")
2. List the STATIC KEYWORDS that identify this log type (not values like "root" or "192.168.1.1")
3. List the PARAMETER TYPES that vary (e.g., "username", "ip_address", not specific values)

CRITICAL: Do NOT create separate templates for different parameter VALUES.
- "user=root" and "user=guest" = SAME template, parameter "username"
- "rhost=192.168.1.1" and "rhost=example.com" = SAME template, parameter "host"

Respond ONLY with JSON:
{{
  "description": "brief description of log type",
  "keywords": ["keyword1", "keyword2", ...],
  "parameters": ["param_type1", "param_type2", ...]
}}
"#,
        log_line = log_line,
        keywords = keywords,
        param_types = param_types,
    );

    // Call LLM (simplified - would use actual LLM client)
    // For now, return a template based on tokenization
    Ok(SemanticTemplate {
        template_id: 0,
        description: "Generated from tokenization".to_string(),
        identifying_keywords: keywords,
        parameters: param_types,
        example: log_line.to_string(),
        pattern: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let log = "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; user=root";
        let tokens = tokenize(log);

        // Should split on spaces, brackets, colons, etc.
        assert!(tokens.contains(&"Jun"));
        assert!(tokens.contains(&"sshd"));
        assert!(tokens.contains(&"authentication"));
        assert!(tokens.contains(&"root"));
    }

    #[test]
    fn test_classify_tokens() {
        let tokens = vec!["sshd", "authentication", "failure", "192.168.1.1", "root", "12345"];
        let (keywords, params) = classify_tokens(&tokens);

        // Keywords: sshd, authentication, failure
        assert!(keywords.contains(&"sshd".to_string()));
        assert!(keywords.contains(&"authentication".to_string()));

        // Parameters: ip_address, value, number
        assert!(params.contains(&"ip_address".to_string()));
    }

    #[test]
    fn test_infer_parameter_type() {
        assert_eq!(infer_parameter_type("192.168.1.1"), "ip_address");
        assert_eq!(infer_parameter_type("example.com"), "hostname");
        assert_eq!(infer_parameter_type("12345"), "number");
        assert_eq!(infer_parameter_type("/var/log"), "path");
        assert_eq!(infer_parameter_type("15:16:01"), "time");
        assert_eq!(infer_parameter_type("Jun"), "month");
    }
}
