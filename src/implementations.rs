use crate::llm_service::LLMServiceClient;
use crate::log_matcher::{LogMatcher, LogTemplate};
use crate::traits::{DatasetLoader, GroundTruthEntry, LogMatcherTrait, TemplateGenerator};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

pub struct LLMTemplateGenerator {
    client: LLMServiceClient,
    name: String,
}

impl LLMTemplateGenerator {
    pub fn new(provider: String, api_key: String, model: String) -> Self {
        let name = format!("{}/{}", provider, model);
        Self {
            client: LLMServiceClient::new(provider, api_key, model),
            name,
        }
    }

    pub fn ollama(model: &str) -> Self {
        Self::new("ollama".to_string(), "".to_string(), model.to_string())
    }

    pub fn mock() -> Self {
        Self::new("mock".to_string(), "".to_string(), "mock".to_string())
    }
}

#[async_trait]
impl TemplateGenerator for LLMTemplateGenerator {
    async fn generate_template(&self, log_line: &str) -> Result<LogTemplate> {
        self.client.generate_template(log_line).await
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Optimized log matcher with zero-copy optimizations
/// Uses LogMatcher internally with SmallVec, thread-local scratch buffers, and inline hints
pub struct RegexLogMatcher {
    matcher: LogMatcher,
}

impl RegexLogMatcher {
    pub fn new() -> Self {
        Self {
            matcher: LogMatcher::new(),
        }
    }
}

impl Default for RegexLogMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl LogMatcherTrait for RegexLogMatcher {
    fn add_template(&mut self, template: LogTemplate) {
        self.matcher.add_template(template);
    }

    fn match_log(&self, log_line: &str) -> Option<u64> {
        self.matcher.match_log(log_line)
    }

    fn match_batch(&self, log_lines: &[&str]) -> Vec<Option<u64>> {
        self.matcher.match_batch(log_lines)
    }

    fn match_batch_parallel(&self, log_lines: &[&str]) -> Vec<Option<u64>> {
        self.matcher.match_batch_parallel(log_lines)
    }

    fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.matcher.get_all_templates()
    }

    fn name(&self) -> &str {
        "OptimizedMatcher"
    }
}

pub struct OpenStackDatasetLoader {
    data_dir: String,
    dataset_name: String,
}

impl OpenStackDatasetLoader {
    pub fn new(data_dir: &str) -> Self {
        Self {
            data_dir: data_dir.to_string(),
            dataset_name: "OpenStack".to_string(),
        }
    }

    fn load_template_definitions(&self) -> HashMap<String, String> {
        let mut templates = HashMap::new();
        let path = format!("{}/OpenStack_2k.log_templates.csv", self.data_dir);

        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            lines.next();

            for line in lines {
                if let Ok(line) = line {
                    let parts: Vec<&str> = line.splitn(2, ',').collect();
                    if parts.len() == 2 {
                        let event_id = parts[0].to_string();
                        let template = parts[1].trim_matches('"').to_string();
                        templates.insert(event_id, template);
                    }
                }
            }
        }

        templates
    }
}

impl DatasetLoader for OpenStackDatasetLoader {
    fn load_raw_logs(&self) -> Result<Vec<String>> {
        let path = format!("{}/OpenStack_2k.log", self.data_dir);
        let mut logs = Vec::new();

        let file = File::open(&path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            logs.push(line?);
        }

        Ok(logs)
    }

    fn load_ground_truth(&self) -> Result<Vec<GroundTruthEntry>> {
        let path = format!("{}/OpenStack_2k.log_structured.csv", self.data_dir);
        let templates = self.load_template_definitions();
        let mut entries = Vec::new();

        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        lines.next();

        for line in lines {
            let line = line?;
            let parts: Vec<&str> = line.splitn(11, ',').collect();

            if parts.len() >= 10 {
                let log_line = parts[1].trim_matches('"').to_string();
                let event_id = parts[9].to_string();
                let expected_template = templates.get(&event_id).cloned();

                entries.push(GroundTruthEntry {
                    log_line,
                    event_id,
                    expected_template,
                });
            }
        }

        Ok(entries)
    }

    fn load_templates(&self) -> Result<HashMap<String, String>> {
        Ok(self.load_template_definitions())
    }

    fn name(&self) -> &str {
        &self.dataset_name
    }

