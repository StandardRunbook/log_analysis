/// Demo Aho-Corasick implementation for experimentation
///
/// This is a simplified version of LogMatcher that exposes the internals
/// for experimenting with different matching strategies.

use aho_corasick::{AhoCorasick, MatchKind};
use regex::Regex;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DemoTemplate {
    pub template_id: u64,
    pub pattern: String,
    pub variables: Vec<String>,
    pub example: String,
}

pub struct DemoMatcher {
    // Core AC automaton
    ac: Option<AhoCorasick>,

    // Fragment -> Template mappings
    fragment_strings: Vec<String>,
    fragment_to_templates: FxHashMap<usize, Vec<(u64, usize)>>, // ac_idx -> [(template_id, frag_idx)]

    // Template data
    template_fragments: FxHashMap<u64, Vec<usize>>, // template_id -> [ac_indices]
    templates: FxHashMap<u64, DemoTemplate>,
    patterns: FxHashMap<u64, Arc<Regex>>,

    // Configuration
    min_fragment_length: usize,
}

impl DemoMatcher {
    pub fn new() -> Self {
        Self {
            ac: None,
            fragment_strings: Vec::new(),
            fragment_to_templates: FxHashMap::default(),
            template_fragments: FxHashMap::default(),
            templates: FxHashMap::default(),
            patterns: FxHashMap::default(),
            min_fragment_length: 2,
        }
    }

    pub fn with_min_fragment_length(mut self, min_length: usize) -> Self {
        self.min_fragment_length = min_length;
        self
    }

    /// Add a template and rebuild the AC automaton
    pub fn add_template(&mut self, template: DemoTemplate) {
        let template_id = template.template_id;

        // Extract fragments from pattern
        let fragments = self.extract_fragments(&template.pattern);

        // Store regex
        if let Ok(regex) = Regex::new(&template.pattern) {
            self.patterns.insert(template_id, Arc::new(regex));
        }

        // Store template
        self.templates.insert(template_id, template);

        // Map fragments to AC indices
        let mut ac_indices = Vec::new();
        for (frag_idx, frag) in fragments.iter().enumerate() {
            // Find or add fragment
            let ac_idx = if let Some(pos) = self.fragment_strings.iter().position(|s| s == frag) {
                pos
            } else {
                let idx = self.fragment_strings.len();
                self.fragment_strings.push(frag.clone());
                idx
            };

            ac_indices.push(ac_idx);

            // Map fragment to template
            self.fragment_to_templates
                .entry(ac_idx)
                .or_insert_with(Vec::new)
                .push((template_id, frag_idx));
        }

        self.template_fragments.insert(template_id, ac_indices);

        // Rebuild AC automaton
        self.rebuild_ac();
    }

