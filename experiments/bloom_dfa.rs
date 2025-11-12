/// Bloom Filter-enhanced DFA for pattern matching
///
/// Each DFA node has bloom filters that quickly check if a substring
/// could lead to a valid transition, avoiding expensive character-by-character matching.

use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Simple bloom filter for substring matching
#[derive(Clone)]
struct BloomFilter {
    bits: Vec<u64>,
    num_bits: usize,
    num_hashes: usize,
}

impl BloomFilter {
    fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        // Calculate optimal size
        let num_bits = Self::optimal_num_bits(expected_items, false_positive_rate);
        let num_hashes = Self::optimal_num_hashes(num_bits, expected_items);

        Self {
            bits: vec![0u64; (num_bits + 63) / 64], // Round up to u64 boundaries
            num_bits,
            num_hashes,
        }
    }

    fn optimal_num_bits(n: usize, p: f64) -> usize {
        let n = n.max(1) as f64;
        let p = p.max(0.0001).min(0.9999);
        (-(n * p.ln()) / (2.0_f64.ln().powi(2))).ceil() as usize
    }

    fn optimal_num_hashes(m: usize, n: usize) -> usize {
        let n = n.max(1) as f64;
        let m = m as f64;
        ((m / n) * 2.0_f64.ln()).ceil().max(1.0) as usize
    }

    fn add(&mut self, item: &str) {
        for hash_val in self.hash_values(item) {
            let idx = hash_val % self.num_bits;
            self.bits[idx / 64] |= 1u64 << (idx % 64);
        }
    }

    fn might_contain(&self, item: &str) -> bool {
        for hash_val in self.hash_values(item) {
            let idx = hash_val % self.num_bits;
            if (self.bits[idx / 64] & (1u64 << (idx % 64))) == 0 {
                return false;
            }
        }
        true
    }

    fn hash_values(&self, item: &str) -> Vec<usize> {
        let mut hashes = Vec::with_capacity(self.num_hashes);

        for i in 0..self.num_hashes {
            let mut hasher = DefaultHasher::new();
            i.hash(&mut hasher);
            item.hash(&mut hasher);
            hashes.push(hasher.finish() as usize);
        }

        hashes
    }
}

/// DFA node with bloom filters for fast transition lookup
struct DFANode {
    /// Node ID
    id: usize,

    /// Bloom filters indexed by fragment length
    /// bloom_by_length[len] checks if a substring of length `len` could be valid
    bloom_by_length: FxHashMap<usize, BloomFilter>,

    /// Actual transitions: substring -> next_node_id
    transitions: FxHashMap<String, usize>,

    /// Templates that match at this node (if any)
    matching_templates: Vec<u64>,
}

impl DFANode {
    fn new(id: usize) -> Self {
        Self {
            id,
            bloom_by_length: FxHashMap::default(),
            transitions: FxHashMap::default(),
            matching_templates: Vec::new(),
        }
    }

    fn add_transition(&mut self, substring: String, next_node: usize) {
        let len = substring.len();

        // Get or create bloom filter for this length
        let bloom = self.bloom_by_length
            .entry(len)
            .or_insert_with(|| BloomFilter::new(100, 0.01));

        bloom.add(&substring);
        self.transitions.insert(substring, next_node);
    }

    /// Fast check: could this substring lead to a valid transition?
    fn might_have_transition(&self, substring: &str) -> bool {
        let len = substring.len();

        match self.bloom_by_length.get(&len) {
            Some(bloom) => bloom.might_contain(substring),
            None => false, // No transitions of this length exist
        }
    }

    fn get_transition(&self, substring: &str) -> Option<usize> {
        // Fast bloom filter check first
        if !self.might_have_transition(substring) {
            return None;
        }

        // Bloom filter says "maybe" - do actual lookup
        self.transitions.get(substring).copied()
    }
}

pub struct BloomDFA {
    nodes: Vec<DFANode>,
    patterns: Vec<String>, // For reference
    pattern_lengths: Vec<usize>, // Pre-computed pattern lengths
}

impl BloomDFA {
    pub fn new() -> Self {
        Self {
            nodes: vec![DFANode::new(0)], // Start with root node
            patterns: Vec::new(),
            pattern_lengths: Vec::new(),
        }
    }

