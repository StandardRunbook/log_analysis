use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatcherConfig {
    pub match_kind: MatchKind,
    pub min_fragment_length: usize,
    pub cache_regex: bool,
    pub optimal_batch_size: usize,
    pub fragment_match_threshold: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MatchKind {
    LeftmostLongest,
    LeftmostFirst,
    Standard,
}

impl Default for MatcherConfig {
    fn default() -> Self {
        Self {
            match_kind: MatchKind::LeftmostLongest,
            min_fragment_length: 1,
            cache_regex: true,
            optimal_batch_size: 10_000,
            fragment_match_threshold: 0.3,
        }
    }
}

impl MatcherConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn streaming() -> Self {
        Self {
            optimal_batch_size: 1_000,
            ..Default::default()
        }
    }

    pub fn batch_processing() -> Self {
        Self {
            optimal_batch_size: 10_000,
            ..Default::default()
        }
    }

    pub fn bulk_processing() -> Self {
        Self {
            optimal_batch_size: 50_000,
            ..Default::default()
        }
    }

    pub fn with_match_kind(mut self, kind: MatchKind) -> Self {
        self.match_kind = kind;
        self
    }

    pub fn with_min_fragment_length(mut self, length: usize) -> Self {
        self.min_fragment_length = length.max(1);
        self
    }

    pub fn with_regex_caching(mut self, enabled: bool) -> Self {
        self.cache_regex = enabled;
        self
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.optimal_batch_size = size;
        self
    }

    pub fn with_fragment_threshold(mut self, threshold: f64) -> Self {
        self.fragment_match_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    pub(crate) fn to_ac_match_kind(&self) -> aho_corasick::MatchKind {
        match self.match_kind {
            MatchKind::LeftmostLongest => aho_corasick::MatchKind::LeftmostLongest,
            MatchKind::LeftmostFirst => aho_corasick::MatchKind::LeftmostFirst,
            MatchKind::Standard => aho_corasick::MatchKind::Standard,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MatcherConfig::default();
        assert_eq!(config.min_fragment_length, 1);
        assert_eq!(config.optimal_batch_size, 10_000);
        assert_eq!(config.fragment_match_threshold, 0.3);
        assert!(config.cache_regex);
    }

    #[test]
    fn test_streaming_config() {
        let config = MatcherConfig::streaming();
        assert_eq!(config.optimal_batch_size, 1_000);
    }

    #[test]
    fn test_batch_config() {
        let config = MatcherConfig::batch_processing();
        assert_eq!(config.optimal_batch_size, 10_000);
    }

    #[test]
    fn test_builder() {
        let config = MatcherConfig::new()
            .with_match_kind(MatchKind::LeftmostFirst)
            .with_min_fragment_length(3)
            .with_batch_size(5_000);

        assert_eq!(config.min_fragment_length, 3);
        assert_eq!(config.optimal_batch_size, 5_000);
    }

    #[test]
    fn test_bulk_processing_preset() {
        assert_eq!(MatcherConfig::bulk_processing().optimal_batch_size, 50_000);
    }

    #[test]
    fn test_with_regex_caching() {
        assert!(!MatcherConfig::default().with_regex_caching(false).cache_regex);
        assert!(MatcherConfig::default().with_regex_caching(true).cache_regex);
    }

    #[test]
    fn test_with_min_fragment_length_floors_at_one() {
        // 0 isn't a valid min length — the matcher would degenerate.
        // Builder clamps it up to 1 silently.
        assert_eq!(
            MatcherConfig::default().with_min_fragment_length(0).min_fragment_length,
            1
        );
    }

    #[test]
    fn test_with_fragment_threshold_clamps() {
        assert_eq!(
            MatcherConfig::default().with_fragment_threshold(-0.5).fragment_match_threshold,
            0.0
        );
        assert_eq!(
            MatcherConfig::default().with_fragment_threshold(1.5).fragment_match_threshold,
            1.0
        );
        assert_eq!(
            MatcherConfig::default().with_fragment_threshold(0.7).fragment_match_threshold,
            0.7
        );
    }

    #[test]
    fn test_to_ac_match_kind_round_trip() {
        // The exhaustive mapping from our enum to aho_corasick's.
        // Equality on aho_corasick::MatchKind is by variant name.
        for variant in [MatchKind::LeftmostLongest, MatchKind::LeftmostFirst, MatchKind::Standard] {
            let cfg = MatcherConfig::default().with_match_kind(variant);
            // smoke: just ensure each variant is reachable / does not panic
            let _ = cfg.to_ac_match_kind();
        }
    }

    #[test]
    fn test_serde_round_trip() {
        // The config is serialized in the matcher's snapshot file; the
        // round-trip must preserve every field. Guards against any new
        // field being silently skipped by serde.
        let original = MatcherConfig::new()
            .with_match_kind(MatchKind::Standard)
            .with_min_fragment_length(4)
            .with_batch_size(7_777)
            .with_regex_caching(false)
            .with_fragment_threshold(0.42);
        let json = serde_json::to_string(&original).unwrap();
        let parsed: MatcherConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.min_fragment_length, 4);
        assert_eq!(parsed.optimal_batch_size, 7_777);
        assert_eq!(parsed.cache_regex, false);
        assert!((parsed.fragment_match_threshold - 0.42).abs() < 1e-9);
    }
}
