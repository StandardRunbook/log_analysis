/// Token Classification for Hierarchical Template Matching
///
/// Classifies tokens into three categories:
/// 1. STATIC: Keywords that define log structure (never change)
/// 2. EPHEMERAL: Values that always change (timestamps, IPs, PIDs)
/// 3. PARAMETER: Business-relevant values that cluster logs (usernames, actions, resources)
///
/// Hierarchy:
/// - Level 1 (Log Type): STATIC keywords only → "auth failure"
/// - Level 2 (Template ID): STATIC + PARAMETER → "auth failure for user=root"
/// - For KL divergence: Track PARAMETER distributions per log type
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenClass {
    /// Static keywords - define log structure
    /// Examples: "authentication", "failure", "sshd", "kernel"
    Static,

    /// Ephemeral values - always change, no semantic meaning for clustering
    /// Examples: timestamps, PIDs, port numbers, IPs
    Ephemeral,

    /// Parameters - business-relevant values used for template clustering
    /// Examples: username, resource_type, action, error_code
    Parameter(ParameterType),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterType {
    /// User-related: username, userid
    User,

    /// Resource-related: filename, table_name, service_name
    Resource,

    /// Action/Result: operation, status, error_code
    Action,

    /// Location: hostname (not IP - IPs are ephemeral)
    Location,

    /// Generic parameter
    Generic,
}

/// Classify a token into STATIC, EPHEMERAL, or PARAMETER
pub fn classify_token(token: &str, context: Option<&str>) -> TokenClass {
    if token.is_empty() {
        return TokenClass::Static;
    }

    // 1. Check if it's a static keyword
    if is_static_keyword(token) {
        return TokenClass::Static;
    }

    // 2. Check if it's ephemeral (timestamps, IPs, PIDs, etc.)
    if is_ephemeral(token) {
        return TokenClass::Ephemeral;
    }

    // 3. Otherwise it's a parameter - classify the type
    let param_type = classify_parameter(token, context);
    TokenClass::Parameter(param_type)
}

/// Check if token is a static keyword that defines log structure
fn is_static_keyword(token: &str) -> bool {
    // Service names
    const SERVICES: &[&str] = &[
        "sshd", "kernel", "cups", "ftpd", "su", "gpm", "systemd",
        "pam_unix", "cron", "nginx", "apache", "mysql", "postgres",
    ];

    // Action verbs
    const ACTIONS: &[&str] = &[
        "authentication", "failure", "success", "opened", "closed",
        "started", "stopped", "connected", "disconnected", "failed",
        "session", "connection", "registered", "unregistered",
    ];

    // Field names (these are structural markers, not values)
    const FIELD_NAMES: &[&str] = &[
        "uid", "euid", "tty", "ruser", "rhost", "logname",
        "pid", "user", "from", "to", "port", "status",
    ];

    let lower = token.to_lowercase();

    SERVICES.iter().any(|&s| lower.contains(s)) ||
    ACTIONS.iter().any(|&a| lower.contains(a)) ||
    FIELD_NAMES.iter().any(|&f| lower == f || lower == format!("{}=", f))
}

/// Check if token is ephemeral (always changes, no clustering value)
fn is_ephemeral(token: &str) -> bool {
    // Pure numbers (PIDs, ports, counts)
    if token.chars().all(|c| c.is_numeric()) {
        return true;
    }

    // IP addresses (v4)
    if Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$").unwrap().is_match(token) {
        return true;
    }

    // IPv6 addresses
    if token.contains("::") || (token.contains(':') && token.chars().filter(|&c| c == ':').count() > 2) {
        return true;
    }

    // Timestamps (HH:MM:SS)
    if Regex::new(r"^\d{2}:\d{2}:\d{2}$").unwrap().is_match(token) {
        return true;
    }

    // Dates (YYYY-MM-DD, MM/DD/YYYY, etc.)
    if Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap().is_match(token) ||
       Regex::new(r"^\d{2}/\d{2}/\d{4}$").unwrap().is_match(token) {
        return true;
    }

    // Months (abbreviated)
    if ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        .contains(&token) {
        return true;
    }

    // UUIDs
    if Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap().is_match(token) {
        return true;
    }

    // Hex numbers (like memory addresses, request IDs)
    if token.starts_with("0x") || (token.len() > 8 && token.chars().all(|c| c.is_ascii_hexdigit())) {
        return true;
    }

    // Very long numbers (timestamps in milliseconds, etc.)
    if token.len() > 10 && token.chars().all(|c| c.is_numeric()) {
        return true;
    }

    false
}

