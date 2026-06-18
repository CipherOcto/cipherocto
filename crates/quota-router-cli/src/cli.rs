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
    List {
        #[arg(long, default_value = "100")]
        prompts: u64,
        #[arg(short, long, default_value = "1")]
        price: u64,
    },
    /// Start proxy server
    Proxy {
        #[arg(short, long, default_value = "8080")]
        proxy_port: u16,
        /// Admin API server port (default: 8081)
        #[arg(long, default_value = "8081")]
        admin_port: u16,
    },
    /// Route a test request
    Route {
        #[arg(long)]
        provider: String,
        #[arg(short, long)]
        prompt: String,
    },
}