    fn expected_template_count(&self) -> Option<usize> {
        Some(self.load_template_definitions().len())
    }
}

pub struct CsvDatasetLoader {
    csv_path: String,
    dataset_name: String,
    has_header: bool,
}

impl CsvDatasetLoader {
    pub fn new(csv_path: &str, dataset_name: &str, has_header: bool) -> Self {
        Self {
            csv_path: csv_path.to_string(),
            dataset_name: dataset_name.to_string(),
            has_header,
        }
    }
}

impl DatasetLoader for CsvDatasetLoader {
    fn load_raw_logs(&self) -> Result<Vec<String>> {
        let file = File::open(&self.csv_path)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        if self.has_header {
            lines.next();
        }

        let mut logs = Vec::new();
        for line in lines {
            let line = line?;
            if let Some(log_line) = line.split(',').next() {
                logs.push(log_line.trim_matches('"').to_string());
            }
        }

        Ok(logs)
    }

    fn load_ground_truth(&self) -> Result<Vec<GroundTruthEntry>> {
        let file = File::open(&self.csv_path)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        if self.has_header {
            lines.next();
        }

        let mut entries = Vec::new();
        for line in lines {
            let line = line?;
            let parts: Vec<&str> = line.split(',').collect();

            if parts.len() >= 2 {
                let log_line = parts[0].trim_matches('"').to_string();
                let event_id = parts[1].trim_matches('"').to_string();
                let expected_template = if parts.len() >= 3 {
                    Some(parts[2].trim_matches('"').to_string())
                } else {
                    None
                };

                entries.push(GroundTruthEntry {
                    log_line,
                    event_id,
                    expected_template,
                });
            }
        }

        Ok(entries)
    }

    fn name(&self) -> &str {
        &self.dataset_name
    }
}

/// In-memory dataset for testing
pub struct InMemoryDataset {
    logs: Vec<String>,
    ground_truth: Vec<GroundTruthEntry>,
    name: String,
}

impl InMemoryDataset {
    pub fn new(name: &str, logs: Vec<String>, ground_truth: Vec<GroundTruthEntry>) -> Self {
        Self {
            logs,
            ground_truth,
            name: name.to_string(),
        }
    }

    /// Create a simple test dataset with a few log patterns
    pub fn simple_test() -> Self {
        let logs = vec![
            "2025-01-15 10:30:45 INFO User alice logged in".to_string(),
            "2025-01-15 10:30:46 INFO User bob logged in".to_string(),
            "2025-01-15 10:30:47 ERROR Connection failed: timeout".to_string(),
            "2025-01-15 10:30:48 INFO User charlie logged in".to_string(),
            "2025-01-15 10:30:49 ERROR Connection failed: refused".to_string(),
        ];

        let ground_truth = vec![
            GroundTruthEntry {
                log_line: logs[0].clone(),
                event_id: "LOGIN".to_string(),
                expected_template: Some(
                    r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} INFO User \w+ logged in".to_string(),
                ),
            },
            GroundTruthEntry {
                log_line: logs[1].clone(),
                event_id: "LOGIN".to_string(),
                expected_template: Some(
                    r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} INFO User \w+ logged in".to_string(),
                ),
            },
            GroundTruthEntry {
                log_line: logs[2].clone(),
                event_id: "ERROR".to_string(),
                expected_template: Some(
                    r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} ERROR Connection failed: \w+".to_string(),
                ),
            },
            GroundTruthEntry {
                log_line: logs[3].clone(),
                event_id: "LOGIN".to_string(),
                expected_template: Some(
                    r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} INFO User \w+ logged in".to_string(),
                ),
            },
            GroundTruthEntry {
                log_line: logs[4].clone(),
                event_id: "ERROR".to_string(),
                expected_template: Some(
                    r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} ERROR Connection failed: \w+".to_string(),
                ),
            },
        ];

        Self::new("SimpleTest", logs, ground_truth)
    }
}

impl DatasetLoader for InMemoryDataset {
    fn load_raw_logs(&self) -> Result<Vec<String>> {
        Ok(self.logs.clone())
    }

    fn load_ground_truth(&self) -> Result<Vec<GroundTruthEntry>> {
        Ok(self.ground_truth.clone())
    }

    fn name(&self) -> &str {
        &self.name
    }
}
