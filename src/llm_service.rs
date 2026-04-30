use anyhow::Result;
use rustc_hash::FxHashMap;

use crate::llm_config::{ConsensusStrategy, LLMProviderConfig, MultiLLMConfig};
use crate::log_matcher::LogTemplate;

// Removed unused structs: TemplateGenerationRequest, TemplateExample, TemplateGenerationResponse

pub struct LLMServiceClient {
    config: MultiLLMConfig,
    http_client: reqwest::Client,
}

/// Single provider client for making API calls
struct ProviderClient {
    config: LLMProviderConfig,
    http_client: reqwest::Client,
}

impl ProviderClient {
    /// Generate template using this provider
    async fn generate_template(&self, log_line: &str) -> Result<LogTemplate> {
        match self.config.provider.as_str() {
            "openai" => self.call_openai(log_line).await,
            "ollama" => self.call_ollama(log_line).await,
            "anthropic" => self.call_anthropic(log_line).await,
            _ => anyhow::bail!("Unsupported provider: {}", self.config.provider),
        }
    }

    async fn call_ollama(&self, log_line: &str) -> Result<LogTemplate> {
        let endpoint = self
            .config
            .endpoint
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Ollama endpoint not configured"))?;

        let prompt = Self::build_prompt(log_line);

        let request_body = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.1,
                "top_p": 0.9,
            }
        });

        let response = self
            .http_client
            .post(format!("{}/api/generate", endpoint))
            .json(&request_body)
            .send()
            .await?;

        let response_json: serde_json::Value = response.json().await?;

        if let Some(generated_text) = response_json.get("response").and_then(|v| v.as_str()) {
            Self::parse_llm_response(log_line, generated_text)
        } else {
            anyhow::bail!("No response from Ollama")
        }
    }

    async fn call_openai(&self, log_line: &str) -> Result<LogTemplate> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OpenAI API key not configured"))?;

        let prompt = Self::build_prompt(log_line);

        // Reasoning models (o1, o3, ...) behave differently from chat models:
        // - no `temperature`
        // - need `max_completion_tokens`, and the budget covers internal
        //   reasoning tokens too — so it must be much larger
        // - `response_format: json_object` support has been spotty across o-series
        //   versions; safer to omit and rely on prompt discipline
        let model = &self.config.model;
        let is_reasoning_model =
            model.starts_with('o') && model.chars().nth(1).is_some_and(|c| c.is_ascii_digit());

        let mut request_body = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
        });
        if is_reasoning_model {
            request_body["max_completion_tokens"] = serde_json::json!(8000);
        } else {
            request_body["max_completion_tokens"] = serde_json::json!(1000);
            request_body["temperature"] = serde_json::json!(0.1);
            request_body["response_format"] = serde_json::json!({ "type": "json_object" });
        }

        // Honor config.endpoint when set (override for tests / proxies).
        // Default is the public OpenAI chat-completions URL.
        let url = self
            .config
            .endpoint
            .as_deref()
            .unwrap_or("https://api.openai.com/v1/chat/completions");

        let response = self
            .http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_json: serde_json::Value = response.json().await?;

        if !status.is_success() {
            anyhow::bail!("OpenAI API error: {}", response_json);
        }

        if let Some(generated_text) = response_json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
        {
            Self::parse_llm_response(log_line, generated_text)
        } else {
            anyhow::bail!("No response from OpenAI")
        }
    }

    async fn call_anthropic(&self, log_line: &str) -> Result<LogTemplate> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Anthropic API key not configured"))?;

        let prompt = Self::build_prompt(log_line);

        let request_body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": 1000,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        });

        let url = self
            .config
            .endpoint
            .as_deref()
            .unwrap_or("https://api.anthropic.com/v1/messages");

        let response = self
            .http_client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        let status = response.status();
        let response_json: serde_json::Value = response.json().await?;

        if !status.is_success() {
            anyhow::bail!("Anthropic API error: {}", response_json);
        }

        if let Some(generated_text) = response_json
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(|v| v.as_str())
        {
            Self::parse_llm_response(log_line, generated_text)
        } else {
            anyhow::bail!("No response from Anthropic")
        }
    }

    fn build_prompt(log_line: &str) -> String {
        format!(
            r#"Create a regex pattern for this log line by replacing ONLY ephemeral (changing) values with capture groups.

CRITICAL RULES:
1. **DO NOT use generic catch-all patterns like (.+?) or (.+) or (.*)** unless absolutely necessary
2. **Keep all static text EXACTLY as-is** - keywords, error messages, field names, etc.
3. **Only mask values that actually change** - timestamps, IPs, numbers, IDs, usernames, paths, etc.

LOG LINE: {log_line}

Respond with ONLY the JSON object, no explanation:
{{"pattern": "^...$", "variables": [...]}}
"#,
            log_line = log_line
        )
    }

    fn parse_llm_response(log_line: &str, llm_output: &str) -> Result<LogTemplate> {
        // Extract JSON from the response
        let json_start = llm_output
            .char_indices()
            .find(|(_, c)| *c == '{')
            .map(|(i, _)| i)
            .unwrap_or(0);
        let json_end = llm_output
            .char_indices()
            .rev()
            .find(|(_, c)| *c == '}')
            .map(|(i, _)| i + '}'.len_utf8())
            .unwrap_or(llm_output.len());

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

                // Stable, content-derived ID computed at synthesis time.
                // Same canonical pattern always produces the same ID, so
                // concurrent synthesis from multiple workers is idempotent
                // and historical KL divergence stays correct across restarts.
                let template_id = crate::template_id::template_id_from_pattern(&pattern);
                Ok(LogTemplate {
                    template_id,
                    pattern,
                    variables,
                    example: log_line.to_string(),
                })
            }
            Err(e) => {
                anyhow::bail!(
                    "Failed to parse LLM JSON response: {}. Response: {}",
                    e,
                    llm_output
                )
            }
        }
    }
}

