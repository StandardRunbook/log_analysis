/// LogHub dataset loader
///
/// Loads datasets from LogHub format with pre-generated templates
use crate::traits::{DatasetLoader, GroundTruthEntry};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Deserialize)]
struct LogHubTemplate {
    #[serde(rename = "EventId")]
    event_id: String,
    #[serde(rename = "EventTemplate")]
    event_template: String,
}

/// Convert LogHub template format (<*>) to regex
fn loghub_template_to_regex(template: &str) -> String {
    // Escape regex special characters except <*>
    let mut result = String::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                // Check if this is <*>
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume *
                    if chars.peek() == Some(&'>') {
                        chars.next(); // consume >
                        result.push_str(r"[\s\S]+?"); // Non-greedy match for anything
                    } else {
                        result.push_str(r"<\*"); // Literal <*
                    }
                } else {
                    result.push('<');
                }
            }
            // Escape regex special characters
            '.' | '^' | '$' | '|' | '?' | '+' | '(' | ')' | '{' | '}' | '\\' | '[' | ']' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }

    result
}

pub struct LogHubDatasetLoader {
    dataset_name: String,
    log_file: String,
    template_file: String,
}

impl LogHubDatasetLoader {
    pub fn new(dataset_name: &str, base_path: &str) -> Self {
        let log_file = format!("{}/{}/{}_2k.log", base_path, dataset_name, dataset_name);
        let template_file = format!(
            "{}/{}/{}_2k.log_templates.csv",
            base_path, dataset_name, dataset_name
        );

        Self {
            dataset_name: dataset_name.to_string(),
            log_file,
            template_file,
        }
    }

    /// Load templates from LogHub CSV format
    pub fn load_templates(&self) -> Result<HashMap<String, String>> {
        let content = fs::read_to_string(&self.template_file)
            .with_context(|| format!("Failed to read template file: {}", self.template_file))?;

        let mut reader = csv::Reader::from_reader(content.as_bytes());
        let mut templates = HashMap::new();

        for result in reader.deserialize() {
            let record: LogHubTemplate = result?;
            let regex = loghub_template_to_regex(&record.event_template);
            templates.insert(record.event_id, regex);
        }

        Ok(templates)
    }
}

impl DatasetLoader for LogHubDatasetLoader {
    fn load_raw_logs(&self) -> Result<Vec<String>> {
        let content = fs::read_to_string(&self.log_file)
            .with_context(|| format!("Failed to read log file: {}", self.log_file))?;

        Ok(content.lines().map(|s| s.to_string()).collect())
    }

    fn load_ground_truth(&self) -> Result<Vec<GroundTruthEntry>> {
        // Load raw logs (one line per log)
        let raw_logs = self.load_raw_logs()?;

        // Load from structured CSV file to get event IDs
        let structured_file = format!(
            "data/loghub/{}/{}_2k.log_structured.csv",
            self.dataset_name, self.dataset_name
        );

        let content = fs::read_to_string(&structured_file)
            .with_context(|| format!("Failed to read structured file: {}", structured_file))?;

        let mut reader = csv::Reader::from_reader(content.as_bytes());
        let mut entries = Vec::new();

        for (idx, result) in reader.deserialize().enumerate() {
            #[derive(serde::Deserialize)]
            struct StructuredLog {
                #[serde(rename = "LineId")]
                _line_id: usize,
                #[serde(rename = "EventId")]
                event_id: String,
            }

            let record: StructuredLog = result?;

            // Use the original log line from raw_logs
            if let Some(log_line) = raw_logs.get(idx) {
                entries.push(GroundTruthEntry {
                    log_line: log_line.clone(),
                    event_id: record.event_id.clone(),
                    expected_template: Some(record.event_id),
                });
            }
        }

        Ok(entries)
    }

    fn name(&self) -> &str {
        &self.dataset_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loghub_template_to_regex() {
        assert_eq!(
            loghub_template_to_regex("<*>Adding an already existing block<*>"),
            r"[\s\S]+?Adding an already existing block[\s\S]+?"
        );

        assert_eq!(
            loghub_template_to_regex("Received block <*> of size <*> from <*>"),
            r"Received block [\s\S]+? of size [\s\S]+? from [\s\S]+?"
        );

        // Test escaping of regex special characters
        assert_eq!(
            loghub_template_to_regex("Error: Connection failed (timeout)"),
            r"Error: Connection failed \(timeout\)"
        );

        // Test with actual OpenStack template
        assert_eq!(
            loghub_template_to_regex("[instance: <*>] Creating image"),
            r"\[instance: [\s\S]+?\] Creating image"
        );
    }
}
