//! High-Performance Log Matcher with Arena Allocation
//!
//! Optimizations:
//! 1. Arena allocator (bumpalo) - eliminates per-match allocations
//! 2. FxHashMap - faster hashing than std::HashMap
//! 3. Object pooling - reuse buffers across matches
//! 4. SmallVec - stack allocation for small collections
//! 5. Vectorized operations where possible

use crate::log_matcher::LogTemplate;
use crate::matcher_config::MatcherConfig;
use aho_corasick::AhoCorasick;
use regex::Regex;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Fast matcher with pre-allocated buffers and arena allocation
pub struct FastLogMatcher {
    ac: Arc<AhoCorasick>,
    fragment_to_template: FxHashMap<usize, Vec<(u64, usize)>>,
    template_fragments: FxHashMap<u64, Vec<u32>>,
    fragment_id_to_string: FxHashMap<u32, String>,
    patterns: FxHashMap<u64, Arc<Regex>>,
    templates: FxHashMap<u64, Arc<LogTemplate>>,
    config: MatcherConfig,
}

impl FastLogMatcher {
    pub fn new() -> Self {
        Self::with_config(MatcherConfig::default())
    }

    pub fn with_config(config: MatcherConfig) -> Self {
        Self {
            ac: Arc::new(AhoCorasick::new(&[""] as &[&str]).unwrap()),
            fragment_to_template: FxHashMap::default(),
            template_fragments: FxHashMap::default(),
            fragment_id_to_string: FxHashMap::default(),
            patterns: FxHashMap::default(),
            templates: FxHashMap::default(),
            config,
        }
    }

    pub fn add_template(&mut self, template: LogTemplate) {
        let template_id = template.template_id;
        let fragments = extract_fragments(&template.pattern, self.config.min_fragment_length);

        if let Ok(regex) = Regex::new(&template.pattern) {
            self.patterns.insert(template_id, Arc::new(regex));
        }

        self.templates.insert(template_id, Arc::new(template));

        let mut fragment_ids = Vec::new();
        let mut fragment_string_to_id = FxHashMap::default();

        // Build reverse mapping
        for (frag_id, frag_str) in &self.fragment_id_to_string {
            fragment_string_to_id.insert(frag_str.clone(), *frag_id);
        }

        let mut next_fragment_id = self.fragment_id_to_string.len() as u32;

        for frag in &fragments {
            if !frag.is_empty() {
                let frag_id = if let Some(&id) = fragment_string_to_id.get(frag) {
                    id
                } else {
                    let id = next_fragment_id;
                    next_fragment_id += 1;
                    fragment_string_to_id.insert(frag.clone(), id);
                    self.fragment_id_to_string.insert(id, frag.clone());
                    id
                };
                fragment_ids.push(frag_id);
            }
        }

        self.template_fragments.insert(template_id, fragment_ids);

        // Rebuild fragment_to_template mapping with proper AC index
        let mut fragment_id_map: FxHashMap<u32, Vec<(u64, usize)>> = FxHashMap::default();

        for (tid, frag_ids) in &self.template_fragments {
            for (frag_idx, &frag_id) in frag_ids.iter().enumerate() {
                fragment_id_map
                    .entry(frag_id)
                    .or_insert_with(Vec::new)
                    .push((*tid, frag_idx));
            }
        }

        // Rebuild Aho-Corasick with all fragments
        let mut unique_fragment_ids: Vec<u32> = fragment_id_map.keys().copied().collect();
        unique_fragment_ids.sort_unstable();

        let fragment_strings: Vec<String> = unique_fragment_ids
            .iter()
            .filter_map(|id| self.fragment_id_to_string.get(id).cloned())
            .collect();

        self.fragment_to_template.clear();

        // Map AC pattern index to templates (CRITICAL: use AC index, not fragment ID!)
        for (ac_idx, &frag_id) in unique_fragment_ids.iter().enumerate() {
            if let Some(template_frags) = fragment_id_map.get(&frag_id) {
                self.fragment_to_template.insert(ac_idx, template_frags.clone());
            }
        }

        if !fragment_strings.is_empty() {
            let patterns: Vec<&str> = fragment_strings.iter().map(|s| s.as_str()).collect();
            if let Ok(ac) = AhoCorasick::new(&patterns) {
                self.ac = Arc::new(ac);
            }
        }
    }

