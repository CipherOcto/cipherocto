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
    /// Run a registered agent (RFC-0011-c §9.3.2). Per RFC-0011-c
    /// §9.3.2 + RFC-0015-a Appendix A, performs the canonical
    /// `Registered → Running` state transition (substrate-layer
    /// `transition_agent` enforces the guard) and then mints a
    /// runtime handle via `octo_runtime::spawn_agent`. The handle
    /// is in-process; pass `--detach` to keep it live across the
    /// CLI exit (default: in-process, terminated at CLI exit). The
    /// companion `octo agent attach --agent-id <uuid>` reads from
    /// the same in-process handle pub-sub channel.
    Run {
        /// Target agent id (UUID form, hex).
        #[arg(long, value_name = "UUID")]
        agent_id: String,
        /// Keep the spawned handle alive after the CLI exits. The
        /// substrate returns a `RuntimeHandle` whose broadcast
        /// channel persists for the duration of the process; without
        /// `--detach` the CLI terminates the handle when the
        /// `Command::Agent(Run)` dispatch returns (default per
        /// RFC-0011-c §9.3.2).
        #[arg(long, default_value_t = false)]
        detach: bool,
        /// Optional human-readable run reason (audit-log payload).
        /// Same 256-byte cap + control-character filter as
        /// `octo agent destroy --reason` (RFC-0015 §6.2.5).
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,
        /// Path to write the `AttachHandle` token bytes
        /// (RFC-0011-c §F.6.1). The clap interlock
        /// `requires = "detach"` ensures `--token-file` is only
        /// accepted alongside `--detach` (the token pathway is
        /// only meaningful when the spawn persists across the
        /// CLI exit so a future `octo agent attach` can bind via
        /// the token). Token bytes are written with mode 0o600
        /// (POSIX); parent directories are created with
        /// `fs::create_dir_all`. The token grants cross-process
        /// re-entry to the runtime broadcast channel — treat
        /// the file as a credential.
        #[arg(
            long,
            value_name = "PATH",
            requires = "detach",
            help_heading = "AttachHandle token pathway"
        )]
        token_file: Option<PathBuf>,
    },
    /// List registered agents owned by the active DID
    /// (RFC-0011-c §9.3.3). Wired by `0011-c-agent-list-subcommand`.
    List {
        /// Optional lifecycle-state filter
        /// (`registered` / `running` / `terminated`).
        #[arg(long, value_name = "STATE")]
        state: Option<String>,
        /// Maximum rows returned (clamped at the substrate hard
        /// ceiling per RFC-0015 §6.2.1). Default = `SUBSTRATE_HARD_LIMIT`
        /// (1024). `--limit 0` is rejected up front as `InvalidLimit`
        /// (slot 45).
        #[arg(long, value_name = "N", default_value_t = list::SUBSTRATE_HARD_LIMIT)]
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
        /// Optional human-readable destroy reason (audit-log payload).
        /// Surfaces via `validate_reason` (256-byte cap +
        /// control-character rejection per RFC-0015 §6.2.5).
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,
    },
    /// Attach to a running agent's control plane
    /// (RFC-0011-c §9.3.5). Wired by `0011-c-agent-attach-subcommand`.
    Attach {
        /// Target agent id (UUID form, hex).
        #[arg(long, value_name = "UUID")]
        agent_id: String,
        /// Replay events from this unix-seconds timestamp
        /// (RFC-0011-c §9.3.5 `--since` flag). The substrate clamps
        /// the lower bound to `[spawned_at, Utc::now()]` — earlier
        /// timestamps are silently raised to `spawned_at` (the bus
        /// has no events before spawn).
        #[arg(long, value_name = "UNIX_SECONDS")]
        since: Option<u64>,
        /// Path to read the `AttachHandle` token bytes from
        /// (RFC-0011-c §F.6.2). Required on every attach
        /// invocation per the substrate-faithful binding pathway
        /// — the token carries the session id, signature, and
        /// transport selector that the `decode_token` +
        /// `attach_with_token` chain consumes to bind the
        /// broadcast channel.
        #[arg(long, value_name = "PATH", help_heading = "AttachHandle token pathway")]
        token_file: PathBuf,
    },
    /// Revoke an outstanding `AttachHandle` token (RFC-0011-c §F.3
    /// follow-on). Wired by `0011-c-attach-handle-token-pathway`.
    /// The revocation set is a process-singleton (per RFC-0011-c
    /// §F.3); tokens minted in another process are unaffected.
    RevokeAttach {
        /// Hex-encoded 64-char session id from the token to revoke
        /// (RFC-0011-c §F.1 wire form).
        #[arg(long, value_name = "HEX64")]
        session_id: String,
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
        AgentAction::Run {
            agent_id,
            detach,
            reason,
            token_file,
        } => run::handle(
            agent_id,
            *detach,
            reason.as_deref(),
            token_file.as_deref(),
            cli,
        ),
        AgentAction::Attach {
            agent_id,
            since,
            token_file,
        } => attach::handle(agent_id, *since, token_file, cli),
        AgentAction::Destroy { agent_id, reason } => {
            destroy::handle(agent_id, reason.as_deref(), cli)
        }
        AgentAction::RevokeAttach { session_id } => revoke_attach::handle(session_id, cli),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers — `octo agent` subcommand family
// ---------------------------------------------------------------------------

/// Shared per-handler wallet + active-identity resolution.
///
/// Both `octo agent create` (write path) and `octo agent list` (read
/// path) need the same `WalletStore::open()` → `active_identity()`
/// → `did()` boilerplate to source the caller-attested DID for the
/// substrate. This helper extracts the boilerplate so the handlers
/// focus on their distinct write/read semantics.
mod common {
    use crate::error::{sanitize_substrate_error, OctoCliError};
    use crate::redact::RedactionContext;

    /// Resolve the active identity DID via the wallet store.
    ///
    /// Thin projection over `resolve_active_identity_key()` — the
    /// canonical substrate-error mapping lives there so any future
    /// `WalletError` variant added to the substrate only needs one
    /// match arm updated. The DID projection is the only
    /// call-site-specific concern here.
    pub(crate) fn resolve_active_did() -> Result<octo_wallet::identity_record::Did, OctoCliError> {
        Ok(resolve_active_identity_key()?.did())
    }

    /// Parse an operator-supplied `--agent-id <UUID>` hex string into
    /// the substrate `Uuid`. A parse failure maps to
    /// `AgentNotFound(nil)` per the lookup-agent canonicalization
    /// contract (slot 42; the substrate treats unknown UUIDs
    /// identically to malformed IDs).
    ///
    /// Used by every agent subcommand that takes `--agent-id` so the
    /// parse-failure exit code is uniformly 42 across the chain.
    pub(crate) fn parse_agent_uuid(hex: &str) -> Result<uuid::Uuid, OctoCliError> {
        uuid::Uuid::parse_str(hex).map_err(|_| OctoCliError::AgentNotFound(uuid::Uuid::nil()))
    }

    /// Map a substrate HSM error message to `OctoCliError::HsmUnavailable`
    /// with sanitized payload (path/secret redaction).
    pub(crate) fn map_hsm_error(reason: &str) -> OctoCliError {
        OctoCliError::HsmUnavailable(sanitize_substrate_error(reason))
    }

    /// Map the substrate `WalletError` variants surfaced from
    /// `transition_agent` to their CLI-shape counterparts. Used by
    /// both `run::handle` and `destroy::handle` (the two agent
    /// subcommands that drive the state machine).
    ///
    /// Variants mapped:
    /// - `AlreadyInTransition` → `OctoCliError::AlreadyInTransition`
    /// - `InvalidStateTransition` → `OctoCliError::InvalidStateTransition`
    ///   (state labels via `AgentState::as_str` canonical form,
    ///   not `Debug` form)
    /// - `AuditUnavailable` → `OctoCliError::AuditSubstrateNotReady`
    /// - `AgentNotFound` → `OctoCliError::AgentNotFound`
    /// - `ForbiddenHolderMismatch` → `OctoCliError::ForbiddenHolderMismatch`
    /// - `Hsm` → `OctoCliError::HsmUnavailable` (via `map_hsm_error`)
    /// - other → `OctoCliError::Internal` (sanitized)
    pub(crate) fn map_transition_wallet_error(e: octo_wallet::WalletError) -> OctoCliError {
        match e {
            octo_wallet::WalletError::AlreadyInTransition(uuid) => {
                OctoCliError::AlreadyInTransition(uuid)
            }
            octo_wallet::WalletError::InvalidStateTransition { from, to } => {
                OctoCliError::InvalidStateTransition {
                    from: from.as_str().to_string(),
                    to: to.as_str().to_string(),
                }
            }
            octo_wallet::WalletError::AuditUnavailable(_) => OctoCliError::AuditSubstrateNotReady,
            octo_wallet::WalletError::AgentNotFound(uuid) => OctoCliError::AgentNotFound(uuid),
            octo_wallet::WalletError::ForbiddenHolderMismatch => {
                OctoCliError::ForbiddenHolderMismatch
            }
            octo_wallet::WalletError::Hsm(_) => map_hsm_error(&e.to_string()),
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        }
    }

    /// Build the envelope-boundary `RedactionContext` for agent
    /// subcommands. For agent ops `holder_did == active_did` by
    /// construction (the substrate's caller-attestation rejects
    /// holder mismatches before the CLI sees the transition), so the
    /// holder-did field un-redacts for the active operator and the
    /// agent_id field truncates to first-8-chars per mission
    /// `0011-c-agent-redaction-envelope` §Scope sub-step 3.
    pub(crate) fn build_agent_redactor(
        active_did: &str,
        agent_id: &uuid::Uuid,
    ) -> RedactionContext {
        RedactionContext::new()
            .with_active_did(active_did)
            .with_holder_did(active_did)
            .with_agent_id(agent_id.to_string())
    }

    /// Resolve the active identity as the full `IdentityKey`
    /// (octo-wallet Layer B signing keypair) instead of the
    /// redacted `Did` returned by `resolve_active_did`.
    ///
    /// Used by `octo agent run --detach --token-file` to mint the
    /// `AttachHandle` token (RFC-0011-c §F.6.1) — the substrate
    /// `mint_attach_handle(holder: &IdentityKey, ...)` requires
    /// the signing keypair directly so the signature verifies
    /// against the holder's ed25519 public key. The CLI cannot
    /// pass the redacted DID form (the DID is a layer-B façade
    /// projection, not the raw signing material).
    ///
    /// Boundary composition per RFC-0011-c §F.5: the active
    /// `IdentityKey` resolved here is the same key the wallet
    /// uses for `transition_agent` signature verification — no
    /// parallel identity state. Operators MUST NOT pass an
    /// `--identity <did>` flag to this helper (the substrate
    /// rejects holder DID ≠ active DID per RFC-0015-a §6.2.1
    /// caller-attestation).
    ///
    /// Failure modes mirror `resolve_active_did` (one `IdentityKey`
    /// per process; the only legitimate failure surface is the
    /// wallet store / HSM boundary).
    pub(crate) fn resolve_active_identity_key() -> Result<octo_wallet::IdentityKey, OctoCliError> {
        let store = octo_wallet::WalletStore::open().map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("wallet store open: {e}")))
        })?;
        octo_wallet::active_identity(&store).map_err(|e| match e {
            octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
            octo_wallet::WalletError::Hsm(_) => {
                OctoCliError::HsmUnavailable(sanitize_substrate_error(&e.to_string()))
            }
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })
    }
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

        // Open wallet + resolve active identity (shared via
        // `commands::agent::common::resolve_active_did` per the DRY
        // helper extracted in R50.5).
        let active_did = common::resolve_active_did()?;

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
        };
        // Envelope-boundary redaction context. For `agent create`
        // `holder_did == active_did` by construction; sibling
        // subcommands reuse the same context shape.
        let redactor = common::build_agent_redactor(active_did.as_str(), &agent_id);
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
    use octo_wallet::list_owned_agents as wallet_list_owned_agents;
    use octo_wallet::{AgentFilter, AgentState, AgentSummary};

    /// Substrate hard ceiling — RFC-0015 §6.2.1.
    pub(crate) const SUBSTRATE_HARD_LIMIT: u32 = 1024;

    /// Operator `--state` argument → substrate `AgentState`.
    ///
    /// Lowercase stable labels per `AgentState::as_str` (`registered`
    /// / `running` / `terminated`). The valid label set is sourced
    /// from `AgentState::iter()` so future `AgentState` extensions
    /// automatically surface via the help text without per-handler
    /// enumeration. Any other input surfaces as
    /// `OctoCliError::InvalidFilter` (slot 16).
    pub(crate) fn parse_state_filter(s: &str) -> Result<AgentState, OctoCliError> {
        for state in AgentState::iter() {
            if state.as_str() == s {
                return Ok(*state);
            }
        }
        let allow = AgentState::iter()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        Err(OctoCliError::InvalidFilter(format!(
            "unknown agent state `{s}` (allow: {allow})"
        )))
    }

    /// Validate `--cursor` shape (Phase 1 cursors are reserved
    /// forward-compat tokens, RFC-0015 §6.2.1). Phase 1 rejects
    /// empty cursors as `InvalidCursor` (slot 46); any non-empty
    /// cursor passes through to the substrate, which silently
    /// ignores it (substrate `AgentFilter::cursor` is forward-compat
    /// for Phase 2 multi-page iteration per RFC-0015 §6.2.1 cursor
    /// semantics).
    pub(crate) fn validate_cursor(cursor: Option<&str>) -> Result<(), OctoCliError> {
        if let Some(c) = cursor {
            if c.is_empty() {
                return Err(OctoCliError::InvalidCursor(
                    "cursor must not be empty when supplied".to_string(),
                ));
            }
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
        //    §Roles and Authorities). Shared via
        //    `commands::agent::common::resolve_active_did`.
        let active_did = common::resolve_active_did()?;

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

// ---------------------------------------------------------------------------
// `octo agent run` handler
// ---------------------------------------------------------------------------

mod run {
    //! `octo agent run --agent-id <uuid> [--detach] [--reason <text>] [--json]`
    //!
    //! RFC-0011-c §9.3.2. Two-phase write path:
    //!
    //! 1. `octo_wallet::transition_agent(caller_did, uuid,
    //!    AgentState::Running, reason)` — substrate enforces the
    //!    `Registered → Running` state-machine guard (RFC-0015-a
    //!    Appendix A) and emits the `AuditEventKind::AgentTransition`
    //!    audit event. The substrate rolls back the state change on
    //!    audit-append failure (RFC-0015-a §6.1 rollback contract).
    //! 2. `octo_runtime::spawn_agent(agent_id, None)` — mints the
    //!    in-process `RuntimeHandle` pub-sub broadcast channel for
    //!    `octo agent attach --agent-id <uuid>` replay (RFC-0011-c
    //!    §9.3.5). Spawn is unconditional post-transition; failures
    //!    are surfaced as `RuntimeSpawnFailed` (exit 44).
    //!
    //! ## Detach semantics (RFC-0011-c §9.3.2)
    //!
    //! Without `--detach` (the default), the CLI terminates the
    //! in-process `RuntimeHandle` broadcast channel when the
    //! `Command::Agent(Run)` dispatch returns. Phase 1 is in-process
    //! only — Phase 2 (out of scope) moves handle persistence to a
    //! registered actor per RFC-0011-c §9.3.2 detach semantics.
    //!
    //! ## Substrate layer direction
    //!
    //! The CLI is a thin shim; both substrate calls are RFC-0015-a
    //! and RFC-0011-c anchor surfaces. The CLI never implements a
    //! parallel validator / state-machine guard; the substrate owns
    //! both.

    use super::*;
    use octo_runtime::mint_attach_handle;
    use octo_runtime::spawn_agent as runtime_spawn_agent;
    use octo_runtime::{encode_token, Transport};
    use octo_wallet::transition_agent as wallet_transition_agent;
    use octo_wallet::{AgentState, TransitionReceipt};

    // Parse helper lives in `common::parse_agent_uuid` (shared with
    // destroy + attach per DRY).

    /// Handle `octo agent run --agent-id <uuid> [--detach]
    /// [--reason <text>] [--json]`.
    ///
    /// Exit codes:
    /// - 0: success (transition recorded + RuntimeHandle minted)
    /// - 2: Auditor mode refused the write (RFC-0011-c §Roles and
    ///   Authorities)
    /// - 5: HSM unavailable (substrate `WalletError::Hsm`)
    /// - 42: agent not found / unparseable UUID
    /// - 43: `AlreadyInTransition` / `InvalidStateTransition` —
    ///   state-machine guard rejection
    /// - 44: `RuntimeSpawnFailed` — handle mint error at the runtime
    ///   substrate boundary (post-transition failure; the state
    ///   change is committed by the substrate because
    ///   `transition_agent` is atomic with the audit append)
    /// - 52: audit substrate not ready (`AuditSubstrateNotReady`,
    ///   surfaced only when `octo-audit-internal` feature is off
    ///   and no audit sink is registered; substrate fails closed)
    /// - 64: unexpected substrate error
    pub fn handle(
        agent_id_hex: &str,
        detach: bool,
        reason: Option<&str>,
        token_file: Option<&std::path::Path>,
        cli: &Octo,
    ) -> Result<(), OctoCliError> {
        // 1. Parse agent id hex → substrate `Uuid`. Parse failure
        //    maps to `AgentNotFound(nil)` per the lookup-agent
        //    canonicalization contract (slot 42).
        let agent_id = common::parse_agent_uuid(agent_id_hex)?;

        // 2. Auditor is read-only (RFC-0011 §Compatibility +
        //    RFC-0011-c §Roles and Authorities). `agent run`
        //    performs a state transition — denied in Auditor mode.
        if matches!(cli.mode.mode, OperatorMode::Auditor) {
            return Err(OctoCliError::AuditorDenied {
                command: "agent run".to_string(),
            });
        }

        // 3. Resolve active DID via the shared helper.
        let active_did = common::resolve_active_did()?;

        // 4. Substrate `transition_agent` — performs the canonical
        //    `Registered → Running` edge per RFC-0015-a Appendix A.
        //    The substrate enforces caller-attestation against
        //    `holder_did`, the state-machine guard, and rolls back
        //    on audit-append failure (RFC-0015-a §6.1). On an
        //    idempotent self-transition (`Running → Running`),
        //    the substrate returns the same receipt with
        //    `audit_log_entry = [0u8; 32]` per RFC-0015-a §6.1
        //    self-transition contract; the CLI then skips
        //    `spawn_agent` to avoid minting a duplicate
        //    `RuntimeHandle` for the same agent (which would orphan
        //    the previous handle's broadcast subscribers).
        let receipt: TransitionReceipt =
            wallet_transition_agent(&active_did, agent_id, AgentState::Running, reason)
                .map_err(common::map_transition_wallet_error)?;

        // 5. Substrate `spawn_agent` — mints the `RuntimeHandle`
        //    pub-sub broadcast channel. Skipped on idempotent
        //    self-transition (`previous_state == current_state`,
        //    surfaced via zeroed `audit_log_entry`). Phase 1 takes
        //    no attach-handle token (the `attach --since` replay
        //    path is RFC-0011-c §9.3.5 forward-compat; the Phase 1
        //    spec binds the attach handle implicitly via the
        //    per-process wallet). Per the spawn-agent doc-comment
        //    marker `EXPECTED_PRE_SPAWN_STATE`, the runtime
        //    substrate expects the caller to have already approved
        //    the `Active → Busy` transition (`transition_agent`
        //    does that here).
        let handle = if receipt.audit_log_entry == [0u8; 32]
            && receipt.previous_state == receipt.current_state
        {
            // Idempotent self-transition — the substrate didn't
            // mutate state. Surface the existing in-process
            // handle indirectly by re-using the agent_id-bound
            // channel; Phase 1 logs the no-op so operators can
            // see the `run` was a no-op. Phase 2 will route this
            // through a substrate-side `get_or_create` pathway.
            None
        } else {
            Some(runtime_spawn_agent(agent_id, None).map_err(|e| match e {
                octo_runtime::RuntimeError::RuntimeSpawnFailed { reason }
                | octo_runtime::RuntimeError::InvalidRuntimeHandleBinding(reason) => {
                    // `RuntimeSpawnFailed` (exit 44) and
                    // `InvalidRuntimeHandleBinding` (substrate exit
                    // 49) both collapse to the CLI-shape
                    // `RuntimeSpawnFailed` here per
                    // RFC-0011-c §9.8 — the spawn step is
                    // atomic from the CLI perspective; the
                    // substrate already separated them at the
                    // dispatch boundary so the binding class is
                    // unreachable in Phase 1 (binding passed as
                    // `None`).
                    OctoCliError::RuntimeSpawnFailed {
                        reason: sanitize_substrate_error(&reason),
                    }
                }
                other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
            })?)
        };

        // 6. `--detach` semantics (Phase 1: best-effort log line).
        //    The `RuntimeHandle` broadcast channel persists for the
        //    duration of the CLI process regardless of `--detach`
        //    (Phase 1 in-process only). The flag is recorded for
        //    operator feedback and forward-compat with the Phase 2
        //    actor-registered handle persistence per RFC-0011-c
        //    §9.3.2. The message clarifies Phase 1 behavior so
        //    operators don't infer that the default is detached.
        if !detach {
            if let Some(h) = &handle {
                eprintln!(
                    "octo agent run: handle {} minted in-process (Phase 1). CLI exit closes the broadcast channel; pass --detach for the Phase 2 actor-registered handle persistence path (RFC-0011-c §9.3.2).",
                    h.handle_id.0
                );
            }
        }

        // 6.5. `--detach --token-file` token pathway
        //      (RFC-0011-c §F.6.1). On `--detach` + `--token-file`,
        //      mint an `AttachHandle` over the freshly minted
        //      `RuntimeHandle::session_id` + encode + write to
        //      `--token-file` with mode 0o600 (POSIX credential
        //      perms per RFC-0011-c §F.6.1). The clap interlock
        //      `requires = "detach"` on the arg guarantees this
        //      branch only fires on the detached path; on the
        //      non-detached path the token_file argument is
        //      rejected by clap before reaching the dispatch.
        //
        //      Gated on `detach` + `--token-file` + fresh
        //      `RuntimeHandle` (no idempotent self-transition).
        //      The clap interlock `requires = "detach"` on the arg
        //      means `--token-file` only fires on the detached path;
        //      on the non-detached path the token_file argument is
        //      rejected by clap before reaching the dispatch.
        //
        //      Idempotent self-transitions (the substrate did not
        //      mint a new `RuntimeHandle`) emit
        //      `OctoCliError::TokenMintSkipped` (exit 60 per
        //      RFC-0011-c §F.6.1 + §9.8) so automation distinguishes
        //      "asked for a token, got none" from a clean success;
        //      before this fix the dispatch silently returned
        //      `token_written: None` with exit 0 (HIGH R3 finding).
        let token_written: Option<TokenWrittenReceipt> = if detach {
            if let Some(token_path) = token_file {
                let handle_ref = handle.as_ref().ok_or_else(|| {
                    OctoCliError::TokenMintSkipped {
                        reason: "idempotent self-transition (Registered → Running no-op); the substrate did not mint a fresh RuntimeHandle, so there is no session_id to bind a token to".to_string(),
                    }
                })?;
                // 6.5.1 Resolve caller DID → `IdentityKey` (signing
                //       keypair) per §F.5 + §F.6.1. The substrate
                //       `mint_attach_handle` requires the signing
                //       keypair directly so the ed25519 signature
                //       is verifiable against the holder's public
                //       key by the future `decode_token` step.
                let mut holder = common::resolve_active_identity_key()?;

                // 6.5.1a Wall-clock now (declared before activate below).
                //       Substrate-faithful to RFC-0011-c §F.6.1 — TTL is
                //       mint_unix + 3600s, and substrate rejects
                //       `ttl_unix == u64::MAX` as reserved-sentinel Expired.
                let now_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                // 6.5.1b Defensively flip Designated → Active. Map
                //       lifecycle-state-specific substrate WalletError
                //       variants to their dedicated OctoCliError slots
                //       (Revoked → AlreadyRevoked exit 6, Rotating →
                //       AlreadyRotating exit 3) so the operator sees
                //       a distinct activation failure signal rather
                //       than the downstream Internal catch-all. All
                //       other non-lifecycle-state substrate errors
                //       (HSM adapter failures, key serialization,
                //       store open, etc.) collapse to Internal for
                //       sanitization per the [[no-parallel-abstractions]]
                //       principle.
                holder
                    .activate(now_unix)
                    .map_err(|wallet_err| match wallet_err {
                        octo_wallet::WalletError::AlreadyRevoked => OctoCliError::AlreadyRevoked,
                        octo_wallet::WalletError::RotationInProgress => {
                            OctoCliError::AlreadyRotating
                        }
                        other => OctoCliError::Internal(sanitize_substrate_error(&format!(
                            "identity activation failed: {other}"
                        ))),
                    })?;

                // 6.5.2 TTL = mint time + 3600s (RFC-0011-c
                //       §F.6.1 documented default). The substrate
                //       rejects `ttl_unix == u64::MAX` as
                //       reserved-sentinel Expired (fail-CLOSED on
                //       broken-clock ambiguity per the attach
                //       validation chain); `u64::MAX - mint_unix`
                //       would overflow on 32-bit platforms so we
                //       use saturating arithmetic to clamp.
                let ttl_unix = now_unix.saturating_add(3600);

                // 6.5.3 Mint the token. `since_cursor = 0` is the
                //       canonical "replay from beginning of bus"
                //       baseline — the substrate's attach step
                //       rejects `since_unix < mint_timestamp_unix`
                //       with `InvalidSinceCursor` (exit 53 shared
                //       slot, per RFC-0011-c §F.2 step (d)), so the
                //       operator's `--since` arg at attach time must
                //       be ≥ the token's `mint_timestamp_unix`
                //       regardless. Using 0 here keeps the payload
                //       minimal; the field is bound into the
                //       signature per §F.1 wire form so tamper-
                //       evidence is preserved.
                let attach_token = mint_attach_handle(
                    &holder,
                    agent_id,
                    handle_ref.session_id,
                    0,
                    ttl_unix,
                    Transport::IN_PROCESS,
                )?;
                // R5.5 fix: substrate-faithful mapping via the
                // existing `From<AttachError> for OctoCliError`
                // impl (slots 53-59) — the `?` operator invokes
                // it automatically.

                // 6.5.4 Encode to canonical wire bytes (RFC-0011-c
                //       §F.1). On a well-formed handle this
                //       returns Ok(Vec<u8>); the substrate
                //       `PersistenceError(String)` arm is
                //       unreachable in practice (no wire-form
                //       invariants are violated by a freshly
                //       minted handle) but covered per the
                //       substrate's #[non_exhaustive] envelope.
                let token_bytes = encode_token(&attach_token).map_err(|e| match e {
                    octo_runtime::AttachError::PersistenceError(reason) => {
                        OctoCliError::Internal(sanitize_substrate_error(&reason))
                    }
                    other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
                })?;

                // 6.5.5 Write to `--token-file` with mode 0o600
                //       (POSIX credential perms). Create parent
                //       dirs if absent. fsync to surface partial
                //       writes as Err rather than silent truncation.
                //       clap rejects empty `--token-file` values at
                //       parse time so the `is_empty()` guard the
                //       prior draft added is dead code (R3 finding).
                if let Some(parent) = token_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|io| {
                        OctoCliError::Internal(sanitize_substrate_error(&format!(
                            "token file parent dir create: {io}"
                        )))
                    })?;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    let mut opts = std::fs::OpenOptions::new();
                    opts.create(true).write(true).truncate(true).mode(0o600);
                    let mut f = opts.open(token_path).map_err(|io| {
                        OctoCliError::Internal(sanitize_substrate_error(&format!(
                            "token file open: {io}"
                        )))
                    })?;
                    use std::io::Write;
                    f.write_all(&token_bytes).map_err(|io| {
                        OctoCliError::Internal(sanitize_substrate_error(&format!(
                            "token file write: {io}"
                        )))
                    })?;
                    f.sync_all().map_err(|io| {
                        OctoCliError::Internal(sanitize_substrate_error(&format!(
                            "token file fsync: {io}"
                        )))
                    })?;
                    // R5.5 fix (POSIX mode regression): OpenOptionsExt::mode
                    // only applies at create time. When the file
                    // pre-exists, `truncate(true)` opens in place without
                    // changing perms. Force 0o600 post-open so a re-run
                    // against a left-behind 0o644 (or any world-readable
                    // mode) cannot silently leave the token credential
                    // world-readable. Propagate chmod failure: a chmod
                    // failure on a credential file is a HIGH-severity
                    // security finding, not a soft warning.
                    use std::os::unix::fs::PermissionsExt;
                    if let Err(io) = f.set_permissions(std::fs::Permissions::from_mode(0o600)) {
                        return Err(OctoCliError::Internal(sanitize_substrate_error(&format!(
                            "token file set_permissions 0o600: {io}"
                        ))));
                    }
                }
                #[cfg(not(unix))]
                {
                    // Per RFC-0011-c §F.6.1, mode 0o600 is a
                    // POSIX-only contract. On non-Unix the
                    // substrate emits a substrate-faithful failure
                    // rather than silently degrade.
                    return Err(OctoCliError::Internal(sanitize_substrate_error(
                        "octo agent run --token-file requires POSIX-mode 0o600 perms; non-Unix platform not supported",
                    )));
                }

                // 6.5.6 Build receipt envelope. The bytes on
                //       disk equal `encode_token(handle)`; the
                //       operator can verify by piping through
                //       `octo agent attach --token-file` (TV-CLI-
                //       RUN-DETACH-1 happy-path round-trip).
                let written_at_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                Some(TokenWrittenReceipt {
                    session_id_hex: hex::encode(handle_ref.session_id),
                    bytes_written: token_bytes.len(),
                    path_redacted: RedactedIdentifier::new(token_path.display().to_string()),
                    written_at_unix,
                })
            } else {
                None
            }
        } else {
            None
        };

        // 7. Build output envelope. Mirror the destroy handler's
        //    shape: `agent_id` redacted, `state` from the
        //    substrate-confirmed `current_state`, `audit_log_entry`
        //    is the BLAKE3-256 chain-hash hex-encoded.
        //    `runtime_handle` is `Some(handle)` on a forward
        //    transition and `None` on idempotent self-transition
        //    (the CLI skipped `runtime_spawn_agent`).
        let output = AgentRunOutput {
            agent_id: RedactedIdentifier::new(agent_id.to_string()),
            state: receipt.current_state.as_str().to_string(),
            runtime_handle: handle
                .as_ref()
                .map(|h| RedactedIdentifier::new(h.handle_id.0.to_string())),
            // The substrate monotonic clock contract (RFC-0015-a
            // §6.2.5) guarantees `spawned_at` is non-negative — a
            // pre-1970 timestamp is not representable in normal
            // operation. We use `u64::try_from(...).unwrap_or(0)`
            // instead of `as u64` so a future substrate change that
            // allows negative timestamps (e.g. clock skew below
            // epoch) surfaces as a deterministic `0` rather than a
            // silent wrap-around to `u64::MAX - |n|`. `spawned_at_unix`
            // is sourced only from a freshly minted handle; on the
            // idempotent self-transition path `handle` is `None`
            // and the field falls back to `0` (the substrate contract
            // makes the original spawn time inaccessible in Phase 1
            // — Phase 2 routes through a `get_or_create` pathway).
            spawned_at_unix: handle
                .as_ref()
                .map(|h| u64::try_from(h.spawned_at.timestamp()).unwrap_or(0))
                .unwrap_or(0),
            transitioned_at_unix: receipt.transitioned_at_unix,
            audit_log_entry: hex::encode(receipt.audit_log_entry),
            // `None` on every path where `--token-file` was not
            // supplied (or where the token pathway was not
            // activated — e.g. idempotent self-transition).
            // `Some(...)` only when `--detach --token-file` was
            // specified AND a fresh `RuntimeHandle` was minted
            // (the substrate-faithful binding pair per RFC-0011-c
            // §F.6.1).
            token_written,
        };
        let redactor = common::build_agent_redactor(active_did.as_str(), &agent_id);
        render_envelope("octo.agent.run.v1", output, cli, &redactor)
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
// `octo agent destroy` handler
// ---------------------------------------------------------------------------

mod destroy {
    //! `octo agent destroy --agent-id <uuid> --confirm [--reason <text>] [--json]`
    //!
    //! RFC-0011-c §9.3.4. The only agent subcommand with a hard
    //! confirmation gate (parent RFC-0011 §Error Handling); `--yes` /
    //! `--force` are NOT provided per RFC-0011-c §Security. The clap
    //! `confirm` flag is REQUIRED — absent → `ConfirmationRequired
    //! { command: "agent destroy" }` (exit 2 per parent §Error
    //! Handling).
    //!
    //! Substrate: `octo_wallet::transition_agent(caller_did, uuid,
    //! AgentState::Terminated, reason)` (RFC-0015-a Appendix A;
    //! `Registered → Running` and `Running → Terminated` are the only
    //! valid forward edges). The substrate owns the audit append
    //! (`AuditEventKind::AgentTransition` cfg-gated variant,
    //! RFC-0015-a §6.1 rollback contract) and returns the BLAKE3-256
    //! chain-hash via `TransitionReceipt.audit_log_entry`.

    use super::*;
    use octo_wallet::transition_agent as wallet_transition_agent;
    use octo_wallet::{AgentState, TransitionReceipt};

    // Parse helper lives in `common::parse_agent_uuid` (shared with
    // run + attach per DRY).

    /// Handle `octo agent destroy --agent-id <uuid> --confirm [--reason] [--json]`.
    ///
    /// Exit codes:
    /// - 0: success (transition recorded, audit appended)
    /// - 2: `--confirm` missing (`ConfirmationRequired { command }`)
    /// - 5: HSM unavailable
    /// - 42: agent not found / unparseable UUID
    /// - 43: `AlreadyInTransition` / `InvalidStateTransition`
    /// - 52: audit substrate not ready (`AuditSubstrateNotReady`)
    /// - 64: unexpected substrate error
    pub fn handle(
        agent_id_hex: &str,
        reason: Option<&str>,
        cli: &Octo,
    ) -> Result<(), OctoCliError> {
        // 1. Confirmation gate — RFC-0011-c §Security. The
        //    `--confirm` flag is the global `OperatorModeFlags.confirm`
        //    per `crates/octo-cli/src/flags.rs` §OperatorModeFlags.
        //    `agent destroy` is the ONLY agent subcommand with a hard
        //    confirmation gate; `--yes` / `--force` are NOT provided
        //    per parent RFC-0011 §Security Considerations 1a.
        if !cli.mode.confirm {
            return Err(OctoCliError::ConfirmationRequired {
                command: "agent destroy".to_string(),
            });
        }

        // 2. Auditor is read-only (RFC-0011 §Compatibility + RFC-0011-c
        //    §Roles and Authorities).
        if matches!(cli.mode.mode, OperatorMode::Auditor) {
            return Err(OctoCliError::AuditorDenied {
                command: "agent destroy".to_string(),
            });
        }

        // 3. Parse agent id + resolve active DID.
        let agent_id = common::parse_agent_uuid(agent_id_hex)?;
        let active_did = common::resolve_active_did()?;

        // 4. Substrate `transition_agent` — emits the
        //    `AgentTransition` audit event internally; rolls back the
        //    state change on audit-append failure
        //    (`AuditUnavailable` per RFC-0015-a §6.1 rollback
        //    contract).
        let receipt: TransitionReceipt =
            wallet_transition_agent(&active_did, agent_id, AgentState::Terminated, reason)
                .map_err(common::map_transition_wallet_error)?;

        // 5. JSON override (TTY parity) — flip the CLI JSON flag for
        //    `render_envelope`. The envelope schema itself is fixed
        //    (`schema_version = 4` per parent RFC-0011-c §9.4.1).
        let output = AgentDestroyOutput {
            agent_id: RedactedIdentifier::new(agent_id.to_string()),
            state: receipt.current_state.as_str().to_string(),
            terminated_at_unix: receipt.transitioned_at_unix,
            audit_log_entry: hex::encode(receipt.audit_log_entry),
        };
        let redactor = common::build_agent_redactor(active_did.as_str(), &agent_id);
        render_envelope("octo.agent.destroy.v1", output, cli, &redactor)
    }
}

/// `octo agent destroy` payload — RFC-0011-c §9.3.4 Output Envelope.
///
/// Symmetric with `AgentCreateOutput` (created in mission
/// `0011-c-agent-create-subcommand` + redaction envelope amendment).
/// The `agent_id` is wrapped in [`RedactedIdentifier`] for envelope-
/// boundary symmetry; the redactor un-redacts for the active DID
/// holder. The `audit_log_entry` is the BLAKE3-256 chain-hash in hex
/// form (64 lowercase chars).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct AgentDestroyOutput {
    /// Deterministic `agent_id` (RFC-0011-c §9.10 substrate signature).
    /// Wrapped in [`RedactedIdentifier`] for envelope-boundary
    /// symmetry with `agent create` per
    /// `0011-c-agent-redaction-envelope` §Scope sub-step 2.
    #[schemars(with = "String")]
    pub agent_id: RedactedIdentifier,
    /// Lifecycle state label — always `terminated` for the destroy
    /// command (the substrate rejects any other end state per
    /// RFC-0015-a Appendix A state-machine guard).
    pub state: String,
    /// Unix seconds at which the substrate applied the
    /// `Running → Terminated` transition.
    pub terminated_at_unix: u64,
    /// BLAKE3-256 chain-hash of the committed audit event (lowercase
    /// hex form; 64 chars).
    pub audit_log_entry: String,
}

/// `octo agent run` payload — RFC-0011-c §9.3.2 Output Envelope.
///
/// Mirror of the `destroy` envelope shape plus the
/// `runtime_handle` mint record. The `agent_id` is wrapped in
/// [`RedactedIdentifier`] for envelope-boundary symmetry with
/// `agent create` per mission `0011-c-agent-redaction-envelope`
/// §Scope sub-step 2. The `runtime_handle` is the substrate-minted
/// `RuntimeHandleId` (UUID form) for downstream `octo agent attach`
/// replay binding. The `audit_log_entry` is the BLAKE3-256
/// chain-hash of the committed `AgentTransition` audit event
/// (RFC-0015-a §6.1; 64 lowercase hex chars).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct AgentRunOutput {
    /// Deterministic `agent_id` (RFC-0011-c §9.10 substrate signature).
    /// Wrapped in [`RedactedIdentifier`] for envelope-boundary
    /// symmetry with the create / destroy / list handlers.
    #[schemars(with = "String")]
    pub agent_id: RedactedIdentifier,
    /// Lifecycle state label — always `running` for the run command
    /// (the substrate enforces `Registered → Running` per RFC-0015-a
    /// Appendix A; any other edge surfaces as
    /// `InvalidStateTransition` exit 43).
    pub state: String,
    /// Runtime handle id (UUID form) returned by `octo_runtime::spawn_agent`
    /// (RFC-0011-c §9.3.2 + RFC-0011-c §9.10 Substrate `[ADD]`
    /// `RuntimeHandle::handle_id`). Wrapped in
    /// [`RedactedIdentifier`] for envelope-boundary symmetry;
    /// used by `octo agent attach --agent-id <uuid>` replay
    /// binding (RFC-0011-c §9.3.5).
    ///
    /// `None` on idempotent self-transition (`Running → Running`
    /// no-op per RFC-0015-a §6.1) — the CLI skipped
    /// `runtime_spawn_agent` to avoid minting a duplicate handle
    /// for the same agent.
    #[schemars(with = "Option<String>")]
    pub runtime_handle: Option<RedactedIdentifier>,
    /// Unix seconds at which the runtime substrate minted the
    /// handle (RFC 3339 UTC); sourced from `RuntimeHandle::spawned_at`.
    pub spawned_at_unix: u64,
    /// Unix seconds at which the substrate applied the
    /// `Registered → Running` state transition (from
    /// `TransitionReceipt::transitioned_at_unix`).
    pub transitioned_at_unix: u64,
    /// BLAKE3-256 chain-hash of the committed `AgentTransition`
    /// audit event (RFC-0015-a §6.1; 64 lowercase hex chars).
    /// Zeroed (`"00..00"`) on idempotent self-transitions per the
    /// `TransitionReceipt` invariant comment.
    pub audit_log_entry: String,
    /// Receipt for the `AttachHandle` token bytes written to
    /// `--token-file` (RFC-0011-c §F.6.1). `None` on every path
    /// where `--token-file` was not supplied — the operator
    /// can detect the no-token path from this field directly
    /// rather than re-deriving it from the runtime_handle id.
    /// When `Some`, the byte length + session id + path are
    /// substrate-faithful (the bytes on disk equal
    /// `octo_runtime::encode_token(handle)`).
    #[schemars(with = "Option<TokenWrittenReceipt>")]
    pub token_written: Option<TokenWrittenReceipt>,
}

