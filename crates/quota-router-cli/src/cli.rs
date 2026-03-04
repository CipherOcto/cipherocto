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
