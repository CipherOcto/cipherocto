//! Example: Using the Telegram adapter with a mock client.
//!
//! Run with: `cargo run -p octo-adapter-telegram --example bot_mode`
//!
//! For real TDLib usage, enable the `real-tdlib` feature and provide
//! a valid bot token via TelegramConfig.

use octo_adapter_telegram::adapter::TelegramAdapter;
use octo_adapter_telegram::client::TelegramClient;
use octo_adapter_telegram::config::TelegramConfig;
use octo_adapter_telegram::mock::MockTelegramClient;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::{BroadcastDomainId, PlatformType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Build config (bot mode, no real token needed for mock)
    // Note: api_id and api_hash are required even for bot mode (TDLib requirement).
    // Get real values from https://my.telegram.org for production use.
    let config = TelegramConfig {
        mode: Some("bot".into()),
        bot_token: Some("1234567890:ABCdefGHIjklMNOpqrsTUVwxyz".into()),
        api_id: Some(12345),
        api_hash: Some("abcdef1234567890abcdef1234567890".into()),
        ..Default::default()
    };
    config.validate()?;

    // 2. Create a mock client (replace with RealTelegramClient for production)
    let client = MockTelegramClient::new();

    // 3. Build the adapter
    let adapter = TelegramAdapter::new(config, client);

    // 4. Register a domain (chat_id must be negative for Telegram groups)
    let domain = BroadcastDomainId::new(PlatformType::Telegram, "-1001234567890");
    adapter.register_domain(&domain, "-1001234567890")?;
    println!("Registered domain: {:?}", domain);

    // 5. Authenticate (mock returns Ok immediately)
    adapter.client.authenticate().await?;
    println!("Authenticated");

    // 6. Receive messages (empty on mock unless injected)
    let messages = adapter.receive_messages(&domain).await?;
    println!("Received {} messages", messages.len());

    // 7. Health check
    let health = adapter.health_check().await;
    println!("Health: {:?}", health);

    // 8. Capabilities
    let caps = adapter.capabilities();
    println!("Max payload: {} bytes", caps.max_payload_bytes);

    println!("\nTo send envelopes, construct a DeterministicEnvelope with");
    println!("signing keys from octo-network and call adapter.send_envelope().");

    Ok(())
}