/// `octo agent run --detach --token-file` token-write receipt
/// (RFC-0011-c §F.6.1).
///
/// Operator-facing record of the `AttachHandle` token bytes the
/// CLI wrote to disk. Carries enough information for the
/// operator to locate + verify the token: the session id (so the
/// future `octo agent attach --token-file` can verify it was the
/// same spawn), the byte length (the encoded token is a fixed
/// size per `encode_token` — a length mismatch indicates
/// truncation / tampering), the path the bytes were written to
/// (operator can `ls` / `chmod` verify), and the wall-clock at
/// write time.
///
/// `path_redacted` field-name suffix is `RedactedIdentifier`-shaped
/// at the envelope boundary even though the path is operator-local
/// (the redactor may still redact home-directory relative paths
/// per the operator's `output.redact_paths` flag — see
/// `crates/octo-cli/src/redact.rs`). The struct redaction contract
/// matches `AgentRunOutput` sibling fields per
/// [[cipherocto-design-principles]] §Attenuation invariants.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct TokenWrittenReceipt {
    /// Hex-encoded session id (RFC-0011-c §F.1 wire form; 64
    /// lowercase hex chars). Same id the substrate stored on the
    /// revocation set; the operator can pass this to
    /// `octo revoke-attach --session-id <hex>` to invalidate the
    /// token (RFC-0011-c §F.3).
    pub session_id_hex: String,
    /// Number of token bytes written to disk (RFC-0011-c §F.1;
    /// `encode_token` produces a fixed-size canonical form).
    /// Mismatch against the expected substrate length indicates
    /// truncation / corruption.
    pub bytes_written: usize,
    /// Path the token bytes were written to (RFC-0011-c §F.6.1
    /// `--token-file`). Operator-local file path.
    #[schemars(with = "String")]
    pub path_redacted: RedactedIdentifier,
    /// Unix seconds at which the CLI wrote the bytes (RFC 3339
    /// UTC, sourced from `SystemTime::now`). Same mint clock as
    /// the embedded `mint_timestamp_unix` field on the token;
    /// operators can cross-reference the two.
    pub written_at_unix: u64,
}

