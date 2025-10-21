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
}
