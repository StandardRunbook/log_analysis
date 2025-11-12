/// Example demonstrating multi-LLM consensus
///
/// Run with: cargo run --example multi_llm_test

use log_analyzer::llm_config::{MultiLLMConfig, LLMProviderConfig, ConsensusStrategy};
use log_analyzer::llm_service::LLMServiceClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Example 1: Single provider (backward compatible)
    println!("\n=== Example 1: Single Provider ===");
    let single_config = MultiLLMConfig {
        providers: vec![
            LLMProviderConfig {
                name: "local-ollama".to_string(),
                provider: "ollama".to_string(),
                model: "llama3".to_string(),
                api_key: None,
                endpoint: Some("http://localhost:11434".to_string()),
                timeout_secs: Some(60),
            }
        ],
        consensus_strategy: ConsensusStrategy::FirstSuccess,
        min_agreement: 1,
    };

    let client = LLMServiceClient::new_with_config(single_config)?;
    println!("Client configured successfully!");

    // Example 2: Multi-provider with majority consensus
    println!("\n=== Example 2: Multi-Provider with Majority Consensus ===");
    let multi_config = MultiLLMConfig {
        providers: vec![
            LLMProviderConfig {
                name: "ollama-llama3".to_string(),
                provider: "ollama".to_string(),
                model: "llama3".to_string(),
                api_key: None,
                endpoint: Some("http://localhost:11434".to_string()),
                timeout_secs: Some(60),
            },
            // Uncomment if you have API keys:
            // LLMProviderConfig {
            //     name: "openai-gpt4".to_string(),
            //     provider: "openai".to_string(),
            //     model: "gpt-4".to_string(),
            //     api_key: Some(std::env::var("OPENAI_API_KEY")?),
            //     endpoint: None,
            //     timeout_secs: Some(60),
            // },
        ],
        consensus_strategy: ConsensusStrategy::FirstSuccess,
        min_agreement: 1,
    };

    let multi_client = LLMServiceClient::new_with_config(multi_config)?;
    println!("Multi-LLM client configured successfully!");

    // Example 3: Test template generation (if Ollama is running)
    println!("\n=== Example 3: Test Template Generation ===");
    let test_log = "2024-01-15 10:30:45 ERROR Connection timeout after 5000ms to host 192.168.1.100";

    println!("Generating template for: {}", test_log);
    match client.generate_template(test_log).await {
        Ok(template) => {
            println!("✅ Template generated successfully!");
            println!("   Pattern: {}", template.pattern);
            println!("   Variables: {:?}", template.variables);
        }
        Err(e) => {
            println!("⚠️  Failed to generate template: {}", e);
            println!("   (Make sure Ollama is running: ollama serve)");
        }
    }

    // Example 4: Load config from file
    println!("\n=== Example 4: Load Config from File ===");
    let config_from_env = MultiLLMConfig::from_env();
    println!("Config providers: {}", config_from_env.providers.len());
    println!("Strategy: {:?}", config_from_env.consensus_strategy);

    println!("\n✅ All examples completed!");
    Ok(())
}