/// Classify what type of parameter this is
fn classify_parameter(token: &str, context: Option<&str>) -> ParameterType {
    let lower = token.to_lowercase();

    // Check context for hints
    if let Some(ctx) = context {
        let ctx_lower = ctx.to_lowercase();

        // User-related
        if ctx_lower.contains("user") || ctx_lower.contains("uid") || ctx_lower.contains("login") {
            return ParameterType::User;
        }

        // Resource-related
        if ctx_lower.contains("file") || ctx_lower.contains("path") || ctx_lower.contains("table") {
            return ParameterType::Resource;
        }

        // Action/Result
        if ctx_lower.contains("status") || ctx_lower.contains("code") || ctx_lower.contains("result") {
            return ParameterType::Action;
        }

        // Location
        if ctx_lower.contains("host") || ctx_lower.contains("server") {
            return ParameterType::Location;
        }
    }

    // Token-based classification
    // User indicators
    if lower.contains("root") || lower.contains("admin") || lower.contains("guest") {
        return ParameterType::User;
    }

    // Hostname (has dots and letters, but not an IP)
    if token.contains('.') && token.chars().any(|c| c.is_alphabetic()) {
        return ParameterType::Location;
    }

    // Path
    if token.starts_with('/') {
        return ParameterType::Resource;
    }

    // Error codes, status codes
    if token.starts_with("ERR") || token.starts_with("OK") || token == "200" || token == "404" || token == "500" {
        return ParameterType::Action;
    }

    // Default: generic parameter
    ParameterType::Generic
}

/// Extract log type signature (STATIC tokens only)
/// This is for Level 1 clustering - finding the log type/structure
pub fn extract_log_type_signature(tokens: &[(&str, TokenClass)]) -> String {
    tokens
        .iter()
        .filter(|(_, class)| matches!(class, TokenClass::Static))
        .map(|(token, _)| *token)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract template signature (STATIC + PARAMETER tokens)
/// This is for Level 2 clustering - finding the specific template variant
pub fn extract_template_signature(tokens: &[(&str, TokenClass)]) -> String {
    tokens
        .iter()
        .filter(|(_, class)| !matches!(class, TokenClass::Ephemeral))
        .map(|(token, class)| {
            match class {
                TokenClass::Static => token.to_string(),
                TokenClass::Ephemeral => "<E>".to_string(), // Should be filtered out
                TokenClass::Parameter(ptype) => format!("<{:?}>", ptype),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_keywords() {
        assert_eq!(classify_token("sshd", None), TokenClass::Static);
        assert_eq!(classify_token("authentication", None), TokenClass::Static);
        assert_eq!(classify_token("failure", None), TokenClass::Static);
        assert_eq!(classify_token("uid=", None), TokenClass::Static);
    }

    #[test]
    fn test_ephemeral() {
        assert_eq!(classify_token("12345", None), TokenClass::Ephemeral); // PID
        assert_eq!(classify_token("192.168.1.1", None), TokenClass::Ephemeral); // IP
        assert_eq!(classify_token("15:30:45", None), TokenClass::Ephemeral); // Time
        assert_eq!(classify_token("Jun", None), TokenClass::Ephemeral); // Month
        assert_eq!(classify_token("550e8400-e29b-41d4-a716-446655440000", None), TokenClass::Ephemeral); // UUID
    }

    #[test]
    fn test_parameters() {
        // User
        assert!(matches!(
            classify_token("root", Some("user=")),
            TokenClass::Parameter(ParameterType::User)
        ));

        // Location (hostname)
        assert!(matches!(
            classify_token("example.com", None),
            TokenClass::Parameter(ParameterType::Location)
        ));

        // Resource (path)
        assert!(matches!(
            classify_token("/var/log", None),
            TokenClass::Parameter(ParameterType::Resource)
        ));
    }

    #[test]
    fn test_log_type_signature() {
        let tokens = vec![
            ("Jun", TokenClass::Ephemeral),
            ("15", TokenClass::Ephemeral),
            ("sshd", TokenClass::Static),
            ("12345", TokenClass::Ephemeral),
            ("authentication", TokenClass::Static),
            ("failure", TokenClass::Static),
            ("root", TokenClass::Parameter(ParameterType::User)),
        ];

        let signature = extract_log_type_signature(&tokens);
        assert_eq!(signature, "sshd authentication failure");
    }

    #[test]
    fn test_template_signature() {
        let tokens = vec![
            ("Jun", TokenClass::Ephemeral),
            ("15", TokenClass::Ephemeral),
            ("sshd", TokenClass::Static),
            ("12345", TokenClass::Ephemeral),
            ("authentication", TokenClass::Static),
            ("failure", TokenClass::Static),
            ("root", TokenClass::Parameter(ParameterType::User)),
            ("example.com", TokenClass::Parameter(ParameterType::Location)),
        ];

        let signature = extract_template_signature(&tokens);
        assert_eq!(signature, "sshd authentication failure <User> <Location>");
    }
}
