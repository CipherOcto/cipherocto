use anyhow::Result;
use clap::Parser;
use quota_router_cli::cli::{Cli, Commands};
use quota_router_cli::commands as cmd;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Handle global --config flag
    if let Some(config_path) = &cli.config {
        println!("Using config: {}", config_path);
    }

    // Handle global --model flag
    if let Some(model) = &cli.model {
        println!("Using model: {}", model);
    }

    match cli.command {
        Some(Commands::Init) => cmd::init().await?,
        Some(Commands::AddProvider { name }) => cmd::add_provider(&name).await?,
        Some(Commands::Balance) => cmd::balance().await?,
        Some(Commands::List { prompts, price }) => cmd::list(prompts, price).await?,
        Some(Commands::Proxy { port }) => cmd::proxy(port).await?,
        Some(Commands::Route { provider, prompt }) => cmd::route(&provider, &prompt).await?,
        Some(Commands::Health) => cmd::health().await?,
        Some(Commands::Embed { model, input }) => cmd::embed(&model, &input).await?,
        Some(Commands::Complete { model, prompt }) => cmd::complete(&model, &prompt).await?,
        None => {
            // No subcommand - show help or start proxy by default
            cmd::health().await?;
        }
    }

    Ok(())
}
