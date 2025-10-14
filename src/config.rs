use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    // Metadata service configuration (gRPC)
    pub metadata_grpc_endpoint: String,

    // ClickHouse configuration
    pub clickhouse_url: String,
    pub clickhouse_user: String,
    pub clickhouse_password: String,
    pub clickhouse_database: String,

    // LLM configuration (Rig)
    pub llm_provider: String, // e.g., "openai", "anthropic", "cohere", "ollama"
    pub llm_api_key: String,
    pub llm_model: String, // e.g., "gpt-4", "claude-3-sonnet"

    // Ollama configuration (optional)
    pub ollama_endpoint: Option<String>, // e.g., "http://localhost:11434"
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let llm_provider = env::var("LLM_PROVIDER").map_err(|_| {
            "LLM_PROVIDER environment variable is required (e.g., 'openai', 'anthropic')"
        })?;

        Ok(Config {
            metadata_grpc_endpoint: env::var("METADATA_GRPC_ENDPOINT")
                .map_err(|_| "METADATA_GRPC_ENDPOINT environment variable is required")?,

            clickhouse_url: env::var("CLICKHOUSE_URL")
                .map_err(|_| "CLICKHOUSE_URL environment variable is required")?,

            clickhouse_user: env::var("CLICKHOUSE_USER")
                .map_err(|_| "CLICKHOUSE_USER environment variable is required")?,

            clickhouse_password: env::var("CLICKHOUSE_PASSWORD")
                .map_err(|_| "CLICKHOUSE_PASSWORD environment variable is required")?,

            clickhouse_database: env::var("CLICKHOUSE_DATABASE")
                .unwrap_or_else(|_| "default".to_string()),

            llm_api_key: env::var("LLM_API_KEY")
                .map_err(|_| "LLM_API_KEY environment variable is required")?,

            llm_model: env::var("LLM_MODEL").unwrap_or_else(|_| {
                // Provide sensible defaults based on provider
                match llm_provider.as_str() {
                    "openai" => "gpt-4".to_string(),
                    "anthropic" => "claude-3-sonnet-20240229".to_string(),
                    "cohere" => "command".to_string(),
                    "ollama" => env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string()),
                    _ => "gpt-4".to_string(),
                }
            }),

            llm_provider,

            ollama_endpoint: env::var("OLLAMA_ENDPOINT").ok(),
        })
    }

    pub fn log_config(&self) {
        tracing::info!("📋 Configuration:");
        tracing::info!("   Metadata gRPC Endpoint: {}", self.metadata_grpc_endpoint);
        tracing::info!("   ClickHouse URL: {}", self.clickhouse_url);
        tracing::info!("   ClickHouse User: {}", self.clickhouse_user);
        tracing::info!(
            "   ClickHouse Password: {}***",
            &self.clickhouse_password.chars().take(2).collect::<String>()
        );
        tracing::info!("   ClickHouse Database: {}", self.clickhouse_database);
        tracing::info!("   LLM Provider: {}", self.llm_provider);
        tracing::info!("   LLM Model: {}", self.llm_model);
        tracing::info!(
            "   LLM API Key: {}***",
            &self.llm_api_key.chars().take(4).collect::<String>()
        );
        if let Some(ref endpoint) = self.ollama_endpoint {
            tracing::info!("   Ollama Endpoint: {}", endpoint);
        }
    }
}
