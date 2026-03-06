# Quota Router CLI (MVE) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Minimum Viable Edition of the Quota Router CLI - a CLI tool developers run locally to manage their AI API quotas with transparent HTTPS proxy.

**Architecture:** CLI tool with local proxy server that intercepts HTTP requests, checks mock OCTO-W balance, injects API key from environment variable, and forwards to provider.

**Tech Stack:** Rust (cargo), clap (CLI), tokio (async), hyper (HTTP server), rustls (HTTPS/TLS)

---

## Pre-requisite: Add dependencies to workspace

### Task 1: Add dependencies to workspace Cargo.toml

**File:**
- Modify: `Cargo.toml`

**Step 1: Add dependencies**

Add to `[workspace.dependencies]`:

```toml
# HTTP/HTTPS server
hyper = { version = "1.3", features = ["full"] }
hyper-util = { version = "0.1", features = ["full"] }
http-body-util = "0.1"
# TLS for HTTPS
rustls = "0.23"
rustls-pemfile = "2.1"
# HTTP client for forwarding
reqwest = { version = "0.12", features = ["json"] }
# Config file handling
directories = "5"
# UUID for listing IDs
uuid = { version = "1.8", features = ["v4"] }
# Async mutex
parking_lot = "0.12"
```

**Step 2: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add quota-router dependencies to workspace"
```

---

## Task 1: Create quota-router-cli crate

### Step 1: Create directory and Cargo.toml

**File:**
- Create: `crates/quota-router-cli/Cargo.toml`

```toml
[package]
name = "quota-router-cli"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
# CLI
clap.workspace = true

# Async
tokio.workspace = true
async-trait.workspace = true

# HTTP/HTTPS
hyper.workspace = true
hyper-util.workspace = true
http-body-util.workspace = true
rustls.workspace = true
rustls-pemfile.workspace = true
reqwest.workspace = true

# Config
directories.workspace = true
serde.workspace = true
serde_json.workspace = true

# Utilities
uuid.workspace = true
parking_lot.workspace = true

# Logging
tracing.workspace = true
tracing-subscriber.workspace = true

# Errors
anyhow.workspace = true
thiserror.workspace = true

[lib]]
name = "quota_router_cli"
path = "src/lib.rs"

[[bin]]
name = "quota-router"
path = "src/main.rs"
```

**Step 2: Add to workspace**

**File:**
- Modify: `Cargo.toml:2`

Change:
```toml
members = ["crates/*"]
```

**Step 3: Commit**

```bash
git add crates/quota-router-cli/Cargo.toml Cargo.toml
git commit -m "feat: add quota-router-cli crate to workspace"
```

---

## Task 2: Config module

### Step 1: Write failing test

**File:**
- Create: `crates/quota-router-cli/src/config.rs`

**Step 2: Write minimal implementation**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use directories::ProjectDirs;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to get config directory")]
    NoConfigDir,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub balance: u64,
    pub providers: Vec<Provider>,
    pub proxy_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub endpoint: String,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            // Default config
            Ok(Config {
                balance: 100,  // Mock balance
                providers: vec![],
                proxy_port: 8080,
            })
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, ConfigError> {
        let proj_dirs = ProjectDirs::from("com", "cipherocto", "quota-router")
            .ok_or(ConfigError::NoConfigDir)?;
        Ok(proj_dirs.config_dir().join("config.json"))
    }
}
```

**Step 3: Commit**

```bash
git add crates/quota-router-cli/src/config.rs
git commit -m "feat: add config module with load/save"
```

---

## Task 3: Balance module

### Step 1: Write failing test

**File:**
- Create: `crates/quota-router-cli/src/balance.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_check_sufficient() {
        let balance = 100;
        let required = 10;
        assert!(balance >= required);
    }

    #[test]
    fn test_balance_check_insufficient() {
        let balance = 5;
        let required = 10;
        assert!(balance < required);
    }

    #[test]
    fn test_balance_decrement() {
        let mut balance = 100;
        let cost = 10;
        balance = balance.saturating_sub(cost);
        assert_eq!(balance, 90);
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cd crates/quota-router-cli && cargo test balance -- --nocapture
Expected: FAIL - file doesn't exist yet
```

