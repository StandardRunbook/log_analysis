use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::log_matcher::LogTemplate;

/// Request to generate a log template from an unmatched log line
#[derive(Debug, Serialize)]
pub struct TemplateGenerationRequest {
    pub log_line: String,
    pub context: Option<Vec<String>>,
    pub instructions: String,
    pub examples: Vec<TemplateExample>,
}

/// Example of a good log template for the LLM to learn from
#[derive(Debug, Serialize)]
pub struct TemplateExample {
    pub log_line: String,
    pub template_pattern: String,
    pub variable_names: Vec<String>,
    pub explanation: String,
}

#[derive(Debug, Deserialize)]
pub struct TemplateGenerationResponse {
    pub template: LogTemplate,
}

pub struct LLMServiceClient {
    provider: String,
    api_key: String,
    model: String,
}

impl LLMServiceClient {
    pub fn new(provider: String, api_key: String, model: String) -> Self {
        tracing::info!(
            "🤖 LLM Service configured with provider: {}, model: {}",
            provider,
            model
        );
        Self {
            provider,
            api_key,
            model,
        }
    }

    /// Send a log line to the LLM service to generate a template
    pub async fn generate_template(&self, log_line: &str) -> Result<LogTemplate> {
        tracing::info!("Requesting LLM to generate template for: {}", log_line);

        // Build the request with instructions and examples
        let _request = self.build_template_request(log_line);
        // In production: let template = self.call_llm_api(&request).await?;

        // For now, return a mock generated template
        Ok(self.mock_template_generation(log_line))
    }

    /// Build a comprehensive request for the LLM with instructions and examples
    fn build_template_request(&self, log_line: &str) -> TemplateGenerationRequest {
        TemplateGenerationRequest {
            log_line: log_line.to_string(),
            context: None,
            instructions: Self::get_template_generation_instructions(),
            examples: Self::get_template_examples(),
        }
    }

    /// Get detailed instructions for the LLM on how to generate templates
    fn get_template_generation_instructions() -> String {
        r#"You are a log template generator. Your task is to create a regex pattern that can match similar log lines by identifying and masking ephemeral (changing) fields.

CRITICAL RULES:
1. **Mask ALL ephemeral fields**: Replace any values that change between log occurrences with regex capture groups
2. **Keep static text**: Preserve exact text that stays constant across similar logs
3. **Use descriptive variable names**: Name captured groups based on what they represent
4. **Be specific but flexible**: Pattern should match variations while being specific enough to identify this log type

EPHEMERAL FIELDS TO MASK (with capture group patterns):
- **Timestamps**: \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2} → Use (\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?)
- **IP Addresses**: 192.168.1.100 → Use (\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})
- **UUIDs/IDs**: 550e8400-e29b-41d4-a716-446655440000 → Use ([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})
- **Request IDs**: req_abc123xyz → Use ([a-zA-Z0-9_-]+)
- **User IDs**: user_12345 → Use ([a-zA-Z0-9_-]+)
- **Session IDs**: sess_a1b2c3d4 → Use ([a-zA-Z0-9_-]+)
- **Decimal numbers**: 45.23, 99.9 → Use (\d+\.\d+)
- **Integers**: 100, 5000 → Use (\d+)
- **Percentages**: 85.5% → Use (\d+\.?\d*)% (keep the % symbol as static text)
- **File paths**: /var/log/app.log → Use ([/\w\.-]+)
- **URLs**: https://api.example.com/v1/users → Use (https?://[^\s]+)
- **Quoted strings**: "some message" → Use "([^"]*)"
- **Durations**: 123ms, 5.5s → Use (\d+\.?\d*)(ms|s|m|h)
- **Byte sizes**: 1024KB, 2.5MB → Use (\d+\.?\d*)(B|KB|MB|GB|TB)
- **HTTP status codes**: 200, 404, 500 → Use (\d{3})
- **Error codes**: ERR_1234, ERROR-5678 → Use ([A-Z_-]+\d+)