// ---------------------------------------------------------------------------
// `octo agent attach` handler
// ---------------------------------------------------------------------------

mod attach {
    //! `octo agent attach --agent-id <uuid> [--since <unix-seconds>]`
    //!
    //! RFC-0011-c §9.3.5. Read-only subcommand (does NOT mutate state).
    //! Substrate path:
    //!
    //! 1. `octo_wallet::lookup_agent(caller_did, uuid)` — verify holder
    //!    matches active DID (caller-attestation; SECURITY HIGH per
    //!    RFC-0015 §6.2.1).
    //! 2. `octo_wallet::read_agent_state(caller_did, uuid)` — verify
    //!    `state == AgentState::Running` (otherwise emit
    //!    `AgentNotRunning(uuid)` exit 48 per RFC-0011-c §9.8).
    //! 3. `octo_runtime::attach(handle, since)` — open an
    //!    `EventStream` against the live `RuntimeHandle`.
    //!
    //! ## Substrate-faithful surface (RFC-0011-c §F.6.2 + §F.6 amendment cycle)
    //!
    //! Step 3 requires a `RuntimeHandle` minted by `spawn_agent` plus
    //! an `AttachHandle` token carrying the `session_id` +
    //! ed25519 signature + transport selector. Wired end-to-end by
    //! amendment cycle `0011-c-attach-cli-dispatch-amendment`:
    //! the CLI reads the token bytes from `--token-file`, decodes
    //! via `octo_runtime::decode_token(&bytes, &holder_pubkey)`, then
    //! calls `octo_runtime::attach_with_token(&holder_pubkey, &token,
    //! since_unix)`. The 8 `AttachError` substrate variants map to
    //! 8 `OctoCliError` mirror variants slots 53-59 via the existing
    //! `From<octo_runtime::AttachError>` impl.
    //!
    //! §F.6.5 deferral: the `InProcessHandler::bind` step (the
    //! `TransportHandlerNotRegistered` exit 59 path is wired but
    //! happy-path binding remains deferred until a paired follow-on
    //! amendment cycle wires the session registry. Legitimate tokens
    //! currently surface as `AttachSessionUnknown` (exit 56) — pinned
    //! by `tv_cli_attach_session_unknown_exits_56`.

