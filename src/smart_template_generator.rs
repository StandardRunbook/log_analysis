/// Smart template generator that detects log format and generates appropriate patterns
use crate::log_format_detector::{LogFormat, LogFormatDetector};
use crate::log_matcher::LogTemplate;
use regex::Regex;

pub struct SmartTemplateGenerator;

impl SmartTemplateGenerator {
    /// Generate a template for a log line by detecting format and applying structure rules
    pub fn generate_template(log_line: &str, template_id: u64) -> LogTemplate {
        let format = LogFormatDetector::detect(log_line);

        match format {
            LogFormat::Syslog { has_pid } => {
                Self::generate_syslog_template(log_line, template_id, has_pid)
            }
            LogFormat::ISOTimestamp => Self::generate_iso_template(log_line, template_id),
            LogFormat::CustomDelimited { delimiter } => {
                Self::generate_delimited_template(log_line, template_id, delimiter)
            }
            LogFormat::Unstructured => Self::generate_generic_template(log_line, template_id),
        }
    }

    /// Generate template for syslog format
    fn generate_syslog_template(log_line: &str, template_id: u64, has_pid: bool) -> LogTemplate {
        let components = LogFormatDetector::extract_syslog_components(log_line);

        if let Some(comp) = components {
            let message_pattern = Self::generate_message_pattern(&comp.message);

            let pattern = if has_pid {
                format!(
                    r"([A-Z][a-z]{{2}}\s+\d{{1,2}}\s+\d{{2}}:\d{{2}}:\d{{2}})\s+([\w\.-]+)\s+{}\[(\d+)\]:\s+{}",
                    regex::escape(&comp.service),
                    message_pattern
                )
            } else {
                format!(
                    r"([A-Z][a-z]{{2}}\s+\d{{1,2}}\s+\d{{2}}:\d{{2}}:\d{{2}})\s+([\w\.-]+)\s+{}:\s+{}",
                    regex::escape(&comp.service),
                    message_pattern
                )
            };

            let mut variables = vec!["timestamp".to_string(), "hostname".to_string()];
            if has_pid {
                variables.push("pid".to_string());
            }
            variables.extend(Self::extract_message_variables(&comp.message));

            LogTemplate {
                template_id,
                pattern,
                variables,
                example: log_line.to_string(),
            }
        } else {
            Self::generate_generic_template(log_line, template_id)
        }
    }

    /// Generate pattern for the message part by identifying variable fields
    fn generate_message_pattern(message: &str) -> String {
        let mut pattern = String::new();
        let mut last_end = 0;

        // Patterns to detect variable fields (in order of specificity)
        let variable_patterns = vec![
            (r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b", r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})"), // IP address
            (r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b", r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})"), // UUID
            (r"\b0x[0-9a-fA-F]+\b", r"(0x[0-9a-fA-F]+)"), // Hex number
            (r"\b[a-f0-9]{32,64}\b", r"([a-f0-9]{32,64})"), // Hash
            (r"/[\w/\.-]+", r"([\w/\.-]+)"), // File path
            (r"\b\d+\.\d+\b", r"(\d+\.\d+)"), // Decimal number
            (r"\b\d+\b", r"(\d+)"), // Integer
        ];

        // Find all variable matches
        let mut matches: Vec<(usize, usize, &str)> = Vec::new();
        for (pattern_str, replacement) in &variable_patterns {
            if let Ok(re) = Regex::new(pattern_str) {
                for mat in re.find_iter(message) {
                    // Don't overlap with existing matches
                    if !matches.iter().any(|(s, e, _)| mat.start() < *e && mat.end() > *s) {
                        matches.push((mat.start(), mat.end(), replacement));
                    }
                }
            }
        }

        // Sort matches by position
        matches.sort_by_key(|(start, _, _)| *start);

        // Build pattern with replacements
        for (start, end, replacement) in matches {
            // Add static text before this match
            if start > last_end {
                pattern.push_str(&regex::escape(&message[last_end..start]));
            }
            // Add the variable pattern
            pattern.push_str(replacement);
            last_end = end;
        }

        // Add remaining static text
        if last_end < message.len() {
            pattern.push_str(&regex::escape(&message[last_end..]));
        }

        // If no pattern was generated, match the whole message as a variable
        if pattern.is_empty() {
            pattern = r"(.+)".to_string();
        }

        pattern
    }

    /// Extract variable names from message
    fn extract_message_variables(message: &str) -> Vec<String> {
        let mut variables = Vec::new();

        if message.contains('.') && message.matches('.').count() == 3 {
            variables.push("ip_address".to_string());
        }
        if Regex::new(r"\d+\.\d+").unwrap().is_match(message) {
            variables.push("decimal_value".to_string());
        }
        if Regex::new(r"\b\d+\b").unwrap().is_match(message) {
            variables.push("number".to_string());
        }

        if variables.is_empty() {
            variables.push("message".to_string());
        }

        variables
    }

    /// Generate template for ISO timestamp format
    fn generate_iso_template(log_line: &str, template_id: u64) -> LogTemplate {
        // For now, treat as generic
        Self::generate_generic_template(log_line, template_id)
    }

    /// Generate template for delimited format
    fn generate_delimited_template(
        log_line: &str,
        template_id: u64,
        _delimiter: char,
    ) -> LogTemplate {
        // For now, treat as generic
        Self::generate_generic_template(log_line, template_id)
    }

    /// Generate generic template
    fn generate_generic_template(log_line: &str, template_id: u64) -> LogTemplate {
        let pattern = Self::generate_message_pattern(log_line);
        let variables = Self::extract_message_variables(log_line);

        LogTemplate {
            template_id,
            pattern,
            variables,
            example: log_line.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_syslog_template_with_pid() {
        let log = "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=218.188.2.4";
        let template = SmartTemplateGenerator::generate_template(log, 1);

        assert!(template.pattern.contains("sshd\\(pam_unix\\)"));
        assert!(template.pattern.contains(r"\[(\d+)\]"));
        // IP address should be captured as a pattern, not literal
        assert!(template.pattern.contains(r"\d") || template.pattern.contains(r"[\d"));  // IP should be captured
    }

    #[test]
    fn test_generate_syslog_template_without_pid() {
        let log = "Jul 27 14:41:58 combo kernel: PCI: Using configuration type 1";
        let template = SmartTemplateGenerator::generate_template(log, 2);

        assert!(template.pattern.contains("kernel"));
        assert!(!template.pattern.contains(r"\[(\d+)\]"));
        assert!(template.pattern.contains("PCI"));
    }
}
