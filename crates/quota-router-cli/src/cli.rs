use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "quota-router")]
#[command(about = "CLI for managing AI API quotas - LiteLLM compatible", long_about = None)]
pub struct Cli {
    /// Path to config file (YAML or JSON)
    #[arg(short, long)]
    pub config: Option<String>,

    /// Model to use
    #[arg(short, long)]
    pub model: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
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
    List {
        #[arg(long, default_value = "100")]
        prompts: u64,
        #[arg(short, long, default_value = "1")]
        price: u64,
    },
    /// Start proxy server
    Proxy {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
    /// Route a test request
    Route {
        #[arg(long)]
        provider: String,
        #[arg(short, long)]
        prompt: String,
    },
    /// Health check
    Health,
    /// Call embedding model
    Embed {
        /// Model name
        #[arg(short, long)]
        model: String,
        /// Input text
        #[arg(short, long)]
        input: String,
    },
    /// Call completion model
    Complete {
        /// Model name
        #[arg(short, long)]
        model: String,
        /// Prompt/message
        #[arg(short, long)]
        prompt: String,
    },
}
