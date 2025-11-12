use serde::{Deserialize, Serialize};

/// Configuration for a single LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProviderConfig {
    pub name: String,
    pub provider: String,  // "openai", "ollama", "anthropic", etc.
    pub model: String,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,  // For Ollama or custom endpoints
    pub timeout_secs: Option<u64>,
}

/// Configuration for multi-LLM consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiLLMConfig {
    pub providers: Vec<LLMProviderConfig>,
    pub consensus_strategy: ConsensusStrategy,
    pub min_agreement: usize,  // Minimum number of LLMs that must agree
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusStrategy {
    /// Require all LLMs to agree
    Unanimous,
    /// Require majority to agree
    Majority,
    /// Require at least N LLMs to agree (specified by min_agreement)
    MinAgreement,
    /// Use first successful response (no consensus)
    FirstSuccess,
}

impl Default for MultiLLMConfig {
    fn default() -> Self {
        Self {
            providers: vec![
                LLMProviderConfig {
                    name: "ollama".to_string(),
                    provider: "ollama".to_string(),
                    model: "llama3".to_string(),
                    api_key: None,
                    endpoint: Some("http://localhost:11434".to_string()),
                    timeout_secs: Some(60),
                }
            ],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        }
    }
}

impl MultiLLMConfig {
    /// Load from environment variables
    pub fn from_env() -> Self {
        // Check if multi-LLM config file exists
        if let Ok(config_path) = std::env::var("LLM_CONFIG_FILE") {
            if let Ok(config_str) = std::fs::read_to_string(config_path) {
                if let Ok(config) = serde_json::from_str(&config_str) {
                    return config;
                }
            }
        }

        // Fall back to single LLM from env vars
        let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
        let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "llama3".to_string());
        let api_key = std::env::var("LLM_API_KEY").ok();
        let endpoint = std::env::var("OLLAMA_ENDPOINT").ok();

        Self {
            providers: vec![
                LLMProviderConfig {
                    name: provider.clone(),
                    provider,
                    model,
                    api_key,
                    endpoint,
                    timeout_secs: Some(60),
                }
            ],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.providers.is_empty() {
            anyhow::bail!("At least one LLM provider must be configured");
        }

        match self.consensus_strategy {
            ConsensusStrategy::Unanimous => {
                if self.providers.len() < 2 {
                    anyhow::bail!("Unanimous consensus requires at least 2 providers");
                }
            }
            ConsensusStrategy::Majority => {
                if self.providers.len() < 2 {
                    anyhow::bail!("Majority consensus requires at least 2 providers");
                }
            }
            ConsensusStrategy::MinAgreement => {
                if self.min_agreement > self.providers.len() {
                    anyhow::bail!(
                        "min_agreement ({}) cannot exceed number of providers ({})",
                        self.min_agreement,
                        self.providers.len()
                    );
                }
                if self.min_agreement < 1 {
                    anyhow::bail!("min_agreement must be at least 1");
                }
            }
            ConsensusStrategy::FirstSuccess => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MultiLLMConfig::default();
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.consensus_strategy, ConsensusStrategy::FirstSuccess);
    }

    #[test]
    fn test_unanimous_validation() {
        let config = MultiLLMConfig {
            providers: vec![
                LLMProviderConfig {
                    name: "provider1".to_string(),
                    provider: "ollama".to_string(),
                    model: "model1".to_string(),
                    api_key: None,
                    endpoint: None,
                    timeout_secs: None,
                }
            ],
            consensus_strategy: ConsensusStrategy::Unanimous,
            min_agreement: 1,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_valid_majority() {
        let config = MultiLLMConfig {
            providers: vec![
                LLMProviderConfig {
                    name: "provider1".to_string(),
                    provider: "ollama".to_string(),
                    model: "model1".to_string(),
                    api_key: None,
                    endpoint: None,
                    timeout_secs: None,
                },
                LLMProviderConfig {
                    name: "provider2".to_string(),
                    provider: "openai".to_string(),
                    model: "gpt-4".to_string(),
                    api_key: Some("key".to_string()),
                    endpoint: None,
                    timeout_secs: None,
                }
            ],
            consensus_strategy: ConsensusStrategy::Majority,
            min_agreement: 2,
        };

        assert!(config.validate().is_ok());
    }
}