    /// Rebuild the Aho-Corasick automaton
    fn rebuild_ac(&mut self) {
        if self.fragment_strings.is_empty() {
            self.ac = None;
            return;
        }

        let fragment_strs: Vec<&str> = self.fragment_strings.iter().map(|s| s.as_str()).collect();

        self.ac = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostFirst)
            .build(&fragment_strs)
            .ok();
    }

    /// Match a log line (simple version - no weighted scoring)
    pub fn match_log(&self, log_line: &str) -> Option<u64> {
        let ac = self.ac.as_ref()?;

        // Track which templates matched which fragments
        let mut template_matches: FxHashMap<u64, usize> = FxHashMap::default();

        for mat in ac.find_iter(log_line) {
            let ac_idx = mat.pattern().as_usize();

            if let Some(template_list) = self.fragment_to_templates.get(&ac_idx) {
                for &(template_id, _frag_idx) in template_list {
                    *template_matches.entry(template_id).or_insert(0) += 1;
                }
            }
        }

        // Find best matching template
        let mut best_template = None;
        let mut best_ratio = 0.0;

        for (template_id, matched_count) in template_matches {
            if let Some(required_fragments) = self.template_fragments.get(&template_id) {
                let ratio = matched_count as f64 / required_fragments.len() as f64;

                if ratio > best_ratio {
                    best_ratio = ratio;
                    best_template = Some(template_id);
                }
            }
        }

        // Require at least 70% match
        if best_ratio >= 0.7 {
            best_template
        } else {
            None
        }
    }

    /// Get all AC matches in a log line (for debugging)
    pub fn get_all_matches(&self, log_line: &str) -> Vec<(usize, String, usize, usize)> {
        let ac = match self.ac.as_ref() {
            Some(ac) => ac,
            None => return Vec::new(),
        };

        let mut matches = Vec::new();
        for mat in ac.find_iter(log_line) {
            let ac_idx = mat.pattern().as_usize();
            let start = mat.start();
            let end = mat.end();
            let fragment = self.fragment_strings[ac_idx].clone();
            matches.push((ac_idx, fragment, start, end));
        }
        matches
    }

    /// Get template fragments for debugging
    pub fn get_template_fragments(&self, template_id: u64) -> Option<Vec<String>> {
        self.template_fragments.get(&template_id).map(|indices| {
            indices.iter()
                .map(|&idx| self.fragment_strings[idx].clone())
                .collect()
        })
    }

    /// Get number of fragments in AC automaton
    pub fn fragment_count(&self) -> usize {
        self.fragment_strings.len()
    }

    /// Get number of templates
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }

    /// Extract fragments from a regex pattern (simple version)
    fn extract_fragments(&self, pattern: &str) -> Vec<String> {
        let mut fragments = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        let mut in_char_class = false;
        let mut chars = pattern.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\\' => {
                    if let Some(&next_ch) = chars.peek() {
                        if depth == 0 && !in_char_class {
                            chars.next();
                            current.push(next_ch);
                        } else {
                            chars.next();
                        }
                    }
                }
                '[' if depth == 0 && !in_char_class => {
                    in_char_class = true;
                    if !current.is_empty() {
                        fragments.push(current.clone());
                        current.clear();
                    }
                }
                ']' if in_char_class => {
                    in_char_class = false;
                }
                '(' if !in_char_class => {
                    depth += 1;
                    if depth == 1 && !current.is_empty() {
                        fragments.push(current.clone());
                        current.clear();
                    }
                }
                ')' if !in_char_class => {
                    depth -= 1;
                }
                '.' | '*' | '+' | '?' | '{' | '}' | '^' | '$' | '|' if depth == 0 && !in_char_class => {
                    if !current.is_empty() {
                        fragments.push(current.clone());
                        current.clear();
                    }
                }
                _ if depth == 0 && !in_char_class => {
                    current.push(ch);
                }
                _ => {}
            }
        }

        if !current.is_empty() {
            fragments.push(current);
        }

        fragments.into_iter()
            .filter(|f| f.len() >= self.min_fragment_length)
            .collect()
    }
}

impl Default for DemoMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_matching() {
        let mut matcher = DemoMatcher::new();

        matcher.add_template(DemoTemplate {
            template_id: 1,
            pattern: r"error: connection timeout after (\d+)ms".to_string(),
            variables: vec!["duration".to_string()],
            example: "error: connection timeout after 5000ms".to_string(),
        });

        assert_eq!(matcher.fragment_count(), 2); // "error: connection timeout after " and "ms"

        let result = matcher.match_log("error: connection timeout after 5000ms");
        assert_eq!(result, Some(1));

        let no_match = matcher.match_log("different error message");
        assert_eq!(no_match, None);
    }

    #[test]
    fn test_fragment_extraction() {
        let matcher = DemoMatcher::new();
        let fragments = matcher.extract_fragments(r"uid=(\d+) euid=(\d+) tty=(\w+)");

        assert_eq!(fragments, vec!["uid=", " euid=", " tty="]);
    }

    #[test]
    fn test_get_matches() {
        let mut matcher = DemoMatcher::new();

        matcher.add_template(DemoTemplate {
            template_id: 1,
            pattern: r"authentication failure; logname= uid=(\d+) euid=(\d+)".to_string(),
            variables: vec!["uid".to_string(), "euid".to_string()],
            example: "authentication failure; logname= uid=0 euid=0".to_string(),
        });

        let log = "authentication failure; logname= uid=0 euid=0";
        let matches = matcher.get_all_matches(log);

        println!("Matches found: {:?}", matches);
        assert!(matches.len() >= 2); // At least "authentication failure; logname= uid=" and " euid="
    }
}
