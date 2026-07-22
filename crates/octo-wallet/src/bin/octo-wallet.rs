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
            std::fs::write(&seed_out, id.seed_bytes())?;
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
    }
}