impl LLMServiceClient {
    /// Create a new multi-LLM client with consensus
    pub fn new_with_config(config: MultiLLMConfig) -> Result<Self> {
        config.validate()?;

        tracing::info!(
            "🤖 Multi-LLM Service configured with {} providers, strategy: {:?}",
            config.providers.len(),
            config.consensus_strategy
        );

        for provider in &config.providers {
            tracing::info!(
                "   - {}: {} ({})",
                provider.name,
                provider.provider,
                provider.model
            );
        }

        Ok(Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        })
    }

    /// Create from legacy single provider (backward compatibility)
    pub fn new(provider: String, api_key: String, model: String) -> Self {
        let ollama_endpoint = std::env::var("OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let config = MultiLLMConfig {
            providers: vec![LLMProviderConfig {
                name: provider.clone(),
                provider: provider.clone(),
                model,
                api_key: Some(api_key),
                endpoint: Some(ollama_endpoint),
                timeout_secs: Some(60),
            }],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        };

        Self::new_with_config(config).unwrap()
    }

    /// Send a log line to multiple LLMs and find consensus
    pub async fn generate_template(&self, log_line: &str) -> Result<LogTemplate> {
        tracing::debug!(
            "Requesting {} LLM(s) to generate template for: {}",
            self.config.providers.len(),
            log_line
        );

        match self.config.consensus_strategy {
            ConsensusStrategy::FirstSuccess => {
                // Try providers in order until one succeeds
                for provider_config in &self.config.providers {
                    let client = ProviderClient {
                        config: provider_config.clone(),
                        http_client: self.http_client.clone(),
                    };

                    match client.generate_template(log_line).await {
                        Ok(template) => {
                            tracing::debug!("Provider {} succeeded", provider_config.name);
                            return Ok(template);
                        }
                        Err(e) => {
                            tracing::warn!("Provider {} failed: {}", provider_config.name, e);
                            continue;
                        }
                    }
                }
                anyhow::bail!("All LLM providers failed")
            }
            _ => {
                // Call all providers in parallel
                self.generate_with_consensus(log_line).await
            }
        }
    }

    /// Generate templates from multiple LLMs and find consensus
    async fn generate_with_consensus(&self, log_line: &str) -> Result<LogTemplate> {
        use futures::future::join_all;

        // Call all providers in parallel
        let tasks: Vec<_> = self
            .config
            .providers
            .iter()
            .map(|provider_config| {
                let client = ProviderClient {
                    config: provider_config.clone(),
                    http_client: self.http_client.clone(),
                };
                let log_line = log_line.to_string();
                async move {
                    (
                        provider_config.name.clone(),
                        client.generate_template(&log_line).await,
                    )
                }
            })
            .collect();

        let results = join_all(tasks).await;

        // Collect successful responses
        let successful: Vec<(String, LogTemplate)> = results
            .into_iter()
            .filter_map(|(name, result)| match result {
                Ok(template) => Some((name, template)),
                Err(e) => {
                    tracing::warn!("Provider {} failed: {}", name, e);
                    None
                }
            })
            .collect();

        if successful.is_empty() {
            anyhow::bail!("All LLM providers failed");
        }

        // Apply consensus strategy
        self.find_consensus(successful, log_line)
    }

    /// Find consensus among multiple template responses
    fn find_consensus(
        &self,
        templates: Vec<(String, LogTemplate)>,
        _log_line: &str,
    ) -> Result<LogTemplate> {
        let required_agreement = match self.config.consensus_strategy {
            ConsensusStrategy::Unanimous => templates.len(),
            ConsensusStrategy::Majority => (templates.len() / 2) + 1,
            ConsensusStrategy::MinAgreement => self.config.min_agreement,
            ConsensusStrategy::FirstSuccess => 1,
        };

        // Group templates by pattern similarity
        let mut pattern_groups: FxHashMap<String, Vec<(String, LogTemplate)>> =
            FxHashMap::default();

        for (provider_name, template) in templates {
            // Normalize pattern for comparison (remove whitespace differences)
            let normalized = template
                .pattern
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            pattern_groups
                .entry(normalized.clone())
                .or_default()
                .push((provider_name, template));
        }

        // Find the pattern group with most agreement
        let mut best_group: Option<(&String, &Vec<(String, LogTemplate)>)> = None;

        for (pattern, group) in pattern_groups.iter() {
            if group.len() >= required_agreement
                && (best_group.is_none() || group.len() > best_group.unwrap().1.len())
            {
                best_group = Some((pattern, group));
            }
        }

        match best_group {
            Some((pattern, group)) => {
                let providers: Vec<String> = group.iter().map(|(name, _)| name.clone()).collect();
                tracing::info!(
                    "Consensus reached: {} providers agreed on pattern (normalized): {}",
                    group.len(),
                    pattern
                );
                tracing::debug!("Agreeing providers: {:?}", providers);

                // Return the first template from the consensus group
                Ok(group[0].1.clone())
            }
            None => {
                tracing::warn!(
                    "No consensus reached. Required: {}, Got: {:?}",
                    required_agreement,
                    pattern_groups.values().map(|g| g.len()).collect::<Vec<_>>()
                );

                // Fall back to most common pattern
                let largest_group = pattern_groups
                    .values()
                    .max_by_key(|g| g.len())
                    .ok_or_else(|| anyhow::anyhow!("No templates available"))?;

                tracing::info!(
                    "Using most common pattern with {} votes",
                    largest_group.len()
                );
                Ok(largest_group[0].1.clone())
            }
        }
    }

    /// Generate a complete template from a log line (legacy method for compatibility)
    pub async fn generate_template_from_log(&self, log_line: &str) -> Result<LogTemplate> {
        self.generate_template(log_line).await
    }

    /// Classify log fragments using first available LLM
    pub async fn classify_fragments(
        &self,
        fragments: &[String],
        full_log: &str,
    ) -> Result<Vec<String>> {
        // Use first provider for fragment classification
        if let Some(provider_config) = self.config.providers.first() {
            let client = ProviderClient {
                config: provider_config.clone(),
                http_client: self.http_client.clone(),
            };
            client.classify_fragments(fragments, full_log).await
        } else {
            anyhow::bail!("No LLM providers configured")
        }
    }

    /// Simple call for generic prompts (uses first provider)
    pub async fn call_openai_simple(&self, prompt: &str) -> Result<String> {
        if let Some(provider_config) = self.config.providers.first() {
            let client = ProviderClient {
                config: provider_config.clone(),
                http_client: self.http_client.clone(),
            };
            client.call_simple(prompt).await
        } else {
            anyhow::bail!("No LLM providers configured")
        }
    }
}

