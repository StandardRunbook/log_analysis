use radix_trie::{Trie, TrieCommon};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTemplate {
    pub template_id: u64,
    pub pattern: String,
    pub variables: Vec<String>,
    pub example: String,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub matched: bool,
    pub template_id: Option<u64>,
    pub extracted_values: HashMap<String, String>,
}

pub struct LogMatcher {
    // Radix trie for fast prefix matching of log templates
    trie: Arc<RwLock<Trie<String, LogTemplate>>>,
    // Compiled regex patterns for each template
    patterns: Arc<RwLock<HashMap<u64, Regex>>>,
    // Auto-incrementing template ID counter
    next_template_id: Arc<AtomicU64>,
}

impl LogMatcher {
    pub fn new() -> Self {
        let mut matcher = Self {
            trie: Arc::new(RwLock::new(Trie::new())),
            patterns: Arc::new(RwLock::new(HashMap::new())),
            next_template_id: Arc::new(AtomicU64::new(1)), // Start from 1
        };

        // Initialize with some default templates
        matcher.add_default_templates();
        matcher
    }

    /// Generate next template ID
    fn next_id(&self) -> u64 {
        self.next_template_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Add default log templates to the trie
    fn add_default_templates(&mut self) {
        let default_templates = vec![
            LogTemplate {
                template_id: self.next_id(),
                pattern: r"cpu_usage: (\d+\.\d+)% - (.*)".to_string(),
                variables: vec!["percentage".to_string(), "message".to_string()],
                example: "cpu_usage: 45.2% - Server load normal".to_string(),
            },
            LogTemplate {
                template_id: self.next_id(),
                pattern: r"memory_usage: (\d+\.\d+)GB - (.*)".to_string(),
                variables: vec!["amount".to_string(), "message".to_string()],
                example: "memory_usage: 2.5GB - Memory consumption stable".to_string(),
            },
            LogTemplate {
                template_id: self.next_id(),
                pattern: r"disk_io: (\d+)MB/s - (.*)".to_string(),
                variables: vec!["throughput".to_string(), "message".to_string()],
                example: "disk_io: 250MB/s - Disk activity moderate".to_string(),
            },
        ];

        for template in default_templates {
            self.add_template(template);
        }
    }

    /// Add a new template to the matcher
    pub fn add_template(&mut self, mut template: LogTemplate) {
        // Assign a unique ID if it's 0 (placeholder from LLM)
        if template.template_id == 0 {
            template.template_id = self.next_id();
        }

        let template_id = template.template_id;

        // Extract prefix for radix trie (use first few characters before variables)
        let prefix = self.extract_prefix(&template.pattern);

        // Compile regex pattern
        if let Ok(regex) = Regex::new(&template.pattern) {
            self.patterns.write().unwrap().insert(template_id, regex);
        }

        // Add to trie
        self.trie.write().unwrap().insert(prefix, template);

        tracing::debug!("Added template: {}", template_id);
    }

    /// Extract a static prefix from a pattern for trie indexing
    fn extract_prefix(&self, pattern: &str) -> String {
        // Take characters up to the first regex metacharacter or variable
        pattern
            .chars()
            .take_while(|c| !matches!(c, '(' | '[' | '.' | '*' | '+' | '?' | '\\'))
            .collect()
    }

    /// Try to match a log line against known templates
    pub fn match_log(&self, log_line: &str) -> MatchResult {
        // First, try to find candidate templates using the trie
        let candidates = self.find_candidate_templates(log_line);

        // Try to match against each candidate
        for template in candidates {
            if let Some(regex) = self.patterns.read().unwrap().get(&template.template_id) {
                if let Some(captures) = regex.captures(log_line) {
                    let mut extracted_values = HashMap::new();

                    // Extract variable values
                    for (i, var_name) in template.variables.iter().enumerate() {
                        if let Some(value) = captures.get(i + 1) {
                            extracted_values.insert(var_name.clone(), value.as_str().to_string());
                        }
                    }

                    tracing::debug!("Matched log with template: {}", template.template_id);

                    return MatchResult {
                        matched: true,
                        template_id: Some(template.template_id.clone()),
                        extracted_values,
                    };
                }
            }
        }

        tracing::debug!("No template match found for log: {}", log_line);

        MatchResult {
            matched: false,
            template_id: None,
            extracted_values: HashMap::new(),
        }
    }

    /// Find candidate templates using radix trie prefix matching
    fn find_candidate_templates(&self, log_line: &str) -> Vec<LogTemplate> {
        let trie = self.trie.read().unwrap();
        let mut candidates = Vec::new();

        // Try different prefix lengths
        for len in (5..=log_line.len().min(30)).rev() {
            let prefix = &log_line[..len];

            // Get all templates with this prefix or shorter
            if let Some(template) = trie.get(prefix) {
                candidates.push(template.clone());
            }

            // Also check subtrie for partial matches
            let subtrie = trie.get_raw_descendant(prefix);
            if let Some(st) = subtrie {
                for (_, template) in st.iter() {
                    candidates.push(template.clone());
                }
            }
        }

        // If no prefix match, return all templates (fallback to brute force)
        if candidates.is_empty() {
            for (_, template) in trie.iter() {
                candidates.push(template.clone());
            }
        }

        candidates
    }

    /// Get all templates for inspection
    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.trie
            .read()
            .unwrap()
            .iter()
            .map(|(_, template)| template.clone())
            .collect()
    }
}

impl Default for LogMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_matching() {
        let matcher = LogMatcher::new();

        let log = "cpu_usage: 67.8% - Server load increased";
        let result = matcher.match_log(log);

        assert!(result.matched);
        assert_eq!(result.template_id, Some(1)); // First template ID
        assert_eq!(
            result.extracted_values.get("percentage"),
            Some(&"67.8".to_string())
        );
    }

    #[test]
    fn test_no_match() {
        let matcher = LogMatcher::new();

        let log = "unknown_format: this is a new log format";
        let result = matcher.match_log(log);

        assert!(!result.matched);
        assert_eq!(result.template_id, None);
    }
}
