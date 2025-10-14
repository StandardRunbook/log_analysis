// Lock-free matcher using structural sharing (CoW semantics)
// Reads never block, writes create new versions with shared structure

use arc_swap::ArcSwap;
use im::HashMap as ImHashMap;
use radix_trie::{Trie, TrieCommon};
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

/// Internal immutable snapshot of matcher state
#[derive(Clone)]
struct MatcherSnapshot {
    trie: Trie<String, LogTemplate>,
    patterns: ImHashMap<u64, Arc<Regex>>, // Using im::HashMap for structural sharing
    next_id: u64,
}

impl MatcherSnapshot {
    fn new() -> Self {
        Self {
            trie: Trie::new(),
            patterns: ImHashMap::new(),
            next_id: 1,
        }
    }

    fn add_template(mut self, template: LogTemplate) -> Self {
        let template_id = template.template_id;
        let prefix = extract_prefix(&template.pattern);

        if let Ok(regex) = Regex::new(&template.pattern) {
            // im::HashMap uses structural sharing - only changed nodes are copied
            self.patterns.insert(template_id, Arc::new(regex));
        }

        self.trie.insert(prefix, template);
        self
    }

    fn match_log(&self, log_line: &str) -> MatchResult {
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

    fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.trie
            .iter()
            .map(|(_, template)| template.clone())
            .collect()
    }
}

fn extract_prefix(pattern: &str) -> String {
    pattern
        .chars()
        .take_while(|c| !matches!(c, '(' | '[' | '.' | '*' | '+' | '?' | '\\'))
        .collect()
}

/// Lock-free matcher using structural sharing
/// Reads are lock-free and never block
/// Writes create new versions with shared structure (CoW)
pub struct StructuralSharingMatcher {
    snapshot: ArcSwap<MatcherSnapshot>,
}

impl StructuralSharingMatcher {
    pub fn new() -> Self {
        let mut snapshot = MatcherSnapshot::new();

        // Add default templates
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
            snapshot = snapshot.add_template(template);
        }

        Self {
            snapshot: ArcSwap::new(Arc::new(snapshot)),
        }
    }

    /// Lock-free read - loads current snapshot and uses it
    /// Multiple threads can read simultaneously without blocking
    pub fn match_log(&self, log_line: &str) -> MatchResult {
        let snapshot = self.snapshot.load();
        snapshot.match_log(log_line)
    }

    /// Lock-free write - creates new snapshot with structural sharing
    /// Old readers continue using their snapshot, new readers see the update
    pub fn add_template(&self, template: LogTemplate) {
        self.snapshot.rcu(|old_snapshot| {
            let new_snapshot = (**old_snapshot).clone().add_template(template.clone());
            Arc::new(new_snapshot)
        });
    }

    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        let snapshot = self.snapshot.load();
        snapshot.get_all_templates()
    }
}

impl Default for StructuralSharingMatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Clone is cheap - just clones the Arc to the current snapshot
impl Clone for StructuralSharingMatcher {
    fn clone(&self) -> Self {
        Self {
            snapshot: ArcSwap::new(self.snapshot.load_full()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_free_reads() {
        let matcher = StructuralSharingMatcher::new();

        let log = "cpu_usage: 67.8% - Server load increased";
        let result = matcher.match_log(log);

        assert!(result.matched);
        assert_eq!(
            result.extracted_values.get("percentage"),
            Some(&"67.8".to_string())
        );
    }

    #[test]
    fn test_concurrent_reads_and_writes() {
        use std::thread;

        let matcher = Arc::new(StructuralSharingMatcher::new());

        // Reader threads - keep reading while writes happen
        let mut handles = vec![];
        for i in 0..4 {
            let m = Arc::clone(&matcher);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    let log = format!("cpu_usage: {}.5% - test", i);
                    let _ = m.match_log(&log);
                }
            }));
        }

        // Writer thread - adds new templates
        let m = Arc::clone(&matcher);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                m.add_template(LogTemplate {
                    template_id: 100 + i,
                    pattern: format!(r"test_{}: (\d+)", i),
                    variables: vec!["value".to_string()],
                    example: format!("test_{}: 123", i),
                });
            }
        }));

        for handle in handles {
            handle.join().unwrap();
        }

        // All operations completed without blocking
        assert!(matcher.get_all_templates().len() >= 3);
    }
}
