/// Dependency injection traits for benchmarking and testing
///
/// This module provides trait-based abstractions for:
/// - Template generation (LLM services)
/// - Log matching
/// - Dataset loading
///
/// This allows you to easily swap implementations for testing, benchmarking,
/// or using different LLM providers.
use crate::log_matcher::LogTemplate;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

// ============================================================================
// Template Generation Trait
// ============================================================================

/// Trait for generating log templates from unmatched log lines
///
/// Implementations can use:
/// - LLM APIs (OpenAI, Anthropic, Ollama, etc.)
/// - Rule-based heuristics
/// - Pre-trained models
/// - Mock/test generators
#[async_trait]
pub trait TemplateGenerator: Send + Sync {
    /// Generate a log template from a log line
    ///
    /// # Arguments
    /// * `log_line` - The log line to generate a template for
    ///
    /// # Returns
    /// A `LogTemplate` with pattern, variables, and example
    async fn generate_template(&self, log_line: &str) -> Result<LogTemplate>;

    /// Optional: Generate templates in batch for efficiency
    ///
    /// Default implementation calls `generate_template` for each line
    async fn generate_batch(&self, log_lines: &[&str]) -> Result<Vec<LogTemplate>> {
        let mut templates = Vec::new();
        for line in log_lines {
            templates.push(self.generate_template(line).await?);
        }
        Ok(templates)
    }

    /// Get the name/identifier of this generator (for reporting)
    fn name(&self) -> &str;
}

// ============================================================================
// Log Matcher Trait
// ============================================================================

/// Trait for matching log lines against templates
///
/// Implementations can use:
/// - Regex-based matching
/// - Aho-Corasick algorithm
/// - Trie-based matching
/// - ML-based classification
pub trait LogMatcherTrait: Send + Sync {
    /// Add a template to the matcher
    fn add_template(&mut self, template: LogTemplate);

    /// Add multiple templates at once
    fn add_templates(&mut self, templates: Vec<LogTemplate>) {
        for template in templates {
            self.add_template(template);
        }
    }

    /// Match a single log line against known templates
    ///
    /// # Returns
    /// - `Some(template_id)` if matched
    /// - `None` if no match found
    fn match_log(&self, log_line: &str) -> Option<u64>;

    /// Match multiple log lines (batch processing)
    ///
    /// Default implementation calls `match_log` for each line
    fn match_batch(&self, log_lines: &[&str]) -> Vec<Option<u64>> {
        log_lines.iter().map(|line| self.match_log(line)).collect()
    }

    /// Get all templates currently in the matcher
    fn get_all_templates(&self) -> Vec<LogTemplate>;

    /// Get the number of templates
    fn template_count(&self) -> usize {
        self.get_all_templates().len()
    }

    /// Get the name/identifier of this matcher (for reporting)
    fn name(&self) -> &str;
}

// ============================================================================
// Dataset Loader Trait
// ============================================================================

/// Ground truth data for a single log line
#[derive(Debug, Clone)]
pub struct GroundTruthEntry {
    /// The original log line
    pub log_line: String,
    /// The event ID (template group) from ground truth
    pub event_id: String,
    /// Optional: the expected template pattern
    pub expected_template: Option<String>,
}

/// Trait for loading datasets for benchmarking
///
/// Implementations can load from:
/// - LogHub datasets (OpenStack, HDFS, etc.)
/// - Custom CSV files
/// - Databases
/// - In-memory test data
pub trait DatasetLoader: Send + Sync {
    /// Load raw log lines (no ground truth)
    fn load_raw_logs(&self) -> Result<Vec<String>>;

    /// Load structured data with ground truth labels
    fn load_ground_truth(&self) -> Result<Vec<GroundTruthEntry>>;

