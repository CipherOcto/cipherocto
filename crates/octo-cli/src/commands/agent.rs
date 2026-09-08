//! `octo agent` — RFC-0011-c §Subcommand Taxonomy.
//!
//! Thin Layer C wrapper over the `octo-wallet::agent` substrate crate
//! (Layer B `[ADD]` per RFC-0011-c §9.10). Operator invocation → clap
//! parse → manifest file read → substrate `register_agent` call →
//! JSON envelope render.
//!
//! ## Phase 1 surface
//!
//! Per mission `0011-c-agent-create-subcommand`, only `octo agent create`
//! is implemented. Sibling subcommands (`run`, `list`, `destroy`,
//! `attach`) are wired as clap variants that emit an explicit
//! "subcommand pending follow-on mission" error so the operator gets a
//! clear UX hint instead of a clap-level "unknown command" rejection.
//! This keeps the CLI surface ahead of substrate per RFC-0011
//! §Compatibility (new subcommands land additive — no central-enum
//! edits when the sibling missions ship).
//!
//! ## Mode gating (RFC-0011-c §Roles and Authorities)
//!
//! `octo agent create` is **write** — denied in Auditor mode per
//! RFC-0011 §Compatibility + RFC-0011-c §Roles and Authorities table
//! (Auditor is read-only). Human / Ci / Dev modes invoke the substrate
//! directly.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::Serialize;

use crate::error::{sanitize_substrate_error, OctoCliError};
use crate::flags::OperatorMode;
use crate::output::OutputEnvelope;
use crate::Octo;

// ---------------------------------------------------------------------------
// Clap surface — `octo agent <action>` (RFC-0011-c §9.3 Subcommand Taxonomy)
// ---------------------------------------------------------------------------

/// Agent lifecycle subcommands.
///
/// `#[non_exhaustive]` per [[cipherocto-design-principles]]
/// §Extension over enumeration: future states / subcommands (e.g.,
/// `pause`, `drain`) land additively without central-enum edits.
#[derive(Subcommand, Debug, Clone)]
#[non_exhaustive]
pub enum AgentAction {
    /// Register a new agent from a manifest file (RFC-0011-c §9.3.1).
    ///
    /// Phase 1 lands in `0011-c-agent-create-subcommand`; sibling
    /// subcommands land in follow-on missions.
    Create {
        /// Path to the agent manifest JSON file
        /// (RFC-0002 §Agent Manifest wire form).
        #[arg(long, value_name = "PATH")]
        manifest_path: PathBuf,
        /// Hex-encoded 64-char capability-root identifier. Phase 1
        /// records the value but does not verify it against the
        /// macaroon substrate — the 6-step capability validation
        /// pipeline (RFC-0002 §Capability Validation) wires in
        /// follow-on substrate amendments.
        #[arg(long, value_name = "HEX64")]
        capability_root: Option<String>,
    },
    /// Run a registered agent (RFC-0011-c §9.3.2). Wired by
    /// `0011-c-agent-run-subcommand` (follow-on).
    Run {
        /// Target agent id (UUID form, hex).
        #[arg(long, value_name = "UUID")]
        agent_id: String,
    },
    /// List registered agents owned by the active DID
    /// (RFC-0011-c §9.3.3). Wired by `0011-c-agent-list-subcommand`.
    List {
        /// Optional holder DID filter (defaults to the active DID).
        #[arg(long)]
        holder_did: Option<String>,
    },
    /// Terminate a registered agent (RFC-0011-c §9.3.4). Wired by
    /// `0011-c-agent-destroy-subcommand`.
    Destroy {
        /// Target agent id (UUID form, hex).
        #[arg(long, value_name = "UUID")]
        agent_id: String,
    },
    /// Attach to a running agent's control plane
    /// (RFC-0011-c §9.3.5). Wired by `0011-c-agent-attach-subcommand`.
    Attach {
        /// Target agent id (UUID form, hex).
        #[arg(long, value_name = "UUID")]
        agent_id: String,
    },
}

/// Dispatch a parsed `octo agent <action>` invocation to its handler.
///
/// `#[non_exhaustive]` on `AgentAction` ensures this `match` will
/// fail to compile when a new variant is added without a corresponding
/// arm — the compiler-enforced amendment contract.
pub fn dispatch(action: &AgentAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        AgentAction::Create { .. } => create::handle(action, cli),
        AgentAction::Run { .. }
        | AgentAction::List { .. }
        | AgentAction::Destroy { .. }
        | AgentAction::Attach { .. } => pending_subcommand(action),
    }
}

