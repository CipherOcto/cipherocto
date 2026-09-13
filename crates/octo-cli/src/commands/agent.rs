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
use crate::redact::{RedactedIdentifier, RedactionContext};
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
        /// Optional lifecycle-state filter
        /// (`registered` / `running` / `terminated`).
        #[arg(long, value_name = "STATE")]
        state: Option<String>,
        /// Maximum rows returned (clamped at 1024 per RFC-0015 §6.2.1
        /// substrate hard ceiling). Default = 1024. `--limit 0` is
        /// rejected up front as `InvalidLimit` (slot 45).
        #[arg(long, value_name = "N", default_value_t = 1024u32)]
        limit: u32,
        /// Opaque pagination cursor (Phase 2 forward-compat; rejected
        /// as `InvalidCursor` on malformed input, slot 46).
        #[arg(long, value_name = "CURSOR")]
        cursor: Option<String>,
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
/// arm — the compiler-enforced amendment contract. The `create` arm
/// destructures the struct variant at the call site so the typed
/// handler signature enforces the contract at compile time (no
/// `unreachable!` panics, no runtime re-dispatch).
pub fn dispatch(action: &AgentAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        AgentAction::Create {
            manifest_path,
            capability_root,
        } => create::handle(manifest_path, capability_root.as_deref(), cli),
        AgentAction::List {
            state,
            limit,
            cursor,
        } => list::handle(state.as_deref(), *limit, cursor.as_deref(), cli),
        AgentAction::Run { .. } | AgentAction::Destroy { .. } | AgentAction::Attach { .. } => {
            pending_subcommand(action)
        }
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
    /// The handler takes the destructured fields directly (the typed
    /// signature from `dispatch` makes the contract compiler-enforced)
    /// rather than re-matching on the enum, eliminating the
    /// `unreachable!` runtime guard.
    ///
    /// Exit codes:
    /// - 0: success (agent registered)
    /// - 2: Auditor mode refused the write (RFC-0011-c §Roles and Authorities)
    /// - 5: HSM unavailable (substrate `WalletError::Hsm` propagation)
    /// - 39: manifest file could not be read, or manifest JSON parse failed
    /// - 40: capability validation pipeline failed (Phase 2; not raised in Phase 1)
    /// - 41: an agent with the derived `agent_id` already exists
    /// - 64: unexpected substrate error
    pub fn handle(
        manifest_path: &std::path::Path,
        capability_root_hex: Option<&str>,
        cli: &Octo,
    ) -> Result<(), OctoCliError> {
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
        let capability_root = match capability_root_hex {
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
            agent_id: RedactedIdentifier::new(agent_id.to_string()),
            holder_did: RedactedIdentifier::new(active_did.as_str()),
            state: "registered".to_string(),
            label: manifest.label.clone(),
            manifest_digest,
            registered_at_unix: now_unix_secs(),
            registered_at: DateTime::<Utc>::from_timestamp(now_unix_secs() as i64, 0)
                .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("epoch")),
        };
        // Envelope-boundary redaction context. For `agent create`
        // `holder_did == active_did` by construction; sibling
        // subcommands reuse the same context shape.
        let redactor = RedactionContext::new()
            .with_active_did(active_did.as_str())
            .with_holder_did(active_did.as_str())
            .with_agent_id(agent_id.to_string());
        render_envelope("octo.agent.create.v1", output, cli, &redactor)
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

// ---------------------------------------------------------------------------
// `octo agent list` handler
// ---------------------------------------------------------------------------

mod list {
    //! `octo agent list [--state <state>] [--limit <N>] [--cursor <c>]`
    //!
    //! Read-only subcommand (RFC-0011-c §9.3.3 + RFC-0015 §6.2.1).
    //! Auditor-mode compatible (no write side-effects). The substrate
    //! `list_owned_agents` enforces caller-attestation; the CLI never
    //! surfaces a `--holder-did` flag because multi-DID enumeration is
    //! denied at the substrate (SECURITY HIGH per RFC-0015 §6.2.1,
    //! exit 17 `ForbiddenHolderMismatch` on attempt).

    use super::*;
    use octo_wallet::agent::list_owned_agents as wallet_list_owned_agents;
    use octo_wallet::{AgentFilter, AgentState, AgentSummary};

    /// Substrate hard ceiling — RFC-0015 §6.2.1.
    const SUBSTRATE_HARD_LIMIT: u32 = 1024;

    /// Operator `--state` argument → substrate `AgentState`.
    ///
    /// Lowercase stable labels per `AgentState::as_str`
    /// (`registered` / `running` / `terminated`). Any other input
    /// surfaces as `OctoCliError::InvalidFilter` (slot 16, mirrored
    /// with `InvalidLimit` exit-code rationale).
    pub(crate) fn parse_state_filter(s: &str) -> Result<AgentState, OctoCliError> {
        match s {
            "registered" => Ok(AgentState::Registered),
            "running" => Ok(AgentState::Running),
            "terminated" => Ok(AgentState::Terminated),
            other => Err(OctoCliError::InvalidFilter(format!(
                "unknown agent state `{other}` (allow: registered | running | terminated)"
            ))),
        }
    }

    /// Validate `--cursor` shape (Phase 1 cursors are reserved
    /// forward-compat tokens, RFC-0015 §6.2.1). Phase 1 rejects all
    /// non-empty cursors as `InvalidCursor` (slot 46) so the operator
    /// gets an explicit error instead of substrate silently ignoring
    /// the unknown pagination state.
    pub(crate) fn validate_cursor(cursor: Option<&str>) -> Result<(), OctoCliError> {
        if let Some(c) = cursor {
            if c.is_empty() {
                return Err(OctoCliError::InvalidCursor(
                    "cursor must not be empty when supplied".to_string(),
                ));
            }
            // Phase 1 cursors are opaque; we reserve the right to add
            // a hex-only check in Phase 2. For now any non-empty
            // string is accepted as a forward-compat placeholder.
            // (The exit-46 surface is reserved by the amendment chain
            // for malformed inputs that the future substrate parser
            // will surface.)
            let _ = c;
        }
        Ok(())
    }

    /// Handle `octo agent list [--state <state>] [--limit <N>] [--cursor <c>]`.
    ///
    /// Exit codes:
    /// - 0: success (empty Vec for empty registry)
    /// - 2: no active identity
    /// - 5: HSM unavailable (substrate `WalletError::Hsm`)
    /// - 16: invalid `--state` argument
    /// - 17: forbidden holder DID mismatch (substrate-fail-closed)
    /// - 45: `--limit` out of range (1..=1024)
    /// - 46: `--cursor` malformed
    /// - 64: unexpected substrate error
    pub fn handle(
        state_filter: Option<&str>,
        limit: u32,
        cursor: Option<&str>,
        cli: &Octo,
    ) -> Result<(), OctoCliError> {
        // 1. Validate operator args up front (no substrate round-trip
        //    for trivially-bad input).
        if limit == 0 || limit > SUBSTRATE_HARD_LIMIT {
            return Err(OctoCliError::InvalidLimit(limit.to_string()));
        }
        validate_cursor(cursor)?;

        let parsed_state = state_filter.map(parse_state_filter).transpose()?;

        // 2. Open wallet + resolve active DID (read-only — no Auditor
        //    gating, this is a read subcommand per RFC-0011-c
        //    §Roles and Authorities).
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

        // 3. Build substrate filter. `holder_did` stays None — the
        //    substrate enforces caller_did == effective_holder.
        let filter = AgentFilter {
            holder_did: None,
            state: parsed_state,
            limit: Some(limit as usize),
            cursor: cursor.map(|s| s.to_string()),
        };

        // 4. Substrate call.
        let summaries = wallet_list_owned_agents(&active_did, &filter).map_err(|e| match e {
            octo_wallet::WalletError::ForbiddenHolderMismatch => {
                OctoCliError::ForbiddenHolderMismatch
            }
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })?;

        // 5. Build output envelope. Each summary's `holder_did` will
        //    match `active_did` by construction (substrate enforced),
        //    so the redactor un-redacts every holder_did row.
        let redactor = RedactionContext::new().with_active_did(active_did.as_str());
        let output = AgentListOutput {
            count: summaries.len(),
            holder_did: RedactedIdentifier::new(active_did.as_str()),
            agents: summaries
                .into_iter()
                .map(|s: AgentSummary| AgentSummaryEnvelope::from_substrate(s))
                .collect(),
        };
        render_envelope("octo.agent.list.v1", output, cli, &redactor)
    }

    /// CLI-side envelope wrapper around substrate `AgentSummary`.
    ///
    /// The `holder_did` is wrapped in [`RedactedIdentifier`] so
    /// `Serialize` always emits `[REDACTED:key]`; the envelope
    /// renderer conditionally un-redacts when `holder_did ==
    /// active_did` (which is substrate-enforced for `list`).
    #[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
    pub struct AgentSummaryEnvelope {
        /// Deterministic `agent_id` (RFC-0011-c §9.10). Wrapped in
        /// [`RedactedIdentifier`] for envelope-boundary symmetry
        /// with `agent create` (mission §Scope sub-step 3).
        #[schemars(with = "String")]
        pub agent_id: RedactedIdentifier,
        /// Subject DID — wrapped in [`RedactedIdentifier`]. For
        /// `list` the substrate guarantees `holder_did == caller_did`
        /// so the redactor un-redacts every row.
        #[schemars(with = "String")]
        pub holder_did: RedactedIdentifier,
        /// Lifecycle state label (`registered` / `running` /
        /// `terminated`).
        pub state: String,
        /// Optional operator-supplied label.
        pub label: Option<String>,
        /// Unix seconds at which the substrate recorded the
        /// registration.
        pub registered_at_unix: u64,
        /// BLAKE3-256 digest of the canonical manifest.
        pub manifest_digest: String,
    }

    impl AgentSummaryEnvelope {
        fn from_substrate(s: AgentSummary) -> Self {
            Self {
                agent_id: RedactedIdentifier::new(s.agent_id.to_string()),
                holder_did: RedactedIdentifier::new(s.holder_did),
                state: s.state.as_str().to_string(),
                label: s.label,
                registered_at_unix: s.registered_at_unix,
                manifest_digest: s.manifest_digest,
            }
        }
    }

    /// `octo agent list` payload — RFC-0011-c §9.3.3 Output Envelope.
    #[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
    pub struct AgentListOutput {
        /// Number of rows in `agents` (after server-side filter and
        /// clamp).
        pub count: usize,
        /// The holder DID the substrate filtered on (= active DID by
        /// caller-attestation; wrapped in [`RedactedIdentifier`] for
        /// envelope-boundary symmetry).
        #[schemars(with = "String")]
        pub holder_did: RedactedIdentifier,
        /// Per-agent rows (deterministic order: registered_at_unix DESC
        /// + agent_id ASC per RFC-0008 Class B).
        pub agents: Vec<AgentSummaryEnvelope>,
    }
}

/// `octo agent create` payload — RFC-0011-c §9.3.1 Output Envelope.
///
/// Built at the dispatch boundary by composing the substrate
/// `register_agent` return value with the manifest's `digest_hex()`.
/// The substrate owns the `agent_id` derivation
/// (UUIDv5 over `(manifest_digest, active_did)`); the CLI surfaces
/// it verbatim.
///
/// Phase 2 envelope-boundary redaction (mission
/// `0011-c-agent-redaction-envelope`): both `holder_did` and
/// `agent_id` are wrapped in [`RedactedIdentifier`] so `Serialize`
/// always emits `[REDACTED:key]`. The envelope renderer then
/// conditionally un-redacts `holder_did` via [`RedactionContext`]
/// when the holder IS the active operator (mission §Scope sub-step 2)
/// and truncates `agent_id` to first 8 chars + `...` for correlation
/// with substrate logs (mission §Scope sub-step 3).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct AgentCreateOutput {
    /// Deterministic `agent_id` (RFC-0011-c §9.10 substrate signature).
    /// Wrapped in [`RedactedIdentifier`] so `Serialize` always emits
    /// `[REDACTED:key]` and the envelope renderer can apply the
    /// mission §Scope sub-step 3 truncation (`first-8-chars + "..."`).
    /// Schemars annotation emits a plain string for the JSON Schema
    /// (the runtime marker is the fixed-string form).
    #[schemars(with = "String")]
    pub agent_id: RedactedIdentifier,
    /// Subject DID (RFC-0010 form) — the active identity at
    /// registration time. Wrapped in [`RedactedIdentifier`] so
    /// `Serialize` always emits `[REDACTED:key]`; the envelope
    /// renderer conditionally un-redacts via [`RedactionContext`]
    /// when `holder_did == active_did`. Schemars annotation emits
    /// a plain string for the JSON Schema (the runtime marker is
    /// always `[REDACTED:key]` so the schema contract is the
    /// fixed-string form).
    #[schemars(with = "String")]
    pub holder_did: RedactedIdentifier,
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
/// Mirrors the `commands::reputation::render_envelope` helper. The
/// `redactor` is the envelope-boundary redaction context (mission
/// `0011-c-agent-redaction-envelope` §Scope sub-step 3); callers
/// that do not need contextual redaction pass `&RedactionContext::new()`.
fn render_envelope<T: serde::Serialize>(
    schema: &'static str,
    data: T,
    cli: &Octo,
    redactor: &RedactionContext,
) -> Result<(), OctoCliError> {
    let env = OutputEnvelope::new(schema, data);
    env.render_with_redaction(cli.output.json, cli.output.no_color, redactor)
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

    /// CLI-arg round-trip: a hex string the operator passes via
    /// `--capability-root` must survive parse → re-encode with byte
    /// equivalence. The substrate (`CapabilityId::from_hex` →
    /// `hex::decode_to_slice`) is case-insensitive but emits lowercase
    /// canonical form; this test pins the canonical-lowercase 64-char
    /// hex round-trip on the CLI-arg → substrate path so a future
    /// amendment that flips canonicalization would surface as a
    /// broken invariant (operators canonicalize before storing).
    #[test]
    fn capability_root_round_trips_byte_for_byte() {
        let original = "ab".repeat(32);
        let cid = CapabilityId::from_hex(&original).expect("64-char hex must parse");
        let reencoded = hex::encode(cid.0);
        assert_eq!(reencoded, original, "round-trip must be byte-for-byte");
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
        // Use `Run` which is still wired to `pending_subcommand`
        // (write-path mission `0011-c-agent-run-subcommand` deferred
        // to Phase 2 per the substrate-first restructure).
        let action = AgentAction::Run {
            agent_id: "00000000-0000-0000-0000-000000000000".to_string(),
        };
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

    /// Pin the schemars contract for `AgentCreateOutput`: both
    /// `agent_id` and `holder_did` are wrapped in `RedactedIdentifier`
    /// and carry the `#[schemars(with = "String")]` annotation. The
    /// JSON Schema must declare BOTH fields as `string` (not as the
    /// underlying `uuid::Uuid` / `String` types — `RedactedIdentifier`
    /// does not implement `JsonSchema`, so the annotation is the only
    /// path that keeps the schema surface stable). Downstream consumers
    /// (e.g., `octo-cli agent list --json | jq`) build their paths
    /// off this contract; a schemars regression would silently shift
    /// the schema and break tooling.
    ///
    /// Mission `0011-c-agent-redaction-envelope` §Risk requires this
    /// mitigation per-field; both fields pin the contract here.
    #[test]
    fn agent_create_output_schema_declares_agent_id_as_string() {
        use schemars::schema_for;
        let schema = schema_for!(AgentCreateOutput);
        let json = serde_json::to_value(&schema).expect("schema is JSON");
        let agent_id = json
            .pointer("/properties/agent_id/type")
            .and_then(|v| v.as_str())
            .expect("agent_id schema must declare a type");
        assert_eq!(
            agent_id, "string",
            "agent_id must round-trip as JSON Schema `string`, got {agent_id:?}",
        );
        let holder_did = json
            .pointer("/properties/holder_did/type")
            .and_then(|v| v.as_str())
            .expect("holder_did schema must declare a type");
        assert_eq!(
            holder_did, "string",
            "holder_did must round-trip as JSON Schema `string`, got {holder_did:?}",
        );
    }

    // ----- `octo agent list` tests (RFC-0011-c §9.3.3 + RFC-0015 §6.2.1) -----

    /// `parse_state_filter` accepts the three stable lowercase labels
    /// (registered / running / terminated per `AgentState::as_str`)
    /// and rejects anything else with `OctoCliError::InvalidFilter`
    /// (exit 16, the existing `OctoCliError::InvalidFilter` slot).
    #[test]
    fn list_parse_state_filter_accepts_known_labels() {
        use octo_wallet::AgentState;
        assert!(matches!(
            list::parse_state_filter("registered").unwrap(),
            AgentState::Registered
        ));
        assert!(matches!(
            list::parse_state_filter("running").unwrap(),
            AgentState::Running
        ));
        assert!(matches!(
            list::parse_state_filter("terminated").unwrap(),
            AgentState::Terminated
        ));
    }

    #[test]
    fn list_parse_state_filter_rejects_unknown_label() {
        let err = list::parse_state_filter("zombie").unwrap_err();
        match err {
            OctoCliError::InvalidFilter(msg) => {
                assert!(
                    msg.contains("zombie"),
                    "InvalidFilter payload must echo the offending label, got {msg}"
                );
            }
            other => panic!("expected InvalidFilter, got {other:?}"),
        }
    }

    /// `validate_cursor` accepts None (no flag) and any non-empty
    /// Phase 1 placeholder string. Empty strings are rejected as
    /// `InvalidCursor` (exit 46). Future Phase 2 cursors will add a
    /// hex-shape check.
    #[test]
    fn list_validate_cursor_accepts_none_and_placeholder() {
        assert!(list::validate_cursor(None).is_ok());
        assert!(list::validate_cursor(Some("opaque-token-v1")).is_ok());
    }

    #[test]
    fn list_validate_cursor_rejects_empty_string() {
        let err = list::validate_cursor(Some("")).unwrap_err();
        match err {
            OctoCliError::InvalidCursor(msg) => {
                assert!(
                    msg.contains("empty"),
                    "InvalidCursor payload must mention empty, got {msg}"
                );
            }
            other => panic!("expected InvalidCursor, got {other:?}"),
        }
    }

    /// The substrate hard ceiling is 1024 per RFC-0015 §6.2.1.
    /// Validate the CLI's up-front `--limit` clamp so the operator
    /// never reaches the substrate with `0` (which the substrate
    /// would silently clamp to 1024 — hiding the operator typo).
    #[test]
    fn list_invalid_limit_exits_45() {
        // Pin the exit code mapping for `InvalidLimit` so a future
        // amendment that shifts the slot surfaces as a broken test.
        let e = OctoCliError::InvalidLimit("0".to_string());
        assert_eq!(e.exit_code(), 45, "InvalidLimit must map to exit 45");
        let e = OctoCliError::InvalidLimit("2048".to_string());
        assert_eq!(e.exit_code(), 45, "InvalidLimit over ceiling maps to 45");
    }

    #[test]
    fn list_invalid_cursor_exits_46() {
        let e = OctoCliError::InvalidCursor("malformed".to_string());
        assert_eq!(e.exit_code(), 46, "InvalidCursor must map to exit 46");
    }

    /// SECURITY HIGH pin: `ForbiddenHolderMismatch` MUST map to exit
    /// 17 per RFC-0011 §Exit Codes 17-63 reserved range. The slot
    /// was deliberately moved from 37 to 17 during R40 (slot 37 is
    /// owned by RFC-0011-g `UnknownAttestationKind`).
    #[test]
    fn list_forbidden_holder_mismatch_exits_17() {
        let e = OctoCliError::ForbiddenHolderMismatch;
        assert_eq!(
            e.exit_code(),
            17,
            "ForbiddenHolderMismatch MUST exit 17 (RFC-0011 reserved range), got {}",
            e.exit_code()
        );
    }

    #[test]
    fn list_agent_not_found_exits_42() {
        let e = OctoCliError::AgentNotFound(uuid::Uuid::nil());
        assert_eq!(
            e.exit_code(),
            42,
            "AgentNotFound MUST exit 42 (RFC-0011-c agent amendment chain slot 42), got {}",
            e.exit_code()
        );
    }

    /// Pin the schemars contract for `AgentListOutput`: `agent_id` and
    /// `holder_did` (on both the outer envelope AND each
    /// `AgentSummaryEnvelope` row) must round-trip as JSON Schema
    /// `string`. Symmetric with `agent_create_output_schema_declares_*`
    /// — keeps the downstream `jq` paths stable.
    #[test]
    fn agent_list_output_schema_declares_string_fields() {
        use schemars::schema_for;
        let schema = schema_for!(list::AgentListOutput);
        let json = serde_json::to_value(&schema).expect("schema is JSON");
        let holder_did = json
            .pointer("/properties/holder_did/type")
            .and_then(|v| v.as_str())
            .expect("holder_did schema must declare a type");
        assert_eq!(
            holder_did, "string",
            "AgentListOutput.holder_did must round-trip as JSON Schema `string`, got {holder_did:?}",
        );
        let count = json
            .pointer("/properties/count/type")
            .and_then(|v| v.as_str())
            .expect("count schema must declare a type");
        assert_eq!(
            count, "integer",
            "AgentListOutput.count must round-trip as JSON Schema `integer`, got {count:?}",
        );
    }
}
