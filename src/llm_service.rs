use anyhow::Result;

use crate::log_matcher::LogTemplate;

// Removed unused structs: TemplateGenerationRequest, TemplateExample, TemplateGenerationResponse

pub struct LLMServiceClient {
    provider: String,
    model: String,
    api_key: String,
    http_client: reqwest::Client,
    ollama_endpoint: String,
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
            model,
            api_key,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            ollama_endpoint: "http://localhost:11434".to_string(),
        }
    }

    /// Send a log line to the LLM service to generate a template
    pub async fn generate_template(&self, log_line: &str) -> Result<LogTemplate> {
        tracing::debug!("Requesting LLM to generate template for: {}", log_line);

        // Use Ollama if provider is "ollama", otherwise use mock
        if self.provider == "ollama" {
            self.call_ollama_api(log_line).await
        } else {
            Ok(self.mock_template_generation(log_line))
        }
    }

    /// Call Ollama API to generate a log template
    async fn call_ollama_api(&self, log_line: &str) -> Result<LogTemplate> {
        let prompt = self.build_ollama_prompt(log_line);

        let request_body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.1,
                "top_p": 0.9,
            }
        });

        let response = self
            .http_client
            .post(format!("{}/api/generate", self.ollama_endpoint))
            .json(&request_body)
            .send()
            .await?;

        let response_json: serde_json::Value = response.json().await?;

        if let Some(generated_text) = response_json.get("response").and_then(|v| v.as_str()) {
            self.parse_llm_response(log_line, generated_text)
        } else {
            tracing::warn!("Failed to parse Ollama response, falling back to mock");
            Ok(self.mock_template_generation(log_line))
        }
    }

    /// Build prompt for Ollama
    fn build_ollama_prompt(&self, log_line: &str) -> String {
        format!(
            r#"Create a regex pattern for this log line by replacing ONLY ephemeral (changing) values with capture groups.

CRITICAL RULES:
1. **DO NOT use generic catch-all patterns like (.+?) or (.+) or (.*)** unless absolutely necessary
2. **Keep all static text EXACTLY as-is** - keywords, error messages, field names, etc.
3. **Only mask values that actually change** - timestamps, IPs, numbers, IDs, usernames, paths, etc.
4. **Be specific with the message part** - break it down into static keywords + variable values

LINUX/UNIX SYSLOG FORMAT:
Most system logs follow: Month Day HH:MM:SS hostname service[pid]: message

STRUCTURE TO PRESERVE:
- Timestamp: ([A-Z][a-z]{{2}}\s+\d{{1,2}} \d{{2}}:\d{{2}}:\d{{2}})
- Hostname: ([\w\.-]+)
- Service name: ([\w()_]+) - keep exact service name if it doesn't change!
- PID (if present): \[(\d+)\]
- Separator: : (colon with space)
- Message: Break down into static keywords + specific variable patterns

WHAT TO MASK (with specific patterns):
- Timestamps: ([A-Z][a-z]{{2}}\s+\d{{1,2}} \d{{2}}:\d{{2}}:\d{{2}})
- IP addresses: (\d{{1,3}}\.\d{{1,3}}\.\d{{1,3}}\.\d{{1,3}})
- Numbers: (\d+)
- Usernames: ([\w]+) after "user " keyword
- Paths: ([/\w\.-]+)
- Hostnames/domains in message: ([\\w.-]+) - MUST use \\w not \\d because hostnames contain letters!

WHAT NOT TO MASK (keep as static text):
- Keywords: "authentication", "failure", "opened", "closed", "startup", "shutdown"
- Field names: "uid=", "euid=", "tty=", "rhost=", "logname="
- Service names: "sshd", "kernel", "cups", "gpm"
- Error messages: "Auto-detected", "session opened", "restart"

GOOD EXAMPLES (from real successfully matching templates - follow these exactly):

Input: Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=218.188.2.4
Output: {{"pattern": "^([A-Z][a-z]{{2}} \\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w-]+) sshd\\(pam_unix\\)\\[(\\d+)\\]: authentication failure; logname=(.*?) uid=(\\d+) euid=(\\d+) tty=([\\w]+) ruser=(.*?) rhost=([\\d.]+)\\s*$", "variables": ["timestamp", "hostname", "pid", "logname", "uid", "euid", "tty", "ruser", "rhost"]}}
Note: Pattern starts with ^ and ends with \\s*$ to match optional trailing whitespace. Keep exact service name "sshd(pam_unix)"

Input: Jun 15 02:04:59 combo sshd(pam_unix)[20882]: authentication failure; logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=220-135-151-1.hinet-ip.hinet.net  user=root
Output: {{"pattern": "^([A-Z][a-z]{{2}} \\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w-]+) sshd\\(pam_unix\\)\\[(\\d+)\\]: authentication failure; logname=(.*?) uid=(\\d+) euid=(\\d+) tty=([\\w]+) ruser=(.*?) rhost=([\\w.-]+)\\s+user=([\\w]+)\\s*$", "variables": ["timestamp", "hostname", "pid", "logname", "uid", "euid", "tty", "ruser", "rhost", "username"]}}
Note: Has "user=" field at end. rhost uses [\\w.-]+ (not [\\d.-]+) because hostnames contain LETTERS. Ends with \\s*$ for trailing spaces

Input: Jun 15 04:06:18 combo su(pam_unix)[21416]: session opened for user cyrus by (uid=0)
Output: {{"pattern": "^([A-Z][a-z]{{2}} \\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w-]+) su\\(pam_unix\\)\\[(\\d+)\\]: session opened for user ([\\w]+) by \\(uid=(\\d+)\\)\\s*$", "variables": ["timestamp", "hostname", "pid", "username", "uid"]}}
Note: Starts with ^, ends with \\s*$. Keep exact service "su(pam_unix)", keep "session opened for user" and "by (uid=" static

Input: Jun 15 04:12:43 combo su(pam_unix)[22644]: session closed for user news
Output: {{"pattern": "^([A-Z][a-z]{{2}} \\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w-]+) su\\(pam_unix\\)\\[(\\d+)\\]: session closed for user ([\\w]+)\\s*$", "variables": ["timestamp", "hostname", "pid", "username"]}}

Input: Jul  7 08:06:12 combo gpm[2094]: imps2: Auto-detected intellimouse PS/2
Output: {{"pattern": "^([A-Z][a-z]{{2}}\\s+\\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w\\.-]+) gpm\\[(\\d+)\\]: imps2: Auto-detected intellimouse PS/2\\s*$", "variables": ["timestamp", "hostname", "pid"]}}
Note: Keep service "gpm", keep ENTIRE message static - nothing changes. Note the \\s+ in timestamp for two-space dates

Input: Jun 19 04:08:57 combo cups: cupsd shutdown succeeded
Output: {{"pattern": "^([A-Z][a-z]{{2}} \\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w\\.-]+) cups: cupsd shutdown succeeded\\s*$", "variables": ["timestamp", "hostname"]}}
Note: No PID here, keep entire message static

Input: Jul 27 14:41:58 combo kernel: usbcore: registered new driver usbfs
Output: {{"pattern": "^([A-Z][a-z]{{2}} \\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w\\.-]+) kernel: usbcore: registered new driver ([\\w]+)\\s*$", "variables": ["timestamp", "hostname", "driver_name"]}}
Note: Keep "usbcore: registered new driver" static, only mask driver name

BAD EXAMPLES (too generic - DO NOT DO THIS):

Input: Jul  7 08:06:12 combo gpm[2094]: imps2: Auto-detected intellimouse PS/2
Bad Output: {{"pattern": "([A-Z][a-z]{{2}}\\s+\\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w\\.-]+) ([\\w()_]+)\\[(\\d+)\\]: (.+)", "variables": ["timestamp", "hostname", "service", "pid", "message"]}}
Why Bad: (.+) matches ANY message, making template too generic

Input: Jun 19 04:08:57 combo cups: cupsd shutdown succeeded
Bad Output: {{"pattern": "([A-Z][a-z]{{2}} \\d{{1,2}} \\d{{2}}:\\d{{2}}:\\d{{2}}) ([\\w\\.-]+) ([\\w]+): (.+)", "variables": ["timestamp", "hostname", "service", "message"]}}
Why Bad: (.+) matches ANY message after service name

Now convert this log line:

LOG LINE: {log_line}

CRITICAL: Pattern must match the ENTIRE log line from start to end:
- Start pattern with ^ (start of line anchor)
- End pattern with $ (end of line anchor)
- Example: ^([A-Z][a-z]{{2}}... your pattern ...)$

Respond with ONLY the JSON object, no explanation:
{{"pattern": "^...$", "variables": [...]}}
"#,
            log_line = log_line
        )
    }

    /// Parse LLM response and extract LogTemplate
    fn parse_llm_response(&self, log_line: &str, llm_output: &str) -> Result<LogTemplate> {
        // Extract JSON from the response (LLM might add extra text)
        // Use char_indices to ensure we slice on character boundaries
        let json_start = llm_output
            .char_indices()
            .find(|(_, c)| *c == '{')
            .map(|(i, _)| i)
            .unwrap_or(0);
        let json_end = llm_output
            .char_indices()
            .rev()
            .find(|(_, c)| *c == '}')
            .map(|(i, _)| i + '}'. len_utf8())
            .unwrap_or(llm_output.len());

        // Ensure indices are valid
        let json_str = if json_start < json_end && json_end <= llm_output.len() {
            &llm_output[json_start..json_end]
        } else {
            llm_output
        };

        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(json) => {
                let pattern = json
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or(log_line)
                    .to_string();

                let variables = json
                    .get("variables")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_else(Vec::new);

                // Generate a unique template ID
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                pattern.hash(&mut hasher);
                let template_id = hasher.finish();

                Ok(LogTemplate {
                    template_id,
                    pattern,
                    variables,
                    example: log_line.to_string(),
                })
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse LLM JSON response: {}. Response: {}",
                    e,
                    llm_output
                );
                Ok(self.mock_template_generation(log_line))
            }
        }
    }

    // Removed unused method: format_examples_for_prompt

    // Removed unused method: build_template_request

    /// Get detailed instructions for the LLM on how to generate templates
    #[allow(dead_code)]
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

    // Removed unused method: get_template_examples

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
        let original = log_line.to_string();
        let mut variables = Vec::new();

        // Handle empty or whitespace-only input
        if original.trim().is_empty() {
            return (".*".to_string(), variables); // Match anything
        }

        // Track replacements to avoid double-replacing
        // Each replacement stores: (start, end, placeholder_marker)
        let mut replacements = Vec::new();

        // 1. ISO 8601 timestamps
        if let Some(mat) = regex::Regex::new(
            r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?",
        )
        .unwrap()
        .find(&original)
        {
            replacements.push((mat.start(), mat.end(), "TIMESTAMP"));
            variables.push("timestamp".to_string());
        }

        // 2. IP addresses
        for mat in regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")
            .unwrap()
            .find_iter(&original)
        {
            replacements.push((mat.start(), mat.end(), "IPADDR"));
            variables.push("ip_address".to_string());
        }

        // 3. UUIDs
        for mat in
            regex::Regex::new(r"\b[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}\b")
                .unwrap()
                .find_iter(&original)
        {
            replacements.push((mat.start(), mat.end(), "UUID"));
            variables.push("uuid".to_string());
        }

        // 4. Percentages (before general decimals)
        for mat in regex::Regex::new(r"\d+\.?\d*%")
            .unwrap()
            .find_iter(&original)
        {
            replacements.push((mat.start(), mat.end() - 1, "PERCENT"));
            variables.push("percentage".to_string());
        }

        // 5. Byte sizes
        for mat in regex::Regex::new(r"\d+\.?\d*(B|KB|MB|GB|TB)\b")
            .unwrap()
            .find_iter(&original)
        {
            let unit_start = mat.end()
                - mat
                    .as_str()
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphabetic())
                    .count();
            replacements.push((mat.start(), unit_start, "BYTESIZE"));
            variables.push("size".to_string());
        }

        // 6. Durations
        for mat in regex::Regex::new(r"\d+\.?\d*(ms|s|m|h)\b")
            .unwrap()
            .find_iter(&original)
        {
            let unit_start = mat.end()
                - mat
                    .as_str()
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphabetic())
                    .count();
            replacements.push((mat.start(), unit_start, "DURATION"));
            variables.push("duration".to_string());
        }

        // 7. Decimal numbers (that aren't part of above patterns)
        for mat in regex::Regex::new(r"\b\d+\.\d+\b")
            .unwrap()
            .find_iter(&original)
        {
            if !replacements
                .iter()
                .any(|(start, end, _)| mat.start() >= *start && mat.end() <= *end)
            {
                replacements.push((mat.start(), mat.end(), "DECIMAL"));
                variables.push("decimal_value".to_string());
            }
        }

        // 8. Integers (that aren't part of above patterns)
        for mat in regex::Regex::new(r"\b\d+\b").unwrap().find_iter(&original) {
            if !replacements
                .iter()
                .any(|(start, end, _)| mat.start() >= *start && mat.end() <= *end)
            {
                replacements.push((mat.start(), mat.end(), "NUMBER"));
                variables.push("numeric_value".to_string());
            }
        }

        // 9. File paths (before hostnames to avoid confusion)
        for mat in regex::Regex::new(r"/[\w/.-]+")
            .unwrap()
            .find_iter(&original)
        {
            if !replacements
                .iter()
                .any(|(start, end, _)| mat.start() >= *start && mat.end() <= *end)
            {
                replacements.push((mat.start(), mat.end(), "FILEPATH"));
                variables.push("file_path".to_string());
            }
        }

        // 10. Hostnames and domains
        for mat in regex::Regex::new(
            r"\b[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+\b",
        )
        .unwrap()
        .find_iter(&original)
        {
            if !replacements
                .iter()
                .any(|(start, end, _)| mat.start() >= *start && mat.end() <= *end)
            {
                replacements.push((mat.start(), mat.end(), "HOSTNAME"));
                variables.push("hostname".to_string());
            }
        }

        // 11. Request/Transaction IDs (req_xxx, txn_xxx, id_xxx patterns)
        for mat in
            regex::Regex::new(r"\b(req|request|txn|transaction|session|id|trace)_[a-zA-Z0-9_-]+\b")
                .unwrap()
                .find_iter(&original)
        {
            if !replacements
                .iter()
                .any(|(start, end, _)| mat.start() >= *start && mat.end() <= *end)
            {
                replacements.push((mat.start(), mat.end(), "REQID"));
                variables.push("request_id".to_string());
            }
        }

        // 12. Common variable words (usernames, etc.)
        // Look for lowercase words after common keywords
        for mat in regex::Regex::new(
            r"(?i)\b(user|host|server|node|instance|account|name)\s+([a-z0-9_-]+)\b",
        )
        .unwrap()
        .find_iter(&original)
        {
            // Only replace the word after the keyword
            if let Some(word_match) = regex::Regex::new(r"([a-z0-9_-]+)$")
                .unwrap()
                .find(mat.as_str())
            {
                let word_start = mat.start() + word_match.start();
                let word_end = mat.start() + word_match.end();

                if !replacements
                    .iter()
                    .any(|(start, end, _)| word_start >= *start && word_end <= *end)
                {
                    replacements.push((word_start, word_end, "WORD"));
                    variables.push("identifier".to_string());
                }
            }
        }

        // Step 1: Build pattern with placeholders
        let mut pattern = original.clone();
        replacements.sort_by(|a, b| b.0.cmp(&a.0));
        for (start, end, placeholder) in &replacements {
            // Ensure we're operating on valid char boundaries
            if *start <= pattern.len() && *end <= pattern.len() && pattern.is_char_boundary(*start) && pattern.is_char_boundary(*end) {
                pattern.replace_range(*start..*end, placeholder);
            }
        }

        // Step 2: Escape special regex characters in static parts (before placeholders)
        // We need to escape these: . ( ) [ ] + * ? $ ^ | { }
        pattern = pattern.replace("\\", r"\\"); // Escape backslash first
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
        pattern = pattern.replace("{", r"\{");
        pattern = pattern.replace("}", r"\}");

        // Step 3: Replace placeholders with actual regex patterns
        pattern = pattern.replace(
            "TIMESTAMP",
            r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?)",
        );
        pattern = pattern.replace("IPADDR", r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})");
        pattern = pattern.replace(
            "UUID",
            r"([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})",
        );
        pattern = pattern.replace("PERCENT", r"(\d+\.?\d*)");
        pattern = pattern.replace("BYTESIZE", r"(\d+\.?\d*)");
        pattern = pattern.replace("DURATION", r"(\d+\.?\d*)");
        pattern = pattern.replace("DECIMAL", r"(\d+\.\d+)");
        pattern = pattern.replace("NUMBER", r"(\d+)");
        pattern = pattern.replace("FILEPATH", r"(/[\w/.-]+)");
        pattern = pattern.replace("HOSTNAME", r"([a-z0-9.-]+)");
        pattern = pattern.replace("REQID", r"([a-zA-Z0-9_-]+)");
        pattern = pattern.replace("WORD", r"([a-zA-Z0-9_-]+)");

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

    /// Generate a complete template from a log line (no fragmentation)
    pub async fn generate_template_from_log(&self, log_line: &str) -> Result<LogTemplate> {
        if self.provider == "openai" {
            self.call_openai_generate_template(log_line).await
        } else {
            self.call_ollama_api(log_line).await
        }
    }

    /// Simple OpenAI call for generic prompts (returns raw text)
    pub async fn call_openai_simple(&self, prompt: &str) -> Result<String> {
        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "temperature": 0.1,
            "max_tokens": 3000
        });

        let response = self
            .http_client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_json: serde_json::Value = response.json().await?;

        if !status.is_success() {
            tracing::error!("OpenAI API error: status={}, response={:?}", status, response_json);
            return Err(anyhow::anyhow!("OpenAI API returned error: {}", response_json));
        }

        if let Some(generated_text) = response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
        {
            Ok(generated_text.to_string())
        } else {
            Err(anyhow::anyhow!("No response from OpenAI"))
        }
    }

    /// Call OpenAI to generate a complete template
    async fn call_openai_generate_template(&self, log_line: &str) -> Result<LogTemplate> {
        let prompt = self.build_ollama_prompt(log_line);

        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "temperature": 0.1,
            "max_tokens": 1000
        });

        let response = self
            .http_client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_json: serde_json::Value = response.json().await?;

        if !status.is_success() {
            tracing::error!("OpenAI API error: status={}, response={:?}", status, response_json);
            return Err(anyhow::anyhow!("OpenAI API returned error: {}", response_json));
        }

        if let Some(generated_text) = response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
        {
            self.parse_llm_response(log_line, generated_text)
        } else {
            Err(anyhow::anyhow!("No response from OpenAI"))
        }
    }

    /// Classify log fragments using LLM
    pub async fn classify_fragments(&self, fragments: &[String], full_log: &str) -> Result<Vec<String>> {
        if self.provider == "openai" {
            self.call_openai_classify(fragments, full_log).await
        } else {
            self.call_ollama_classify(fragments, full_log).await
        }
    }

    /// Call OpenAI API to classify fragments
    async fn call_openai_classify(&self, fragments: &[String], full_log: &str) -> Result<Vec<String>> {
        let prompt = super::fragment_classifier::FragmentClassifier::build_classification_prompt(fragments, full_log);

        let request_body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "temperature": 0.1,
            "max_tokens": 2000
        });

        let response = self
            .http_client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_json: serde_json::Value = response.json().await?;

        if !status.is_success() {
            tracing::error!("OpenAI API error: status={}, response={:?}", status, response_json);
            return Err(anyhow::anyhow!("OpenAI API returned error: {}", response_json));
        }

        if let Some(generated_text) = response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
        {
            // Extract JSON array from response
            let json_start = generated_text.find('[').ok_or_else(|| anyhow::anyhow!("No JSON array found"))?;
            let json_end = generated_text.rfind(']').ok_or_else(|| anyhow::anyhow!("No JSON array end found"))?;
            let json_str = &generated_text[json_start..=json_end];

            let classifications: Vec<String> = serde_json::from_str(json_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;

            Ok(classifications)
        } else {
            Err(anyhow::anyhow!("No response from OpenAI"))
        }
    }

    /// Call Ollama API to classify fragments
    async fn call_ollama_classify(&self, fragments: &[String], full_log: &str) -> Result<Vec<String>> {
        let prompt = super::fragment_classifier::FragmentClassifier::build_classification_prompt(fragments, full_log);

        let request_body = serde_json::json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.1,
                "top_p": 0.9,
            }
        });

        let response = self
            .http_client
            .post(format!("{}/api/generate", self.ollama_endpoint))
            .json(&request_body)
            .send()
            .await?;

        let response_json: serde_json::Value = response.json().await?;

        if let Some(generated_text) = response_json.get("response").and_then(|v| v.as_str()) {
            // Extract JSON array from response
            let json_start = generated_text.find('[').ok_or_else(|| anyhow::anyhow!("No JSON array found"))?;
            let json_end = generated_text.rfind(']').ok_or_else(|| anyhow::anyhow!("No JSON array end found"))?;
            let json_str = &generated_text[json_start..=json_end];

            let classifications: Vec<String> = serde_json::from_str(json_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;

            Ok(classifications)
        } else {
            Err(anyhow::anyhow!("No response from Ollama"))
        }
    }
}

// Add uuid dependency for generating unique IDs
#[allow(dead_code)]
mod uuid {
    #[allow(dead_code)]
    pub struct Uuid;

    #[allow(dead_code)]
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
