use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram {
    pub counts: HashMap<u64, usize>,
    pub total: usize,
}

impl Histogram {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
            total: 0,
        }
    }

    /// Add a template ID to the histogram
    pub fn add(&mut self, template_id: u64) {
        *self.counts.entry(template_id).or_insert(0) += 1;
        self.total += 1;
    }

    /// Get the probability distribution from the histogram
    /// Optimized to avoid repeated divisions by pre-calculating the inverse
    pub fn get_distribution(&self) -> HashMap<u64, f64> {
        if self.total == 0 {
            return HashMap::new();
        }

        // Pre-calculate the inverse to convert divisions into multiplications (faster)
        let inv_total = 1.0 / self.total as f64;

        self.counts
            .iter()
            .map(|(&template_id, &count)| (template_id, count as f64 * inv_total))
            .collect()
    }

    /// Get all unique template IDs
    pub fn get_template_ids(&self) -> Vec<u64> {
        self.counts.keys().copied().collect()
    }

    /// Get count for a specific template ID
    pub fn get_count(&self, template_id: u64) -> usize {
        *self.counts.get(&template_id).unwrap_or(&0)
    }

    /// Merge another histogram into this one
    pub fn merge(&mut self, other: &Histogram) {
        for (&template_id, count) in &other.counts {
            *self.counts.entry(template_id).or_insert(0) += count;
            self.total += count;
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_histogram_counts() {
        let mut hist = Histogram::new();
        hist.add(1);
        hist.add(1);
        hist.add(2);

        assert_eq!(hist.total, 3);
        assert_eq!(hist.get_count(1), 2);
        assert_eq!(hist.get_count(2), 1);
    }

    #[test]
    fn test_distribution() {
        let mut hist = Histogram::new();
        hist.add(1);
        hist.add(1);
        hist.add(2);
        hist.add(3);

        let dist = hist.get_distribution();
        assert_eq!(dist.get(&1), Some(&0.5));
        assert_eq!(dist.get(&2), Some(&0.25));
        assert_eq!(dist.get(&3), Some(&0.25));
    }

    #[test]
    fn test_merge() {
        let mut hist1 = Histogram::new();
        hist1.add(1);
        hist1.add(2);

        let mut hist2 = Histogram::new();
        hist2.add(1);
        hist2.add(3);

        hist1.merge(&hist2);

        assert_eq!(hist1.total, 4);
        assert_eq!(hist1.get_count(1), 2);
        assert_eq!(hist1.get_count(2), 1);
        assert_eq!(hist1.get_count(3), 1);
    }
}
