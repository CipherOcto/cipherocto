use anyhow::Result;
use clap::Parser;
use quota_router_cli::cli::{Cli, Commands};
use quota_router_cli::commands as cmd;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => cmd::init().await?,
        Commands::AddProvider { name } => cmd::add_provider(&name).await?,
        Commands::Balance => cmd::balance().await?,
        Commands::List { prompts, price } => cmd::list(prompts, price).await?,
        Commands::Proxy { port } => cmd::proxy(port).await?,
        Commands::Route { provider, prompt } => cmd::route(&provider, &prompt).await?,
    }

    Ok(())
}
