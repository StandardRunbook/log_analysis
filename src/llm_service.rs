use anyhow::Result;
use rustc_hash::FxHashMap;

use crate::log_matcher::LogTemplate;
use crate::llm_config::{MultiLLMConfig, LLMProviderConfig, ConsensusStrategy};

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
        let endpoint = self.config.endpoint.as_ref()
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

        let response = self.http_client
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
        let api_key = self.config.api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("OpenAI API key not configured"))?;

        let prompt = Self::build_prompt(log_line);

        let request_body = serde_json::json!({
            "model": self.config.model,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "temperature": 0.1,
            "max_tokens": 1000
        });

        let response = self.http_client
            .post("https://api.openai.com/v1/chat/completions")
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
        let api_key = self.config.api_key.as_ref()
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

        let response = self.http_client
            .post("https://api.anthropic.com/v1/messages")
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
            .map(|(i, _)| i + '}'. len_utf8())
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

                // Use placeholder ID - ClickHouse will assign
                Ok(LogTemplate {
                    template_id: 0,
                    pattern,
                    variables,
                    example: log_line.to_string(),
                })
            }
            Err(e) => {
                anyhow::bail!("Failed to parse LLM JSON response: {}. Response: {}", e, llm_output)
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
        tracing::debug!("Requesting {} LLM(s) to generate template for: {}",
                       self.config.providers.len(), log_line);

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
        let tasks: Vec<_> = self.config.providers.iter().map(|provider_config| {
            let client = ProviderClient {
                config: provider_config.clone(),
                http_client: self.http_client.clone(),
            };
            let log_line = log_line.to_string();
            async move {
                (provider_config.name.clone(), client.generate_template(&log_line).await)
            }
        }).collect();

        let results = join_all(tasks).await;

        // Collect successful responses
        let successful: Vec<(String, LogTemplate)> = results
            .into_iter()
            .filter_map(|(name, result)| {
                match result {
                    Ok(template) => Some((name, template)),
                    Err(e) => {
                        tracing::warn!("Provider {} failed: {}", name, e);
                        None
                    }
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
    fn find_consensus(&self, templates: Vec<(String, LogTemplate)>, _log_line: &str) -> Result<LogTemplate> {
        let required_agreement = match self.config.consensus_strategy {
            ConsensusStrategy::Unanimous => templates.len(),
            ConsensusStrategy::Majority => (templates.len() / 2) + 1,
            ConsensusStrategy::MinAgreement => self.config.min_agreement,
            ConsensusStrategy::FirstSuccess => 1,
        };

        // Group templates by pattern similarity
        let mut pattern_groups: FxHashMap<String, Vec<(String, LogTemplate)>> = FxHashMap::default();

        for (provider_name, template) in templates {
            // Normalize pattern for comparison (remove whitespace differences)
            let normalized = template.pattern.split_whitespace().collect::<Vec<_>>().join(" ");
            pattern_groups.entry(normalized.clone())
                .or_insert_with(Vec::new)
                .push((provider_name, template));
        }

        // Find the pattern group with most agreement
        let mut best_group: Option<(&String, &Vec<(String, LogTemplate)>)> = None;

        for (pattern, group) in pattern_groups.iter() {
            if group.len() >= required_agreement {
                if best_group.is_none() || group.len() > best_group.unwrap().1.len() {
                    best_group = Some((pattern, group));
                }
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
                    pattern_groups.iter().map(|(_, g)| g.len()).collect::<Vec<_>>()
                );

                // Fall back to most common pattern
                let largest_group = pattern_groups
                    .values()
                    .max_by_key(|g| g.len())
                    .ok_or_else(|| anyhow::anyhow!("No templates available"))?;

                tracing::info!("Using most common pattern with {} votes", largest_group.len());
                Ok(largest_group[0].1.clone())
            }
        }
    }

    /// Generate a complete template from a log line (legacy method for compatibility)
    pub async fn generate_template_from_log(&self, log_line: &str) -> Result<LogTemplate> {
        self.generate_template(log_line).await
    }

    /// Classify log fragments using first available LLM
    pub async fn classify_fragments(&self, fragments: &[String], full_log: &str) -> Result<Vec<String>> {
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
                let api_key = self.config.api_key.as_ref()
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

                let response = self.http_client
                    .post("https://api.openai.com/v1/chat/completions")
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
            _ => anyhow::bail!("call_simple only supported for OpenAI provider")
        }
    }

    /// Classify log fragments
    async fn classify_fragments(&self, fragments: &[String], full_log: &str) -> Result<Vec<String>> {
        let prompt = Self::build_classification_prompt(fragments, full_log);

        match self.config.provider.as_str() {
            "openai" => {
                let api_key = self.config.api_key.as_ref()
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

                let response = self.http_client
                    .post("https://api.openai.com/v1/chat/completions")
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
                let endpoint = self.config.endpoint.as_ref()
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

                let response = self.http_client
                    .post(format!("{}/api/generate", endpoint))
                    .json(&request_body)
                    .send()
                    .await?;

                let response_json: serde_json::Value = response.json().await?;

                if let Some(generated_text) = response_json.get("response").and_then(|v| v.as_str()) {
                    Self::parse_classification_response(generated_text)
                } else {
                    anyhow::bail!("No response from Ollama")
                }
            }
            _ => anyhow::bail!("Fragment classification not supported for provider: {}", self.config.provider)
        }
    }

    fn build_classification_prompt(fragments: &[String], full_log: &str) -> String {
        // Use the existing fragment classifier prompt building logic
        crate::fragment_classifier::FragmentClassifier::build_classification_prompt(fragments, full_log)
    }

    fn parse_classification_response(response: &str) -> Result<Vec<String>> {
        // Extract JSON array from response
        let json_start = response.find('[').ok_or_else(|| anyhow::anyhow!("No JSON array found"))?;
        let json_end = response.rfind(']').ok_or_else(|| anyhow::anyhow!("No JSON array end found"))?;
        let json_str = &response[json_start..=json_end];

        let classifications: Vec<String> = serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;

        Ok(classifications)
    }
}