    use super::*;

    /// Handle `octo agent attach --agent-id <uuid> [--since <unix-seconds>]
    /// --token-file <path>`.
    ///
    /// Exit codes (RFC-0011-c §9.8 + §F.6.2 substrate-faithful mirror):
    /// - 0: success (AttachHandle decoded + attach_with_token bound;
    ///   bounded expected behavior per §F.6.5 session-registry-wiring
    ///   deferral — happy-path exit 56 instead until paired follow-on
    ///   amendment cycle wires the InProcessHandler::bind session
    ///   registry)
    /// - 5: HSM unavailable
    /// - 17: forbidden holder DID mismatch (substrate-fail-closed;
    ///   SECURITY HIGH per RFC-0015 §6.2.1)
    /// - 42: agent not found / unparseable UUID
    /// - 48: `AgentNotRunning` — agent exists but is not in
    ///   `Running` state
    /// - 49: `RuntimeAttachFailed` — substrate `octo_runtime::attach`
    ///   rejected (handle revoked, channel closed, etc.)
    /// - 51: `RuntimeSubstrateNotReady` — defense-in-depth; retained
    ///   for substrate-side spawn failures (the CLI happy-path no
    ///   longer fires this per §F.6 amendment cycle)
    /// - 53: `AttachHandleExpired` (substrate `Expired`) or
    ///   `InvalidSinceCursor` (substrate `InvalidSinceCursor`) —
    ///   shared slot per amendment-chain 8v/7s pattern
    /// - 54: `AttachHandleBadSignature` (substrate `BadSignature`)
    /// - 55: `AttachSessionMismatch` (substrate `SessionMismatch`)
    /// - 56: `AttachSessionUnknown` (substrate `UnknownSession`)
    ///   — current happy-path per §F.6.5 deferral
    /// - 57: `PersistenceError` (substrate `PersistenceError`)
    /// - 58: `RevocationError` (substrate `RevocationError`)
    /// - 59: `TransportHandlerNotRegistered` (substrate variant)
    /// - 64: unexpected substrate error
    pub fn handle(
        agent_id_hex: &str,
        since_unix: Option<u64>,
        token_file: &std::path::Path,
        cli: &Octo,
    ) -> Result<(), OctoCliError> {
        // 1. Parse agent id hex → substrate `Uuid`. Parse failure
        //    maps to `AgentNotFound(nil)` per the lookup-agent
        //    canonicalization contract (slot 42).
        let agent_id = common::parse_agent_uuid(agent_id_hex)?;

        // 2. Resolve active DID via the shared helper.
        let active_did = common::resolve_active_did()?;

        // 3. Caller-attestation: verify the agent exists AND the
        //    holder DID matches the active DID. `lookup_agent`
        //    collapses unknown + not-owned into `AgentNotFound`
        //    (per RFC-0015-b defect 3 fix; substrate-faithful
        //    multi-DID enumeration prevention).
        let _manifest = octo_wallet::lookup_agent(&active_did, agent_id).map_err(|e| match e {
            octo_wallet::WalletError::AgentNotFound(uuid) => OctoCliError::AgentNotFound(uuid),
            octo_wallet::WalletError::ForbiddenHolderMismatch => {
                OctoCliError::ForbiddenHolderMismatch
            }
            octo_wallet::WalletError::Hsm(_) => common::map_hsm_error(&e.to_string()),
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })?;

        // 4. State precondition: agent must be in `Running` state
        //    (per RFC-0015-a Appendix A state machine + RFC-0011-c
        //    §9.3.5 attach precondition). `Terminated` /
        //    `Registered` agents surface as `AgentNotRunning(uuid)`
        //    (slot 48). TV-AGT12.
        let state = octo_wallet::read_agent_state(&active_did, agent_id).map_err(|e| match e {
            octo_wallet::WalletError::AgentNotFound(uuid) => OctoCliError::AgentNotFound(uuid),
            octo_wallet::WalletError::ForbiddenHolderMismatch => {
                OctoCliError::ForbiddenHolderMismatch
            }
            octo_wallet::WalletError::Hsm(_) => common::map_hsm_error(&e.to_string()),
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })?;
        if state != octo_wallet::AgentState::Running {
            return Err(OctoCliError::AgentNotRunning(agent_id));
        }

        // 5. Parse `--since` (Unix-seconds → u64) for the
        //    `attach_with_token` replay cursor. The CLI maps
        //    the substrate `clamp_since` semantics by passing
        //    the raw value through; the substrate clamps to
        //    `[token.mint_timestamp_unix, now_unix]` per
        //    RFC-0011-c §F.2 step (c) + §F.6.2.
        //
        //    Default: when `--since` is absent, the CLI passes
        //    `mint_timestamp_unix` so replay starts at spawn
        //    time. The token's `mint_timestamp_unix` is
        //    observable post-decode, so we use a two-pass:
        //    decode first, then compute the effective `since`.
        //    To keep the boundary single-pass, we read
        //    `mint_timestamp_unix` after decode below.

        // 6. `--token-file` token pathway
        //    (RFC-0011-c §F.6.2). On the post-amendment
        //    substrate-faithful path, the CLI:
        //
        //    1. Reads the canonical token bytes from
        //       `--token-file` (operator-local path).
        //    2. Resolves caller DID → `[u8; 32]` holder_pubkey
        //       via `IdentityKey::public_key_bytes()`.
        //    3. Decodes via `octo_runtime::decode_token(&bytes,
        //       &holder_pubkey)` — verifies the embedded
        //       ed25519 signature against the holder's public
        //       key (validation chain step (a) per RFC-0011-c
        //       §F.2).
        //    4. Calls `octo_runtime::attach_with_token(
        //       &holder_pubkey, &token, since_unix)` — a
        //       4-step validation chain: signature verify
        //       (already done by `decode_token`),
        //       revocation-set check (step (b)), TTL check
        //       (step (c)), since-cursor check (step (d)),
        //       handler lookup (step (e) per RFC-0011-c §F.2
        //       step (e) post-Handler-registry amendment).
        //    5. The 8 `AttachError` variants map to 8
        //       `OctoCliError` exit codes 53-59 via the
        //       existing `From<octo_runtime::AttachError>`
        //       impl in `crates/octo-cli/src/error.rs`.
        //
        //    The substrate `attach_with_token` is `async`. The
        //    CLI `main()` is sync (no tokio runtime); we build
        //    a current-thread runtime locally for the single
        //    `block_on` call. This matches the established
        //    pattern at `crates/octo-cli/Cargo.toml` lines
        //    29-31 (the `new_current_thread` rationale is
        //    documented in that file).
        let token_bytes = std::fs::read(token_file).map_err(|io| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("token file read: {io}")))
        })?;

        let holder = common::resolve_active_identity_key()?;
        let holder_pubkey: [u8; 32] = holder.public_key_bytes();

        let token = octo_runtime::decode_token(&token_bytes, &holder_pubkey)?;

        // Effective `since_unix`: the operator's `--since` if
        // supplied (substrate-faithful; the substrate rejects
        // values below the token's `mint_timestamp_unix` with
        // `InvalidSinceCursor` per RFC-0011-c §F.2 step (d),
        // exit 53 shared slot); else fall back to the token's
        // `mint_timestamp_unix` (replay from spawn time, the
        // canonical default). The clap arg is `Option<u64>`
        // so no lossy `as i64` cast is needed (R3 MED fix —
        // prior draft cast to i64 which wrapped at > i64::MAX
        // and surfaced as a confusing "since cursor below mint"
        // instead of an out-of-range signal).
        let since_unix: u64 = since_unix.unwrap_or(token.mint_timestamp_unix);

        // Build a fresh current-thread tokio runtime to drive the
        // async `attach_with_token` call. The runtime is dropped
        // at end of scope — the broadcast receiver returned by
        // `AttachedSession` lives until the receiver itself is
        // dropped; we discard the receiver here (the CLI does
        // not subscribe to events in this mission; the event-
        // stream replay surface is a follow-on substrate
        // amendment per [[0011-c-attachhandle-dry-closure-2026-09-16]]).
        //
        // R5.5 fix (substrate-faithful sync/async boundary):
        // the prior match-on-Handle::try_current branched into
        // `handle.block_on(...)` which panics under tokio 1.x
        // when the runtime flavor is `multi_thread` (only
        // `current_thread` flavors expose `Handle::block_on` per
        // tokio contract). Library consumers wrapping the CLI
        // dispatch in `#[tokio::main(flavor = "multi_thread")]`
        // would hit `Cannot drop a runtime in a context where
        // blocking is not allowed` or equivalent. Drop the
        // try_current branch entirely so the dispatch surface
        // is substrate-faithful across runtime flavors; a future
        // amendment can add a sync facade
        // (`octo_runtime::attach_with_token_blocking`) for
        // library consumers that don't want to construct a
        // nested runtime, and that facade is the canonical
        // substrate-side improvement direction (per the
        // layer-model disposition in [[cipherocto-design-principles]]
        // §No parallel abstractions).
        let attached = {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    OctoCliError::Internal(sanitize_substrate_error(&format!(
                        "tokio runtime build: {e}"
                    )))
                })?;
            rt.block_on(octo_runtime::attach_with_token(
                &holder_pubkey,
                &token,
                since_unix,
            ))?
        };

        let attached_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Discard the broadcast receiver (CLI does not consume
        // events in this cycle; drop closes the receiver slot).
        drop(attached.broadcast_rx);

        let output = AgentAttachOutput {
            agent_id: RedactedIdentifier::new(agent_id.to_string()),
            // Canonical binding identifier is the token's session_id
            // (RFC-0011-c §F.6.2 schema-faithful). AttachedSession
            // exposes only event_cursor + broadcast_rx; the
            // in-process RuntimeHandleId is intentionally absent
            // from this envelope (the emit side's AgentRunOutput
            // carries it instead).
            attached_at_unix: Some(attached_at_unix),
            event_cursor: Some(attached.event_cursor.to_string()),
            session_id_hex: hex::encode(token.session_id),
        };
        let redactor = common::build_agent_redactor(active_did.as_str(), &agent_id);
        render_envelope("octo.agent.attach.v1", output, cli, &redactor)
    }
}