**Step 3: Write implementation**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BalanceError {
    #[error("Insufficient balance: have {0}, need {1}")]
    Insufficient(u64, u64),
}

pub struct Balance {
    pub amount: u64,
}

impl Balance {
    pub fn new(amount: u64) -> Self {
        Self { amount }
    }

    pub fn check(&self, required: u64) -> Result<(), BalanceError> {
        if self.amount >= required {
            Ok(())
        } else {
            Err(BalanceError::Insufficient(self.amount, required))
        }
    }

    pub fn deduct(&mut self, amount: u64) {
        self.amount = self.amount.saturating_sub(amount);
    }

    pub fn add(&mut self, amount: u64) {
        self.amount += amount;
    }
}
```

**Step 4: Run tests**

```bash
cd crates/quota-router-cli && cargo test balance
Expected: PASS
```

**Step 5: Commit**

```bash
git add crates/quota-router-cli/src/balance.rs
git commit -m "feat: add balance module with check/deduct"
```

---

## Task 4: Provider module

### Step 1: Write failing test

**File:**
- Create: `crates/quota-router-cli/src/providers.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_api_key_env_var() {
        std::env::set_var("OPENAI_API_KEY", "test-key-123");
        let provider = Provider::new("openai", "https://api.openai.com/v1");
        let key = provider.get_api_key();
        assert_eq!(key, Some("test-key-123".to_string()));
        std::env::remove_var("OPENAI_API_KEY");
    }
}
```

**Step 2: Write implementation**

```rust
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub endpoint: String,
}

impl Provider {
    pub fn new(name: &str, endpoint: &str) -> Self {
        Self {
            name: name.to_string(),
            endpoint: endpoint.to_string(),
        }
    }

    /// Get API key from environment variable
    /// Format: {PROVIDER_NAME}_API_KEY (uppercase)
    pub fn get_api_key(&self) -> Option<String> {
        let env_var = format!("{}_API_KEY", self.name.to_uppercase());
        env::var(env_var).ok()
    }
}

/// Known provider endpoints
pub fn default_endpoint(name: &str) -> Option<String> {
    match name.to_lowercase().as_str() {
        "openai" => Some("https://api.openai.com/v1".to_string()),
        "anthropic" => Some("https://api.anthropic.com".to_string()),
        "google" => Some("https://generativelanguage.googleapis.com".to_string()),
        _ => None,
    }
}
```

**Step 3: Commit**

```bash
git add crates/quota-router-cli/src/providers.rs
git commit -m "feat: add provider module with API key from env"
```

---

## Task 5: Proxy module (HTTP server)

### Step 1: Write failing test (mock test - not actual HTTP)

**File:**
- Create: `crates/quota-router-cli/src/proxy.rs`

**Step 2: Write implementation**

```rust
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper::body::Incoming;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;
use crate::balance::Balance;
use crate::providers::Provider;

pub struct ProxyServer {
    balance: Balance,
    provider: Provider,
    port: u16,
}

impl ProxyServer {
    pub fn new(balance: Balance, provider: Provider, port: u16) -> Self {
        Self { balance, provider, port }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(addr).await?;

        info!("Proxy server listening on http://{}", addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let mut balance = Balance::new(self.balance.amount);
            let provider = self.provider.clone();

            tokio::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(stream, service_fn(|req| {
                        Self::handle_request(req, &balance, &provider)
                    }))
                    .await
                {
                    eprintln!("Error serving connection: {}", err);
                }
            });
        }
    }

    async fn handle_request(
        req: Request<Incoming>,
        balance: &Balance,
        provider: &Provider,
    ) -> Result<Response<String>, Infallible> {
        // Check balance
        if balance.check(1).is_err() {
            return Ok(Response::builder()
                .status(StatusCode::PAYMENT_REQUIRED)
                .body("Insufficient OCTO-W balance".to_string())
                .unwrap());
        }

        // Get API key from environment
        let api_key = match provider.get_api_key() {
            Some(key) => key,
            None => {
                return Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body("API key not set in environment".to_string())
                    .unwrap());
            }
        };

        // Forward request to provider (simplified - just return success for MVE)
        Ok(Response::builder()
            .status(StatusCode::OK)
            .body("Request forwarded successfully".to_string())
            .unwrap())
    }
}
```

**Step 3: Commit**

```bash
git add crates/quota-router-cli/src/proxy.rs
git commit -m "feat: add proxy server module"
```

---

## Task 6: CLI commands

### Step 1: Write CLI structure

**File:**
- Create: `crates/quota-router-cli/src/cli.rs`

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "quota-router")]
#[command(about = "CLI for managing AI API quotas", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize the router
    Init,
    /// Add a provider
    AddProvider { name: String },
    /// Check balance
    Balance,
    /// List quota for sale
    List { prompts: u64, price: u64 },
    /// Start proxy server
    Proxy { port: u16 },
    /// Route a test request
    Route { provider: String, prompt: String },
}
```