    /// Add a pattern to the DFA
    /// For simplicity, we just add the entire pattern as a single transition from root
    pub fn add_pattern(&mut self, pattern: &str, template_id: u64) {
        self.patterns.push(pattern.to_string());

        let pattern_len = pattern.len();
        if !self.pattern_lengths.contains(&pattern_len) {
            self.pattern_lengths.push(pattern_len);
        }

        // Check if we already have this exact pattern
        if let Some(&next_node) = self.nodes[0].transitions.get(pattern) {
            // Add template to existing node
            if !self.nodes[next_node].matching_templates.contains(&template_id) {
                self.nodes[next_node].matching_templates.push(template_id);
            }
        } else {
            // Create new leaf node
            let new_node_id = self.nodes.len();
            let mut new_node = DFANode::new(new_node_id);
            new_node.matching_templates.push(template_id);
            self.nodes.push(new_node);

            // Add transition from root with bloom filter
            self.nodes[0].add_transition(pattern.to_string(), new_node_id);
        }
    }

    /// Search for patterns in text using bloom filters at each node
    pub fn search(&self, text: &str) -> Vec<(u64, usize, usize)> {
        let mut matches = Vec::new();
        let text_len = text.len();

        // Try starting from each position in the text
        for start_pos in 0..text_len {
            // For each known pattern length, try to match from root
            for &pattern_len in &self.pattern_lengths {
                if start_pos + pattern_len > text_len {
                    continue;
                }

                let substring = &text[start_pos..start_pos + pattern_len];

                // Fast bloom filter check at root node
                if !self.nodes[0].might_have_transition(substring) {
                    continue; // Bloom filter says definitely not there
                }

                // Get actual transition (bloom filter said "maybe")
                if let Some(next_node) = self.nodes[0].get_transition(substring) {
                    // Found a match! Record all templates at this node
                    for &template_id in &self.nodes[next_node].matching_templates {
                        matches.push((template_id, start_pos, start_pos + pattern_len));
                    }
                }
            }
        }

        matches
    }

    /// Search with explicit length hints (for better performance)
    pub fn search_with_lengths(&self, text: &str, fragment_lengths: &[usize]) -> Vec<(u64, usize, usize)> {
        let mut matches = Vec::new();
        let text_len = text.len();

        for start_pos in 0..text_len {
            let mut current_node = 0;

            for &frag_len in fragment_lengths {
                if start_pos + frag_len > text_len {
                    break;
                }

                let substring = &text[start_pos..start_pos + frag_len];

                // Bloom filter check
                if !self.nodes[current_node].might_have_transition(substring) {
                    break;
                }

                // Get transition
                if let Some(next_node) = self.nodes[current_node].get_transition(substring) {
                    current_node = next_node;

                    // Record matches
                    for &template_id in &self.nodes[current_node].matching_templates {
                        matches.push((template_id, start_pos, start_pos + frag_len));
                    }
                } else {
                    break;
                }
            }
        }

        matches
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }
}

impl Default for BloomDFA {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter_basic() {
        let mut bloom = BloomFilter::new(100, 0.01);

        bloom.add("hello");
        bloom.add("world");

        assert!(bloom.might_contain("hello"));
        assert!(bloom.might_contain("world"));
        assert!(!bloom.might_contain("foo")); // Should probably be false
    }

    #[test]
    fn test_bloom_dfa_simple() {
        let mut dfa = BloomDFA::new();

        dfa.add_pattern("error", 1);
        dfa.add_pattern("warning", 2);

        let matches = dfa.search("this is an error message with a warning");

        println!("Matches: {:?}", matches);
        assert!(matches.len() >= 1);
    }

    #[test]
    fn test_bloom_dfa_fragments() {
        let mut dfa = BloomDFA::new();

        // Simulate fragments from a template
        dfa.add_pattern("error: ", 1);
        dfa.add_pattern("ms", 1);

        let text = "error: connection timeout after 5000ms";
        let matches = dfa.search(text);

        println!("Fragment matches: {:?}", matches);
        assert!(matches.len() >= 1);
    }

    #[test]
    fn test_bloom_filter_length_optimization() {
        let mut dfa = BloomDFA::new();

        dfa.add_pattern("uid=", 1);
        dfa.add_pattern("euid=", 1);

        // The bloom filters should be keyed by length
        assert_eq!(dfa.nodes[0].bloom_by_length.len(), 2); // Two different lengths
    }
}
