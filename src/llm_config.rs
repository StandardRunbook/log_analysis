use serde::{Deserialize, Serialize};

/// Configuration for a single LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProviderConfig {
    pub name: String,
    pub provider: String, // "openai", "ollama", "anthropic", etc.
    pub model: String,
    pub api_key: Option<String>,
    pub endpoint: Option<String>, // For Ollama or custom endpoints
    pub timeout_secs: Option<u64>,
}

/// Configuration for multi-LLM consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiLLMConfig {
    pub providers: Vec<LLMProviderConfig>,
    pub consensus_strategy: ConsensusStrategy,
    pub min_agreement: usize, // Minimum number of LLMs that must agree
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
            providers: vec![LLMProviderConfig {
                name: "ollama".to_string(),
                provider: "ollama".to_string(),
                model: "llama3".to_string(),
                api_key: None,
                endpoint: Some("http://localhost:11434".to_string()),
                timeout_secs: Some(60),
            }],
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
            providers: vec![LLMProviderConfig {
                name: provider.clone(),
                provider,
                model,
                api_key,
                endpoint,
                timeout_secs: Some(60),
            }],
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
            providers: vec![LLMProviderConfig {
                name: "provider1".to_string(),
                provider: "ollama".to_string(),
                model: "model1".to_string(),
                api_key: None,
                endpoint: None,
                timeout_secs: None,
            }],
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
                },
            ],
            consensus_strategy: ConsensusStrategy::Majority,
            min_agreement: 2,
        };

        assert!(config.validate().is_ok());
    }

    fn provider(name: &str) -> LLMProviderConfig {
        LLMProviderConfig {
            name: name.to_string(),
            provider: "ollama".to_string(),
            model: "m".to_string(),
            api_key: None,
            endpoint: None,
            timeout_secs: None,
        }
    }

    #[test]
    fn test_validate_empty_providers_rejects() {
        let cfg = MultiLLMConfig {
            providers: vec![],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_majority_needs_two_providers() {
        let cfg = MultiLLMConfig {
            providers: vec![provider("only")],
            consensus_strategy: ConsensusStrategy::Majority,
            min_agreement: 1,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_min_agreement_must_not_exceed_providers() {
        let cfg = MultiLLMConfig {
            providers: vec![provider("a"), provider("b")],
            consensus_strategy: ConsensusStrategy::MinAgreement,
            min_agreement: 5, // > 2 providers
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_min_agreement_must_be_positive() {
        let cfg = MultiLLMConfig {
            providers: vec![provider("a"), provider("b")],
            consensus_strategy: ConsensusStrategy::MinAgreement,
            min_agreement: 0,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_min_agreement_happy_path() {
        let cfg = MultiLLMConfig {
            providers: vec![provider("a"), provider("b"), provider("c")],
            consensus_strategy: ConsensusStrategy::MinAgreement,
            min_agreement: 2,
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_first_success_always_valid_with_one_provider() {
        let cfg = MultiLLMConfig {
            providers: vec![provider("solo")],
            consensus_strategy: ConsensusStrategy::FirstSuccess,
            min_agreement: 1,
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_consensus_strategy_serde_snake_case() {
        // The serialization rename_all = "snake_case" matters because
        // config files are hand-edited; test each variant round-trips.
        let cases: &[(ConsensusStrategy, &str)] = &[
            (ConsensusStrategy::Unanimous, "\"unanimous\""),
            (ConsensusStrategy::Majority, "\"majority\""),
            (ConsensusStrategy::MinAgreement, "\"min_agreement\""),
            (ConsensusStrategy::FirstSuccess, "\"first_success\""),
        ];
        for (variant, expected_json) in cases {
            assert_eq!(serde_json::to_string(variant).unwrap(), *expected_json);
            let parsed: ConsensusStrategy = serde_json::from_str(expected_json).unwrap();
            assert_eq!(&parsed, variant);
        }
    }

    #[test]
    fn test_from_env_falls_back_when_no_file_set() {
        // When LLM_CONFIG_FILE isn't set, from_env builds a single-provider
        // config from LLM_PROVIDER / LLM_MODEL / LLM_API_KEY / OLLAMA_ENDPOINT.
        // Use std::env serially with a unique-prefix to avoid stomping on
        // other tests; this test mutates env vars so don't run in parallel.
        // (cargo test serializes within a process by default for #[test]
        // functions in the same module.)
        // Note: we don't unset existing env vars; this just verifies the
        // function doesn't panic on common shapes.
        std::env::remove_var("LLM_CONFIG_FILE");
        std::env::set_var("LLM_PROVIDER", "openai");
        std::env::set_var("LLM_MODEL", "gpt-4o");
        std::env::set_var("LLM_API_KEY", "sk-test");
        let cfg = MultiLLMConfig::from_env();
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.providers[0].provider, "openai");
        assert_eq!(cfg.providers[0].model, "gpt-4o");
        assert_eq!(cfg.providers[0].api_key, Some("sk-test".to_string()));
        std::env::remove_var("LLM_PROVIDER");
        std::env::remove_var("LLM_MODEL");
        std::env::remove_var("LLM_API_KEY");
    }

    #[test]
    fn test_from_env_loads_config_file() {
        // Write a valid multi-LLM config to a tempfile, set
        // LLM_CONFIG_FILE, and verify from_env reads it.
        let tmp = std::env::temp_dir().join("llm_config_test.json");
        let cfg = MultiLLMConfig {
            providers: vec![provider("a"), provider("b")],
            consensus_strategy: ConsensusStrategy::Majority,
            min_agreement: 2,
        };
        std::fs::write(&tmp, serde_json::to_string(&cfg).unwrap()).unwrap();
        std::env::set_var("LLM_CONFIG_FILE", &tmp);
        let loaded = MultiLLMConfig::from_env();
        assert_eq!(loaded.providers.len(), 2);
        assert_eq!(loaded.consensus_strategy, ConsensusStrategy::Majority);
        std::env::remove_var("LLM_CONFIG_FILE");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_from_env_invalid_config_file_falls_back() {
        // If LLM_CONFIG_FILE points at garbage, from_env should fall
        // through to the env-var path rather than panic.
        let tmp = std::env::temp_dir().join("llm_config_garbage.json");
        std::fs::write(&tmp, "this is not json").unwrap();
        std::env::set_var("LLM_CONFIG_FILE", &tmp);
        let _ = MultiLLMConfig::from_env(); // shouldn't panic
        std::env::remove_var("LLM_CONFIG_FILE");
        let _ = std::fs::remove_file(&tmp);
    }
}