/// Map a not-yet-implemented subcommand to a clear "pending follow-on
/// mission" error so the operator does not see a clap-level "unknown
/// command" rejection. Exit 39 is shared with `ManifestParseError`
/// per RFC-0011-c §9.8 (slot 39-52 reserved for the agent amendment
/// chain; `pending` claims slot 39 to keep the range contiguous and
/// distinguishable from `ManifestParseError`'s `path`/`reason`
/// payload shape).
fn pending_subcommand(_action: &AgentAction) -> Result<(), OctoCliError> {
    Err(OctoCliError::Internal(
        "agent subcommand pending follow-on mission (0011-c-agent-{run,list,destroy,attach}-subcommand); only `octo agent create` is wired in this mission".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// `octo agent create` handler
// ---------------------------------------------------------------------------

mod create {
    use super::*;
    use octo_wallet::register_agent as wallet_register_agent;
    use octo_wallet::{AgentManifest, CapabilityId};

    /// Handle `octo agent create --manifest-path <path>
    /// [--capability-root <hex64>]`.
    ///
    /// Exit codes:
    /// - 0: success (agent registered)
    /// - 2: Auditor mode refused the write (RFC-0011-c §Roles and Authorities)
    /// - 5: HSM unavailable (substrate `WalletError::Hsm` propagation)
    /// - 39: manifest file could not be read, or manifest JSON parse failed
    /// - 40: capability validation pipeline failed (Phase 2; not raised in Phase 1)
    /// - 41: an agent with the derived `agent_id` already exists
    /// - 64: unexpected substrate error
    pub fn handle(action: &AgentAction, cli: &Octo) -> Result<(), OctoCliError> {
        let (manifest_path, capability_root_hex) = match action {
            AgentAction::Create {
                manifest_path,
                capability_root,
            } => (manifest_path, capability_root),
            _ => unreachable!("create::handle only dispatches on Create"),
        };

        // Auditor is read-only (RFC-0011 §Compatibility + RFC-0011-c
        // §Roles and Authorities).
        if matches!(cli.mode.mode, OperatorMode::Auditor) {
            return Err(OctoCliError::AuditorDenied {
                command: "agent create".to_string(),
            });
        }

        // Read manifest file → parse to substrate struct.
        let body =
            fs::read_to_string(manifest_path).map_err(|e| OctoCliError::ManifestParseError {
                path: manifest_path.display().to_string(),
                reason: format!("read failed: {e}"),
            })?;
        let manifest = AgentManifest::from_json(&manifest_path.display().to_string(), &body)
            .map_err(|e| match e {
                octo_wallet::WalletError::ManifestParse { path, reason } => {
                    OctoCliError::ManifestParseError { path, reason }
                }
                other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
            })?;

        // Capability root — Phase 1 records but does not verify.
        let capability_root = match capability_root_hex.as_deref() {
            Some(hex) => Some(parse_capability_root(hex)?),
            None => None,
        };

        // Open wallet + resolve active identity.
        let store = octo_wallet::WalletStore::open().map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("wallet store open: {e}")))
        })?;
        let active_key = octo_wallet::active_identity(&store).map_err(|e| match e {
            octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
            octo_wallet::WalletError::Hsm(_) => {
                OctoCliError::HsmUnavailable(sanitize_substrate_error(&e.to_string()))
            }
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })?;
        let active_did = active_key.did();

        // Layer B substrate call.
        let cap_root = capability_root.unwrap_or(CapabilityId([0u8; 32]));
        let agent_id =
            wallet_register_agent(&manifest, &cap_root, &active_did).map_err(|e| match e {
                octo_wallet::WalletError::ManifestParse { path, reason } => {
                    OctoCliError::ManifestParseError { path, reason }
                }
                octo_wallet::WalletError::CapabilityValidationFailed(step) => {
                    OctoCliError::CapabilityValidationFailed(step)
                }
                octo_wallet::WalletError::AgentAlreadyExists(uuid) => {
                    OctoCliError::AgentAlreadyExists(uuid)
                }
                other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
            })?;

        // Build output envelope.
        let manifest_digest = manifest.digest_hex();
        let output = AgentCreateOutput {
            agent_id,
            holder_did: active_did.as_str().to_string(),
            state: "registered".to_string(),
            label: manifest.label.clone(),
            manifest_digest,
            registered_at_unix: now_unix_secs(),
            registered_at: DateTime::<Utc>::from_timestamp(now_unix_secs() as i64, 0)
                .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("epoch")),
        };
        render_envelope("octo.agent.create.v1", output, cli)
    }

    /// Best-effort wall-clock — same Phase-1 caveat as the substrate
    /// `register_agent` helper (RFC-0008 Class B: Phase 2 routes through
    /// the monotonic substrate clock for cross-replica determinism).
    fn now_unix_secs() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Parse the operator-supplied `--capability-root <hex64>` argument
    /// into the substrate `CapabilityId`. A parse failure maps to
    /// `OctoCliError::CapabilityValidationFailed(0)` — the step marker
    /// is preserved (substrate step 0 = "input validation") so log
    /// scrapers can correlate the failure with the capability
    /// validation pipeline (RFC-0002 §Capability Validation).
    fn parse_capability_root(hex: &str) -> Result<CapabilityId, OctoCliError> {
        CapabilityId::from_hex(hex).map_err(|e| match e {
            octo_wallet::WalletError::InvalidSlotId(_) => {
                OctoCliError::CapabilityValidationFailed(0)
            }
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })
    }
}

