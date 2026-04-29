/// Train/Test split functionality for benchmarking
///
/// Splits datasets to evaluate parsing techniques:
/// - Train set: Generate templates
/// - Test set: Evaluate accuracy
///
/// Supports stratified splitting to ensure all templates appear in both sets
use crate::traits::{DatasetLoader, GroundTruthEntry};
use anyhow::Result;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DatasetSplit {
    pub train_logs: Vec<String>,
    pub train_ground_truth: Vec<GroundTruthEntry>,
    pub test_logs: Vec<String>,
    pub test_ground_truth: Vec<GroundTruthEntry>,
}

impl DatasetSplit {
    /// Get statistics about the split
    pub fn stats(&self) -> SplitStats {
        let train_templates: std::collections::HashSet<_> = self
            .train_ground_truth
            .iter()
            .map(|e| e.event_id.clone())
            .collect();

        let test_templates: std::collections::HashSet<_> = self
            .test_ground_truth
            .iter()
            .map(|e| e.event_id.clone())
            .collect();

        SplitStats {
            train_size: self.train_logs.len(),
            test_size: self.test_logs.len(),
            train_templates: train_templates.len(),
            test_templates: test_templates.len(),
            total_size: self.train_logs.len() + self.test_logs.len(),
            total_templates: train_templates.union(&test_templates).count(),
        }
    }
}

#[derive(Debug)]
pub struct SplitStats {
    pub train_size: usize,
    pub test_size: usize,
    pub train_templates: usize,
    pub test_templates: usize,
    pub total_size: usize,
    pub total_templates: usize,
}

impl SplitStats {
    pub fn train_ratio(&self) -> f64 {
        self.train_size as f64 / self.total_size as f64
    }

    pub fn test_ratio(&self) -> f64 {
        self.test_size as f64 / self.total_size as f64
    }
}

/// Configuration for dataset splitting
#[derive(Debug, Clone)]
pub struct SplitConfig {
    /// Ratio of data to use for training (0.0 to 1.0)
    pub train_ratio: f64,
    /// Random seed for reproducibility
    pub seed: u64,
    /// Whether to use stratified split (ensure all templates in both sets)
    pub stratified: bool,
    /// Minimum samples per template in test set (for stratified split)
    pub min_test_samples: usize,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            train_ratio: 0.8,
            seed: 42,
            stratified: true,
            min_test_samples: 1,
        }
    }
}

/// Split a dataset into train and test sets
pub fn split_dataset(dataset: &impl DatasetLoader, config: &SplitConfig) -> Result<DatasetSplit> {
    let logs = dataset.load_raw_logs()?;
    let ground_truth = dataset.load_ground_truth()?;

    if logs.len() != ground_truth.len() {
        anyhow::bail!(
            "Logs and ground truth size mismatch: {} vs {}",
            logs.len(),
            ground_truth.len()
        );
    }

    if config.stratified {
        stratified_split(&logs, &ground_truth, config)
    } else {
        random_split(&logs, &ground_truth, config)
    }
}

/// Simple random split (may not include all templates in both sets)
fn random_split(
    logs: &[String],
    ground_truth: &[GroundTruthEntry],
    config: &SplitConfig,
) -> Result<DatasetSplit> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(config.seed);
    let mut indices: Vec<usize> = (0..logs.len()).collect();
    indices.shuffle(&mut rng);

    let train_size = (logs.len() as f64 * config.train_ratio) as usize;
    let train_indices = &indices[..train_size];
    let test_indices = &indices[train_size..];

    let train_logs = train_indices.iter().map(|&i| logs[i].clone()).collect();
    let train_ground_truth = train_indices
        .iter()
        .map(|&i| ground_truth[i].clone())
        .collect();

    let test_logs = test_indices.iter().map(|&i| logs[i].clone()).collect();
    let test_ground_truth = test_indices
        .iter()
        .map(|&i| ground_truth[i].clone())
        .collect();

    Ok(DatasetSplit {
        train_logs,
        train_ground_truth,
        test_logs,
        test_ground_truth,
    })
}

