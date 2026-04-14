use crate::balance::Balance;
use crate::config::Config;
use crate::providers::{default_endpoint, Provider};
use crate::proxy::ProxyServer;
use anyhow::Result;
use quota_router_core::admin::AdminServer;
use quota_router_core::{init_database, StoolapKeyStorage};
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

pub async fn proxy(proxy_port: u16, admin_port: u16) -> Result<()> {
    let config = Config::load()?;

    // Ensure db_path parent directory exists
    if let Some(parent) = config.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Open database and initialize schema
    let db = stoolap::Database::open(&format!("file://{}", config.db_path.display()))?;
    init_database(&db)?;

    // Create storage and admin server
    let storage = StoolapKeyStorage::new(db);
    let mut admin_server = AdminServer::new(storage, admin_port);

    // Get provider for proxy
    let provider = config
        .providers
        .first()
        .cloned()
        .unwrap_or_else(|| Provider::new("openai", "https://api.openai.com/v1"));
    let balance = Balance::new(config.balance);
    let mut proxy_server = ProxyServer::new(balance, provider, proxy_port);

    // Run both servers
    tokio::spawn(async move {
        if let Err(e) = admin_server.run().await {
            eprintln!("Admin server error: {}", e);
        }
    });

    info!("Starting proxy server on port {}", proxy_port);
    info!("Starting admin API server on port {}", admin_port);

    proxy_server
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