/// `octo agent create` payload — RFC-0011-c §9.3.1 Output Envelope.
///
/// Built at the dispatch boundary by composing the substrate
/// `register_agent` return value with the manifest's `digest_hex()`.
/// The substrate owns the `agent_id` derivation
/// (UUIDv5 over `(manifest_digest, active_did)`); the CLI surfaces
/// it verbatim.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct AgentCreateOutput {
    /// Deterministic `agent_id` (RFC-0011-c §9.10 substrate signature).
    /// Schemars annotation emits a plain string for the JSON Schema
    /// (matches the runtime UUID canonical form); the `uuid` crate
    /// itself does not implement `JsonSchema` (see `crates/octo-cli`
    /// Cargo.toml — `schemars` appears in two versions transitively,
    /// so the blanket `JsonSchema` derive does not see the
    /// `uuid::Uuid` impl).
    #[schemars(with = "String")]
    pub agent_id: uuid::Uuid,
    /// Subject DID (RFC-0010 form) — the active identity at
    /// registration time.
    pub holder_did: String,
    /// Lifecycle state label — always `"registered"` for the create
    /// command (the substrate transitions to `Running` only via the
    /// `octo agent run` subcommand, wired by a follow-on mission).
    pub state: String,
    /// Operator-supplied label (from the manifest).
    pub label: Option<String>,
    /// BLAKE3-256 digest of the canonical manifest
    /// (RFC-0002 §Agent Manifest §Canonical Hash).
    pub manifest_digest: String,
    /// Unix seconds at which the substrate recorded the registration.
    pub registered_at_unix: u64,
    /// RFC 3339 UTC timestamp of registration (human-readable mirror
    /// of `registered_at_unix`).
    pub registered_at: DateTime<Utc>,
}

/// Render an output envelope for the given payload (serializable).
///
/// Mirrors the `commands::reputation::render_envelope` helper.
fn render_envelope<T: serde::Serialize>(
    schema: &'static str,
    data: T,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let env = OutputEnvelope::new(schema, data);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

// ---------------------------------------------------------------------------
// Tests — manifest parse + capability-root parse + error variants
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::sanitize_substrate_error;
    use octo_wallet::CapabilityId;

    #[test]
    fn manifest_path_display_renders_verbatim() {
        // ManifestParseError surfaces the path the operator supplied —
        // not a redacted form — so they can correct the file
        // reference. The `path` field is not subject to
        // `sanitize_substrate_error` (paths are operator input).
        let e = OctoCliError::ManifestParseError {
            path: "/tmp/agent.json".to_string(),
            reason: "missing field `signature_hex`".to_string(),
        };
        let msg = e.to_string();
        assert!(msg.contains("/tmp/agent.json"), "{msg}");
    }

    #[test]
    fn capability_id_parses_64_char_hex() {
        let hex = "ab".repeat(32);
        let cid = CapabilityId::from_hex(&hex).expect("64-char hex must parse");
        assert_eq!(cid.0, [0xab; 32]);
    }

    #[test]
    fn capability_id_rejects_short_hex() {
        let err = CapabilityId::from_hex("abcd").unwrap_err();
        assert!(matches!(err, octo_wallet::WalletError::InvalidSlotId(_)));
    }

    #[test]
    fn capability_id_rejects_non_hex_chars() {
        let hex = "zz".repeat(32);
        let err = CapabilityId::from_hex(&hex).unwrap_err();
        assert!(matches!(err, octo_wallet::WalletError::InvalidSlotId(_)));
    }

    #[test]
    fn pending_subcommand_returns_internal_error() {
        let action = AgentAction::List { holder_did: None };
        let err = pending_subcommand(&action).unwrap_err();
        // The pending-subcommand path emits `Internal` (exit 64) —
        // a deliberately loud failure mode that surfaces the
        // follow-on mission slug in the message so operators have a
        // hint. Future amendments wire each sibling subcommand to
        // its own substrate surface.
        match err {
            OctoCliError::Internal(msg) => {
                assert!(
                    msg.contains("follow-on mission"),
                    "pending subcommand message must mention follow-on mission: {msg}"
                );
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn sanitize_substrate_error_redacts_paths() {
        let s = "register_agent failed at crates/octo-wallet/src/agent.rs:42";
        let clean = sanitize_substrate_error(s);
        assert!(!clean.contains("crates/octo-"), "{clean}");
    }
}