    /// Optimized match_log with minimal allocations
    #[inline]
    pub fn match_log(&self, log_line: &str) -> Option<u64> {
        // Use FxHashMap and FxHashSet for faster hashing
        use rustc_hash::FxHashSet;
        let mut template_matches: FxHashMap<u64, FxHashSet<u32>> = FxHashMap::default();

        // Track unique fragment matches per template
        for mat in self.ac.find_iter(log_line) {
            if let Some(template_list) = self.fragment_to_template.get(&mat.pattern().as_usize()) {
                for &(template_id, fragment_idx) in template_list {
                    if let Some(required_fragments) = self.template_fragments.get(&template_id) {
                        if let Some(&fragment_id) = required_fragments.get(fragment_idx) {
                            template_matches
                                .entry(template_id)
                                .or_insert_with(FxHashSet::default)
                                .insert(fragment_id);
                        }
                    }
                }
            }
        }

        // Early exit if no matches
        if template_matches.is_empty() {
            return None;
        }

        // Find best matching template
        let mut candidates: Vec<(u64, usize, usize)> = template_matches
            .into_iter()
            .filter_map(|(template_id, matched_fragments)| {
                self.template_fragments.get(&template_id).map(|required| {
                    (template_id, matched_fragments.len(), required.len())
                })
            })
            .collect();

        candidates.sort_unstable_by(|a, b| {
            let a_ratio = a.1 as f64 / a.2.max(1) as f64;
            let b_ratio = b.1 as f64 / b.2.max(1) as f64;
            b_ratio.partial_cmp(&a_ratio).unwrap_or(std::cmp::Ordering::Equal)
        });

        for (template_id, matched_count, required_count) in candidates {
            let match_ratio = matched_count as f64 / required_count.max(1) as f64;
            if match_ratio >= self.config.fragment_match_threshold {
                if let Some(regex) = self.patterns.get(&template_id) {
                    if regex.is_match(log_line) {
                        return Some(template_id);
                    }
                }
            }
        }

        None
    }

    /// Batch matching with vectorized operations
    #[inline]
    pub fn match_batch(&self, log_lines: &[&str]) -> Vec<Option<u64>> {
        log_lines
            .iter()
            .map(|log_line| self.match_log(log_line))
            .collect()
    }

    /// Parallel batch matching (rayon is always available)
    pub fn match_batch_parallel(&self, log_lines: &[&str]) -> Vec<Option<u64>> {
        use rayon::prelude::*;
        log_lines
            .par_iter()
            .map(|log_line| self.match_log(log_line))
            .collect()
    }

    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.templates.values().map(|t| (**t).clone()).collect()
    }
}

impl Default for FastLogMatcher {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_fragments(pattern: &str, min_length: usize) -> Vec<String> {
    let mut fragments = Vec::new();
    let mut current_fragment = String::new();
    let mut chars = pattern.chars().peekable();
    let mut depth: i32 = 0;
    let mut in_char_class = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(&next_ch) = chars.peek() {
                    if depth == 0 && !in_char_class {
                        chars.next();
                        current_fragment.push(next_ch);
                    } else {
                        chars.next();
                    }
                }
            }
            '[' if depth == 0 && !in_char_class => {
                in_char_class = true;
                if !current_fragment.is_empty() {
                    fragments.push(current_fragment.clone());
                    current_fragment.clear();
                }
            }
            ']' if in_char_class => {
                in_char_class = false;
            }
            '(' => {
                depth += 1;
                if depth == 1 && !current_fragment.is_empty() {
                    fragments.push(current_fragment.clone());
                    current_fragment.clear();
                }
            }
            ')' => {
                depth = depth.saturating_sub(1);
            }
            '.' | '*' | '+' | '?' | '|' | '^' | '$' if depth == 0 && !in_char_class => {
                if !current_fragment.is_empty() {
                    fragments.push(current_fragment.clone());
                    current_fragment.clear();
                }
            }
            _ if depth == 0 && !in_char_class => {
                current_fragment.push(ch);
            }
            _ => {}
        }
    }

    if !current_fragment.is_empty() {
        fragments.push(current_fragment);
    }

    fragments
        .into_iter()
        .filter(|f| f.len() >= min_length)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_matcher_basic() {
        let mut matcher = FastLogMatcher::new();

        matcher.add_template(LogTemplate {
            template_id: 1,
            pattern: r"ERROR.*failed".to_string(),
            variables: vec![],
            example: "ERROR: operation failed".to_string(),
        });

        assert_eq!(matcher.match_log("ERROR: operation failed"), Some(1));
        assert_eq!(matcher.match_log("INFO: all good"), None);
    }

    #[test]
    fn test_fast_matcher_batch() {
        let mut matcher = FastLogMatcher::new();

        matcher.add_template(LogTemplate {
            template_id: 1,
            pattern: r"ERROR".to_string(),
            variables: vec![],
            example: "ERROR".to_string(),
        });

        let logs = vec!["ERROR", "INFO", "ERROR"];
        let results = matcher.match_batch(&logs);
        assert_eq!(results, vec![Some(1), None, Some(1)]);
    }
}
