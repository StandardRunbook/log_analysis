// Immutable lock-free matcher for true parallel processing
// No locks means no contention - perfect for multi-threaded benchmarks

use radix_trie::Trie;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;

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

/// Completely immutable matcher - can be shared across threads with just Arc
pub struct ImmutableLogMatcher {
    trie: Trie<String, LogTemplate>,
    patterns: HashMap<u64, Regex>,
}

impl ImmutableLogMatcher {
    pub fn new() -> Self {
        let mut matcher = Self {
            trie: Trie::new(),
            patterns: HashMap::new(),
        };
        matcher.add_default_templates();
        matcher
    }

    fn add_default_templates(&mut self) {
        let default_templates = vec![
            LogTemplate {
                template_id: 1,
                pattern: r"cpu_usage: (\d+\.\d+)% - (.*)".to_string(),
                variables: vec!["percentage".to_string(), "message".to_string()],
                example: "cpu_usage: 45.2% - Server load normal".to_string(),
            },
            LogTemplate {
                template_id: 2,
                pattern: r"memory_usage: (\d+\.\d+)GB - (.*)".to_string(),
                variables: vec!["amount".to_string(), "message".to_string()],
                example: "memory_usage: 2.5GB - Memory consumption stable".to_string(),
            },
            LogTemplate {
                template_id: 3,
                pattern: r"disk_io: (\d+)MB/s - (.*)".to_string(),
                variables: vec!["throughput".to_string(), "message".to_string()],
                example: "disk_io: 250MB/s - Disk activity moderate".to_string(),
            },
        ];

        for template in default_templates {
            self.add_template(template);
        }
    }

    pub fn add_template(&mut self, template: LogTemplate) {
        let template_id = template.template_id;
        let prefix = self.extract_prefix(&template.pattern);

        if let Ok(regex) = Regex::new(&template.pattern) {
            self.patterns.insert(template_id, regex);
        }

        self.trie.insert(prefix, template);
    }

    fn extract_prefix(&self, pattern: &str) -> String {
        pattern
            .chars()
            .take_while(|c| !matches!(c, '(' | '[' | '.' | '*' | '+' | '?' | '\\'))
            .collect()
    }

    /// Match a log - this is 100% read-only, no locks needed
    pub fn match_log(&self, log_line: &str) -> MatchResult {
        let candidates = self.find_candidate_templates(log_line);

        for template in candidates {
            if let Some(regex) = self.patterns.get(&template.template_id) {
                if let Some(captures) = regex.captures(log_line) {
                    let mut extracted_values = HashMap::new();

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

    fn find_candidate_templates(&self, log_line: &str) -> Vec<LogTemplate> {
        let mut candidates = Vec::new();

        for len in (5..=log_line.len().min(30)).rev() {
            let prefix = &log_line[..len];

            if let Some(template) = self.trie.get(prefix) {
                candidates.push(template.clone());
            }

            if let Some(subtrie) = self.trie.get_raw_descendant(prefix) {
                for (_, template) in subtrie.iter() {
                    candidates.push(template.clone());
                }
            }
        }

        if candidates.is_empty() {
            for (_, template) in self.trie.iter() {
                candidates.push(template.clone());
            }
        }

        candidates
    }

    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.trie
            .iter()
            .map(|(_, template)| template.clone())
            .collect()
    }
}

impl Default for ImmutableLogMatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper that makes it easy to share across threads
pub struct SharedMatcher {
    matcher: Arc<ImmutableLogMatcher>,
}

impl SharedMatcher {
    pub fn new() -> Self {
        Self {
            matcher: Arc::new(ImmutableLogMatcher::new()),
        }
    }

    pub fn with_templates(templates: Vec<LogTemplate>) -> Self {
        let mut matcher = ImmutableLogMatcher::new();
        for template in templates {
            matcher.add_template(template);
        }
        Self {
            matcher: Arc::new(matcher),
        }
    }

    pub fn match_log(&self, log_line: &str) -> MatchResult {
        self.matcher.match_log(log_line)
    }

    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.matcher.get_all_templates()
    }
}

impl Clone for SharedMatcher {
    fn clone(&self) -> Self {
        Self {
            matcher: Arc::clone(&self.matcher),
        }
    }
}
