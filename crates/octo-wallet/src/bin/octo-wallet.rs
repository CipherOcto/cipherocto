//! `octo-wallet` CLI binary.
//!
//! Subcommands per S01 plan §3 Step 6 + mission `0102-a-wallet-foundation` §Acceptance:
//! - `init --node-type <wholesale|self-host|hybrid>` — bootstrap identity
//! - `import --from starkli --path <keystore.json>` — (S01 Step 2; Starkli import)
//! - `export --to starkli --out <keystore.json>` — (S01 Step 2; Starkli export)
//! - `derive-cap --audience <DID> --channel <id>` — derive capability key
//! - `vault put --slot <id>` — store provider key (passphrase prompted)
//! - `vault get --slot <id>` — retrieve provider key (passphrase prompted)
//! - `vault list` — list slot IDs
//!
//! Passphrase is ALWAYS prompted via stdin (`rpassword`) — NEVER argv (visible in `ps`).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use std::io::{Read as _, Write as _};

use octo_wallet::{vault::Vault, AudienceId, CapabilityKey, ChannelId, IdentityKey, NodeType};
use quota_router_storage::ask::{
    AskError, AskSigned, AskSignedError, AskUnsignedPayload, ModelRateTable, ModelRef,
    NodeType as StorageNodeType,
};