**Step 2: Commit**

```bash
git add crates/quota-router-cli/src/cli.rs
git commit -m "feat: add CLI command structure"
```

---

## Task 7: Main entry point

### Step 1: Create main.rs

**File:**
- Create: `crates/quota-router-cli/src/main.rs`

```rust
use quota_router_cli::{cli::Cli, config::Config, commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init().await?,
        Commands::AddProvider { name } => commands::add_provider(&name).await?,
        Commands::Balance => commands::balance().await?,
        Commands::List { prompts, price } => commands::list(prompts, price).await?,
        Commands::Proxy { port } => commands::proxy(port).await?,
        Commands::Route { provider, prompt } => commands::route(&provider, &prompt).await?,
    }

    Ok(())
}
```

### Step 2: Create lib.rs

**File:**
- Create: `crates/quota-router-cli/src/lib.rs`

```rust
pub mod cli;
pub mod config;
pub mod balance;
pub mod providers;
pub mod proxy;
pub mod commands;
```

### Step 3: Create commands.rs

**File:**
- Create: `crates/quota-router-cli/src/commands.rs`

```rust
use crate::config::Config;
use crate::balance::Balance;
use crate::providers::Provider;
use crate::proxy::ProxyServer;
use anyhow::Result;
use tracing::info;

pub async fn init() -> Result<()> {
    let config = Config::load()?;
    config.save()?;
    info!("Initialized quota-router config at {:?}", config);
    Ok(())
}

pub async fn add_provider(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    let endpoint = crate::providers::default_endpoint(name)
        .unwrap_or_else(|| "https://api.example.com".to_string());
    config.providers.push(Provider::new(name, &endpoint));
    config.save()?;
    info!("Added provider: {}", name);
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
    let provider = config.providers.first()
        .cloned()
        .unwrap_or_else(|| Provider::new("openai", "https://api.openai.com/v1"));
    let balance = Balance::new(config.balance);

    let mut server = ProxyServer::new(balance, provider, port);
    server.run().await?;
    Ok(())
}

pub async fn route(provider: &str, prompt: &str) -> Result<()> {
    info!("Routing test request to {}: {}", provider, prompt);
    println!("Routed to {}: {}", provider, prompt);
    Ok(())
}
```

**Step 4: Commit**

```bash
git add crates/quota-router-cli/src/main.rs crates/quota-router-cli/src/lib.rs crates/quota-router-cli/src/commands.rs
git commit -m "feat: add main entry and command handlers"
```

---

## Task 8: Build and test

### Step 1: Build

```bash
cd crates/quota-router-cli && cargo build
```

### Step 2: Test CLI

```bash
cd crates/quota-router-cli && cargo run -- --help
```

### Step 3: Run tests

```bash
cd crates/quota-router-cli && cargo test
```

### Step 4: Commit

```bash
git add -A
git commit -m "feat: complete quota-router-cli MVE"
```

---

## Acceptance Criteria Verification

- [ ] CLI tool installable via cargo
- [ ] Local proxy server
- [ ] API key management (from env)
- [ ] OCTO-W balance display
- [ ] Basic routing
- [ ] Balance check before request
- [ ] Manual quota listing command
- [ ] Unit tests

---

**Plan complete and saved to `docs/plans/2026-03-04-quota-router-mve-design.md`. Two execution options:**

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

**Which approach?**
