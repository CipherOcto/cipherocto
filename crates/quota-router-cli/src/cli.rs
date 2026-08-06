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
    /// Show reputation for a canonical DID (RFC-0968 / mission 0968-b Phase E).
    ///
    /// Reads the persisted RFC-0968 aggregate for `--did`, displays
    /// `score_ewma` (Dfp), the 0-100 presentation score, samples, and
    /// last_signal_at_unix. Replaces the legacy `provider --name` /
    /// `seller --wallet` / `leaderboard` / `multiplier` subcommands.
    #[command(name = "reputation-show")]
    ReputationShow {
        /// Canonical DID. Both forms accepted per RFC-0010:
        /// W3C `did:octo:z<base58btc>` (53-54 chars) or legacy
        /// `did:octo:b<52>` (62 chars) during the deprecation window.
        #[arg(long)]
        did: String,
        /// Backend store: `memory` (in-memory, default) or `stoolap`
        /// (open the production DB at `--db-path`).
        #[arg(long, default_value = "memory")]
        backend: String,
        /// Path to the stoolap DB file (only when `--backend stoolap`).
        #[arg(long)]
        db_path: Option<PathBuf>,
        /// Refuse under `--strict-deprecation` once legacy CLI subcommands
        /// are retired.
        #[arg(long, default_value_t = false)]
        strict_deprecation: bool,
    },

    /// Compute settlement hash from a partial envelope JSON (RFC-0959 §CLI).
    ///
    /// Reads a JSON `SettlementEnvelope` from `--from` (or stdin via `-`),
    /// recomputes `settlement_hash` via `SettlementEnvelope::compute_settlement_hash()`,
    /// and emits the full envelope JSON with the hash field filled.
    Settle {
        /// Path to envelope JSON, or `-` for stdin.
        #[arg(long, default_value = "-")]
        from: String,
    },

    /// Verify a settlement envelope against replay defense (RFC-0959 §CLI).
    ///
    /// Reads a JSON `SettlementEnvelope` (with `settlement_hash` filled)
    /// from `--from` (or stdin via `-`), recomputes the hash, checks it
    /// matches the embedded field, then checks the nonce against the
    /// persisted `consumed_receipt_index` table (in-memory by default;
    /// pass `--db-path` for file-backed persistence that survives across
    /// CLI invocations). On success, inserts the nonce. On replay
    /// (already-consumed nonce), returns an error.
    SettleReplay {
        /// Path to envelope JSON, or `-` for stdin.
        #[arg(long, default_value = "-")]
        from: String,
        /// Open a file-backed stoolap DB at this path instead of the
        /// in-memory default. Replay-defense state persists across CLI
        /// invocations against the same path.
        #[arg(long)]
        db_path: Option<String>,
    },

    /// List persisted settlement events for an asker (RFC-0959 §Event Sourcing).
    ///
    /// Queries the `settlement_events` table for all events matching
    /// `--asker-did`, ordered by `settled_at_unix` ASC. Each row is
    /// printed as `settlement_hash + ask_id + nonce + cost_micro_octo_w`.
    /// Pair with `--db-path` to query a file-backed DB (must be the same
    /// path used by `settle-replay` to populate the table).
    SettleList {
        /// Asker DID (RFC-0009) to filter events by.
        #[arg(long)]
        asker_did: String,
        /// Path to file-backed stoolap DB; defaults to in-memory (always empty).
        #[arg(long)]
        db_path: Option<String>,
    },

    /// Verify ZK capability proofs (mission zk-proof-verification AC-1 / AC-2 / AC-5).
    ///
    /// Sub-modes (mutually exclusive):
    ///   --from <path-or-"-">    Verify a single proof JSON envelope
    ///   --batch --from <path>    Verify a batch (JSON array of envelopes)
    ///   history                  Print verification history (JSONL log)
    ///
    /// Envelope schema (single proof):
    /// ```json
    /// {
    ///   "proof_bundle":   { "proof_bytes": "<hex>" },
    ///   "public_inputs":  { "proof_issued_at_unix": 1700000000,
    ///                        "verifier_local_unix_time": 1700000005,
    ///                        "compiled_casm_hash": "<hex>",
    ///                        "capability_root_hash": "<hex>",
    ///                        "provider_slot_id": "slot-a" },
    ///   "casm_hash":      "<hex>"
    /// }
    /// ```
    /// Batch envelope: top-level JSON array of single envelopes.
    ///
    /// History defaults to `${XDG_DATA_HOME:-~/.local/share}/quota-router/verify-history.jsonl`
    /// (override via `--history-path`).
    Verify {
        /// Path to proof JSON file, or `-` for stdin. Ignored when sub-mode is `history`.
        #[arg(long, default_value = "-")]
        from: String,
        /// Batch mode: input is a JSON array of single envelopes.
        #[arg(long, default_value_t = false)]
        batch: bool,
        /// Print verification history (newest first).
        #[arg(long, default_value_t = false)]
        history: bool,
        /// Override the verification history file path.
        #[arg(long)]
        history_path: Option<PathBuf>,
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

    #[test]
    fn parse_reputation_show_canonical_did() {
        // 62 chars total: did:octo:b (10) + 52 z's.
        let mut padded = String::from("did:octo:b");
        for _ in 0..52 {
            padded.push('z');
        }
        let cli = Cli::try_parse_from([
            "quota-router",
            "reputation-show",
            "--did",
            padded.as_str(),
            "--backend",
            "memory",
        ]);
        let cli = match cli {
            Ok(c) => c,
            Err(e) => panic!("should parse: got error {e}"),
        };
        match cli.command {
            Commands::ReputationShow {
                did: got,
                backend,
                db_path,
                strict_deprecation,
            } => {
                assert_eq!(got, padded);
                assert_eq!(backend, "memory");
                assert!(db_path.is_none());
                assert!(!strict_deprecation);
            }
            _ => panic!("expected ReputationShow"),
        }
    }

    #[test]
    fn parse_verify_default_single_mode() {
        // No flags: single-mode, `--from -` (stdin), history off.
        let cli = Cli::try_parse_from(["quota-router", "verify"]);
        let cli = cli.expect("verify default args should parse");
        match cli.command {
            Commands::Verify {
                from,
                batch,
                history,
                history_path,
            } => {
                assert_eq!(from, "-");
                assert!(!batch);
                assert!(!history);
                assert!(history_path.is_none());
            }
            _ => panic!("expected Verify"),
        }
    }

    #[test]
    fn parse_verify_batch_with_from() {
        let cli = Cli::try_parse_from([
            "quota-router",
            "verify",
            "--batch",
            "--from",
            "/tmp/proofs.json",
        ]);
        let cli = cli.expect("verify --batch --from should parse");
        match cli.command {
            Commands::Verify {
                from,
                batch,
                history,
                history_path,
            } => {
                assert_eq!(from, "/tmp/proofs.json");
                assert!(batch);
                assert!(!history);
                assert!(history_path.is_none());
            }
            _ => panic!("expected Verify"),
        }
    }

    #[test]
    fn parse_verify_history_with_path_override() {
        // `--history` is a flag (not a subcommand); `--history-path` overrides
        // the default XDG location. Both flags must coexist on the same
        // command line.
        let cli = Cli::try_parse_from([
            "quota-router",
            "verify",
            "--history",
            "--history-path",
            "/var/log/verify.jsonl",
        ]);
        let cli = cli.expect("verify --history --history-path should parse");
        match cli.command {
            Commands::Verify {
                history,
                history_path,
                ..
            } => {
                assert!(history);
                assert_eq!(history_path, Some(PathBuf::from("/var/log/verify.jsonl")));
            }
            _ => panic!("expected Verify"),
        }
    }

    #[test]
    fn parse_verify_history_flag_explicit() {
        let cli = Cli::try_parse_from(["quota-router", "verify", "--history"]);
        let cli = cli.expect("verify --history flag should parse");
        match cli.command {
            Commands::Verify { history, .. } => assert!(history),
            _ => panic!("expected Verify"),
        }
    }
}