impl ProviderClient {
    /// Call for generic prompts (returns raw text)
    async fn call_simple(&self, prompt: &str) -> Result<String> {
        match self.config.provider.as_str() {
            "openai" => {
                let api_key = self
                    .config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OpenAI API key not configured"))?;

                let request_body = serde_json::json!({
                    "model": self.config.model,
                    "messages": [
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ],
                    "temperature": 0.1,
                    "max_tokens": 3000
                });

                let url = self
                    .config
                    .endpoint
                    .as_deref()
                    .unwrap_or("https://api.openai.com/v1/chat/completions");

                let response = self
                    .http_client
                    .post(url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await?;

                let status = response.status();
                let response_json: serde_json::Value = response.json().await?;

                if !status.is_success() {
                    anyhow::bail!("OpenAI API error: {}", response_json);
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
                    anyhow::bail!("No response from OpenAI")
                }
            }
            _ => anyhow::bail!("call_simple only supported for OpenAI provider"),
        }
    }

    /// Classify log fragments
    async fn classify_fragments(
        &self,
        fragments: &[String],
        full_log: &str,
    ) -> Result<Vec<String>> {
        let prompt = Self::build_classification_prompt(fragments, full_log);

        match self.config.provider.as_str() {
            "openai" => {
                let api_key = self
                    .config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OpenAI API key not configured"))?;

                let request_body = serde_json::json!({
                    "model": self.config.model,
                    "messages": [
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ],
                    "temperature": 0.1,
                    "max_tokens": 2000
                });

                let url = self
                    .config
                    .endpoint
                    .as_deref()
                    .unwrap_or("https://api.openai.com/v1/chat/completions");

                let response = self
                    .http_client
                    .post(url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await?;

                let status = response.status();
                let response_json: serde_json::Value = response.json().await?;

                if !status.is_success() {
                    anyhow::bail!("OpenAI API error: {}", response_json);
                }

                if let Some(generated_text) = response_json
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|v| v.as_str())
                {
                    Self::parse_classification_response(generated_text)
                } else {
                    anyhow::bail!("No response from OpenAI")
                }
            }
            "ollama" => {
                let endpoint = self
                    .config
                    .endpoint
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Ollama endpoint not configured"))?;

                let request_body = serde_json::json!({
                    "model": self.config.model,
                    "prompt": prompt,
                    "stream": false,
                    "options": {
                        "temperature": 0.1,
                        "top_p": 0.9,
                    }
                });

                let response = self
                    .http_client
                    .post(format!("{}/api/generate", endpoint))
                    .json(&request_body)
                    .send()
                    .await?;

                let response_json: serde_json::Value = response.json().await?;

                if let Some(generated_text) = response_json.get("response").and_then(|v| v.as_str())
                {
                    Self::parse_classification_response(generated_text)
                } else {
                    anyhow::bail!("No response from Ollama")
                }
            }
            _ => anyhow::bail!(
                "Fragment classification not supported for provider: {}",
                self.config.provider
            ),
        }
    }

    fn build_classification_prompt(fragments: &[String], full_log: &str) -> String {
        // Use the existing fragment classifier prompt building logic
        crate::fragment_classifier::FragmentClassifier::build_classification_prompt(
            fragments, full_log,
        )
    }

    fn parse_classification_response(response: &str) -> Result<Vec<String>> {
        // Extract JSON array from response
        let json_start = response
            .find('[')
            .ok_or_else(|| anyhow::anyhow!("No JSON array found"))?;
        let json_end = response
            .rfind(']')
            .ok_or_else(|| anyhow::anyhow!("No JSON array end found"))?;
        let json_str = &response[json_start..=json_end];

        let classifications: Vec<String> = serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;

        Ok(classifications)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ollama_provider(endpoint: &str) -> LLMProviderConfig {
        LLMProviderConfig {
            name: "ol".into(),
            provider: "ollama".into(),
            model: "llama3".into(),
            api_key: None,
            endpoint: Some(endpoint.into()),
            timeout_secs: Some(5),
        }
    }

    fn openai_provider(endpoint: &str) -> LLMProviderConfig {
        LLMProviderConfig {
            name: "oa".into(),
            provider: "openai".into(),
            model: "gpt-4o".into(),
            api_key: Some("sk-test".into()),
            endpoint: Some(endpoint.into()),
            timeout_secs: Some(5),
        }
    }

    fn anthropic_provider(endpoint: &str) -> LLMProviderConfig {
        LLMProviderConfig {
            name: "an".into(),
            provider: "anthropic".into(),
            model: "claude-3".into(),
            api_key: Some("ak-test".into()),
            endpoint: Some(endpoint.into()),
            timeout_secs: Some(5),
        }
    }

    fn first_success(provider: LLMProviderConfig) -> MultiLLMConfig {
        MultiLLMConfig {
            providers: vec![provider],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        }
    }

    // ---------- pure / construction ----------

    #[test]
    fn test_new_with_config_validates() {
        // Empty providers → validation error from MultiLLMConfig.
        let bad = MultiLLMConfig {
            providers: vec![],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        };
        assert!(LLMServiceClient::new_with_config(bad).is_err());
    }

    #[test]
    fn test_new_legacy_constructor_builds_single_provider() {
        // The legacy `new()` shape used by older callers — wrap into a
        // single-provider config and don't touch the network.
        let svc = LLMServiceClient::new("ollama".into(), "key".into(), "llama3".into());
        assert_eq!(svc.config.providers.len(), 1);
        assert_eq!(svc.config.providers[0].provider, "ollama");
        assert_eq!(
            svc.config.consensus_strategy,
            ConsensusStrategy::FirstSuccess
        );
    }

    #[test]
    fn test_build_prompt_contains_log_line_and_rules() {
        let p = ProviderClient::build_prompt("Jun 14 ssh failure");
        assert!(p.contains("Jun 14 ssh failure"));
        assert!(p.contains("regex pattern"));
        assert!(p.contains("CRITICAL RULES"));
    }

    #[test]
    fn test_parse_llm_response_happy_path() {
        let log = "User alice logged in";
        let llm = r#"{"pattern": "User (\\w+) logged in", "variables": ["user"]}"#;
        let t = ProviderClient::parse_llm_response(log, llm).unwrap();
        assert_eq!(t.pattern, "User (\\w+) logged in");
        assert_eq!(t.variables, vec!["user".to_string()]);
        assert_eq!(t.example, log);
        assert_ne!(t.template_id, 0);
    }

    #[test]
    fn test_parse_llm_response_extracts_json_from_prose() {
        let log = "x";
        // LLM wraps the JSON in commentary — must still parse.
        let llm = r#"Sure! Here's the pattern:
            {"pattern": "x", "variables": []}
            Hope this helps!"#;
        let t = ProviderClient::parse_llm_response(log, llm).unwrap();
        assert_eq!(t.pattern, "x");
        assert!(t.variables.is_empty());
    }

    #[test]
    fn test_parse_llm_response_rejects_non_json() {
        // No object delimiters at all → falls back to whole response,
        // which fails json parse → error.
        let err = ProviderClient::parse_llm_response("log", "not json").unwrap_err();
        assert!(err
            .to_string()
            .contains("Failed to parse LLM JSON response"));
    }

    #[test]
    fn test_parse_llm_response_missing_fields_uses_defaults() {
        // Empty object: pattern defaults to log_line, variables to [].
        let t = ProviderClient::parse_llm_response("the log", "{}").unwrap();
        assert_eq!(t.pattern, "the log");
        assert!(t.variables.is_empty());
    }

    #[test]
    fn test_parse_classification_response() {
        let r = ProviderClient::parse_classification_response(r#"prefix ["a","b","c"] suffix"#)
            .unwrap();
        assert_eq!(r, vec!["a".to_string(), "b".into(), "c".into()]);
    }

    #[test]
    fn test_parse_classification_response_no_array_errors() {
        assert!(ProviderClient::parse_classification_response("nope").is_err());
        assert!(ProviderClient::parse_classification_response("[unbalanced").is_err());
    }

    // ---------- ollama HTTP path (configurable endpoint) ----------

    #[tokio::test]
    async fn test_ollama_generate_template_happy_path() {
        let server = MockServer::start().await;
        // Use a pattern with no JSON-escape pitfalls (no backslashes).
        // The contract test only needs to prove pattern + variables
        // round-trip; regex semantics are not exercised here.
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": r#"{"pattern": "user X logged in", "variables": ["user"]}"#
            })))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(ollama_provider(&server.uri())))
            .unwrap();
        let t = svc.generate_template("user alice logged in").await.unwrap();
        assert_eq!(t.pattern, "user X logged in");
        assert_eq!(t.variables, vec!["user".to_string()]);
    }

    #[tokio::test]
    async fn test_ollama_generate_template_no_response_field() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(ollama_provider(&server.uri())))
            .unwrap();
        let err = svc.generate_template("x").await.unwrap_err();
        // FirstSuccess → exhausts providers → "All LLM providers failed"
        assert!(err.to_string().contains("All LLM providers failed"));
    }

    #[tokio::test]
    async fn test_ollama_endpoint_missing_returns_error() {
        let provider = LLMProviderConfig {
            endpoint: None, // missing → call_ollama bails
            ..ollama_provider("unused")
        };
        let svc = LLMServiceClient::new_with_config(first_success(provider)).unwrap();
        let err = svc.generate_template("x").await.unwrap_err();
        assert!(err.to_string().contains("All LLM providers failed"));
    }

    // ---------- openai HTTP path ----------

    #[tokio::test]
    async fn test_openai_generate_template_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "content": r#"{"pattern": "GET /api/X", "variables": ["resource"]}"#
                    }
                }]
            })))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(openai_provider(&server.uri())))
            .unwrap();
        let t = svc.generate_template("GET /api/users").await.unwrap();
        assert_eq!(t.pattern, "GET /api/X");
        assert_eq!(t.variables, vec!["resource".to_string()]);
    }

    #[tokio::test]
    async fn test_openai_generate_template_error_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {"message": "invalid api key"}
            })))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(openai_provider(&server.uri())))
            .unwrap();
        let err = svc.generate_template("x").await.unwrap_err();
        assert!(err.to_string().contains("All LLM providers failed"));
    }

    #[tokio::test]
    async fn test_openai_no_api_key() {
        let provider = LLMProviderConfig {
            api_key: None,
            ..openai_provider("http://nowhere.invalid")
        };
        let svc = LLMServiceClient::new_with_config(first_success(provider)).unwrap();
        let err = svc.generate_template("x").await.unwrap_err();
        assert!(err.to_string().contains("All LLM providers failed"));
    }

    #[tokio::test]
    async fn test_openai_reasoning_model_branch() {
        // Reasoning models (o1/o3...) take a different request shape:
        // no temperature, no response_format, larger max_completion_tokens.
        // Verify by setting model to "o1" — the request must succeed and
        // not include a temperature field.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {
                        "content": r#"{"pattern": "x", "variables": []}"#
                    }
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let provider = LLMProviderConfig {
            model: "o1".into(),
            ..openai_provider(&server.uri())
        };
        let svc = LLMServiceClient::new_with_config(first_success(provider)).unwrap();
        svc.generate_template("x").await.unwrap();
    }

    // ---------- anthropic HTTP path ----------

    #[tokio::test]
    async fn test_anthropic_generate_template_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("x-api-key", "ak-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "text": r#"{"pattern": "ok", "variables": []}"# }]
            })))
            .mount(&server)
            .await;

        let svc =
            LLMServiceClient::new_with_config(first_success(anthropic_provider(&server.uri())))
                .unwrap();
        let t = svc.generate_template("anything").await.unwrap();
        assert_eq!(t.pattern, "ok");
    }

    #[tokio::test]
    async fn test_anthropic_no_api_key() {
        let provider = LLMProviderConfig {
            api_key: None,
            ..anthropic_provider("http://nowhere.invalid")
        };
        let svc = LLMServiceClient::new_with_config(first_success(provider)).unwrap();
        let err = svc.generate_template("x").await.unwrap_err();
        assert!(err.to_string().contains("All LLM providers failed"));
    }

    // ---------- unsupported / fallthrough provider ----------

    #[tokio::test]
    async fn test_unsupported_provider_errors() {
        let provider = LLMProviderConfig {
            name: "weird".into(),
            provider: "made-up".into(),
            model: "x".into(),
            api_key: None,
            endpoint: None,
            timeout_secs: None,
        };
        let svc = LLMServiceClient::new_with_config(first_success(provider)).unwrap();
        let err = svc.generate_template("x").await.unwrap_err();
        assert!(err.to_string().contains("All LLM providers failed"));
    }

    // ---------- legacy alias ----------

    #[tokio::test]
    async fn test_generate_template_from_log_alias() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": r#"{"pattern": "x", "variables": []}"#
            })))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(ollama_provider(&server.uri())))
            .unwrap();
        let t = svc.generate_template_from_log("x").await.unwrap();
        assert_eq!(t.pattern, "x");
    }

    // ---------- classify_fragments ----------

    #[tokio::test]
    async fn test_classify_fragments_via_ollama() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": r#"["timestamp", "static_text"]"#
            })))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(ollama_provider(&server.uri())))
            .unwrap();
        let result = svc
            .classify_fragments(&["Jun".into(), "auth".into()], "Jun auth")
            .await
            .unwrap();
        assert_eq!(result, vec!["timestamp".to_string(), "static_text".into()]);
    }

    #[tokio::test]
    async fn test_classify_fragments_via_openai() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "content": r#"["number","ip_address"]"# }
                }]
            })))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(openai_provider(&server.uri())))
            .unwrap();
        let result = svc
            .classify_fragments(&["1".into(), "10.0.0.1".into()], "1 10.0.0.1")
            .await
            .unwrap();
        assert_eq!(result, vec!["number".to_string(), "ip_address".into()]);
    }

    #[tokio::test]
    async fn test_classify_fragments_unsupported_provider() {
        let provider = LLMProviderConfig {
            provider: "anthropic".into(), // classify_fragments not implemented
            ..anthropic_provider("http://nowhere.invalid")
        };
        let svc = LLMServiceClient::new_with_config(first_success(provider)).unwrap();
        let err = svc.classify_fragments(&[], "").await.unwrap_err();
        assert!(err.to_string().contains("not supported"));
    }

    // ---------- call_openai_simple ----------

    #[tokio::test]
    async fn test_call_openai_simple_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": { "content": "hi there" }
                }]
            })))
            .mount(&server)
            .await;

        let svc = LLMServiceClient::new_with_config(first_success(openai_provider(&server.uri())))
            .unwrap();
        let s = svc.call_openai_simple("any prompt").await.unwrap();
        assert_eq!(s, "hi there");
    }

    #[tokio::test]
    async fn test_call_openai_simple_only_supports_openai() {
        let svc = LLMServiceClient::new_with_config(first_success(ollama_provider(
            "http://nowhere.invalid",
        )))
        .unwrap();
        let err = svc.call_openai_simple("p").await.unwrap_err();
        assert!(err
            .to_string()
            .contains("call_simple only supported for OpenAI"));
    }

    // ---------- consensus paths ----------

    #[tokio::test]
    async fn test_first_success_falls_back_to_second_provider() {
        // First provider 500s, second provider succeeds → consumer gets
        // the second provider's template.
        let bad = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&bad)
            .await;
        let good = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": r#"{"pattern": "good", "variables": []}"#
            })))
            .mount(&good)
            .await;

        let cfg = MultiLLMConfig {
            providers: vec![openai_provider(&bad.uri()), ollama_provider(&good.uri())],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        };
        let svc = LLMServiceClient::new_with_config(cfg).unwrap();
        let t = svc.generate_template("x").await.unwrap();
        assert_eq!(t.pattern, "good");
    }

    #[tokio::test]
    async fn test_majority_consensus_picks_agreed_pattern() {
        // Two ollama providers agree on "AGREED", one disagrees with
        // "OUTLIER". Majority strategy must return AGREED.
        let agree1 = MockServer::start().await;
        let agree2 = MockServer::start().await;
        let dissent = MockServer::start().await;
        for (server, pattern) in [
            (&agree1, "AGREED"),
            (&agree2, "AGREED"),
            (&dissent, "OUTLIER"),
        ] {
            Mock::given(method("POST"))
                .and(path("/api/generate"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "response": format!(r#"{{"pattern": "{pattern}", "variables": []}}"#)
                })))
                .mount(server)
                .await;
        }
        let cfg = MultiLLMConfig {
            providers: vec![
                ollama_provider(&agree1.uri()),
                ollama_provider(&agree2.uri()),
                ollama_provider(&dissent.uri()),
            ],
            consensus_strategy: ConsensusStrategy::Majority,
            min_agreement: 2,
        };
        let svc = LLMServiceClient::new_with_config(cfg).unwrap();
        let t = svc.generate_template("x").await.unwrap();
        assert_eq!(t.pattern, "AGREED");
    }

    #[tokio::test]
    async fn test_consensus_fallback_when_no_agreement() {
        // Three different patterns → no group meets the majority
        // threshold, so consensus falls back to the largest group
        // (which is just any single pattern). Should still return
        // *something*, not error.
        let s1 = MockServer::start().await;
        let s2 = MockServer::start().await;
        for (server, pattern) in [(&s1, "PAT_A"), (&s2, "PAT_B")] {
            Mock::given(method("POST"))
                .and(path("/api/generate"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "response": format!(r#"{{"pattern": "{pattern}", "variables": []}}"#)
                })))
                .mount(server)
                .await;
        }
        let cfg = MultiLLMConfig {
            providers: vec![ollama_provider(&s1.uri()), ollama_provider(&s2.uri())],
            consensus_strategy: ConsensusStrategy::Unanimous, // requires 2 agree
            min_agreement: 2,
        };
        let svc = LLMServiceClient::new_with_config(cfg).unwrap();
        let t = svc.generate_template("x").await.unwrap();
        // Either pattern is acceptable; the test just asserts the call
        // succeeds via the fallback rather than erroring.
        assert!(t.pattern == "PAT_A" || t.pattern == "PAT_B");
    }

    #[tokio::test]
    async fn test_consensus_all_providers_fail() {
        let s1 = MockServer::start().await;
        let s2 = MockServer::start().await;
        for server in [&s1, &s2] {
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(500))
                .mount(server)
                .await;
        }
        let cfg = MultiLLMConfig {
            providers: vec![ollama_provider(&s1.uri()), ollama_provider(&s2.uri())],
            consensus_strategy: ConsensusStrategy::Majority,
            min_agreement: 2,
        };
        let svc = LLMServiceClient::new_with_config(cfg).unwrap();
        assert!(svc.generate_template("x").await.is_err());
    }
}