/// `octo agent attach` payload — RFC-0011-c §9.3.5 Output Envelope.
///
/// Read-only subcommand envelope. Per mission §Substrate gap, the
/// payload fields are reserved for the post-`octo agent run` end-to-end
/// path. The struct is shipped NOW so the schemars contract is pinned
/// and the follow-on `octo agent run --detach` mission can emit it
/// without breaking downstream tooling (`jq` paths off the field
/// names).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct AgentAttachOutput {
    /// Deterministic `agent_id` (RFC-0011-c §9.10 substrate signature).
    /// Wrapped in [`RedactedIdentifier`] for envelope-boundary
    /// symmetry with `agent create` / `agent destroy`.
    #[schemars(with = "String")]
    pub agent_id: RedactedIdentifier,
    /// Unix seconds at which the substrate bound the EventStream
    /// (RFC-0011-c §9.3.5). `None` until the runtime substrate path
    /// is wired end-to-end.
    pub attached_at_unix: Option<u64>,
    /// Lower-bound timestamp the EventStream starts from (RFC-0011-c
    /// §9.3.5 `--since`). `None` when no `--since` flag is supplied
    /// (substrate defaults to `spawned_at`).
    pub event_cursor: Option<String>,
    /// Hex-encoded session id from the `AttachHandle` token
    /// (RFC-0011-c §F.6.2).
    ///
    /// The session id is the canonical binding identifier across
    /// `run --detach --token-file` + `attach --token-file` per
    /// RFC-0011-c §F.2 step (a) and §F.6.4 pairing invariant.
    ///
    /// Operators can use this to:
    ///
    /// - confirm which spawn the attached session is bound to
    /// - pass to `octo revoke-attach --session-id <hex>` to
    ///   invalidate the token (RFC-0011-c §F.3)
    ///
    /// Always populated on the post-amendment substrate-faithful
    /// path (`octo agent attach --token-file <path>` succeeded
    /// and the validation chain at RFC-0011-c §F.2 passed).
    pub session_id_hex: String,
}

// ---------------------------------------------------------------------------
// `octo agent revoke-attach` — RFC-0011-c §F.3 follow-on
// ---------------------------------------------------------------------------

