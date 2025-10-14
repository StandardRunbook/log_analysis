// SIMD-optimized matcher using memchr for fast prefix scanning
// Uses SIMD instructions to scan for template prefixes in parallel

use arc_swap::ArcSwap;
use im::HashMap as ImHashMap;
use lru::LruCache;
use memchr::memmem;
use radix_trie::{Trie, TrieCommon};
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
    pub prefix: String, // Static prefix for SIMD matching
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub matched: bool,
    pub template_id: Option<u64>,
    pub extracted_values: HashMap<String, String>,
}

#[derive(Clone)]
struct MatcherSnapshot {
    trie: Trie<String, LogTemplate>,
    patterns: ImHashMap<u64, Arc<Regex>>,
    // SIMD-optimized prefix searchers
    prefix_finders: ImHashMap<u64, Arc<memmem::Finder<'static>>>,
    next_id: u64,
}

impl MatcherSnapshot {
    fn new() -> Self {
        Self {
            trie: Trie::new(),
            patterns: ImHashMap::new(),
            prefix_finders: ImHashMap::new(),
            next_id: 1,
        }
    }

    fn add_template(mut self, template: LogTemplate) -> Self {
        let template_id = template.template_id;
        let prefix = extract_prefix(&template.pattern);

        // Compile regex pattern
        if let Ok(regex) = Regex::new(&template.pattern) {
            self.patterns.insert(template_id, Arc::new(regex));
        }

        // Create SIMD-optimized prefix finder
        if !prefix.is_empty() {
            // memchr::memmem::Finder uses SIMD (AVX2, SSE, NEON) internally
            let finder = memmem::Finder::new(&prefix);
            // Leak the string to get 'static lifetime (safe for benchmarks)
            let static_prefix = Box::leak(prefix.clone().into_boxed_str());
            let static_finder = memmem::Finder::new(static_prefix);
            self.prefix_finders
                .insert(template_id, Arc::new(static_finder));
        }

        self.trie.insert(prefix, template);
        self
    }

    fn match_log(&self, log_line: &str) -> MatchResult {
        // SIMD-accelerated prefix matching
        let candidates = self.find_candidates_simd(log_line);

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

    fn try_template(&self, template_id: u64, log_line: &str) -> Option<MatchResult> {
        // SIMD prefix check first (ultra-fast rejection)
        if let Some(finder) = self.prefix_finders.get(&template_id) {
            if finder.find(log_line.as_bytes()).is_none() {
                return None; // Fast path - prefix not found
            }
        }

        let regex = self.patterns.get(&template_id)?;
        let captures = regex.captures(log_line)?;

        let template = self
            .trie
            .iter()
            .find(|(_, t)| t.template_id == template_id)?
            .1;

        let mut extracted_values = HashMap::new();
        for (i, var_name) in template.variables.iter().enumerate() {
            if let Some(value) = captures.get(i + 1) {
                extracted_values.insert(var_name.clone(), value.as_str().to_string());
            }
        }

        Some(MatchResult {
            matched: true,
            template_id: Some(template_id),
            extracted_values,
        })
    }

    /// SIMD-accelerated candidate finding
    fn find_candidates_simd(&self, log_line: &str) -> Vec<LogTemplate> {
        let mut candidates = Vec::new();
        let log_bytes = log_line.as_bytes();

        // Use SIMD to quickly scan for all prefix matches
        for (_, template) in self.trie.iter() {
            if let Some(finder) = self.prefix_finders.get(&template.template_id) {
                // SIMD search - uses AVX2/SSE4.2 on x86, NEON on ARM
                if finder.find(log_bytes).is_some() {
                    candidates.push(template.clone());
                }
            } else {
                // Fallback for templates without prefix
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

/// SIMD-optimized matcher with LRU cache
pub struct SimdMatcher {
    snapshot: ArcSwap<MatcherSnapshot>,
    cache: Arc<Mutex<LruCache<String, u64>>>,
}

impl SimdMatcher {
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

    /// SIMD + cache optimized matching
    pub fn match_log(&self, log_line: &str) -> MatchResult {
        let cache_key: String = log_line.chars().take(30).collect();

        // Cache check first
        if let Ok(mut cache) = self.cache.try_lock() {
            if let Some(&template_id) = cache.get(&cache_key) {
                let snapshot = self.snapshot.load();
                if let Some(result) = snapshot.try_template(template_id, log_line) {
                    return result;
                }
                cache.pop(&cache_key);
            }
        }

        // SIMD-accelerated full search
        let snapshot = self.snapshot.load();
        let result = snapshot.match_log(log_line);

        // Update cache
        if result.matched {
            if let Some(template_id) = result.template_id {
                if let Ok(mut cache) = self.cache.try_lock() {
                    cache.put(cache_key, template_id);
                }
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

impl Clone for SimdMatcher {
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
    fn test_simd_matching() {
        let matcher = SimdMatcher::new(100);

        let log = "cpu_usage: 67.8% - Server load increased";
        let result = matcher.match_log(log);

        assert!(result.matched);
        assert_eq!(
            result.extracted_values.get("percentage"),
            Some(&"67.8".to_string())
        );
    }
}