PATTERN CONSTRUCTION:
1. Start with the exact log line
2. Identify all ephemeral fields (see list above)
3. Replace each ephemeral field with an appropriate capture group
4. Escape special regex characters in the static parts: . → \., ( → \(, [ → \[, etc.
5. Keep structural characters like colons, hyphens, spaces as-is
6. Ensure the pattern will match ONLY similar logs, not unrelated ones

VARIABLE NAMING:
- Use semantic names: "timestamp", "ip_address", "user_id", "error_code", "duration_ms"
- Avoid generic names like "value1", "field2" unless the field purpose is unclear
- Use snake_case for multi-word names
- Be consistent with naming conventions

OUTPUT FORMAT:
Return a JSON object with:
{
  "template_id": "unique_identifier",
  "pattern": "regex_pattern_with_capture_groups",
  "variables": ["list", "of", "variable_names"],
  "example": "original_log_line"
}

VALIDATION:
- The pattern MUST be valid regex
- Number of variables MUST equal number of capture groups in pattern
- Pattern MUST match the original log line
- Pattern should NOT be too generic (avoid matching unrelated logs)
- Pattern should NOT be too specific (should match similar logs with different values)
"#.to_string()
    }

    /// Get example templates to help the LLM understand the task
    fn get_template_examples() -> Vec<TemplateExample> {
        vec![
            TemplateExample {
                log_line: "2025-01-15T10:30:45Z [INFO] User user_12345 logged in from 192.168.1.100".to_string(),
                template_pattern: r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z) \[INFO\] User ([a-zA-Z0-9_]+) logged in from (\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})".to_string(),
                variable_names: vec!["timestamp".to_string(), "user_id".to_string(), "ip_address".to_string()],
                explanation: "Masked timestamp (ISO 8601), user ID (alphanumeric), and IP address. Kept static text '[INFO] User' and 'logged in from'.".to_string(),
            },
            TemplateExample {
                log_line: "Request req_abc123 completed in 145ms with status 200".to_string(),
                template_pattern: r"Request ([a-zA-Z0-9_]+) completed in (\d+)ms with status (\d{3})".to_string(),
                variable_names: vec!["request_id".to_string(), "duration_ms".to_string(), "status_code".to_string()],
                explanation: "Masked request ID, duration (integer), and HTTP status code (3 digits). Kept all connecting words.".to_string(),
            },
            TemplateExample {
                log_line: "ERROR: Database connection failed - host: db.example.com:5432, error: timeout after 30s".to_string(),
                template_pattern: r"ERROR: Database connection failed - host: ([a-zA-Z0-9\.-]+):(\d+), error: timeout after (\d+)s".to_string(),
                variable_names: vec!["hostname".to_string(), "port".to_string(), "timeout_seconds".to_string()],
                explanation: "Masked hostname (domain with dots), port number, and timeout value. Kept error message structure and 'timeout after' text.".to_string(),
            },
            TemplateExample {
                log_line: "cpu_usage: 67.8% - Server load increased".to_string(),
                template_pattern: r"cpu_usage: (\d+\.?\d*)% - (.*)".to_string(),
                variable_names: vec!["percentage".to_string(), "message".to_string()],
                explanation: "Masked percentage value (decimal or integer) and free-form message. Kept metric name 'cpu_usage:' and % symbol.".to_string(),
            },
            TemplateExample {
                log_line: "Processing file /var/log/app-2025-01-15.log size: 2.5MB".to_string(),
                template_pattern: r"Processing file ([/\w\.-]+) size: (\d+\.?\d*)(MB|GB|KB)".to_string(),
                variable_names: vec!["file_path".to_string(), "size_value".to_string(), "size_unit".to_string()],
                explanation: "Masked file path, size value, and size unit separately for flexibility. Kept 'Processing file' and 'size:'.".to_string(),
            },
            TemplateExample {
                log_line: "API call to https://api.service.com/v1/users/123 returned {\"status\":\"success\",\"count\":5}".to_string(),
                template_pattern: r"API call to (https?://[^\s]+) returned (\{.*\})".to_string(),
                variable_names: vec!["url".to_string(), "response_json".to_string()],
                explanation: "Masked full URL and JSON response body. Kept 'API call to' and 'returned' as static text.".to_string(),
            },
            TemplateExample {
                log_line: "Transaction txn_a1b2c3d4 from account_12345 to account_67890 amount: $150.00 status: completed".to_string(),
                template_pattern: r"Transaction ([a-zA-Z0-9_]+) from account_(\d+) to account_(\d+) amount: \$(\d+\.\d{2}) status: (\w+)".to_string(),
                variable_names: vec!["transaction_id".to_string(), "from_account".to_string(), "to_account".to_string(), "amount".to_string(), "status".to_string()],
                explanation: "Masked transaction ID, account numbers, amount (with 2 decimals), and status. Kept all structural words and $ symbol.".to_string(),
            },
            TemplateExample {
                log_line: "WARN [2025-01-15 10:30:45] Thread pool-3-thread-7 - Queue size: 1024/2048 (50%)".to_string(),
                template_pattern: r"WARN \[(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\] Thread ([a-zA-Z0-9\-]+) - Queue size: (\d+)/(\d+) \((\d+)%\)".to_string(),
                variable_names: vec!["timestamp".to_string(), "thread_name".to_string(), "current_size".to_string(), "max_size".to_string(), "percentage".to_string()],
                explanation: "Masked timestamp, thread name, current/max queue sizes, and percentage. Kept log level 'WARN' and structural text.".to_string(),
            },
        ]
    }

    /// Mock implementation - replace with actual LLM API call in production
    fn mock_template_generation(&self, log_line: &str) -> LogTemplate {
        // Extract a pattern using improved heuristics
        let (pattern, variables) = self.extract_pattern_with_variables(log_line);

        // Note: template_id will be assigned by LogMatcher when template is added
        LogTemplate {
            template_id: 0, // Placeholder, will be replaced by LogMatcher
            pattern,
            variables,
            example: log_line.to_string(),
        }
    }

    /// Improved pattern extraction with variable naming
    fn extract_pattern_with_variables(&self, log_line: &str) -> (String, Vec<String>) {
        let mut pattern = log_line.to_string();
        let mut variables = Vec::new();

        // Track replacements to avoid double-replacing
        let mut replacements = Vec::new();

        // 1. ISO 8601 timestamps
        if let Some(mat) = regex::Regex::new(
            r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?",
        )
        .unwrap()
        .find(&pattern)
        {
            replacements.push((
                mat.start(),
                mat.end(),
                r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?)"
                    .to_string(),
            ));
            variables.push("timestamp".to_string());
        }

        // 2. IP addresses
        for mat in regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")
            .unwrap()
            .find_iter(&pattern)
        {
            replacements.push((
                mat.start(),
                mat.end(),
                r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})".to_string(),
            ));
            variables.push("ip_address".to_string());
        }

        // 3. UUIDs
        for mat in
            regex::Regex::new(r"\b[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\b")
                .unwrap()
                .find_iter(&pattern)
        {
            replacements.push((
                mat.start(),
                mat.end(),
                r"([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})".to_string(),
            ));
            variables.push("uuid".to_string());
        }

        // 4. Percentages (before general decimals)
        for mat in regex::Regex::new(r"\d+\.?\d*%")
            .unwrap()
            .find_iter(&pattern)
        {
            replacements.push((mat.start(), mat.end() - 1, r"(\d+\.?\d*)".to_string()));
            variables.push("percentage".to_string());
        }

        // 5. Byte sizes
        for mat in regex::Regex::new(r"\d+\.?\d*(B|KB|MB|GB|TB)\b")
            .unwrap()
            .find_iter(&pattern)
        {
            let unit_start = mat.end()
                - mat
                    .as_str()
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphabetic())
                    .count();
            replacements.push((mat.start(), unit_start, r"(\d+\.?\d*)".to_string()));
            variables.push("size".to_string());
        }

        // 6. Durations
        for mat in regex::Regex::new(r"\d+\.?\d*(ms|s|m|h)\b")
            .unwrap()
            .find_iter(&pattern)
        {
            let unit_start = mat.end()
                - mat
                    .as_str()
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphabetic())
                    .count();
            replacements.push((mat.start(), unit_start, r"(\d+\.?\d*)".to_string()));
            variables.push("duration".to_string());
        }

        // 7. Decimal numbers (that aren't part of above patterns)
        for mat in regex::Regex::new(r"\b\d+\.\d+\b")
            .unwrap()
            .find_iter(&pattern)
        {
            if !replacements
                .iter()
                .any(|(start, end, _)| mat.start() >= *start && mat.end() <= *end)
            {
                replacements.push((mat.start(), mat.end(), r"(\d+\.\d+)".to_string()));
                variables.push("decimal_value".to_string());
            }
        }

        // 8. Integers (that aren't part of above patterns)
        for mat in regex::Regex::new(r"\b\d+\b").unwrap().find_iter(&pattern) {
            if !replacements
                .iter()
                .any(|(start, end, _)| mat.start() >= *start && mat.end() <= *end)
            {
                replacements.push((mat.start(), mat.end(), r"(\d+)".to_string()));
                variables.push("numeric_value".to_string());
            }
        }

        // Apply replacements in reverse order to maintain positions
        replacements.sort_by(|a, b| b.0.cmp(&a.0));
        for (start, end, replacement) in replacements {
            pattern.replace_range(start..end, &replacement);
        }

        // Escape special regex characters in the static parts
        pattern = pattern.replace(".", r"\.");
        pattern = pattern.replace("(", r"\(");
        pattern = pattern.replace(")", r"\)");
        pattern = pattern.replace("[", r"\[");
        pattern = pattern.replace("]", r"\]");
        pattern = pattern.replace("+", r"\+");
        pattern = pattern.replace("*", r"\*");
        pattern = pattern.replace("?", r"\?");
        pattern = pattern.replace("$", r"\$");
        pattern = pattern.replace("^", r"\^");
        pattern = pattern.replace("|", r"\|");

        (pattern, variables)
    }

    // Uncomment this for actual LLM API integration using Rig
    /*
    async fn call_llm_api(&self, request: &TemplateGenerationRequest) -> Result<LogTemplate> {
        use rig_core::{completion::Prompt, providers};

        // Create the appropriate client based on provider
        let client = match self.provider.as_str() {
            "openai" => {
                providers::openai::Client::new(&self.api_key)
                    .completion_model(&self.model)
            }
            "anthropic" => {
                providers::anthropic::Client::new(&self.api_key)
                    .completion_model(&self.model)
            }
            _ => return Err(anyhow::anyhow!("Unsupported LLM provider: {}", self.provider)),
        };

        // Build the prompt with instructions
        let prompt = format!(
            "{}\n\nLog line to analyze:\n{}\n\nProvide the template in JSON format with fields: template_id, pattern, variable_names.",
            request.instructions,
            request.log_line
        );

        // Call the LLM
        let response = client.prompt(&prompt).await?;

        // Parse the response (this is simplified - you'd want better JSON extraction)
        // For now, return a mock response
        // TODO: Parse the actual LLM response and extract the template
        Ok(LogTemplate {
            template_id: 1,
            pattern: "placeholder".to_string(),
            variable_names: vec![],
        })
    }
    */
}

// Add uuid dependency for generating unique IDs
mod uuid {
    pub struct Uuid;

    impl Uuid {
        pub fn new_v4() -> Self {
            Uuid
        }

        pub fn to_string(&self) -> String {
            use std::time::{SystemTime, UNIX_EPOCH};
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            format!("{:x}", timestamp)
        }
    }
}