/// Stratified split - ensures all templates appear in both train and test
fn stratified_split(
    logs: &[String],
    ground_truth: &[GroundTruthEntry],
    config: &SplitConfig,
) -> Result<DatasetSplit> {
    // Group indices by template ID
    let mut template_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, entry) in ground_truth.iter().enumerate() {
        template_groups
            .entry(entry.event_id.clone())
            .or_default()
            .push(i);
    }

    let mut rng = rand::rngs::StdRng::seed_from_u64(config.seed);
    let mut train_indices = Vec::new();
    let mut test_indices = Vec::new();

    // For each template, split its samples
    for (_template_id, indices) in template_groups.iter_mut() {
        // Shuffle indices for this template
        indices.shuffle(&mut rng);

        let template_train_size = (indices.len() as f64 * config.train_ratio) as usize;

        // Ensure at least min_test_samples in test set
        let template_train_size = if indices.len() - template_train_size < config.min_test_samples {
            indices.len().saturating_sub(config.min_test_samples)
        } else {
            template_train_size
        };

        // Also ensure at least 1 in train set
        let template_train_size = template_train_size.max(1.min(indices.len() - 1));

        train_indices.extend_from_slice(&indices[..template_train_size]);
        test_indices.extend_from_slice(&indices[template_train_size..]);
    }

    // Shuffle the final indices to mix templates
    train_indices.shuffle(&mut rng);
    test_indices.shuffle(&mut rng);

    let train_logs = train_indices.iter().map(|&i| logs[i].clone()).collect();
    let train_ground_truth = train_indices
        .iter()
        .map(|&i| ground_truth[i].clone())
        .collect();

    let test_logs = test_indices.iter().map(|&i| logs[i].clone()).collect();
    let test_ground_truth = test_indices
        .iter()
        .map(|&i| ground_truth[i].clone())
        .collect();

    Ok(DatasetSplit {
        train_logs,
        train_ground_truth,
        test_logs,
        test_ground_truth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementations::InMemoryDataset;

    #[test]
    fn test_random_split() {
        let logs = vec![
            "log1".to_string(),
            "log2".to_string(),
            "log3".to_string(),
            "log4".to_string(),
            "log5".to_string(),
        ];

        let ground_truth = vec![
            GroundTruthEntry {
                log_line: "log1".to_string(),
                event_id: "E1".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log2".to_string(),
                event_id: "E1".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log3".to_string(),
                event_id: "E2".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log4".to_string(),
                event_id: "E2".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log5".to_string(),
                event_id: "E3".to_string(),
                expected_template: None,
            },
        ];

        let dataset = InMemoryDataset::new("test", logs, ground_truth);
        let config = SplitConfig {
            train_ratio: 0.6,
            seed: 42,
            stratified: false,
            min_test_samples: 1,
        };

        let split = split_dataset(&dataset, &config).unwrap();
        let stats = split.stats();

        assert_eq!(stats.train_size, 3); // 60% of 5
        assert_eq!(stats.test_size, 2); // 40% of 5
        assert_eq!(stats.total_size, 5);
    }

    #[test]
    fn test_stratified_split() {
        let logs = vec![
            "log1".to_string(),
            "log2".to_string(),
            "log3".to_string(),
            "log4".to_string(),
            "log5".to_string(),
            "log6".to_string(),
        ];

        let ground_truth = vec![
            GroundTruthEntry {
                log_line: "log1".to_string(),
                event_id: "E1".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log2".to_string(),
                event_id: "E1".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log3".to_string(),
                event_id: "E2".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log4".to_string(),
                event_id: "E2".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log5".to_string(),
                event_id: "E3".to_string(),
                expected_template: None,
            },
            GroundTruthEntry {
                log_line: "log6".to_string(),
                event_id: "E3".to_string(),
                expected_template: None,
            },
        ];

        let dataset = InMemoryDataset::new("test", logs, ground_truth);
        let config = SplitConfig {
            train_ratio: 0.8,
            seed: 42,
            stratified: true,
            min_test_samples: 1,
        };

        let split = split_dataset(&dataset, &config).unwrap();
        let stats = split.stats();

        // All 3 templates should appear in both sets
        assert_eq!(stats.train_templates, 3);
        assert_eq!(stats.test_templates, 3);
    }

    fn make_ds(logs_len: usize, templates: usize) -> InMemoryDataset {
        let logs: Vec<String> = (0..logs_len).map(|i| format!("log{i}")).collect();
        let ground_truth: Vec<GroundTruthEntry> = (0..logs_len)
            .map(|i| GroundTruthEntry {
                log_line: format!("log{i}"),
                event_id: format!("E{}", i % templates),
                expected_template: None,
            })
            .collect();
        InMemoryDataset::new("test", logs, ground_truth)
    }

    #[test]
    fn test_split_size_mismatch_returns_error() {
        // Construct a dataset with mismatched logs / ground-truth counts.
        let dataset = InMemoryDataset::new(
            "bad",
            vec!["a".into(), "b".into()],
            vec![GroundTruthEntry {
                log_line: "a".into(),
                event_id: "E1".into(),
                expected_template: None,
            }],
        );
        let result = split_dataset(&dataset, &SplitConfig::default());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("size mismatch"));
    }

    #[test]
    fn test_default_config() {
        let cfg = SplitConfig::default();
        assert_eq!(cfg.train_ratio, 0.8);
        assert_eq!(cfg.seed, 42);
        assert!(cfg.stratified);
        assert_eq!(cfg.min_test_samples, 1);
    }

    #[test]
    fn test_split_stats_ratios_sum_to_one() {
        let dataset = make_ds(20, 4);
        let split = split_dataset(&dataset, &SplitConfig::default()).unwrap();
        let s = split.stats();
        assert!((s.train_ratio() + s.test_ratio() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_random_split_is_reproducible_with_same_seed() {
        let dataset = make_ds(100, 5);
        let cfg = SplitConfig {
            train_ratio: 0.7,
            seed: 1234,
            stratified: false,
            min_test_samples: 1,
        };
        let s1 = split_dataset(&dataset, &cfg).unwrap();
        let s2 = split_dataset(&dataset, &cfg).unwrap();
        assert_eq!(s1.train_logs, s2.train_logs);
        assert_eq!(s1.test_logs, s2.test_logs);
    }

    #[test]
    fn test_stratified_min_test_samples_honored() {
        // With min_test_samples=2 every template must contribute at
        // least 2 entries to the test set.
        let dataset = make_ds(60, 3);
        let cfg = SplitConfig {
            train_ratio: 0.9,
            seed: 1,
            stratified: true,
            min_test_samples: 2,
        };
        let split = split_dataset(&dataset, &cfg).unwrap();
        let mut per_template: HashMap<String, usize> = HashMap::new();
        for e in &split.test_ground_truth {
            *per_template.entry(e.event_id.clone()).or_insert(0) += 1;
        }
        for count in per_template.values() {
            assert!(*count >= 2, "got per-template counts {per_template:?}");
        }
    }

    #[test]
    fn test_stats_total_templates_is_union() {
        // Construct two-set membership: train has E1,E2; test has E2,E3.
        // total_templates should be |union| = 3.
        let split = DatasetSplit {
            train_logs: vec!["a".into(), "b".into()],
            train_ground_truth: vec![
                GroundTruthEntry { log_line: "a".into(), event_id: "E1".into(), expected_template: None },
                GroundTruthEntry { log_line: "b".into(), event_id: "E2".into(), expected_template: None },
            ],
            test_logs: vec!["c".into(), "d".into()],
            test_ground_truth: vec![
                GroundTruthEntry { log_line: "c".into(), event_id: "E2".into(), expected_template: None },
                GroundTruthEntry { log_line: "d".into(), event_id: "E3".into(), expected_template: None },
            ],
        };
        let stats = split.stats();
        assert_eq!(stats.train_templates, 2);
        assert_eq!(stats.test_templates, 2);
        assert_eq!(stats.total_templates, 3); // union {E1,E2,E3}
    }
}