#[derive(Debug, Parser)]
#[command(name = "octo-wallet", version, about = "CipherOcto wallet CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Initialize a new identity (writes 32-byte seed to `--seed-out`).
    Init {
        #[arg(long, value_enum)]
        node_type: CliNodeType,
        #[arg(long, default_value = "identity.seed")]
        seed_out: PathBuf,
    },

    /// Derive a capability key for (audience, channel).
    DeriveCap {
        #[arg(long)]
        audience: String,
        #[arg(long)]
        channel: String,
        #[arg(long)]
        seed: PathBuf,
        /// Hex output (default); `--no-hex` prints raw bytes to stdout (UNSAFE for shell pipes).
        #[arg(long, default_value_t = true)]
        hex: bool,
    },

    /// Vault operations.
    Vault {
        #[command(subcommand)]
        op: VaultOp,
    },

    /// Ask marketplace operations (RFC-0959 §CLI).
    Ask {
        #[command(subcommand)]
        op: AskOp,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliNodeType {
    Wholesale,
    SelfHost,
    Hybrid,
}

impl From<CliNodeType> for NodeType {
    fn from(c: CliNodeType) -> Self {
        match c {
            CliNodeType::Wholesale => Self::Wholesale,
            CliNodeType::SelfHost => Self::SelfHost,
            CliNodeType::Hybrid => Self::Hybrid,
        }
    }
}

impl From<CliNodeType> for StorageNodeType {
    fn from(c: CliNodeType) -> Self {
        match c {
            CliNodeType::Wholesale => Self::Wholesale,
            CliNodeType::SelfHost => Self::SelfHost,
            CliNodeType::Hybrid => Self::Hybrid,
        }
    }
}

#[derive(Debug, Subcommand)]
enum VaultOp {
    /// Encrypt a key into the named vault slot. Passphrase prompted.
    Put {
        #[arg(long)]
        slot: String,
        /// Read plaintext from stdin instead of prompting.
        #[arg(long, default_value_t = false)]
        stdin: bool,
    },
    /// Decrypt + print a vault slot's key. Passphrase prompted.
    Get {
        #[arg(long)]
        slot: String,
    },
    /// List all vault slot IDs.
    List,
}

/// Ask marketplace subcommands (RFC-0959 §CLI).
///
/// `publish` builds a signed `AskSigned` from CLI args + the wallet seed
/// and emits the JSON envelope to stdout. The signer is the Ed25519 key
/// derived from `--seed` (same as `init`).
#[derive(Debug, Subcommand)]
enum AskOp {
    /// Publish a signed Ask (RFC-0959 §CLI; mission 0959-a AC).
    ///
    /// Reads the wallet seed from `--seed`, builds an `AskUnsignedPayload`
    /// from the CLI args, signs via `AskSigned::sign`, and emits the
    /// JSON envelope to stdout. The asker DID is derived from the seed
    /// (Ed25519 verifying key → `did:octo:b<base58btc>` form per RFC-0009).
    Publish {
        /// `NodeType` of the asker (`wholesale` / `self-host` / `hybrid`).
        #[arg(long, value_enum)]
        node_type: CliNodeType,
        /// Model reference `namespace/family` (e.g., `openai/gpt-4`).
        #[arg(long)]
        model: String,
        /// Per-axis rate entries as `axis_id:rate_per_1k` pairs, comma-separated.
        /// Example: `--axes input_tokens_per_1k:30,output_tokens_per_1k:60`
        #[arg(long, value_delimiter = ',')]
        axes: Vec<String>,
        /// TTL in Unix seconds (Ask is invalid after this).
        #[arg(long)]
        ttl_unix: u64,
        /// Jurisdiction tag(s) (at least one required by RFC-0959).
        #[arg(long, value_delimiter = ',')]
        jurisdiction: Vec<String>,
        /// 32-hex-character nonce (16 bytes). Defaults to BLAKE3("ask-nonce:v1")
        /// for deterministic test fixtures; pass explicit hex for production.
        #[arg(
            long,
            default_value = "8c3e6f4b2a1d9e7c5f8b3a6d9c2e5f8b1a4d7c0e3f6b9a2d5c8e1f4b7a0d3c6f"
        )]
        nonce_hex: String,
        /// Unix timestamp at which the payload is assembled. Defaults to
        /// wall-clock `now` (Unix seconds).
        #[arg(long, default_value_t = 0)]
        published_at_unix: u64,
        /// Path to the 32-byte wallet seed (written by `init`).
        #[arg(long, default_value = "identity.seed")]
        seed: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.cmd {
        Cmd::Init {
            node_type,
            seed_out,
        } => {
            let id = IdentityKey::generate()?;
            std::fs::write(&seed_out, id.seed_bytes_for_hkdf()?)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&seed_out, std::fs::Permissions::from_mode(0o600))?;
            }
            let nt: NodeType = node_type.into();
            eprintln!(
                "initialized identity (node-type={nt}) seed={} public_key={}",
                seed_out.display(),
                hex::encode(id.public_key_bytes()),
            );
            Ok(())
        }

        Cmd::DeriveCap {
            audience,
            channel,
            seed,
            hex,
        } => {
            let audience: AudienceId = audience.parse()?;
            let channel: ChannelId = channel.parse()?;
            let seed_bytes = std::fs::read(&seed)?;
            if seed_bytes.len() != 32 {
                return Err(format!("seed must be 32 bytes, got {}", seed_bytes.len()).into());
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&seed_bytes);
            let id = IdentityKey::from_seed(arr);
            let cap: CapabilityKey = octo_wallet::derive_capability_key(&id, &audience, &channel)?;
            if hex {
                println!("{}", hex::encode(cap.as_bytes()));
            } else {
                std::io::stdout().write_all(cap.as_bytes())?;
            }
            Ok(())
        }

        Cmd::Vault { op } => {
            let vault = Vault::open_default()?;
            match op {
                VaultOp::Put { slot, stdin } => {
                    let passphrase = rpassword::prompt_password("vault passphrase: ")?;
                    let plaintext = if stdin {
                        let mut buf = Vec::new();
                        std::io::stdin().read_to_end(&mut buf)?;
                        buf
                    } else {
                        rpassword::prompt_password("provider key (input hidden): ")?.into_bytes()
                    };
                    vault.put(&slot, &plaintext, &passphrase)?;
                    eprintln!("stored slot `{slot}`");
                    Ok(())
                }
                VaultOp::Get { slot } => {
                    let passphrase = rpassword::prompt_password("vault passphrase: ")?;
                    let mut buf = Vec::new();
                    let handle = vault.get(&slot, &passphrase, &mut buf)?;
                    std::io::stdout().write_all(handle.as_bytes())?;
                    std::io::stdout().flush()?;
                    Ok(())
                }
                VaultOp::List => {
                    for s in vault.list()? {
                        println!("{s}");
                    }
                    Ok(())
                }
            }
        }

        Cmd::Ask { op } => match op {
            AskOp::Publish {
                node_type,
                model,
                axes,
                ttl_unix,
                jurisdiction,
                nonce_hex,
                published_at_unix,
                seed,
            } => ask_publish(
                node_type,
                &model,
                &axes,
                ttl_unix,
                &jurisdiction,
                &nonce_hex,
                published_at_unix,
                &seed,
            ),
        },
    }
}