    /// Load template definitions (event_id -> template pattern)
    fn load_templates(&self) -> Result<HashMap<String, String>> {
        // Default implementation: extract from ground truth
        let gt = self.load_ground_truth()?;
        let mut templates = HashMap::new();
        for entry in gt {
            if let Some(template) = entry.expected_template {
                templates.insert(entry.event_id, template);
            }
        }
        Ok(templates)
    }

    /// Get the dataset name (for reporting)
    fn name(&self) -> &str;

    /// Get the expected number of unique templates/groups
    fn expected_template_count(&self) -> Option<usize> {
        None // Optional override
    }
}

// ============================================================================
// Benchmark Configuration
// ============================================================================

/// Configuration for running benchmarks
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Maximum number of logs to process (for quick tests)
    pub max_logs: Option<usize>,
    /// Whether to use batch processing where available
    pub use_batch: bool,
    /// Print detailed progress updates
    pub verbose: bool,
    /// Minimum expected accuracy (for assertions)
    pub min_accuracy: f64,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            max_logs: None,
            use_batch: true,
            verbose: true,
            min_accuracy: 70.0,
            metadata: HashMap::new(),
        }
    }
}

/// Results from a benchmark run
#[derive(Debug, Clone)]
pub struct BenchmarkResults {
    /// Total number of logs processed
    pub total_logs: usize,
    /// Number of templates generated
    pub templates_generated: usize,
    /// Total processing time in seconds
    pub elapsed_secs: f64,
    /// Throughput (logs/sec)
    pub throughput: f64,
    /// Average latency per log (ms)
    pub avg_latency_ms: f64,
    /// Grouping accuracy (0-100)
    pub grouping_accuracy: f64,
    /// Correctly grouped logs
    pub correct: usize,
    /// Incorrectly grouped logs
    pub incorrect: usize,
    /// Unmatched logs
    pub unmatched: usize,
    /// Expected number of groups (from dataset)
    pub expected_groups: usize,
    /// Actual number of groups (templates generated)
    pub actual_groups: usize,
    /// Additional metrics
    pub metadata: HashMap<String, String>,
}

impl BenchmarkResults {
    /// Pretty-print the results
    pub fn print(&self, title: &str) {
        println!("\n{}", "=".repeat(80));
        println!("📊 {}", title);
        println!("{}\n", "=".repeat(80));

        println!("📈 Performance Metrics:");
        println!("   Total logs:              {:>10}", self.total_logs);
        println!(
            "   Templates generated:     {:>10}",
            self.templates_generated
        );
        println!("   Parse time:              {:>10.2}s", self.elapsed_secs);
        println!(
            "   Throughput:              {:>10.0} logs/sec",
            self.throughput
        );
        println!(
            "   Avg latency:             {:>10.2}ms per log\n",
            self.avg_latency_ms
        );

        println!("🎯 Accuracy Metrics:");
        println!("   Expected groups:         {:>10}", self.expected_groups);
        println!("   Actual groups:           {:>10}", self.actual_groups);
        println!(
            "   Group ratio:             {:>10.2}x",
            self.actual_groups as f64 / self.expected_groups.max(1) as f64
        );
        println!();
        println!(
            "   Correctly grouped:       {:>10} ({:.1}%)",
            self.correct,
            (self.correct as f64 / self.total_logs as f64) * 100.0
        );
        println!(
            "   Incorrectly grouped:     {:>10} ({:.1}%)",
            self.incorrect,
            (self.incorrect as f64 / self.total_logs as f64) * 100.0
        );
        println!(
            "   Unmatched:               {:>10} ({:.1}%)",
            self.unmatched,
            (self.unmatched as f64 / self.total_logs as f64) * 100.0
        );
        println!();
        println!(
            "   🎯 Grouping Accuracy:     {:>9.2}%",
            self.grouping_accuracy
        );

        if !self.metadata.is_empty() {
            println!("\n📝 Additional Metadata:");
            for (key, value) in &self.metadata {
                println!("   {}: {}", key, value);
            }
        }

        println!("\n{}", "=".repeat(80));
    }
}
