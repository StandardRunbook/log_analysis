// Lock-free version of LogMatcher for single-threaded benchmark tests
// This version removes Arc<RwLock<>> overhead for better performance measurement

use radix_trie::{Trie, TrieCommon};
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone)]
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

pub struct LockFreeLogMatcher {
    // Radix trie for fast prefix matching - no Arc/RwLock needed for tests
    trie: Trie<String, LogTemplate>,
    // Compiled regex patterns for each template
    patterns: HashMap<u64, Regex>,
    // Auto-incrementing template ID counter
    next_template_id: u64,
}

impl LockFreeLogMatcher {
    pub fn new() -> Self {
        let mut matcher = Self {
            trie: Trie::new(),
            patterns: HashMap::new(),
            next_template_id: 1,
        };

        // Initialize with some default templates
        matcher.add_default_templates();
        matcher
    }

    /// Generate next template ID
    fn next_id(&mut self) -> u64 {
        let id = self.next_template_id;
        self.next_template_id += 1;
        id
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
        // Assign a unique ID if it's 0 (placeholder)
        if template.template_id == 0 {
            template.template_id = self.next_id();
        }

        let template_id = template.template_id;

        // Extract prefix for radix trie (use first few characters before variables)
        let prefix = self.extract_prefix(&template.pattern);

        // Compile regex pattern
        if let Ok(regex) = Regex::new(&template.pattern) {
            self.patterns.insert(template_id, regex);
        }

        // Add to trie
        self.trie.insert(prefix, template);
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
            if let Some(regex) = self.patterns.get(&template.template_id) {
                if let Some(captures) = regex.captures(log_line) {
                    let mut extracted_values = HashMap::new();

                    // Extract variable values
                    for (i, var_name) in template.variables.iter().enumerate() {
                        if let Some(value) = captures.get(i + 1) {
                            extracted_values.insert(var_name.clone(), value.as_str().to_string());
                        }
                    }

                    return MatchResult {
                        matched: true,
                        template_id: Some(template.template_id),
                        extracted_values,
                    };
                }
            }
        }

        MatchResult {
            matched: false,
            template_id: None,
            extracted_values: HashMap::new(),
        }
    }

    /// Find candidate templates using radix trie prefix matching
    fn find_candidate_templates(&self, log_line: &str) -> Vec<LogTemplate> {
        let mut candidates = Vec::new();

        // Try different prefix lengths
        for len in (5..=log_line.len().min(30)).rev() {
            let prefix = &log_line[..len];

            // Get all templates with this prefix or shorter
            if let Some(template) = self.trie.get(prefix) {
                candidates.push(template.clone());
            }

            // Also check subtrie for partial matches
            let subtrie = self.trie.get_raw_descendant(prefix);
            if let Some(st) = subtrie {
                for (_, template) in st.iter() {
                    candidates.push(template.clone());
                }
            }
        }

        // If no prefix match, return all templates (fallback to brute force)
        if candidates.is_empty() {
            for (_, template) in self.trie.iter() {
                candidates.push(template.clone());
            }
        }

        candidates
    }

    /// Get all templates for inspection
    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.trie
            .iter()
            .map(|(_, template)| template.clone())
            .collect()
    }
}

impl Default for LockFreeLogMatcher {
    fn default() -> Self {
        Self::new()
    }
}