/// Build + sign an Ask, emit JSON to stdout (RFC-0959 §CLI).
#[allow(clippy::too_many_arguments)]
fn ask_publish(
    node_type: CliNodeType,
    model: &str,
    axes: &[String],
    ttl_unix: u64,
    jurisdiction: &[String],
    nonce_hex: &str,
    published_at_unix: u64,
    seed: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{SystemTime, UNIX_EPOCH};

    // 1. Parse node_type + model.
    let nt: StorageNodeType = node_type.into();
    let model_ref = ModelRef::parse(model).map_err(|e| format!("invalid --model: {e}"))?;

    // 2. Parse axes (axis_id:rate_per_1k pairs).
    let mut rates = Vec::with_capacity(axes.len());
    for entry in axes {
        let (axis_id, rate_str) = entry
            .split_once(':')
            .ok_or_else(|| format!("invalid --axes entry `{entry}` (expected `<axis>:<rate>`)"))?;
        let rate: u128 = rate_str
            .parse()
            .map_err(|e| format!("invalid rate for axis `{axis_id}`: {e}"))?;
        rates.push(quota_router_storage::ask::AxisRate {
            axis: axis_id.to_owned(),
            rate_per_1k: {
                let v: i64 = rate
                    .try_into()
                    .map_err(|_| format!("rate `{rate}` out of i64 range"))?;
                octo_determin::Dqa::new(v, 0)
                    .map_err(|e| format!("invalid rate `{rate}`: {e:?}"))?
            },
        });
    }
    let rate_table = ModelRateTable {
        model: model_ref.clone(),
        rates,
    };

    // 3. Parse nonce (16 bytes hex).
    let nonce_bytes = hex::decode(nonce_hex).map_err(|e| format!("invalid --nonce-hex: {e}"))?;
    if nonce_bytes.len() != 16 {
        return Err(format!(
            "nonce must be 16 bytes (32 hex chars), got {}",
            nonce_bytes.len()
        )
        .into());
    }
    let mut nonce = [0u8; 16];
    nonce.copy_from_slice(&nonce_bytes);

    // 4. Default published_at_unix to wall-clock now if not supplied.
    let published_at_unix = if published_at_unix == 0 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    } else {
        published_at_unix
    };

    // 5. Read 32-byte seed.
    let seed_bytes = std::fs::read(seed)?;
    if seed_bytes.len() != 32 {
        return Err(format!("seed must be 32 bytes, got {}", seed_bytes.len()).into());
    }
    let mut seed_arr = [0u8; 32];
    seed_arr.copy_from_slice(&seed_bytes);

    // 6. Build unsigned payload.
    let asker_did = derive_asker_did(&seed_arr);
    let payload = AskUnsignedPayload::new(
        asker_did,
        nt,
        model_ref.clone(),
        rate_table,
        ttl_unix,
        jurisdiction.to_vec(),
        published_at_unix,
        nonce,
    )
    .map_err(|e| match e {
        AskError::EmptyAskerDid => "asker DID is empty",
        AskError::EmptyModel => "model is empty",
        AskError::EmptyJurisdiction => "jurisdiction list is empty (at least one required)",
        AskError::EmptyNonce => "nonce is all-zero (use a real CSPRNG-generated nonce)",
        AskError::EmptyIdentitySeed => "identity seed is all-zero",
    })?;

    // 7. Sign.
    let signed = AskSigned::sign(payload, &seed_arr).map_err(|e| match e {
        AskSignedError::EmptyIdentitySeed => "identity seed is all-zero".to_owned(),
        AskSignedError::CanonicalSer(s) => format!("canonical_ser(payload) failed: {s}"),
        _ => "AskSigned::sign failed".to_owned(),
    })?;

    // 8. Emit JSON envelope.
    let out = serde_json::to_string(&signed).map_err(|e| format!("serialize AskSigned: {e}"))?;
    println!("{out}");
    Ok(())
}

/// Derive asker DID from the 32-byte Ed25519 seed (RFC-0009 §Identity Key Format).
///
/// Canonical form: `did:octo:b<hex-encoded 32-byte public-key>`. The proper
/// W3C `z<base58btc>` form requires the multibase + multihash codec wrappers
/// (RFC-0010 `unprefixed-bytes`); the hex form is the dev-friendly fallback
/// accepted by `quota-router-core::marketplace::reputation_compat::parse_canonical_did`.
fn derive_asker_did(seed_arr: &[u8; 32]) -> String {
    let id = IdentityKey::from_seed(*seed_arr);
    let pk_bytes = id.public_key_bytes();
    let pk_hex = hex::encode(pk_bytes);
    format!("did:octo:b{pk_hex}")
}
