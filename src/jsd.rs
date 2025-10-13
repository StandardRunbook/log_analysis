use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::histogram::Histogram;

/// Jensen-Shannon Divergence calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSDResult {
    pub jsd_score: f64,
    pub template_contributions: Vec<TemplateContribution>,
}

/// Contribution of a single template to the JSD score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateContribution {
    pub template_id: u64,
    pub baseline_probability: f64,
    pub current_probability: f64,
    pub contribution: f64,
    pub relative_change: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representative_logs: Option<Vec<String>>,
}

/// Calculate Jensen-Shannon Divergence between two probability distributions
///
/// This implementation uses several optimizations for robustness and performance:
/// - Single pass over all templates (no redundant lookups)
/// - Pre-allocated vectors to reduce allocations
/// - Numerically stable logarithm calculations
/// - Proper handling of zero probabilities
/// - Avoids division by zero and log(0) errors
pub fn calculate_jsd(baseline: &Histogram, current: &Histogram) -> JSDResult {
    // Input validation
    if baseline.total == 0 || current.total == 0 {
        return JSDResult {
            jsd_score: 0.0,
            template_contributions: Vec::new(),
        };
    }

    let baseline_dist = baseline.get_distribution();
    let current_dist = current.get_distribution();

    // Get all unique template IDs from both distributions
    let mut all_templates: HashSet<u64> =
        HashSet::with_capacity(baseline_dist.len() + current_dist.len());
    all_templates.extend(baseline_dist.keys().copied());
    all_templates.extend(current_dist.keys().copied());

    // Pre-allocate for performance
    let mut template_contributions = Vec::with_capacity(all_templates.len());

    let mut kl_baseline_mixture = 0.0;
    let mut kl_current_mixture = 0.0;

    // Single pass calculation
    for &template_id in &all_templates {
        let p = baseline_dist.get(&template_id).copied().unwrap_or(0.0);
        let q = current_dist.get(&template_id).copied().unwrap_or(0.0);

        // Calculate mixture distribution inline: M = (P + Q) / 2
        let m = (p + q) * 0.5;

        // Numerical stability: Only calculate KL terms when probabilities are non-zero
        // Using direct comparison with 0.0 is safe for probabilities
        let kl_p_m = if p > 0.0 && m > 0.0 {
            // Use the mathematically stable form: p * ln(p/m) = p * (ln(p) - ln(m))
            p * (p.ln() - m.ln())
        } else {
            0.0
        };

        let kl_q_m = if q > 0.0 && m > 0.0 {
            q * (q.ln() - m.ln())
        } else {
            0.0
        };

        kl_baseline_mixture += kl_p_m;
        kl_current_mixture += kl_q_m;

        // Individual contribution to JSD for this template
        let contribution = (kl_p_m + kl_q_m) * 0.5;

        // Calculate relative change with proper handling of edge cases
        let relative_change = if p > 0.0 {
            ((q - p) / p) * 100.0
        } else if q > 0.0 {
            100.0 // New template appeared
        } else {
            0.0 // Both are zero (shouldn't happen, but defensive)
        };

        template_contributions.push(TemplateContribution {
            template_id,
            baseline_probability: p,
            current_probability: q,
            contribution: contribution.max(0.0), // Ensure non-negative due to floating point errors
            relative_change,
            representative_logs: None, // Will be populated later with actual logs
        });
    }

    // JSD = (KL(P||M) + KL(Q||M)) / 2
    let jsd_score = ((kl_baseline_mixture + kl_current_mixture) * 0.5).max(0.0);

    // Sort by contribution (highest first) - use unstable sort for better performance
    template_contributions.sort_unstable_by(|a, b| {
        b.contribution
            .partial_cmp(&a.contribution)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    JSDResult {
        jsd_score,
        template_contributions,
    }
}

/// Get the top N templates with highest contribution to JSD
pub fn get_top_contributors(jsd_result: &JSDResult, n: usize) -> Vec<TemplateContribution> {
    jsd_result
        .template_contributions
        .iter()
        .take(n)
        .cloned()
        .collect()
}

/// Calculate JSD in bits (base 2) instead of nats (base e)
pub fn calculate_jsd_bits(baseline: &Histogram, current: &Histogram) -> JSDResult {
    let mut result = calculate_jsd(baseline, current);

    // Convert from nats to bits by dividing by ln(2)
    let ln2 = 2.0_f64.ln();
    result.jsd_score /= ln2;

    for contrib in &mut result.template_contributions {
        contrib.contribution /= ln2;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_distributions() {
        let mut hist1 = Histogram::new();
        hist1.add(1);
        hist1.add(2);

        let mut hist2 = Histogram::new();
        hist2.add(1);
        hist2.add(2);

        let result = calculate_jsd(&hist1, &hist2);
        assert!(result.jsd_score < 1e-10); // Should be approximately 0
    }

    #[test]
    fn test_different_distributions() {
        let mut baseline = Histogram::new();
        baseline.add(1);
        baseline.add(1);
        baseline.add(2);

        let mut current = Histogram::new();
        current.add(2);
        current.add(2);
        current.add(3);

        let result = calculate_jsd(&baseline, &current);
        assert!(result.jsd_score > 0.0); // Should be non-zero
        assert_eq!(result.template_contributions.len(), 3);
    }

    #[test]
    fn test_top_contributors() {
        let mut baseline = Histogram::new();
        baseline.add(1);
        baseline.add(2);
        baseline.add(3);

        let mut current = Histogram::new();
        current.add(1);
        current.add(3);
        current.add(3);
        current.add(3);

        let result = calculate_jsd(&baseline, &current);
        let top_2 = get_top_contributors(&result, 2);

        assert_eq!(top_2.len(), 2);
        // Template with highest contribution should be first
        assert!(top_2[0].contribution >= top_2[1].contribution);
    }

    #[test]
    fn test_new_template_appears() {
        let mut baseline = Histogram::new();
        baseline.add(1);

        let mut current = Histogram::new();
        current.add(1);
        current.add(2);

        let result = calculate_jsd(&baseline, &current);

        // Find template 2's contribution
        let template_2_contrib = result
            .template_contributions
            .iter()
            .find(|c| c.template_id == 2)
            .unwrap();

        assert_eq!(template_2_contrib.baseline_probability, 0.0);
        assert!(template_2_contrib.current_probability > 0.0);
        assert_eq!(template_2_contrib.relative_change, 100.0);
    }

    #[test]
    fn test_jsd_bits_conversion() {
        let mut hist1 = Histogram::new();
        hist1.add(1);

        let mut hist2 = Histogram::new();
        hist2.add(2);

        let result_nats = calculate_jsd(&hist1, &hist2);
        let result_bits = calculate_jsd_bits(&hist1, &hist2);

        let ln2 = 2.0_f64.ln();
        assert!((result_bits.jsd_score - result_nats.jsd_score / ln2).abs() < 1e-10);
    }

    #[test]
    fn test_empty_histogram() {
        let empty = Histogram::new();
        let mut hist = Histogram::new();
        hist.add(1);

        // Empty baseline should return early with zero JSD
        let result = calculate_jsd(&empty, &hist);
        assert_eq!(result.jsd_score, 0.0);
        assert_eq!(result.template_contributions.len(), 0);

        // Empty current should also return early
        let result = calculate_jsd(&hist, &empty);
        assert_eq!(result.jsd_score, 0.0);
        assert_eq!(result.template_contributions.len(), 0);
    }

    #[test]
    fn test_very_skewed_distribution() {
        let mut baseline = Histogram::new();
        // Heavily skewed baseline
        for _ in 0..1000 {
            baseline.add(1);
        }
        baseline.add(2);

        let mut current = Histogram::new();
        // Different skew
        for _ in 0..1000 {
            current.add(2);
        }
        current.add(1);

        let result = calculate_jsd(&baseline, &current);

        // JSD should be non-zero and finite
        assert!(result.jsd_score > 0.0);
        assert!(result.jsd_score.is_finite());
        assert!(!result.jsd_score.is_nan());

        // All contributions should be non-negative and finite
        for contrib in &result.template_contributions {
            assert!(contrib.contribution >= 0.0);
            assert!(contrib.contribution.is_finite());
            assert!(!contrib.contribution.is_nan());
        }
    }

    #[test]
    fn test_many_templates() {
        let mut baseline = Histogram::new();
        let mut current = Histogram::new();

        // Add many templates to test performance
        for i in 0..1000 {
            baseline.add(i);
            if i % 2 == 0 {
                current.add(i);
            } else {
                current.add(i + 1000);
            }
        }

        let result = calculate_jsd(&baseline, &current);

        // Should handle large number of templates efficiently
        assert!(result.jsd_score > 0.0);
        assert!(result.jsd_score.is_finite());
        assert_eq!(result.template_contributions.len(), 1500); // 1000 from baseline + 500 unique from current
    }

    #[test]
    fn test_numerical_stability_small_probabilities() {
        let mut baseline = Histogram::new();
        let mut current = Histogram::new();

        // Add one very rare event and many common events
        baseline.add(1);
        current.add(1);

        for _ in 0..100000 {
            baseline.add(2);
            current.add(2);
        }

        let result = calculate_jsd(&baseline, &current);

        // Should handle very small probabilities without numerical issues
        assert!(result.jsd_score >= 0.0);
        assert!(result.jsd_score < 1e-5); // Should be very close to zero (nearly identical)
        assert!(result.jsd_score.is_finite());
        assert!(!result.jsd_score.is_nan());
    }
}
