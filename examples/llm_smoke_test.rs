//! Smoke test for the configured LLM.
//!
//! Loads .env (or environment) for LLM_PROVIDER / LLM_MODEL / LLM_API_KEY,
//! sends a single log line through the LLM service, and prints the result.
//! Exits non-zero if the API call fails — useful as a first validation
//! before running larger tests.
//!
//! Run with: cargo run --example llm_smoke_test

use log_analyzer::llm_config::MultiLLMConfig;
use log_analyzer::llm_service::LLMServiceClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = MultiLLMConfig::from_env();
    let provider = &config.providers[0];
    let key_status = match &provider.api_key {
        Some(k) if !k.is_empty() => format!("set ({} chars)", k.len()),
        _ => "<unset>".to_string(),
    };

    println!("=== LLM smoke test ===");
    println!("provider:  {}", provider.provider);
    println!("model:     {}", provider.model);
    println!("api_key:   {}", key_status);
    println!(
        "endpoint:  {}",
        provider.endpoint.as_deref().unwrap_or("<default>")
    );
    println!();

    let client = LLMServiceClient::new_with_config(config)?;

    let test_log = "Jun 14 15:16:01 combo sshd(pam_unix)[19939]: authentication failure; \
                    logname= uid=0 euid=0 tty=NODEVssh ruser= rhost=218.188.2.4";
    println!("Sending test log:");
    println!("  {}", test_log);
    println!();

    let start = std::time::Instant::now();
    match client.generate_template(test_log).await {
        Ok(template) => {
            let elapsed = start.elapsed();
            println!("✅ Success in {:?}", elapsed);
            println!();
            println!("template_id: {}", template.template_id);
            println!("pattern:     {}", template.pattern);
            println!("variables:   {:?}", template.variables);
            println!("example:     {}", template.example);
            Ok(())
        }
        Err(e) => {
            println!("❌ LLM call failed: {}", e);
            std::process::exit(1);
        }
    }
}
