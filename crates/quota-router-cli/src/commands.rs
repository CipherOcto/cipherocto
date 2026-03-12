use crate::balance::Balance;
use crate::completion::{self, Message};
use crate::config::Config;
use crate::providers::{default_endpoint, Provider};
use crate::proxy::ProxyServer;
use anyhow::Result;
use tracing::info;

pub async fn init() -> Result<()> {
    let config = Config::load()?;
    config.save()?;
    info!("Initialized quota-router config");
    println!("Initialized quota-router config");
    Ok(())
}

pub async fn add_provider(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    let endpoint = default_endpoint(name).unwrap_or_else(|| "https://api.example.com".to_string());
    config.providers.push(Provider::new(name, &endpoint));
    config.save()?;
    info!("Added provider: {}", name);
    println!("Added provider: {}", name);
    Ok(())
}

pub async fn balance() -> Result<()> {
    let config = Config::load()?;
    println!("OCTO-W Balance: {}", config.balance);
    Ok(())
}

pub async fn list(prompts: u64, price: u64) -> Result<()> {
    info!("Listed {} prompts at {} OCTO-W each", prompts, price);
    println!("Listed {} prompts at {} OCTO-W each", prompts, price);
    Ok(())
}

pub async fn proxy(port: u16) -> Result<()> {
    let config = Config::load()?;
    let provider = config
        .providers
        .first()
        .cloned()
        .unwrap_or_else(|| Provider::new("openai", "https://api.openai.com/v1"));
    let balance = Balance::new(config.balance);

    let mut server = ProxyServer::new(balance, provider, port);
    server
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("Proxy error: {}", e))?;
    Ok(())
}

pub async fn route(provider: &str, prompt: &str) -> Result<()> {
    info!("Routing test request to {}: {}", provider, prompt);
    println!("Routed to {}: {}", provider, prompt);
    Ok(())
}

/// Health check command
pub async fn health() -> Result<()> {
    println!("✅ quota-router is running");
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    let config = Config::load()?;
    println!("Providers: {}", config.providers.len());
    println!("Balance: {} OCTO-W", config.balance);
    Ok(())
}

/// Embed command - call embedding model
pub async fn embed(model: &str, input: &str) -> Result<()> {
    info!("Embedding request: model={}, input={}", model, input);

    let result = completion::embedding(vec![input.to_string()], model.to_string())?;

    println!("Embedding response:");
    println!("  model: {}", result.model);
    println!("  tokens: {}", result.usage.total_tokens);
    println!(
        "  embedding[0]: {} dimensions",
        result.data[0].embedding.len()
    );

    Ok(())
}

/// Complete command - call completion model
pub async fn complete(model: &str, prompt: &str) -> Result<()> {
    info!("Completion request: model={}, prompt={}", model, prompt);

    let messages = vec![Message::new("user", prompt)];
    let result = completion::completion(model.to_string(), messages)?;

    println!("Completion response:");
    println!("  model: {}", result.model);
    println!("  choices: {}", result.choices.len());
    for (i, choice) in result.choices.iter().enumerate() {
        println!("  choice[{}]: {}", i, choice.message.content);
    }

    Ok(())
}
