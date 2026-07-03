use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

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
    /// Start the mesh daemon (RFC-0870 §Wiring Diagram)
    ///
    /// Binds a TcpAdapter to `listen_addr`, constructs a
    /// QuotaRouterNode from `network_config`, and runs the
    /// gossip / announce / inbound-dispatch loops until SIGTERM.
    Serve {
        /// Listen address for the mesh TCP transport (RFC-0850 §8.8)
        #[arg(long, default_value = "0.0.0.0:9100")]
        listen_addr: SocketAddr,
        /// Path to network config (TOML: node_id, network_id,
        /// peer addresses, providers)
        #[arg(long)]
        network_config: PathBuf,
        /// Mock-provider mode: returns deterministic responses
        /// instead of calling a real LLM provider. Required for
        /// docker tests.
        #[arg(long)]
        mock_provider: bool,
        /// Peer endpoints (comma-separated `node_id:addr`).
        #[arg(long, value_delimiter = ',')]
        peers: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_serve_basic() {
        let cli = Cli::try_parse_from([
            "quota-router",
            "serve",
            "--network-config",
            "/tmp/mesh.toml",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Commands::Serve {
                listen_addr,
                network_config,
                mock_provider,
                peers,
            } => {
                assert_eq!(listen_addr, "0.0.0.0:9100".parse::<SocketAddr>().unwrap());
                assert_eq!(network_config, PathBuf::from("/tmp/mesh.toml"));
                assert!(!mock_provider);
                assert!(peers.is_empty());
            }
            _ => panic!("expected Serve"),
        }
    }

    #[test]
    fn parse_serve_with_peers() {
        let cli = Cli::try_parse_from([
            "quota-router",
            "serve",
            "--network-config",
            "/tmp/mesh.toml",
            "--listen-addr",
            "127.0.0.1:9200",
            "--mock-provider",
            "--peers",
            "abc123:127.0.0.1:9100,def456:127.0.0.1:9101",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Commands::Serve {
                listen_addr,
                network_config,
                mock_provider,
                peers,
            } => {
                assert_eq!(listen_addr, "127.0.0.1:9200".parse::<SocketAddr>().unwrap());
                assert_eq!(network_config, PathBuf::from("/tmp/mesh.toml"));
                assert!(mock_provider);
                assert_eq!(peers.len(), 2);
            }
            _ => panic!("expected Serve"),
        }
    }

    #[test]
    fn parse_proxy() {
        let cli = Cli::try_parse_from(["quota-router", "proxy", "-p", "3000"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Commands::Proxy {
                proxy_port,
                admin_port,
            } => {
                assert_eq!(proxy_port, 3000);
                assert_eq!(admin_port, 8081);
            }
            _ => panic!("expected Proxy"),
        }
    }

    #[test]
    fn parse_route() {
        let cli = Cli::try_parse_from([
            "quota-router",
            "route",
            "--provider",
            "openai",
            "-p",
            "hello world",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Commands::Route { provider, prompt } => {
                assert_eq!(provider, "openai");
                assert_eq!(prompt, "hello world");
            }
            _ => panic!("expected Route"),
        }
    }

    #[test]
    fn parse_init() {
        let cli = Cli::try_parse_from(["quota-router", "init"]);
        assert!(cli.is_ok());
        assert!(matches!(cli.unwrap().command, Commands::Init));
    }

    #[test]
    fn parse_balance() {
        let cli = Cli::try_parse_from(["quota-router", "balance"]);
        assert!(cli.is_ok());
        assert!(matches!(cli.unwrap().command, Commands::Balance));
    }
}