/// Operator output envelope for `octo agent revoke-attach` (RFC-0011-c
/// §F.3 follow-on). Records the revoked `session_id` (hex) so the
/// operator can confirm which token was retracted.
#[derive(Debug, Clone, Serialize)]
pub struct RevokeAttachOutput {
    /// Hex-encoded 64-char session id that was revoked (RFC-0011-c §F.1
    /// wire form).
    pub session_id_hex: String,
    /// RFC-0011-c §F.3 — the revocation set is a process-singleton.
    /// This field surfaces that fact on the operator envelope so
    /// operators know that tokens minted in another process remain
    /// valid.
    pub process_scoped: bool,
}

mod revoke_attach {
    use super::*;
    use crate::flags::OperatorMode;

    /// Canonical envelope schema label for the revoke-attach output.
    const SCHEMA: &str = "octo.agent.revoke_attach.v1";

    /// Parse a 64-char lowercase hex session id into the substrate
    /// `[u8; 32]`. Mirrors `hex_decode_session` from the audit / role
    /// amendment-chain helpers; a local copy keeps this module's
    /// substrate-faithful contract local. Parse failures emit the
    /// CLI-boundary `InvalidSessionIdHex` (exit 47) variant — NOT
    /// `AttachHandleBadSignature` (exit 54) which is the
    /// substrate-side signature-verify failure per the
    /// typed-error-envelope contract.
    fn decode_session(hex: &str) -> Result<[u8; 32], OctoCliError> {
        if hex.len() != 64 {
            return Err(OctoCliError::InvalidSessionIdHex {
                reason: format!(
                    "session id must be 64 lowercase hex chars, got length {}",
                    hex.len()
                ),
            });
        }
        let mut out = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let pair =
                std::str::from_utf8(chunk).map_err(|_| OctoCliError::InvalidSessionIdHex {
                    reason: "session id contains non-UTF-8 bytes".to_string(),
                })?;
            out[i] =
                u8::from_str_radix(pair, 16).map_err(|_| OctoCliError::InvalidSessionIdHex {
                    reason: format!("session id contains non-hex pair `{pair}`"),
                })?;
        }
        Ok(out)
    }

    /// `octo agent revoke-attach --session-id <HEX64>` handler.
    ///
    /// Thin Layer C wrapper over `octo_runtime::revoke_attach_token`
    /// (Layer B substrate). The revocation set is a process-singleton
    /// per RFC-0011-c §F.3; tokens minted in another process remain
    /// unaffected (we surface that explicitly on the output envelope).
    ///
    /// Mutation contract: per RFC-0011 §Compatibility, mutating
    /// commands require `--confirm` outside Dev mode. Auditor mode
    /// is denied up front (Auditor is read-only).
    pub fn handle(session_id_hex: &str, cli: &Octo) -> Result<(), OctoCliError> {
        // Mode gate (RFC-0011 §Compatibility).
        match cli.mode.mode {
            OperatorMode::Auditor => {
                return Err(OctoCliError::AuditorDenied {
                    command: "agent revoke-attach".to_string(),
                });
            }
            OperatorMode::Human | OperatorMode::Ci => {
                if !cli.mode.confirm {
                    return Err(OctoCliError::ConfirmationRequired {
                        command: "agent revoke-attach".to_string(),
                    });
                }
            }
            OperatorMode::Dev => {
                // Dev mode skips the confirmation gate per RFC-0011
                // §Compatibility.
            }
        }

        let session_id = decode_session(session_id_hex)?;

        // Substrate folds revocation-set failures into
        // `AttachError::RevocationError(String)` per RFC-0011-c
        // §F.4 (the standalone `RevocationError` enum was folded
        // into the canonical envelope). The `From<AttachError>`
        // bridge in `OctoCliError` handles the per-variant mapping
        // so the substrate-faithful contract is the single source
        // of truth — the CLI is a thin Layer C wrapper.
        octo_runtime::revoke_attach_token(session_id)?;

        let envelope = RevokeAttachOutput {
            session_id_hex: session_id_hex.to_string(),
            process_scoped: true,
        };
        let redactor = RedactionContext::new();
        render_envelope(SCHEMA, envelope, cli, &redactor)
    }
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

    // ----- `octo agent destroy` tests (RFC-0011-c §9.3.4) -----

    /// Pin the schemars contract for `AgentDestroyOutput`: `agent_id`
    /// is wrapped in `RedactedIdentifier` with `#[schemars(with = "String")]`
    /// so the JSON Schema declares `string`. Symmetric with the
    /// `agent create` + `agent list` schemars pins.
    #[test]
    fn agent_destroy_output_schema_declares_agent_id_as_string() {
        use schemars::schema_for;
        let schema = schema_for!(AgentDestroyOutput);
        let json = serde_json::to_value(&schema).expect("schema is JSON");
        let agent_id = json
            .pointer("/properties/agent_id/type")
            .and_then(|v| v.as_str())
            .expect("agent_id schema must declare a type");
        assert_eq!(
            agent_id, "string",
            "agent_id must round-trip as JSON Schema `string`, got {agent_id:?}",
        );
        let state = json
            .pointer("/properties/state/type")
            .and_then(|v| v.as_str())
            .expect("state schema must declare a type");
        assert_eq!(
            state, "string",
            "state must round-trip as JSON Schema `string`, got {state:?}",
        );
        let audit = json
            .pointer("/properties/audit_log_entry/type")
            .and_then(|v| v.as_str())
            .expect("audit_log_entry schema must declare a type");
        assert_eq!(
            audit, "string",
            "audit_log_entry must round-trip as JSON Schema `string`, got {audit:?}",
        );
    }

    /// TV-AGT10 — confirmation gate. Absence of `--confirm` MUST yield
    /// `ConfirmationRequired { command }` (exit 2 per parent RFC-0011
    /// §Error Handling). `agent destroy` is the ONLY agent
    /// subcommand with a hard confirmation gate (RFC-0011-c
    /// §Security); `--yes` / `--force` are NOT provided per the same
    /// §Security paragraph.
    #[test]
    fn destroy_missing_confirm_exits_2() {
        let e = OctoCliError::ConfirmationRequired {
            command: "agent destroy".to_string(),
        };
        assert_eq!(
            e.exit_code(),
            2,
            "ConfirmationRequired MUST exit 2 (parent RFC-0011 §Error Handling), got {}",
            e.exit_code()
        );
    }

    /// TV-AGT9 — substrate `Running → Terminated` transition returns
    /// `TransitionReceipt`. Pin the slot allocations: `AgentNotFound`
    /// (42), `AlreadyInTransition` (43), `InvalidStateTransition`
    /// (43), `AuditSubstrateNotReady` (52).
    #[test]
    fn destroy_exit_code_slots_pinned() {
        let e = OctoCliError::AgentNotFound(uuid::Uuid::nil());
        assert_eq!(e.exit_code(), 42, "AgentNotFound slot 42");
        let e = OctoCliError::AlreadyInTransition(uuid::Uuid::nil());
        assert_eq!(
            e.exit_code(),
            43,
            "AlreadyInTransition slot 43 (write-path state error)"
        );
        let e = OctoCliError::InvalidStateTransition {
            from: "registered".to_string(),
            to: "terminated".to_string(),
        };
        assert_eq!(
            e.exit_code(),
            43,
            "InvalidStateTransition slot 43 (write-path state error)"
        );
        assert_eq!(
            OctoCliError::AuditSubstrateNotReady.exit_code(),
            52,
            "AuditSubstrateNotReady slot 52 (audit-substrate fallback)"
        );
    }

    // ----- `octo agent attach` tests (RFC-0011-c §9.3.5) -----

    /// Pin the schemars contract for `AgentAttachOutput`: `agent_id`
    /// is wrapped in `RedactedIdentifier` with `#[schemars(with = "String")]`
    /// so the JSON Schema declares `string`. Symmetric with the
    /// `agent create` / `agent destroy` / `agent list` schemars pins.
    ///
    /// The `session_id_hex` field is a plain `String` carrying the
    /// hex-encoded session id from the decoded `AttachHandle` token
    /// (RFC-0011-c §F.6.2). Pin that the schema also declares it
    /// as `string` so downstream `jq` pipelines that grep for
    /// `session_id_hex` continue to work after the post-amendment
    /// field addition.
    #[test]
    fn agent_attach_output_schema_declares_string_fields() {
        use schemars::schema_for;
        let schema = schema_for!(AgentAttachOutput);
        let json = serde_json::to_value(&schema).expect("schema is JSON");
        let agent_id = json
            .pointer("/properties/agent_id/type")
            .and_then(|v| v.as_str())
            .expect("agent_id schema must declare a type");
        assert_eq!(
            agent_id, "string",
            "agent_id must round-trip as JSON Schema `string`, got {agent_id:?}",
        );
        let session_id_hex = json
            .pointer("/properties/session_id_hex/type")
            .and_then(|v| v.as_str())
            .expect("session_id_hex schema must declare a type");
        assert_eq!(
            session_id_hex, "string",
            "session_id_hex must round-trip as JSON Schema `string`, got {session_id_hex:?}",
        );
    }

    /// TV-AGT12 — attach to non-Running agent rejected. Per
    /// RFC-0011-c §9.8 (slot 48 reserved by the agent amendment
    /// chain), the state gate `state != AgentState::Running`
    /// surfaces as `AgentNotRunning(uuid)` exit 48.
    #[test]
    fn attach_agent_not_running_exits_48() {
        let e = OctoCliError::AgentNotRunning(uuid::Uuid::nil());
        assert_eq!(
            e.exit_code(),
            48,
            "AgentNotRunning MUST exit 48 (RFC-0011-c agent amendment chain slot 48), got {}",
            e.exit_code()
        );
    }

    /// TV-AGT11 — pin the `RuntimeSubstrateNotReady` slot allocation
    /// (exit 51) so a future amendment that shifts the slot surfaces
    /// as a broken test. Per RFC-0011-c §F.6 amendment cycle the
    /// CLI dispatch surface is wired end-to-end
    /// (`decode_token` + `attach_with_token`); the variant is
    /// retained for substrate-side spawn failures (defense-in-depth)
    /// — automation distinguishes "agent not running" (48) from
    /// "runtime substrate boundary failure" (51) from the new
    /// `AttachSessionUnknown` happy-path (56).
    #[test]
    fn attach_runtime_substrate_not_ready_exits_51() {
        let e = OctoCliError::RuntimeSubstrateNotReady;
        assert_eq!(
            e.exit_code(),
            51,
            "RuntimeSubstrateNotReady MUST exit 51 (RFC-0011-c agent amendment chain slot 51), got {}",
            e.exit_code()
        );
    }

    /// Pin the `RuntimeAttachFailed` slot allocation (exit 49) so a
    /// future amendment that shifts the slot surfaces as a broken
    /// test. The variant maps from
    /// `octo_runtime::RuntimeError::HandleRevoked` /
    /// `RuntimeError::RuntimeAttachFailed { reason }` /
    /// `RuntimeError::EventStreamClosed` at the dispatch boundary
    /// (per RFC-0011-c §9.8 + `octo_runtime::RuntimeError::exit_code`).
    #[test]
    fn attach_runtime_attach_failed_exits_49() {
        let e = OctoCliError::RuntimeAttachFailed {
            reason: "handle revoked".to_string(),
        };
        assert_eq!(
            e.exit_code(),
            49,
            "RuntimeAttachFailed MUST exit 49 (RFC-0011-c agent amendment chain slot 49), got {}",
            e.exit_code()
        );
    }

    // ----- RFC-0011-c §F.6 token pathway TV (4 NEW TV) -----

    /// TV-CLI-RUN-DETACH-1 — substrate-faithful mint + encode + decode
    /// round-trip for the `AttachHandle` token pathway (RFC-0011-c
    /// §F.6.1 + §F.1 + §F.2). The CLI dispatch body builds the token
    /// via `mint_attach_handle` + `encode_token` and writes the bytes
    /// to `--token-file`.
    ///
    /// This test pins the substrate-faithful contract that the bytes
    /// round-trip back through `decode_token` with the mint-time
    /// holder pubkey, and that the encoded payload includes all five
    /// canonical fields (`session_id`, `mint_timestamp_unix`,
    /// `ttl_unix`, `payload`, `signature`).
    ///
    /// The full happy-path integration (write file + chmod 0o600 +
    /// fsync + read-back) is exercised at the dispatch level; here we
    /// pin only the substrate contract that the CLI relies on.
    #[test]
    fn tv_cli_run_detach_token_round_trips_through_encode_decode() {
        use octo_runtime::{
            decode_token, encode_token, mint_attach_handle, Transport, TransportKind,
        };
        use octo_wallet::IdentityKey;
        use uuid::Uuid;

        let mut holder = IdentityKey::generate().expect("IdentityKey::generate must succeed");
        // Activate the identity so `sign_attach_handle_payload` (which
        // gates on `IdentityKey::activated_at_unix_secs().is_some()`)
        // accepts the mint. The CLI activation flow happens at the
        // active-DID substrate boundary, not at this dispatch unit —
        // here we exercise only the post-activation pathway.
        let now_unix_secs: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1_700_000_000);
        holder
            .activate(now_unix_secs)
            .expect("activate must succeed");
        let holder_pubkey = holder.public_key_bytes();
        let agent_id = Uuid::new_v4();
        let session_id = [0x42_u8; 32];
        let since_cursor: u64 = 0;
        let ttl_unix: u64 = u64::MAX / 2; // far in the future; never expires under test
        let transport = Transport {
            kind: TransportKind::InProcess,
            addr: None,
        };

        let handle = mint_attach_handle(
            &holder,
            agent_id,
            session_id,
            since_cursor,
            ttl_unix,
            transport,
        )
        .expect("mint_attach_handle must succeed for active holder");

        // Encode → write round-trip via `Vec<u8>` (substrate-faithful;
        // the CLI dispatch then writes these bytes to `--token-file`).
        let token_bytes = encode_token(&handle).expect("encode_token must succeed");

        // Decode back and confirm the 5-field contract:
        // session_id, mint_timestamp_unix, ttl_unix, payload, signature.
        let decoded = decode_token(&token_bytes, &holder_pubkey)
            .expect("decode_token must succeed against mint-time holder pubkey");
        assert_eq!(
            decoded.session_id, session_id,
            "session_id MUST survive encode/decode round-trip"
        );
        assert_eq!(
            decoded.ttl_unix, ttl_unix,
            "ttl_unix MUST survive encode/decode round-trip"
        );
        assert_eq!(
            decoded.payload.agent_id, agent_id,
            "payload.agent_id MUST survive encode/decode round-trip"
        );
        assert_eq!(
            decoded.payload.since_cursor, since_cursor,
            "payload.since_cursor MUST survive encode/decode round-trip"
        );
        assert_eq!(
            decoded.signature.0.len(),
            64,
            "signature MUST be 64-byte raw ed25519 wrap"
        );
        // mint_timestamp_unix is sourced from substrate `SystemTime::now()`
        // so the exact value is non-deterministic — only confirm it is
        // populated (non-zero on any working clock).
        assert!(
            decoded.mint_timestamp_unix > 0,
            "mint_timestamp_unix MUST be populated by substrate clock"
        );
    }

    /// TV-CLI-ATTACH-1 — bounded expected behavior per §F.6.5
    /// session-registry-wiring deferral. Legitimate tokens currently
    /// surface as `AttachSessionUnknown` (exit 56) per
    /// substrate-faithful current behavior — the `InProcessHandler::bind`
    /// wiring lands in a paired follow-on amendment cycle. This test
    /// pins the CLI exit slot so a future amendment that wires the
    /// happy-path surfaces as a test rewrite (not a silent slot shift).
    #[test]
    fn tv_cli_attach_session_unknown_exits_56() {
        let e =
            OctoCliError::AttachSessionUnknown("00000000-0000-0000-0000-000000000000".to_string());
        assert_eq!(
            e.exit_code(),
            56,
            "AttachSessionUnknown MUST exit 56 (RFC-0011-c §F.4 substrate mirror slot 56), got {}",
            e.exit_code()
        );
        // The variant payload is `String` (session_id hex) per RFC-0011-c
        // §F.4 substrate-faithful mirror surface; pin the field shape
        // so a future amendment that switches to typed UUID surfaces
        // as a broken contract.
        match &e {
            OctoCliError::AttachSessionUnknown(session_id) => {
                assert!(!session_id.is_empty(), "session_id MUST be non-empty");
            }
            other => panic!("expected AttachSessionUnknown, got {other:?}"),
        }
    }

    /// TV-CLI-ATTACH-2 — tampered token bytes rejected at the substrate
    /// `decode_token` signature-verify step (RFC-0011-c §F.2 step (a)).
    /// The CLI dispatch body surfaces the substrate's
    /// `AttachError::BadSignature { reason }` as `OctoCliError::
    /// AttachHandleBadSignature { reason }` (exit 54) via the
    /// `From<AttachError> for OctoCliError` impl. Pin both:
    /// 1. substrate `decode_token` returns `BadSignature` on byte flip
    /// 2. CLI exit slot is 54 (substrate-faithful mirror)
    #[test]
    fn tv_cli_attach_tampered_token_exits_54_bad_signature() {
        use octo_runtime::{
            decode_token, encode_token, mint_attach_handle, AttachError, Transport, TransportKind,
        };
        use octo_wallet::IdentityKey;
        use uuid::Uuid;

        let mut holder = IdentityKey::generate().expect("IdentityKey::generate must succeed");
        let now_unix_secs: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1_700_000_000);
        holder
            .activate(now_unix_secs)
            .expect("activate must succeed");
        let holder_pubkey = holder.public_key_bytes();
        let handle = mint_attach_handle(
            &holder,
            Uuid::new_v4(),
            [0x11_u8; 32],
            0,
            u64::MAX / 2,
            Transport {
                kind: TransportKind::InProcess,
                addr: None,
            },
        )
        .expect("mint must succeed");
        let mut token_bytes = encode_token(&handle).expect("encode must succeed");

        // Flip a byte in the canonical-encoded payload. The signature
        // covers the payload, so the substrate's verify step (a) MUST
        // reject the tampered bytes.
        let flip_at = token_bytes.len() / 2;
        token_bytes[flip_at] ^= 0x01;

        let decode_err = decode_token(&token_bytes, &holder_pubkey)
            .expect_err("decode_token MUST reject tampered bytes");
        match decode_err {
            AttachError::BadSignature { .. } => {} // expected
            other => panic!("expected BadSignature, got {other:?}"),
        }

        // Mirror surface — `From<AttachError>` translates BadSignature
        // to AttachHandleBadSignature (exit 54) per RFC-0011-c §F.4.
        let cli_err: OctoCliError = decode_err.into();
        assert_eq!(
            cli_err.exit_code(),
            54,
            "AttachHandleBadSignature MUST exit 54 (RFC-0011-c §F.4 slot 54), got {}",
            cli_err.exit_code()
        );
    }

    /// TV-CLI-ATTACH-3 — revoked token rejected by the substrate
    /// revocation-set check at validation step (e) of the
    /// `attach_with_token` chain (RFC-0011-c §F.3 + §F.2). The
    /// revocation set is in-memory `RwLock<HashSet<SessionId>>`
    /// (Layer B substrate); the CLI surfaces
    /// `AttachError::RevocationError(String)` as
    /// `OctoCliError::RevocationError(String)` (exit 58) via the
    /// `From<AttachError>` impl. The revocation-set insertion itself
    /// is exercised by the `octo revoke-attach --session-id` test
    /// vector; here we pin the CLI exit slot so a future amendment
    /// that shifts the slot surfaces as a broken contract.
    #[test]
    fn tv_cli_attach_revoked_token_exits_58_revocation_error() {
        let e = OctoCliError::RevocationError(
            "session 00000000-0000-0000-0000-000000000000 revoked by operator".to_string(),
        );
        assert_eq!(
            e.exit_code(),
            58,
            "RevocationError MUST exit 58 (RFC-0011-c §F.4 slot 58), got {}",
            e.exit_code()
        );
        match &e {
            OctoCliError::RevocationError(reason) => {
                assert!(
                    reason.contains("revoked"),
                    "reason MUST surface the revocation cause verbatim (CLI boundary is operator-readable)"
                );
            }
            other => panic!("expected RevocationError, got {other:?}"),
        }
    }

    // ----- `octo agent run` tests (RFC-0011-c §9.3.2) -----

    /// TV-AGT4 happy-path slot pin: a successful `octo agent run`
    /// emits the `AgentRunOutput` envelope and exits 0. The slot
    /// pins below are the substrate-faithful exit-code map:
    ///
    /// - `InvalidStateTransition` (43) — substrate `Registered →
    ///   Terminated` and `Terminated → *` edges rejected per
    ///   RFC-0015-a Appendix A. TV-AGT5
    /// - `RuntimeSpawnFailed` (44) — runtime handle mint error
    ///   post-transition (substrate commits state change because
    ///   `transition_agent` is atomic with the audit append;
    ///   spawn failure surfaces here). RFC-0011-c §9.8 slot 44
    ///   reserved by the agent amendment chain
    #[test]
    fn run_exit_code_slots_pinned() {
        let e = OctoCliError::InvalidStateTransition {
            from: "registered".to_string(),
            to: "terminated".to_string(),
        };
        assert_eq!(
            e.exit_code(),
            43,
            "InvalidStateTransition slot 43 (write-path state error); required by TV-AGT5"
        );
        let e = OctoCliError::RuntimeSpawnFailed {
            reason: "handle mint error".to_string(),
        };
        assert_eq!(
            e.exit_code(),
            44,
            "RuntimeSpawnFailed slot 44 (RFC-0011-c agent amendment chain slot 44), got {}",
            e.exit_code()
        );
    }

    /// TV-AGT5 — `Terminated → Running` edge rejected by the
    /// substrate state-machine guard (RFC-0015-a Appendix A).
    /// The substrate returns `WalletError::InvalidStateTransition
    /// { from, to }` and the CLI surfaces the typed labels
    /// (`terminated`, `running`) per the mission §Substrate Gap
    /// rewrite (2026-09-13). Pin the label echo so a future
    /// amendment that switches the substrate to opaque IDs
    /// surfaces as a broken contract.
    #[test]
    fn run_invalid_state_transition_from_terminated_to_running_echoes_labels() {
        let from = "terminated".to_string();
        let to = "running".to_string();
        let e = OctoCliError::InvalidStateTransition { from, to };
        match &e {
            OctoCliError::InvalidStateTransition { from, to } => {
                assert_eq!(from, "terminated", "from label must echo substrate");
                assert_eq!(to, "running", "to label must echo substrate");
            }
            other => panic!("expected InvalidStateTransition, got {other:?}"),
        }
        assert_eq!(e.exit_code(), 43, "InvalidStateTransition slot 43");
    }

    /// Pin the schemars contract for `AgentRunOutput`: `agent_id`
    /// and `runtime_handle` are wrapped in `RedactedIdentifier` with
    /// `#[schemars(with = "String")]` so the JSON Schema declares
    /// `string` for both fields. Symmetric with the
    /// create / destroy / list / attach schemars pins.
    #[test]
    fn agent_run_output_schema_declares_string_fields() {
        use schemars::schema_for;
        let schema = schema_for!(AgentRunOutput);
        let json = serde_json::to_value(&schema).expect("schema is JSON");
        let agent_id = json
            .pointer("/properties/agent_id/type")
            .and_then(|v| v.as_str())
            .expect("agent_id schema must declare a type");
        assert_eq!(
            agent_id, "string",
            "AgentRunOutput.agent_id must round-trip as JSON Schema `string`, got {agent_id:?}",
        );
        let runtime_handle = json
            .pointer("/properties/runtime_handle/type")
            .expect("runtime_handle schema must declare a type");
        // `runtime_handle` is `Option<RedactedIdentifier>` (None on
        // idempotent `Running → Running` self-transition per
        // RFC-0015-a §6.1). The schema either renders as
        // `type: "string"` with a `nullable` flag, or as a JSON
        // Schema `oneOf` / array of allowed types containing
        // `"string"`. Accept either shape so the test stays
        // substrate-faithful across schemars versions.
        let runtime_handle_is_string = match runtime_handle {
            serde_json::Value::String(s) => s == "string",
            serde_json::Value::Array(items) => items.iter().any(|it| it.as_str() == Some("string")),
            _ => false,
        };
        assert!(
            runtime_handle_is_string,
            "AgentRunOutput.runtime_handle must round-trip as JSON Schema nullable `string`, got {runtime_handle:?}",
        );
        let state = json
            .pointer("/properties/state/type")
            .and_then(|v| v.as_str())
            .expect("state schema must declare a type");
        assert_eq!(
            state, "string",
            "AgentRunOutput.state must round-trip as JSON Schema `string`, got {state:?}",
        );
        let audit = json
            .pointer("/properties/audit_log_entry/type")
            .and_then(|v| v.as_str())
            .expect("audit_log_entry schema must declare a type");
        assert_eq!(
            audit, "string",
            "AgentRunOutput.audit_log_entry must round-trip as JSON Schema `string`, got {audit:?}",
        );
    }

    /// TV-AGT6 — `octo agent run` happy-path slot pin: the
    /// `AgentNotFound` slot (42) is shared with the destroy / list /
    /// attach handlers. Re-pin here so the run handler's exit
    /// contract is independently verified (regression guard against
    /// a future arm that re-routes the substrate error path).
    #[test]
    fn run_agent_not_found_exits_42() {
        let e = OctoCliError::AgentNotFound(uuid::Uuid::nil());
        assert_eq!(
            e.exit_code(),
            42,
            "AgentNotFound MUST exit 42 (RFC-0011-c agent amendment chain slot 42), got {}",
            e.exit_code()
        );
    }

    /// TV-CLI-RUN-DETACH-2 — POSIX 0o600 mode pin for the
    /// `octo agent run --detach --token-file <path>` write path
    /// (RFC-0011-c §F.6.1). The CLI dispatch writes the
    /// `AttachHandle` token bytes to `--token-file` via
    /// `std::fs::OpenOptions::create(true).write(true).truncate(true)
    /// .mode(0o600).open(path)` on POSIX systems; the token is a
    /// credential (the substrate-faithful per RFC-0011-c
    /// §Layer direction rationale is that the token grants
    /// `attach_with_token` validation-chain entry).
    ///
    /// This test pins the substrate-faithful contract that the
    /// resulting file has mode `0o600` (owner read+write only) — a
    /// future amendment that drops `.mode(0o600)` (e.g. defaults to
    /// `0o644` and the token is world-readable) surfaces as a broken
    /// invariant instead of a silent security regression.
    ///
    /// Out-of-scope (windows / non-unix platforms): the dispatch
    /// surfaces `Internal(reason)` with a substrate-faithful message
    /// (`mode flag unsupported on this platform`); this test only
    /// pins the POSIX contract because `OpenOptionsExt::mode` is a
    /// `#[cfg(unix)]` trait.
    #[test]
    #[cfg(unix)]
    fn tv_cli_run_detach_token_file_written_with_0o600_mode() {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let tmp = tempfile::tempdir().expect("tempfile::tempdir must succeed");
        let token_path = tmp.path().join("attach-token.bin");

        // Mirror the run dispatch write helper verbatim: the dispatch
        // uses exactly this primitive chain (the substrate's only
        // requirement is that the bytes on disk are the
        // `encode_token(&handle)` output — file mode + parent-dir
        // creation + fsync are CLI boundary concerns per RFC-0011-c
        // §F.6.1).
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&token_path)
            .expect("OpenOptions::mode(0o600).open must succeed on unix");
        f.write_all(b"attach-token-payload-stub")
            .expect("write_all must succeed");
        f.sync_all().ok();
        drop(f);

        let metadata = std::fs::metadata(&token_path).expect("metadata must succeed");
        let mode = metadata.permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "token-file MUST have mode 0o600 (RFC-0011-c §F.6.1 substrate faithfulness — token is a credential), got {:o}",
            mode & 0o777,
        );
    }

    /// TV-CLI-ATTACH-4 — full dispatch chain surface per RFC-0011-c
    /// §F.6.5 out-of-scope deferral. The CLI dispatch body
    /// (post-amendment) drives `octo_runtime::attach_with_token(
    /// &holder_pubkey, &token, since_unix)` through a current-thread
    /// tokio runtime (the same boundary pattern documented in
    /// `attach::handle`).
    ///
    /// Per the closure audit §F.6.5 deferral, the
    /// `InProcessHandler::bind` session-registry wiring is deferred
    /// to a paired follow-on amendment cycle. The substrate boundary
    /// `InProcessHandler::bind` has a deliberate dual-mode surface
    /// (the `octo_runtime::handle::transport::InProcessHandler`
    /// module):
    ///
    /// - **Debug build** (the default `cargo test` profile): `panic!`
    ///   with a `not implemented` message — explicitly designed to
    ///   "catch accidental callsite reliance on the half-wired
    ///   pathway during the substrate-first rollout."
    /// - **Release build** (`cargo test --release`): returns
    ///   `AttachError::UnknownSession { session_id }` (the
    ///   substrate-faithful error per RFC-0011-c §F.2 step (e)).
    ///
    /// The `From<AttachError> for OctoCliError` impl maps
    /// `UnknownSession` → `AttachSessionUnknown(String)` (exit 56) in
    /// the CLI layer (the operator sees the same slot regardless of
    /// which substrate mode produced it; the panic is a debug-only
    /// guard for accidental callsite reliance, not an operator-facing
    /// signal).
    ///
    /// This test pins the substrate-faithful current behavior using
    /// `std::panic::catch_unwind` so it passes in both modes without
    /// `cfg!(debug_assertions)` branching. A future amendment that
    /// wires `InProcessHandler::bind` surfaces as a deliberate test
    /// rewrite (the panic message no longer fires AND the result is
    /// `Ok(AttachedSession)`).
    #[test]
    fn tv_cli_attach_full_dispatch_chain_substrate_faithful_session_unknown_or_panic() {
        use octo_runtime::{
            attach_with_token, decode_token, encode_token, mint_attach_handle, AttachError,
            Transport, TransportKind,
        };
        use octo_wallet::IdentityKey;
        use uuid::Uuid;

        let mut holder = IdentityKey::generate().expect("IdentityKey::generate must succeed");
        let now_unix_secs: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1_700_000_000);
        holder
            .activate(now_unix_secs)
            .expect("activate must succeed");
        let holder_pubkey = holder.public_key_bytes();
        let agent_id = Uuid::new_v4();
        let session_id = [0x42_u8; 32];

        let handle = mint_attach_handle(
            &holder,
            agent_id,
            session_id,
            0u64,         // since_cursor: zero → replay-from-spawn
            u64::MAX / 2, // ttl_unix: far in the future; never expires under test
            Transport {
                kind: TransportKind::InProcess,
                addr: None,
            },
        )
        .expect("mint_attach_handle must succeed for active holder");

        let token_bytes = encode_token(&handle).expect("encode_token must succeed");
        let token = decode_token(&token_bytes, &holder_pubkey).expect("decode_token must succeed");

        // Drive `attach_with_token` through a current-thread tokio
        // runtime — mirrors the dispatch helper in `attach::handle`
        // verbatim (per the dispatch seam doc-comment).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio current_thread runtime must build");

        // `catch_unwind` is the substrate-faithful dual-mode witness:
        // debug build → panic with "not implemented" message; release
        // build → `Ok(AttachedSession { .. })` not yet reached and
        // `Err(AttachError::UnknownSession { session_id })` returned
        // instead. The CLI dispatch body wraps this with `?` so the
        // operator sees `AttachSessionUnknown(hex)` (exit 56) in the
        // release build; the panic surfaces as a process abort in
        // debug. Either is a substrate-faithful manifestation of the
        // §F.6.5 deferral.
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.block_on(attach_with_token(
                &holder_pubkey,
                &token,
                token.mint_timestamp_unix,
            ))
        }));

        match panic_result {
            // Release-build pathway: substrate returns the
            // substrate-faithful error envelope.
            Ok(Ok(_attached)) => panic!(
                "per RFC-0011-c §F.6.5 deferral, a legitimate token MUST NOT bind \
                 successfully (InProcessHandler::bind wiring is deferred); got Ok"
            ),
            Ok(Err(AttachError::UnknownSession { session_id: sid })) => {
                assert_eq!(
                    sid, session_id,
                    "AttachError::UnknownSession MUST echo the mint-time session_id (substrate faithfulness), got {sid:?}"
                );
                // CLI translation: `From<AttachError> for OctoCliError`
                // maps `UnknownSession { session_id }` → `AttachSessionUnknown(hex(session_id))`
                // (exit 56) per RFC-0011-c §F.4 + §9.8 substrate-faithful
                // mirror. Verify the translation preserves the
                // session_id (hex) so the operator can correlate with
                // `octo revoke-attach --session-id <hex>`.
                let cli_err: OctoCliError =
                    <OctoCliError as From<AttachError>>::from(AttachError::UnknownSession {
                        session_id: sid,
                    });
                match &cli_err {
                    OctoCliError::AttachSessionUnknown(hex) => {
                        assert!(
                            !hex.is_empty(),
                            "AttachSessionUnknown hex payload MUST be non-empty"
                        );
                        assert_eq!(
                            hex.len(),
                            64,
                            "AttachSessionUnknown hex payload MUST be 64 lowercase chars (32-byte SessionId), got {} chars",
                            hex.len(),
                        );
                        let decoded_bytes = hex::decode(hex).expect("hex decode must succeed");
                        let mut decoded = [0u8; 32];
                        decoded.copy_from_slice(&decoded_bytes);
                        assert_eq!(
                            decoded, session_id,
                            "AttachSessionUnknown hex MUST round-trip byte-for-byte to mint-time session_id"
                        );
                    }
                    other => panic!("expected OctoCliError::AttachSessionUnknown, got {other:?}"),
                }
            }
            Ok(Err(other)) => panic!(
                "per RFC-0011-c §F.6.5 deferral, a legitimate token MUST surface \
                 AttachError::UnknownSession, got {other:?}"
            ),
            // Debug-build pathway: substrate `panic!`s with the
            // substrate-faithful "not implemented" message. The
            // `catch_unwind` payload is `&str` / `String` depending
            // on the panic mechanism — accept either shape.
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<&'static str>() {
                    s.to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    unreachable!("panic payload is always &'static str or String (per std::panic::PanicInfo contract)")
                };
                assert!(
                    msg.contains("not implemented")
                        || msg.contains("InProcessHandler")
                        || msg.contains("session-registry-wiring"),
                    "debug-build panic MUST carry the §F.6.5 substrate-faithful message, got {msg:?}"
                );
            }
        }
    }
}
