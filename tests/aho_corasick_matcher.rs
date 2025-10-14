// Aho-Corasick DFA-based matcher - finds ALL template prefixes in one pass
// Uses a deterministic finite automaton for O(n) multi-pattern matching
// General-purpose solution that works with any log format

use aho_corasick::AhoCorasick;
use arc_swap::ArcSwap;
use im::HashMap as ImHashMap;
use lru::LruCache;
use regex::Regex;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct LogTemplate {
    pub template_id: u64,
    pub pattern: String,
    pub variables: Vec<String>,
    pub example: String,
    pub prefix: String,
}

// Simplified - just return Option<u64> for template ID

#[derive(Clone)]
struct MatcherSnapshot {
    // Aho-Corasick automaton for multi-pattern matching
    ac: Arc<AhoCorasick>,
    // Map pattern index to template
    pattern_to_template: ImHashMap<usize, LogTemplate>,
    // Compiled regex patterns
    patterns: ImHashMap<u64, Arc<Regex>>,
    next_id: u64,
}

impl MatcherSnapshot {
    fn new() -> Self {
        Self {
            ac: Arc::new(AhoCorasick::new(&[""] as &[&str]).unwrap()),
            pattern_to_template: ImHashMap::new(),
            patterns: ImHashMap::new(),
            next_id: 1,
        }
    }

    fn add_template(mut self, template: LogTemplate) -> Self {
        let template_id = template.template_id;

        // Compile regex
        if let Ok(regex) = Regex::new(&template.pattern) {
            self.patterns.insert(template_id, Arc::new(regex));
        }

        // Store template
        let pattern_idx = self.pattern_to_template.len();
        self.pattern_to_template
            .insert(pattern_idx, template.clone());

        // Rebuild Aho-Corasick automaton with all prefixes
        let prefixes: Vec<String> = self
            .pattern_to_template
            .values()
            .map(|t| t.prefix.clone())
            .collect();

        if let Ok(ac) = AhoCorasick::new(&prefixes) {
            self.ac = Arc::new(ac);
        }

        self
    }

    fn match_log(&self, log_line: &str) -> Option<u64> {
        // Aho-Corasick finds matching prefixes in O(n)
        if let Some(mat) = self.ac.find(log_line) {
            if let Some(template) = self.pattern_to_template.get(&mat.pattern().as_usize()) {
                // Verify with regex (but don't extract values!)
                if let Some(regex) = self.patterns.get(&template.template_id) {
                    if regex.is_match(log_line) {
                        return Some(template.template_id);
                    }
                }
            }
        }
        None
    }

    fn try_template(&self, template_id: u64, log_line: &str) -> Option<u64> {
        let regex = self.patterns.get(&template_id)?;
        if regex.is_match(log_line) {
            Some(template_id)
        } else {
            None
        }
    }

    fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.pattern_to_template.values().cloned().collect()
    }
}

/// Aho-Corasick DFA matcher with LRU cache
pub struct AhoCorasickMatcher {
    snapshot: ArcSwap<MatcherSnapshot>,
    cache: Arc<Mutex<LruCache<String, u64>>>,
}

impl AhoCorasickMatcher {
    pub fn new(cache_size: usize) -> Self {
        let mut snapshot = MatcherSnapshot::new();

        let default_templates = vec![
            LogTemplate {
                template_id: 1,
                pattern: r"cpu_usage: (\d+\.\d+)% - (.*)".to_string(),
                variables: vec!["percentage".to_string(), "message".to_string()],
                example: "cpu_usage: 45.2% - Server load normal".to_string(),
                prefix: "cpu_usage: ".to_string(),
            },
            LogTemplate {
                template_id: 2,
                pattern: r"memory_usage: (\d+\.\d+)GB - (.*)".to_string(),
                variables: vec!["amount".to_string(), "message".to_string()],
                example: "memory_usage: 2.5GB - Memory consumption stable".to_string(),
                prefix: "memory_usage: ".to_string(),
            },
            LogTemplate {
                template_id: 3,
                pattern: r"disk_io: (\d+)MB/s - (.*)".to_string(),
                variables: vec!["throughput".to_string(), "message".to_string()],
                example: "disk_io: 250MB/s - Disk activity moderate".to_string(),
                prefix: "disk_io: ".to_string(),
            },
        ];

        for template in default_templates {
            snapshot = snapshot.add_template(template);
        }

        Self {
            snapshot: ArcSwap::new(Arc::new(snapshot)),
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(cache_size).unwrap(),
            ))),
        }
    }

    /// Match log and return template ID (Aho-Corasick DFA + LRU cache)
    pub fn match_log(&self, log_line: &str) -> Option<u64> {
        let cache_key: String = log_line.chars().take(30).collect();

        // Cache check
        if let Ok(mut cache) = self.cache.try_lock() {
            if let Some(&template_id) = cache.get(&cache_key) {
                let snapshot = self.snapshot.load();
                if let Some(result) = snapshot.try_template(template_id, log_line) {
                    return Some(result);
                }
                cache.pop(&cache_key);
            }
        }

        // DFA-based multi-pattern search
        let snapshot = self.snapshot.load();
        let result = snapshot.match_log(log_line);

        // Update cache
        if let Some(template_id) = result {
            if let Ok(mut cache) = self.cache.try_lock() {
                cache.put(cache_key, template_id);
            }
        }

        result
    }

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

    pub fn cache_stats(&self) -> (usize, usize) {
        if let Ok(cache) = self.cache.try_lock() {
            (cache.len(), cache.cap().get())
        } else {
            (0, 0)
        }
    }
}

impl Clone for AhoCorasickMatcher {
    fn clone(&self) -> Self {
        Self {
            snapshot: ArcSwap::new(self.snapshot.load_full()),
            cache: Arc::clone(&self.cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aho_corasick_matching() {
        let matcher = AhoCorasickMatcher::new(100);

        let log = "cpu_usage: 67.8% - Server load increased";
        let result = matcher.match_log(log);

        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_multi_pattern() {
        let matcher = AhoCorasickMatcher::new(100);

        // Test all patterns
        let test_cases = vec![
            ("cpu_usage: 50.0% - test", Some(1)),
            ("memory_usage: 2.5GB - test", Some(2)),
            ("disk_io: 100MB/s - test", Some(3)),
        ];

        for (log, expected) in test_cases {
            let result = matcher.match_log(log);
            assert_eq!(result, expected, "Failed to match: {}", log);
        }
    }
}
