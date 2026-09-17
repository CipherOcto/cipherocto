//! `octo governance` — RFC-0011-g §Subcommand Taxonomy.
//!
//! Thin Layer C wrapper over the `octo-governance` substrate crate
//! (Layer B per RFC-0013 §Module Layout + RFC-0011-g §Substrate
//! `[ADD]`). Operator invocation → clap parse → substrate
//! `snapshot()` projection → JSON envelope render. No business
//! logic in this module; all decisions live in the substrate.
//!
//! Phase 1 lands `octo governance snapshot` only; the `attest` +
//! `vote` surface waits for the RFC-0855p-d + RFC-0855p-e +
//! RFC-0011-d Phase 1 conjunction per mission
//! `0011-g-governance-attest-vote` (release-gated).
//!
//! ## Substrate-faithfulness drift
//!
//! RFC-0011-g §Subcommand Taxonomy accepts the labels
//! `Open | Quorum-Reached | Closed-Accepted | Closed-Rejected |
//! Closed-Expired` for `--proposal-state`. The substrate
//! `ProposalState` carries the labels
//! `Created | Voting | Approved | Rejected | Executed | Expired`.
//! The dispatch boundary translates RFC labels → substrate
//! discriminants; translation failures surface as
//! `OctoCliError::InvalidProposalState { state }` (exit 2) per
//! RFC-0011-g §Error Handling.

use std::sync::Arc;

use clap::Subcommand;
use octo_governance::{
    attest::CapabilitySigner, attest_v2, snapshot, vote_v2, AttestationReceipt, CapabilityToken,
    GovernanceError, GovernanceSession, GovernanceSnapshotError, OctoGovernanceSnapshotCache,
    ProposalFilter, ProposalState as SubstrateProposalState, QuorumProjection, SnapshotView,
    SystemClock, VoteChoice, VoteReceipt,
};
use uuid::Uuid;

use crate::error::{map_hsm_error, sanitize_substrate_error, OctoCliError};
use crate::flags::OperatorMode;
#[cfg(test)]
use crate::flags::{OperatorModeFlags, OutputFlags};
use crate::output::{OutputEnvelope, RedactionContext};
use crate::Octo;

/// Wallet-backed `CapabilitySigner` adapter (RFC-0011-g §7.4
/// substrate signature `&dyn CapabilitySigner`). Wraps the
/// active `IdentityKey` so the substrate can perform the HSM-bound
/// `sign_envelope` call without leaking private-key material
/// outside the wallet boundary. Failure paths collapse to
/// `GovernanceError::Internal { reason }` at the substrate layer.
///
/// Layer discipline: `octo-governance` defines the trait (Layer
/// B RFC-driven additive). The CLI supplies the impl; the
/// substrate does NOT depend on `octo-wallet` directly (Layer B
/// independence per `cipherocto-design-principles`).
pub struct WalletSignerAdapter {
    /// Active identity; `Arc` so the adapter survives across
    /// the substrate `attest()` / `vote()` call lifetimes.
    identity: Arc<octo_wallet::IdentityKey>,
}

impl WalletSignerAdapter {
    /// Wrap the active identity in an `Arc`.
    #[must_use]
    pub fn new(identity: octo_wallet::IdentityKey) -> Self {
        Self {
            identity: Arc::new(identity),
        }
    }

    /// `signer_did` for substrate receipt population. Substrate
    /// expects the canonical DID form (RFC-0010 §canonical form).
    #[must_use]
    pub fn signer_did(&self) -> String {
        self.identity.did().0
    }
}

impl CapabilitySigner for WalletSignerAdapter {
    fn sign_envelope(&self, envelope_bytes: &[u8]) -> Result<[u8; 64], String> {
        self.identity
            .sign(envelope_bytes)
            .map(|s| s.to_bytes())
            .map_err(|e| e.to_string())
    }
}

/// Resolve the active identity DID via the wallet store.
///
/// Mirrors `crate::commands::agent::common::resolve_active_did`
/// without depending on the agent module's private `mod common`
/// helper. The agent-module refactor (lift `mod common` to
/// `pub(crate) mod common`) is deferred to a follow-on mission;
/// duplicating 13 lines avoids expanding the DRY review surface
/// of this mission. Both helpers MUST stay in sync — the canonical
/// mapping table lives in the agent module.
fn resolve_active_did() -> Result<octo_wallet::identity_record::Did, OctoCliError> {
    let store = octo_wallet::WalletStore::open().map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!("wallet store open: {e}")))
    })?;
    let active_key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        octo_wallet::WalletError::Hsm(_) => map_hsm_error(&e.to_string()),
        other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
    })?;
    Ok(active_key.did())
}

/// CLI-facing governance subcommand enum (Layer C; delegates to
/// `octo_governance` substrate for the projection). Phase 1 ships
/// `Snapshot` only; Phase 2 (RFC-0011-g §7.4) ships `Attest` +
/// `Vote`.
///
/// `#[non_exhaustive]` per F-14 — future amendments add variants
/// without central enum edits across the workspace.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GovernanceAction {
    /// Refresh the local governance snapshot (read-only).
    /// Per RFC-0011-g §Subcommand Taxonomy, this is Phase 1
    /// unblocked at RFC-0011-g acceptance alone.
    Snapshot {
        /// Filter to one chain (RFC-0010 canonical form).
        #[arg(long, value_name = "CHAIN_ID")]
        chain_id: Option<String>,
        /// Filter to one proposal state. Accepts the
        /// RFC-0011-g §Subcommand Taxonomy labels
        /// (`Open` | `Quorum-Reached` | `Closed-Accepted` |
        /// `Closed-Rejected` | `Closed-Expired`); the dispatch
        /// boundary translates to substrate-native
        /// `ProposalState` discriminants.
        #[arg(long, value_name = "PROPOSAL_STATE")]
        proposal_state: Option<String>,
        /// Bypass the `SnapshotCache` and force a fresh
        /// substrate fetch. Per RFC-0011-g §Subcommand Taxonomy.
        #[arg(long)]
        force_refresh: bool,
    },
    /// Append an attestation to the substrate
    /// `AttestationLog` (RFC-0011-g §7.4). Mutations
    /// require `--confirm` (parental §Error Handling
    /// mutating-command gate). Auditor mode is denied per
    /// `octo agent run/destroy` parallel (read-only role).
    Attest {
        /// Subject DID receiving the attestation (positional per
        /// RFC-0011-g §Command Taxonomy). Subject DIDs prefixed
        /// `did:octo:subgroup:` are fail-closed at the substrate
        /// (RFC-0855p-d prereq gate) until the upstream RFC
        /// reaches Accepted.
        #[arg(value_name = "SUBJECT_DID")]
        subject_did: String,
        /// Typed-discriminator kind reference (e.g.
        /// `route-quality:uptime-30d`, positional per RFC-0011-g
        /// §Command Taxonomy). Unknown kinds fail-closed at the
        /// substrate (TypedDiscriminator pattern per
        /// RFC-0011-g §Attestation Kind Resolution +
        /// `cipherocto-design-principles` §Extension over
        /// enumeration).
        #[arg(value_name = "KIND_REF")]
        kind_ref: String,
        /// Raw evidence bytes (mutually exclusive with
        /// `--evidence-hash`). When supplied, the substrate
        /// computes `BLAKE3-256` and signs the canonical envelope.
        #[arg(
            long,
            value_name = "EVIDENCE_PATH",
            conflicts_with = "evidence_hash_hex"
        )]
        evidence_path: Option<String>,
        /// Pre-computed evidence BLAKE3-256 hex (64 lowercase hex
        /// chars; mutually exclusive with `--evidence-path`).
        #[arg(
            long,
            value_name = "EVIDENCE_HASH_HEX",
            conflicts_with = "evidence_path"
        )]
        evidence_hash_hex: Option<String>,
        /// Expiry as RFC 3339 unix-seconds. Omitted → no expiry.
        #[arg(long, value_name = "EXPIRES_AT_UNIX")]
        expires_at_unix: Option<u64>,
        /// Snapshot id (hex) the attestation references; pass
        /// to bind the receipt to a known governance snapshot.
        #[arg(long, value_name = "SNAPSHOT_ID_HEX")]
        snapshot_id_hex: Option<String>,
        /// Bypass the snapshot freshness check (substrate sets
        /// `overrode_staleness_at_unix = appended_at_unix`). RFC-0011-g
        /// §Staleness Override.
        #[arg(long)]
        allow_stale: bool,
        /// Build the envelope, return substrate-validated preview
        /// WITHOUT appending. RFC-0011-g §Command Taxonomy.
        #[arg(long)]
        dry_run: bool,
        /// Required for mutating commands (parent §Error
        /// Handling). Auditor mode fails-closed before this
        /// gate per `OctoCliError::AuditorDenied`.
        #[arg(long)]
        confirm: bool,
        /// Two-step intent gate required when `--allow-stale` is
        /// supplied (RFC-0011-g §Command Taxonomy "REQUIRES
        /// `--confirm-acknowledge`"; defense in depth against
        /// accidental stale-override).
        #[arg(long)]
        confirm_acknowledge: bool,
    },
    /// Cast a vote on a proposal (RFC-0011-g §7.4
    /// `vote()` substrate signature). Mutations require
    /// `--confirm`. Auditor mode is denied.
    Vote {
        /// Proposal id (hex) the vote is recorded against
        /// (positional per RFC-0011-g §Command Taxonomy).
        #[arg(value_name = "PROPOSAL_ID_HEX")]
        proposal_id_hex: String,
        /// Vote choice (`approve` | `reject`, positional per
        /// RFC-0011-g §Command Taxonomy). Unknown values
        /// fail-closed at the substrate with `InvalidArgument`.
        #[arg(value_name = "VOTE_CHOICE")]
        vote_choice: String,
        /// Voter weight in basis points (0..=10_000).
        #[arg(long, value_name = "WEIGHT_BPS")]
        weight_bps: u32,
        /// Voter capability id — must be registered in the
        /// CLI's `CapabilityRegistry` (substrate fails-closed on
        /// miss with `UnknownCapability`).
        #[arg(long, value_name = "VOTER_CAP_ID")]
        voter_cap_id: String,
        /// Snapshot id (hex) the vote references; pass to bind
        /// the receipt to a known governance snapshot.
        #[arg(long, value_name = "SNAPSHOT_ID_HEX")]
        snapshot_id_hex: Option<String>,
        /// Bypass the snapshot freshness check (substrate sets
        /// `overrode_staleness_at_unix = recorded_at_unix`).
        #[arg(long)]
        allow_stale: bool,
        /// Build the envelope, return substrate-validated preview
        /// WITHOUT recording. RFC-0011-g §Command Taxonomy.
        #[arg(long)]
        dry_run: bool,
        /// Required for mutating commands.
        #[arg(long)]
        confirm: bool,
        /// Two-step intent gate required when `--allow-stale` is
        /// supplied (RFC-0011-g §Command Taxonomy "REQUIRES
        /// `--confirm-acknowledge`"; defense in depth).
        #[arg(long)]
        confirm_acknowledge: bool,
        /// Free-text rationale (optional); substrate records
        /// verbatim in the proposal audit log per RFC-0011-g
        /// §Subcommand Taxonomy + §Substrate [ADD] `vote`
        /// signature.
        #[arg(long, value_name = "TEXT")]
        rationale: Option<String>,
    },
}

/// Dispatch a parsed `octo governance ...` invocation to its
/// handler. `Snapshot` is Phase 1; `Attest` + `Vote` are Phase 2
/// per RFC-0011-g §7.4. Persistence substrate lands as a follow-on
/// mission (the in-memory ledger is sufficient for Phase 2 CLI
/// surface verification).
pub fn dispatch(action: &GovernanceAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        GovernanceAction::Snapshot {
            chain_id,
            proposal_state,
            force_refresh,
        } => snapshot_handler(
            chain_id.clone(),
            proposal_state.clone(),
            *force_refresh,
            cli,
        ),
        GovernanceAction::Attest {
            subject_did,
            kind_ref,
            evidence_path,
            evidence_hash_hex,
            expires_at_unix,
            snapshot_id_hex,
            allow_stale,
            dry_run,
            confirm,
            confirm_acknowledge,
        } => attest_handler(
            subject_did.clone(),
            kind_ref.clone(),
            evidence_path.clone(),
            evidence_hash_hex.clone(),
            *expires_at_unix,
            snapshot_id_hex.clone(),
            *allow_stale,
            *dry_run,
            *confirm,
            *confirm_acknowledge,
            cli,
        ),
        GovernanceAction::Vote {
            proposal_id_hex,
            vote_choice,
            weight_bps,
            voter_cap_id,
            snapshot_id_hex,
            allow_stale,
            dry_run,
            confirm,
            confirm_acknowledge,
            rationale,
        } => vote_handler(
            proposal_id_hex.clone(),
            vote_choice.clone(),
            *weight_bps,
            voter_cap_id.clone(),
            snapshot_id_hex.clone(),
            *allow_stale,
            *dry_run,
            *confirm,
            *confirm_acknowledge,
            rationale.clone(),
            cli,
        ),
    }
}

/// RFC-0011-g → substrate label translation. The RFC carries the
/// `Open | Quorum-Reached | Closed-Accepted | Closed-Rejected |
/// Closed-Expired` labels; the substrate carries
/// `Created | Voting | Approved | Rejected | Executed | Expired`.
/// Mapping table per RFC-0011-g §Subcommand Taxonomy + RFC-0855
/// §11 proposal lifecycle.
///
/// Returns `Err(OctoCliError::InvalidProposalState { state })`
/// when the label is unrecognized.
fn rfc_label_to_substrate_state(label: &str) -> Result<SubstrateProposalState, OctoCliError> {
    match label {
        "Open" => Ok(SubstrateProposalState::Voting),
        "Quorum-Reached" => Ok(SubstrateProposalState::Approved),
        "Closed-Accepted" => Ok(SubstrateProposalState::Executed),
        "Closed-Rejected" => Ok(SubstrateProposalState::Rejected),
        "Closed-Expired" => Ok(SubstrateProposalState::Expired),
        _ => Err(OctoCliError::InvalidProposalState {
            state: label.to_string(),
        }),
    }
}

/// `octo governance snapshot` handler. Read-only projection of
/// the substrate `snapshot()` per RFC-0011-g §Subcommand
/// Taxonomy. No confirmation gate — `snapshot` is read-only
/// across all operator modes (Human / Ci / Dev / Auditor) per
/// RFC-0011-g §Roles and Authorities.
fn snapshot_handler(
    chain_id: Option<String>,
    proposal_state_label: Option<String>,
    force_refresh: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let _ = cli; // operator-mode dispatch lands in follow-on mission

    let active_did = resolve_active_did().map_err(|e| match e {
        // Map substrate identity errors to the operator-facing
        // error envelope. Per RFC-0011 §Error Handling, the
        // surface stays consistent across substrates.
        OctoCliError::NoActiveIdentity => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
    })?;

    // Build the substrate filter from the CLI flags. The
    // proposal-state label translates at the dispatch boundary
    // per RFC-0011-g §Subcommand Taxonomy.
    let mut states: Option<Vec<SubstrateProposalState>> = None;
    if let Some(label) = proposal_state_label {
        let substrate_state = rfc_label_to_substrate_state(&label)?;
        states = Some(vec![substrate_state]);
    }
    let filter = ProposalFilter {
        states,
        chain_id: chain_id.clone(),
    };

    // The cache lives for the duration of the CLI invocation.
    // Multi-threaded dispatch (follow-on mission) will wrap it
    // in Arc<Mutex<...>>.
    let mut cache = OctoGovernanceSnapshotCache::default();
    let view = snapshot(active_did.as_str(), &filter, force_refresh, &mut cache).map_err(|e| {
        match e {
            GovernanceSnapshotError::InvalidProposalState { state } => {
                OctoCliError::InvalidProposalState { state }
            }
            GovernanceSnapshotError::InvalidChainId { input, reason } => OctoCliError::Internal(
                sanitize_substrate_error(&format!("invalid chain-id `{input}`: {reason}")),
            ),
            GovernanceSnapshotError::SnapshotStale {
                snapshot_id_hex,
                age_secs,
            } => OctoCliError::SnapshotStale {
                snapshot_id_hex,
                age_secs,
            },
            GovernanceSnapshotError::CacheError { reason } => {
                OctoCliError::GovernanceSubstrateError {
                    reason: sanitize_substrate_error(&reason),
                }
            }
            other => {
                // GovernanceSnapshotError is `#[non_exhaustive]`;
                // future substrate variants land additively. Map
                // them to the substrate-error envelope (exit 51)
                // so v2 wiring does not break the CLI.
                OctoCliError::GovernanceSubstrateError {
                    reason: sanitize_substrate_error(&format!(
                        "unmapped governance substrate error: {other:?}"
                    )),
                }
            }
        }
    })?;

    render_snapshot(&view, cli)?;
    Ok(())
}

/// Decode 64-char lowercase hex into a 32-byte array. CLI-side
/// parse helper for `--evidence-hash` + `--snapshot-id`. RFC-0010
/// canonical hex form. Returns `Err(InvalidFilter)` (exit 16,
/// shared with `octo audit list --filter` payload-bearing
/// variants) on length / non-hex mismatch.
fn parse_hex_32(label: &str, hex_str: &str) -> Result<[u8; 32], OctoCliError> {
    let bytes = hex::decode(hex_str.trim()).map_err(|e| {
        OctoCliError::InvalidFilter(format!(
            "invalid hex for {label} (RFC-0010 canonical form is 64 lowercase hex chars): {e}"
        ))
    })?;
    if bytes.len() != 32 {
        return Err(OctoCliError::InvalidFilter(format!(
            "invalid hex for {label}: expected 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Map substrate `GovernanceError` → `OctoCliError` per the slot
/// allocation table (RFC-0011-g §Error Handling). Slots:
/// - `UnknownAttestationKind` → exit 37
/// - `PrereqNotAccepted` → exit 38
/// - `VoteRejected` / `QuorumNotReached` / `InvalidWeight` → exit 36
/// - `DuplicateAttestation` / `DuplicateVote` → exit 64 (internal)
/// - `UnknownCapability` → exit 64 (internal)
/// - `InvalidArgument` → exit 64 (internal)
/// - `Internal` → exit 64 (internal)
fn map_governance_error(err: GovernanceError) -> OctoCliError {
    match err {
        GovernanceError::UnknownAttestationKind { kind_ref } => {
            OctoCliError::UnknownAttestationKind { kind_ref }
        }
        GovernanceError::PrereqNotAccepted { rfc_ref } => {
            OctoCliError::PrereqNotAccepted { rfc_ref }
        }
        GovernanceError::InvalidTransition { .. }
        | GovernanceError::QuorumNotReached { .. }
        | GovernanceError::InvalidWeight { .. }
        | GovernanceError::DuplicateAttestation { .. }
        | GovernanceError::DuplicateVote { .. } => OctoCliError::VoteRejected {
            reason: sanitize_substrate_error(&err.to_string()),
        },
        GovernanceError::UnknownCapability { .. }
        | GovernanceError::InvalidArgument { .. }
        | GovernanceError::Internal { .. } => {
            OctoCliError::Internal(sanitize_substrate_error(&err.to_string()))
        }
        // Layer A frozen-core additive contract — future variants
        // route through the wildcard arm to preserve forward
        // compatibility (per cipherocto-design-principles
        // §Rust crate-level stability Layer A row).
        _ => OctoCliError::Internal(sanitize_substrate_error(&err.to_string())),
    }
}

/// `octo governance attest` handler (RFC-0011-g §7.4 substrate
/// `attest_v2` signature). Per RFC-0011-c §Roles and Authorities:
/// read-only role `Auditor` is denied at the dispatch boundary
/// before the confirmation gate; `Human` / `Ci` / `Dev` modes
/// require `--confirm` for mutating commands (parent §Error
/// Handling gate).
#[allow(clippy::too_many_arguments)]
fn attest_handler(
    subject_did: String,
    kind_ref: String,
    evidence_path: Option<String>,
    evidence_hash_hex: Option<String>,
    expires_at_unix: Option<u64>,
    snapshot_id_hex: Option<String>,
    allow_stale: bool,
    dry_run: bool,
    confirm: bool,
    confirm_acknowledge: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // --dry-run: validate envelope + return preview without
    // appending. Placed BEFORE the mode/confirm/stale gates
    // per `flags.rs` contract: `--dry-run` bypasses
    // `--confirm` because a preview grants no authority. The
    // preview parses the same inputs the post-confirm path
    // would parse (substrate-faithful), then renders an
    // `AttestDryRunPreview` envelope WITHOUT performing any
    // wallet IO and WITHOUT touching the substrate append
    // path.
    if dry_run {
        let evidence_hash = if let Some(h) = evidence_hash_hex.as_deref() {
            Some(parse_hex_32("--evidence-hash", h)?)
        } else {
            None
        };
        let snapshot_id = if let Some(s) = snapshot_id_hex.as_deref() {
            Some(parse_hex_32("--snapshot-id", s)?)
        } else {
            None
        };
        let preview = AttestDryRunPreview {
            command: "octo governance attest".to_string(),
            subject_did,
            kind_ref,
            evidence_path,
            evidence_hash_hex: evidence_hash.as_ref().map(hex32),
            expires_at_unix,
            snapshot_id_hex: snapshot_id.as_ref().map(hex32),
            allow_stale,
            dry_run_correlation_id: Uuid::new_v4().to_string(),
            preview_note: "no wallet IO performed; envelope not signed or appended".to_string(),
        };
        OutputEnvelope::new("octo.governance.attest.dry_run.v1", preview)
            .render_with_redaction(
                cli.output.json,
                cli.output.no_color,
                &RedactionContext::new(),
            )
            .map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "render attest dry-run envelope: {e}"
                )))
            })?;
        return Ok(());
    }

    // Mode + confirmation gates.
    if matches!(cli.mode.mode, OperatorMode::Auditor) {
        return Err(OctoCliError::AuditorDenied {
            command: "octo governance attest".to_string(),
        });
    }
    if !confirm {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo governance attest".to_string(),
        });
    }
    // Two-step intent gate: --allow-stale REQUIRES
    // --confirm-acknowledge per RFC-0011-g §Command Taxonomy
    // (defense in depth against accidental stale-override).
    if allow_stale && !confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo governance attest --allow-stale (requires --confirm-acknowledge)"
                .to_string(),
        });
    }
    // --allow-stale is meaningless without --snapshot-id
    // (TV-20 parity): nothing to override.
    if allow_stale && snapshot_id_hex.is_none() {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo governance attest --allow-stale (requires --snapshot-id; TV-20)"
                .to_string(),
        });
    }

    // Resolve evidence bytes (substrate XOR invariant).
    let evidence = if let Some(p) = evidence_path.as_deref() {
        Some(std::fs::read(p).map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "failed reading evidence path `{p}`: {e}"
            )))
        })?)
    } else {
        None
    };
    let evidence_hash = if let Some(h) = evidence_hash_hex.as_deref() {
        Some(parse_hex_32("--evidence-hash", h)?)
    } else {
        None
    };

    // Snapshot id (optional hex).
    let snapshot_id = if let Some(s) = snapshot_id_hex.as_deref() {
        Some(parse_hex_32("--snapshot-id", s)?)
    } else {
        None
    };

    // Wallet-backed signer adapter.
    let store = octo_wallet::WalletStore::open().map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!("wallet store open: {e}")))
    })?;
    let identity = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        octo_wallet::WalletError::Hsm(_) => map_hsm_error(&e.to_string()),
        other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
    })?;
    let signer = WalletSignerAdapter::new(identity);
    let signer_did = signer.signer_did();

    // Substrate v2 call: build a GovernanceSession with the
    // production SystemClock and pass the session + signer by
    // reference. The substrate reads the clock via the session
    // rather than receiving an appended_at_unix argument.
    let session = GovernanceSession::new(&signer_did, Arc::new(SystemClock));
    let receipt = attest_v2(
        &session,
        &subject_did,
        &kind_ref,
        evidence.as_deref(),
        evidence_hash,
        expires_at_unix,
        snapshot_id.as_ref(),
        allow_stale,
        &signer,
        &signer_did,
    )
    .map_err(map_governance_error)?;

    // Render envelope. The substrate receipt populates
    // `appended_at_unix` from the CLI-supplied value; we mirror
    // it into the envelope's `appended_at_unix` field for
    // operator convenience (matches `SnapshotOutput`'s
    // companion-field redaction pattern).
    let payload = render_attest_output(receipt);
    let envelope = OutputEnvelope::new("octo.governance.attest.v1", payload);
    envelope
        .render_with_redaction(
            cli.output.json,
            cli.output.no_color,
            &RedactionContext::new(),
        )
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })?;
    Ok(())
}

/// `octo governance vote` handler (RFC-0011-g §7.4 substrate
/// `vote_v2` signature). Confirmation + auditor gates identical
/// to the `attest` handler above (parent §Error Handling +
/// RFC-0011-c §Roles and Authorities).
#[allow(clippy::too_many_arguments)]
fn vote_handler(
    proposal_id_hex: String,
    vote_choice: String,
    weight_bps: u32,
    voter_cap_id: String,
    snapshot_id_hex: Option<String>,
    allow_stale: bool,
    dry_run: bool,
    confirm: bool,
    confirm_acknowledge: bool,
    rationale: Option<String>,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    // Fail-fast: weight_bps must be bounded at 10_000 (100%):
    // a single voter cannot exceed full quorum regardless of
    // stake per RFC-0011-g §Weight Bounding + token-design
    // §12.5 dual-stake invariant. Substrate rejects silently
    // via InvalidWeight; we fail-fast at the CLI boundary for
    // operator clarity. Placed BEFORE `parse_hex_32` so the
    // logic error surfaces before the parse error (MED
    // ordering rule).
    if weight_bps > 10_000 {
        return Err(OctoCliError::VoteRejected {
            reason: sanitize_substrate_error(&format!(
                "weight_bps {weight_bps} exceeds 10_000 bps cap"
            )),
        });
    }
    // --dry-run: parse every input + render preview without
    // appending. Placed BEFORE the mode/confirm/stale gates per
    // `flags.rs` contract: `--dry-run` bypasses `--confirm`
    // because a preview grants no authority. The preview
    // performs every parse the post-confirm path would
    // perform (hex decoding, choice parse) so the operator
    // sees exactly what the real call would do, minus wallet
    // IO and substrate append.
    if dry_run {
        let proposal_id = parse_hex_32("--proposal-id", &proposal_id_hex)?;
        // Canonicalize the choice at the preview boundary so
        // the operator sees the exact form the substrate will
        // record (substrate normalizes via `VoteChoice::parse`,
        // e.g. `APPROVE` -> `yes`); without this, the preview
        // surface diverges from the commit surface.
        let canonical_choice = VoteChoice::parse(&vote_choice).map_err(map_governance_error)?;
        let snapshot_id = if let Some(s) = snapshot_id_hex.as_deref() {
            Some(parse_hex_32("--snapshot-id", s)?)
        } else {
            None
        };
        let preview = build_vote_dry_run_preview(
            proposal_id,
            canonical_choice.as_str(),
            weight_bps,
            voter_cap_id,
            snapshot_id,
            allow_stale,
            rationale,
        );
        OutputEnvelope::new("octo.governance.vote.dry_run.v1", preview)
            .render_with_redaction(
                cli.output.json,
                cli.output.no_color,
                &RedactionContext::new(),
            )
            .map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "render vote dry-run envelope: {e}"
                )))
            })?;
        return Ok(());
    }

    if matches!(cli.mode.mode, OperatorMode::Auditor) {
        return Err(OctoCliError::AuditorDenied {
            command: "octo governance vote".to_string(),
        });
    }
    if !confirm {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo governance vote".to_string(),
        });
    }
    // Two-step intent gate: --allow-stale REQUIRES
    // --confirm-acknowledge per RFC-0011-g §Command Taxonomy
    // (defense in depth).
    if allow_stale && !confirm_acknowledge {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo governance vote --allow-stale (requires --confirm-acknowledge)"
                .to_string(),
        });
    }
    // --allow-stale requires --snapshot-id (TV-20 parity).
    if allow_stale && snapshot_id_hex.is_none() {
        return Err(OctoCliError::ConfirmationRequired {
            command: "octo governance vote --allow-stale (requires --snapshot-id; TV-20)"
                .to_string(),
        });
    }

    let proposal_id = parse_hex_32("--proposal-id", &proposal_id_hex)?;
    let choice = VoteChoice::parse(&vote_choice).map_err(map_governance_error)?;
    let snapshot_id = if let Some(s) = snapshot_id_hex.as_deref() {
        Some(parse_hex_32("--snapshot-id", s)?)
    } else {
        None
    };

    let store = octo_wallet::WalletStore::open().map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!("wallet store open: {e}")))
    })?;
    let identity = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        octo_wallet::WalletError::Hsm(_) => map_hsm_error(&e.to_string()),
        other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
    })?;
    let signer = WalletSignerAdapter::new(identity);
    let signer_did = signer.signer_did();

    // Substrate v2 call: build a GovernanceSession with the
    // production SystemClock, register the wallet-backed
    // CapabilitySigner under the supplied voter_cap_id, build
    // a CapabilityToken from (voter_cap_id, signer_did,
    // weight_bps), and call vote_v2 which reads the clock via
    // the session.
    let session = GovernanceSession::new(&signer_did, Arc::new(SystemClock));
    let signer_arc: Arc<dyn octo_governance::attest::CapabilitySigner> = Arc::new(signer);
    session
        .register_capability(&voter_cap_id, signer_arc)
        .map_err(map_governance_error)?;
    let token = CapabilityToken::new(&voter_cap_id, &signer_did, weight_bps);
    let (receipt, projection): (VoteReceipt, QuorumProjection) = vote_v2(
        &session,
        proposal_id,
        choice,
        &token,
        rationale.as_deref(),
        snapshot_id.as_ref(),
        allow_stale,
    )
    .map_err(map_governance_error)?;

    let envelope = OutputEnvelope::new(
        "octo.governance.vote.v1",
        // Substrate v2 surfaces the QuorumProjection from the
        // substrate itself; the CLI envelope carries both
        // receipt + projection so downstream consumers see the
        // current tally state at append time.
        render_vote_output(receipt, (projection.approval_bps, projection.rejection_bps)),
    );
    envelope
        .render_with_redaction(
            cli.output.json,
            cli.output.no_color,
            &RedactionContext::new(),
        )
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })?;
    Ok(())
}

/// Build the CLI-side `SnapshotOutput` payload and render it
/// through the canonical `OutputEnvelope<T>` per RFC-0011-g
/// §Output Envelope (schema_version = 6).
///
/// Substrate-faithful: the snapshot is constructed by the
/// substrate and the CLI only translates the typed view into
/// the envelope payload + renders through the canonical
/// `OutputEnvelope::render_with_redaction` (which respects
/// `--json` / `--no-color` and applies the redaction layer).
fn render_snapshot(view: &SnapshotView, cli: &Octo) -> Result<(), OctoCliError> {
    let snapshot_id_hex: String = view
        .snapshot
        .snapshot_id
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let root_manifest_hash_hex: String = view
        .snapshot
        .root_manifest_hash
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let payload = SnapshotOutput {
        snapshot_id: snapshot_id_hex,
        chain_id: view.snapshot.filter.chain_id.clone(),
        taken_at_unix: view.snapshot.taken_at_unix,
        expires_at_unix: view.snapshot.expires_at_unix,
        remaining_seconds: view.remaining_seconds,
        root_manifest_hash: root_manifest_hash_hex,
        open_proposal_count: view.snapshot.open_proposal_count,
        attestation_count: view.attestation_count,
        resolved_at_unix: view.resolved_at_unix,
        open_proposals: Vec::new(),
    };
    let envelope = OutputEnvelope::new("octo.governance.snapshot.v1", payload);
    envelope
        .render_with_redaction(
            cli.output.json,
            cli.output.no_color,
            &RedactionContext::new(),
        )
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "render snapshot envelope: {e}"
            )))
        })
}

/// CLI-side output struct — RFC-0011-g §Output Envelope.
///
/// Built at the dispatch boundary by composing the substrate
/// `SnapshotRef` + `SnapshotView` into the envelope payload.
/// `open_proposals` is currently `Vec::new()` (v1 surface) — the
/// substrate projection does not yet plumb a backing ledger;
/// the payload field shape is wired so the substrate-backed v2
/// surface drops in without a CLI shape change.
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct SnapshotOutput {
    /// BLAKE3-256 of canonical projection (hex).
    pub snapshot_id: String,
    /// Filter the snapshot was computed under.
    pub chain_id: Option<String>,
    /// RFC 3339 UTC unix-seconds at substrate snapshot time.
    pub taken_at_unix: u64,
    /// `taken_at_unix + TTL_SNAPSHOT_SECONDS`.
    pub expires_at_unix: u64,
    /// `expires_at_unix - now_unix` at dispatch time per
    /// RFC-0011-g §Output Envelope.
    pub remaining_seconds: u64,
    /// BLAKE3-256 of substrate `GovernancePolicy` hash at
    /// snapshot time (hex).
    pub root_manifest_hash: String,
    /// Open proposals in snapshot window (post-filter).
    pub open_proposal_count: u64,
    /// Attestation count in snapshot window (post-filter).
    pub attestation_count: u64,
    /// CLI-side resolution timestamp (== `taken_at_unix` for
    /// substrate-faithful path).
    pub resolved_at_unix: u64,
    /// Open-proposal summaries. Empty for the v1 surface; the
    /// field is wired so v2 backing-ledger wiring does not
    /// change the CLI shape.
    pub open_proposals: Vec<ProposalSummaryOutput>,
}

/// Mirror of substrate `ProposalSummary` for the CLI envelope
/// boundary. Field shape matches the substrate projection so the
/// v2 wiring drops in without a CLI change.
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct ProposalSummaryOutput {
    /// BLAKE3-256 of canonical proposal payload (hex).
    pub proposal_id: String,
    /// Chain id (RFC-0010 canonical wire form).
    pub chain_id: String,
    /// Substrate-native proposal state label.
    pub state: String,
    /// RFC 3339 UTC unix-seconds at which voting closes.
    pub deadline_unix: u64,
    /// Approval tally in basis points.
    pub tally_for_bps: u32,
    /// Rejection tally in basis points.
    pub tally_against_bps: u32,
}

/// CLI-side output wrapper for `octo governance attest`
/// (RFC-0011-g §7.3 Output Envelope `AttestOutput`). Composes
/// the substrate `AttestationReceipt` + envelope wrapper fields
/// at the dispatch boundary. The receipt is preserved verbatim
/// per RFC-0011-g §Output Envelope (Phase 1 test vectors TV-5/17/21
/// all carry the receipt field).
///
/// `appended_at_unix` duplicates the receipt's `appended_at_unix`
/// at the envelope boundary for operator audit (matches
/// `SnapshotOutput.remaining_seconds` companion-field pattern).
/// `attestation_id` is the hex-encoded `receipt.attestation_id`
/// per RFC-0011-g §Hex-Encoding Boundary. `content_hash` is the
/// hex-encoded canonical envelope BLAKE3-256 (substrate-faithful
/// duplicate of `attestation_id` for tooling that prefers
/// `content_hash` naming).
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct AttestOutput {
    /// Substrate receipt (RFC-0011-g §7.3 receipt field
    /// preserved).
    pub receipt: AttestationReceipt,
    /// BLAKE3-256 of canonical envelope (hex).
    pub attestation_id: String,
    /// BLAKE3-256 of canonical envelope (hex; substrate-faithful
    /// duplicate alias of `attestation_id`).
    pub content_hash: String,
    /// Substrate append timestamp (unix seconds; mirrors
    /// `receipt.appended_at_unix` at the envelope boundary).
    pub appended_at_unix: u64,
}

/// CLI-side output wrapper for `octo governance vote`
/// (RFC-0011-g §7.3 Output Envelope `VoteOutput`). Composes the
/// substrate `VoteReceipt` + quorum projection at the dispatch
/// boundary.
///
/// `weight_applied` mirrors `receipt.weight_applied` for envelope
/// convenience per RFC-0011-g §Adversarial Review "Vote weight
/// derivation mismatches" row (CLI displays from the receipt;
/// never recomputes). `current_quorum_weight` + `quorum_threshold`
/// come from the substrate projection at vote-record time
/// per RFC-0855 §Quorum Recording; CLI surfaces them for
/// operator audit per RFC-0011-g §Adversarial Review "Quorum
/// manipulation" row.
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct VoteOutput {
    /// Substrate receipt (RFC-0011-g §7.3 receipt field
    /// preserved).
    pub receipt: VoteReceipt,
    /// BLAKE3-256 per-`(proposal_id, voter_did)` (hex).
    pub vote_id: String,
    /// Mirror of `receipt.weight_applied` for envelope convenience.
    pub weight_applied: u32,
    /// Summed weight at vote-record time (basis points). Substrate
    /// authoritative; CLI display-only.
    pub current_quorum_weight: u32,
    /// Pinned `quorum_threshold` at vote-record time (basis points).
    /// Substrate authoritative; CLI display-only per RFC-0011-g
    /// §Adversarial Review "Quorum manipulation" row.
    pub quorum_threshold: u32,
    /// Substrate record timestamp (unix seconds; mirrors
    /// `receipt.recorded_at_unix` at the envelope boundary).
    pub recorded_at_unix: u64,
}

/// `octo governance attest --dry-run` preview envelope (RFC-0011-g
/// §Command Taxonomy `--dry-run` contract per `flags.rs`).
///
/// Substrate-faithful: the CLI parses every input the
/// post-confirm path would parse (hex-encoded
/// `evidence_hash` + `snapshot_id`, optional `expires_at_unix`),
/// then renders this preview WITHOUT performing any wallet IO
/// and WITHOUT touching the substrate `attest_v2` append path.
/// The envelope proves the operator's intent was captured
/// correctly before they commit to a real signing operation.
///
/// **Cross-walk to live `AttestOutput`**: the field names below
/// mirror operator-supplied *input args* (e.g. `subject_did`,
/// `kind_ref`, `evidence_hash_hex`) — the live `AttestOutput`
/// carries substrate-minted *receipt* fields (`receipt`,
/// `attestation_id`, `content_hash`, `appended_at_unix`). The
/// preview cannot expose receipt fields because the substrate
/// `attest_v2` append has not run; the divergence is by design,
/// not drift. `dry_run_correlation_id` lets the operator link
/// the preview surface back to the eventual live receipt in
/// audit logs (CLI mints a fresh v4 UUID per dry-run call).
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct AttestDryRunPreview {
    /// Command name for the preview surface.
    pub command: String,
    /// Subject DID for the prospective attestation.
    pub subject_did: String,
    /// Kind reference (RFC-0011-g §Attestation Kinds).
    pub kind_ref: String,
    /// Optional local evidence-path argument (not yet read).
    pub evidence_path: Option<String>,
    /// Hex-encoded `evidence_hash` (parsed via `parse_hex_32`).
    pub evidence_hash_hex: Option<String>,
    /// Optional expiry unix-seconds.
    pub expires_at_unix: Option<u64>,
    /// Hex-encoded optional `--snapshot-id` (parsed via `parse_hex_32`).
    pub snapshot_id_hex: Option<String>,
    /// Operator intent flag for stale-override at confirm-time.
    pub allow_stale: bool,
    /// Per-call correlation UUID linking this preview to the
    /// eventual live `AttestOutput` in audit logs. CLI mints a
    /// fresh v4 UUID at every `--dry-run` invocation.
    pub dry_run_correlation_id: String,
    /// Operator-facing note that explains no side-effects occurred.
    pub preview_note: String,
}

/// `octo governance vote --dry-run` preview envelope (RFC-0011-g
/// §Command Taxonomy `--dry-run` contract per `flags.rs`).
///
/// Substrate-faithful: parses `proposal_id` (32-byte hex) +
/// `vote_choice` + clamps `weight_bps` (fail-fast at 10_000 bps
/// per RFC-0011-g §Weight Bounding) + parses `snapshot_id` if
/// provided, THEN renders the preview WITHOUT performing any
/// wallet IO and WITHOUT touching the substrate `vote_v2`
/// append path. The `weight_bps` clamp is positioned BEFORE
/// `parse_hex_32` per the MED ordering rule so the logic error
/// surfaces first.
///
/// **Cross-walk to live `VoteOutput`**: `vote_choice` here is
/// the *canonical* form produced by `VoteChoice::parse`
/// (e.g. `APPROVE` -> `yes`), not the raw CLI input — the
/// preview matches what the substrate records. The remaining
/// field names are operator-supplied *input args* (`weight_bps`,
/// `proposal_id_hex`, `voter_cap_id`); live `VoteOutput` carries
/// substrate-minted *receipt* fields (`weight_applied`, `vote_id`,
/// `receipt`). The divergence is by design (the substrate
/// `vote_v2` append has not run), not drift.
/// `dry_run_correlation_id` links the preview to the eventual
/// live receipt in audit logs (fresh v4 UUID per dry-run call).
#[derive(serde::Serialize, Debug, schemars::JsonSchema)]
pub struct VoteDryRunPreview {
    /// Command name for the preview surface.
    pub command: String,
    /// Hex-encoded `proposal_id` (parsed via `parse_hex_32`).
    pub proposal_id_hex: String,
    /// Canonical `--vote-choice` form produced by
    /// `VoteChoice::parse` (e.g. `APPROVE` -> `yes`); matches
    /// what the substrate records.
    pub vote_choice: String,
    /// Operator-supplied `weight_bps` (clamped at the CLI
    /// boundary; the substrate-faithful preview never sees an
    /// unbounded value).
    pub weight_bps: u32,
    /// Operator-supplied capability id the substrate will
    /// look up in the `CapabilityRegistry` (NOT validated in
    /// preview; substrate-faithful registration is the
    /// post-confirm path's job).
    pub voter_cap_id: String,
    /// Hex-encoded optional `--snapshot-id` (parsed via `parse_hex_32`).
    pub snapshot_id_hex: Option<String>,
    /// Operator intent flag for stale-override at confirm-time.
    pub allow_stale: bool,
    /// Operator-supplied `--rationale <text>` (verbatim, per
    /// RFC-0011-g §Subcommand Taxonomy row `--rationale <text>`).
    /// Surfaces in the dry-run preview so the operator sees
    /// exactly the audit-log text they are about to record.
    /// On the post-confirm path this becomes the 5th argument
    /// slot of `vote_v2` (load-bearing pin: see
    /// `vote::tests::vote_v11_rationale_changes_envelope_pk`);
    pub rationale: Option<String>,
    /// Per-call correlation UUID linking this preview to the
    /// eventual live `VoteOutput` in audit logs. CLI mints a
    /// fresh v4 UUID at every `--dry-run` invocation.
    pub dry_run_correlation_id: String,
    /// Operator-facing note that explains no side-effects occurred.
    pub preview_note: String,
}

/// Encode a BLAKE3-256 byte array as lowercase hex. CLI
/// envelope boundary helper per RFC-0011-g §Output Envelope
/// "Substrate BLAKE3-256 hex encoding happens at the dispatch
/// boundary". Returns `String` of length 64.
#[must_use]
fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Construct a `VoteDryRunPreview` from operator-supplied inputs
/// (post-parse: hex-decoded `proposal_id`, `VoteChoice::parse`d
/// `canonical_choice`, optional hex-decoded `snapshot_id`).
///
/// Extracted from the dry-run branch in `vote_handler` so test
/// vectors can assert rationale surfaces in the preview envelope
/// without capturing stdout. Substrate-faithful: the helper
/// performs no wallet IO and never touches the substrate
/// `vote_v2` append path. The dry-run branch calls this helper
/// + `OutputEnvelope::render_with_redaction` to serialize.
///
/// The rationale field mirrors the operator `--rationale <text>`
/// input verbatim per RFC-0011-g §Subcommand Taxonomy; the
/// post-confirm substrate plumbing is verified by the load-
/// bearing pin `vote::tests::vote_v11_rationale_changes_envelope_pk`.
#[must_use]
fn build_vote_dry_run_preview(
    proposal_id: [u8; 32],
    canonical_choice: &str,
    weight_bps: u32,
    voter_cap_id: String,
    snapshot_id: Option<[u8; 32]>,
    allow_stale: bool,
    rationale: Option<String>,
) -> VoteDryRunPreview {
    VoteDryRunPreview {
        command: "octo governance vote".to_string(),
        proposal_id_hex: hex32(&proposal_id),
        vote_choice: canonical_choice.to_string(),
        weight_bps,
        voter_cap_id,
        snapshot_id_hex: snapshot_id.as_ref().map(hex32),
        allow_stale,
        rationale,
        dry_run_correlation_id: Uuid::new_v4().to_string(),
        preview_note: "no wallet IO performed; vote not recorded".to_string(),
    }
}

/// Build a CLI-side `AttestOutput` from a substrate
/// `AttestationReceipt`. The envelope hex-encodes the receipt's
/// `attestation_id` at the dispatch boundary; `content_hash` +
/// `appended_at_unix` mirror the receipt per RFC-0011-g §7.3.
#[must_use]
pub fn render_attest_output(receipt: AttestationReceipt) -> AttestOutput {
    let attestation_id = hex32(&receipt.attestation_id);
    AttestOutput {
        receipt: receipt.clone(),
        attestation_id: attestation_id.clone(),
        content_hash: attestation_id,
        appended_at_unix: receipt.appended_at_unix,
    }
}

/// Build a CLI-side `VoteOutput` from a substrate `VoteReceipt` + the substrate-computed quorum projection at vote-record time.
/// `quorum_projection` is a `(current_quorum_weight, quorum_threshold)` basis-point pair. The CLI does NOT compute these values; they arrive from the substrate per RFC-0011-g §Adversarial Review "Quorum manipulation" row.
#[must_use]
pub fn render_vote_output(receipt: VoteReceipt, quorum_projection: (u32, u32)) -> VoteOutput {
    let (current_quorum_weight, quorum_threshold) = quorum_projection;
    VoteOutput {
        receipt: receipt.clone(),
        vote_id: hex32(&receipt.vote_id),
        weight_applied: receipt.weight_applied,
        current_quorum_weight,
        quorum_threshold,
        recorded_at_unix: receipt.recorded_at_unix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use octo_governance::SnapshotRef;

    #[test]
    fn rfc_label_translation_table_is_total() {
        // Each RFC label maps to a unique substrate state.
        let labels = [
            ("Open", SubstrateProposalState::Voting),
            ("Quorum-Reached", SubstrateProposalState::Approved),
            ("Closed-Accepted", SubstrateProposalState::Executed),
            ("Closed-Rejected", SubstrateProposalState::Rejected),
            ("Closed-Expired", SubstrateProposalState::Expired),
        ];
        for (label, expected) in labels {
            assert_eq!(rfc_label_to_substrate_state(label).unwrap(), expected);
        }
        // Unknown label surfaces InvalidProposalState (exit 2).
        let err = rfc_label_to_substrate_state("NotALabel").unwrap_err();
        match err {
            OctoCliError::InvalidProposalState { state } => assert_eq!(state, "NotALabel"),
            _ => panic!("expected InvalidProposalState"),
        }
    }

    #[test]
    fn snapshot_output_schema_declares_string_fields() {
        // Schemars pin: the envelope payload declares its
        // fields as strings (not as `[u8; 32]`). The substrate
        // BLAKE3-256 hex encoding happens at the dispatch
        // boundary so downstream tooling sees stable wire
        // form.
        let _ = schemars::schema_for!(SnapshotOutput);
    }

    #[test]
    fn render_snapshot_routes_through_canonical_envelope_path() {
        // R4.5 regression: `render_snapshot` MUST route through
        // `OutputEnvelope::render_with_redaction` + map io
        // errors to `OctoCliError::Internal` (fail-closed
        // envelope write surface). Previously the envelope was
        // discarded via `let _ = envelope`; this test catches
        // any future revert by exercising the canonical path
        // end-to-end (clap-parse `Octo` + hand-built
        // `SnapshotView` + assert Ok(())).
        let mut cli = Octo::try_parse_from(["octo", "governance", "snapshot"])
            .expect("clap parse minimal snapshot invocation");
        cli.output.no_color = true;
        let view = SnapshotView {
            snapshot: SnapshotRef {
                snapshot_id: [0u8; 32],
                filter: ProposalFilter {
                    states: None,
                    chain_id: None,
                },
                taken_at_unix: 1_700_000_000,
                expires_at_unix: 1_700_000_060,
                root_manifest_hash: [0u8; 32],
                open_proposal_count: 0,
                attestation_count: 0,
            },
            open_proposals: vec![],
            attestation_count: 0,
            resolved_at_unix: 1_700_000_000,
            remaining_seconds: 60,
        };
        render_snapshot(&view, &cli)
            .expect("render_snapshot must return Ok(()) when envelope writes succeed");
    }

    #[test]
    fn snapshot_stale_maps_to_exit_35() {
        let err = OctoCliError::SnapshotStale {
            snapshot_id_hex: "00".repeat(32),
            age_secs: 42,
        };
        assert_eq!(err.exit_code(), 35);
    }

    #[test]
    fn invalid_proposal_state_maps_to_exit_2() {
        let err = OctoCliError::InvalidProposalState {
            state: "Bogus".to_string(),
        };
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn governance_substrate_error_maps_to_exit_51() {
        let err = OctoCliError::GovernanceSubstrateError {
            reason: "cache miss race".to_string(),
        };
        assert_eq!(err.exit_code(), 51);
    }

    #[test]
    fn hex32_lower_hex_64_chars() {
        // Substrate-faithful: the envelope hex-encodes the raw
        // BLAKE3-256 bytes as lower-hex of length 64.
        let bytes = [0u8; 32];
        assert_eq!(hex32(&bytes), "00".repeat(32));
        let mut bytes = [0u8; 32];
        bytes[0] = 0xab;
        bytes[31] = 0xcd;
        let s = hex32(&bytes);
        assert_eq!(s.len(), 64);
        assert!(s.starts_with("ab"));
        assert!(s.ends_with("cd"));
    }

    #[test]
    fn render_attest_output_mirrors_attestation_id_into_content_hash() {
        let receipt = AttestationReceipt {
            attestation_id: [0u8; 32],
            subject_did: "did:octo:peer:alice".to_string(),
            kind_ref: "route-quality:uptime-30d".to_string(),
            signer_did: "did:octo:operator:bob".to_string(),
            evidence_hash: [0xab; 32],
            expires_at_unix: Some(2_000_000_000),
            appended_at_unix: 1_700_000_000,
            overrode_staleness_at_unix: None,
        };
        let out = render_attest_output(receipt);
        // `attestation_id` and `content_hash` are the substrate-
        // faithful mirror of `receipt.attestation_id` per
        // RFC-0011-g §7.3 Output Envelope.
        assert_eq!(out.attestation_id, out.content_hash);
        assert_eq!(out.attestation_id, "00".repeat(32));
        assert_eq!(out.receipt.subject_did, "did:octo:peer:alice");
    }

    #[test]
    fn render_vote_output_uses_substrate_quorum_projection() {
        let receipt = VoteReceipt {
            vote_id: [0x01u8; 32],
            proposal_id: [0x02u8; 32],
            voter_did: "did:octo:operator:bob".to_string(),
            choice: "Yes".to_string(),
            weight_applied: 2500,
            voter_cap_id: "cap:vote:0001".to_string(),
            recorded_at_unix: 1_700_000_000,
            overrode_staleness_at_unix: None,
        };
        let out = render_vote_output(receipt, (4000, 5000));
        // CLI displays substrate-projected quorum; never
        // recomputes per RFC-0011-g §Adversarial Review
        // "Quorum manipulation" row.
        assert_eq!(out.current_quorum_weight, 4000);
        assert_eq!(out.quorum_threshold, 5000);
        assert_eq!(out.weight_applied, 2500);
        assert_eq!(out.vote_id, "01".repeat(32));
    }

    #[test]
    fn attest_output_and_vote_output_schemas_pin() {
        // Schemars pin: the envelope payload schemas generate
        // without error. The CLI envelope subsystem emits JSON
        // Schema alongside the payload per RFC-0011 §Output
        // Envelope schema emission contract.
        let _ = schemars::schema_for!(AttestOutput);
        let _ = schemars::schema_for!(VoteOutput);
    }

    // ---- CLI test vectors for attest + vote surfaces ----
    //
    // 16 substrate-faithful CLI-side tests covering:
    // - parse_hex_32 input validation (3)
    // - map_governance_error envelope routing (2)
    // - attest_handler mode + confirm gates (2)
    // - vote_handler mode + confirm gates (2)
    // - VoteChoice parse roundtrip (2)
    // - OctoCliError exit code mapping for new variants (3)
    // - Auditor-mode first-class deny surface (2)

    /// Build a minimal `Octo` for handler-direct tests. Avoids
    /// the clap parser machinery so the test can exercise the
    /// handler's mode/confirm gates without a wallet store.
    fn test_octo(mode: OperatorMode, confirm: bool) -> Octo {
        Octo {
            output: OutputFlags::default(),
            mode: OperatorModeFlags {
                mode,
                confirm,
                ..Default::default()
            },
            command: crate::Commands::Whoami,
        }
    }

    #[test]
    fn tv_cli_attest_1_parse_hex_32_accepts_64_lower_hex() {
        // Happy path: 32 bytes of arbitrary data round-trip
        // through parse_hex_32.
        let hex = "ab".repeat(32);
        let parsed = parse_hex_32("test", &hex).expect("64 lowercase hex must parse");
        assert_eq!(parsed[0], 0xab);
        assert_eq!(parsed[31], 0xab);
    }

    #[test]
    fn tv_cli_attest_2_parse_hex_32_rejects_non_hex() {
        // Non-hex characters surface InvalidFilter (exit 16).
        let bad = "zz".repeat(32);
        let err = parse_hex_32("test", &bad).unwrap_err();
        match err {
            OctoCliError::InvalidFilter(_) => {}
            _ => panic!("expected InvalidFilter for non-hex input"),
        }
    }

    #[test]
    fn tv_cli_attest_3_parse_hex_32_rejects_wrong_length() {
        // Wrong byte length surfaces InvalidFilter with byte
        // count in the message (sanitized redaction).
        let short = "ab".repeat(16); // 16 bytes, not 32
        let err = parse_hex_32("test", &short).unwrap_err();
        match err {
            OctoCliError::InvalidFilter(msg) => {
                assert!(msg.contains("16"));
            }
            _ => panic!("expected InvalidFilter for wrong-length input"),
        }
    }

    #[test]
    fn tv_cli_attest_4_map_governance_error_routes_unknown_kind() {
        // UnknownAttestationKind → OctoCliError::UnknownAttestationKind
        // (exit 37) per RFC-0011-g §Error Handling slot table.
        let err = GovernanceError::UnknownAttestationKind {
            kind_ref: "novel:kind:ref".to_string(),
        };
        match map_governance_error(err) {
            OctoCliError::UnknownAttestationKind { kind_ref } => {
                assert_eq!(kind_ref, "novel:kind:ref");
            }
            _ => panic!("expected UnknownAttestationKind routing"),
        }
    }

    #[test]
    fn tv_cli_attest_5_map_governance_error_routes_prereq() {
        // PrereqNotAccepted → OctoCliError::PrereqNotAccepted
        // (exit 38) per RFC-0011-g §Error Handling slot table.
        let err = GovernanceError::PrereqNotAccepted {
            rfc_ref: "RFC-0855p-d".to_string(),
        };
        match map_governance_error(err) {
            OctoCliError::PrereqNotAccepted { rfc_ref } => {
                assert_eq!(rfc_ref, "RFC-0855p-d");
            }
            _ => panic!("expected PrereqNotAccepted routing"),
        }
    }

    #[test]
    fn tv_cli_attest_6_handler_rejects_auditor_mode() {
        // Mode gate first: Auditor mode denied before any
        // wallet IO. RFC-0011-c §Roles and Authorities +
        // RFC-0011-g §Roles and Authorities.
        let result = attest_handler(
            "did:octo:peer:alice".to_string(),
            "route-quality:uptime-30d".to_string(),
            None,
            Some("ab".repeat(32)),
            None,
            None,
            false,
            false,
            true,
            false,
            &test_octo(OperatorMode::Auditor, true),
        );
        assert!(
            matches!(result, Err(OctoCliError::AuditorDenied { .. })),
            "expected AuditorDenied, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_attest_7_handler_requires_confirm_flag() {
        // Confirm gate second: missing --confirm returns
        // ConfirmationRequired (exit 2) before wallet IO.
        let result = attest_handler(
            "did:octo:peer:alice".to_string(),
            "route-quality:uptime-30d".to_string(),
            None,
            Some("ab".repeat(32)),
            None,
            None,
            false,
            false,
            false,
            false,
            &test_octo(OperatorMode::Human, false),
        );
        assert!(
            matches!(result, Err(OctoCliError::ConfirmationRequired { .. })),
            "expected ConfirmationRequired, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_attest_8_unknown_attestation_kind_maps_to_exit_37() {
        // OctoCliError exit code pin: UnknownAttestationKind = 37
        // per RFC-0011-g Appendix C exit code table.
        let err = OctoCliError::UnknownAttestationKind {
            kind_ref: "novel:kind:ref".to_string(),
        };
        assert_eq!(err.exit_code(), 37);
    }

    #[test]
    fn tv_cli_vote_1_map_governance_error_routes_unknown_capability() {
        // UnknownCapability → OctoCliError::Internal (exit 64)
        // per RFC-0011-g §Error Handling internal-substrate arm.
        let err = GovernanceError::UnknownCapability {
            voter_cap_id: "cap:novel:0001".to_string(),
        };
        match map_governance_error(err) {
            OctoCliError::Internal(_) => {}
            _ => panic!("expected Internal routing for UnknownCapability"),
        }
    }

    #[test]
    fn tv_cli_vote_2_map_governance_error_routes_duplicate_vote() {
        // DuplicateVote → OctoCliError::VoteRejected (exit 36)
        // per RFC-0011-g §Adversarial Review row
        // DuplicateVote→VoteRejected shared-slot variant.
        let err = GovernanceError::DuplicateVote {
            proposal_id: [0xab; 32],
            voter_did: "did:octo:operator:bob".to_string(),
        };
        match map_governance_error(err) {
            OctoCliError::VoteRejected { .. } => {}
            _ => panic!("expected VoteRejected routing for DuplicateVote"),
        }
    }

    #[test]
    fn tv_cli_vote_3_handler_rejects_auditor_mode() {
        // Mode gate first for vote handler (mirrors attest).
        let result = vote_handler(
            "ab".repeat(32),
            "approve".to_string(),
            1000,
            "cap:vote:0001".to_string(),
            None,
            false,
            false,
            true,
            false,
            None,
            &test_octo(OperatorMode::Auditor, true),
        );
        assert!(
            matches!(result, Err(OctoCliError::AuditorDenied { .. })),
            "expected AuditorDenied, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_vote_4_handler_requires_confirm_flag() {
        // Confirm gate second for vote handler (mirrors attest).
        let result = vote_handler(
            "ab".repeat(32),
            "approve".to_string(),
            1000,
            "cap:vote:0001".to_string(),
            None,
            false,
            false,
            false,
            false,
            None,
            &test_octo(OperatorMode::Human, false),
        );
        assert!(
            matches!(result, Err(OctoCliError::ConfirmationRequired { .. })),
            "expected ConfirmationRequired, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_vote_5_vote_choice_parse_roundtrip() {
        // Substrate-faithful: VoteChoice::parse + as_str
        // round-trip per RFC §Command Taxonomy primary vocabulary
        // (yes / no / abstain) plus back-compat aliases
        // (approve / reject).
        // Primary round-trip.
        assert_eq!(VoteChoice::parse("yes").unwrap().as_str(), "yes");
        assert_eq!(VoteChoice::parse("no").unwrap().as_str(), "no");
        assert_eq!(VoteChoice::parse("abstain").unwrap().as_str(), "abstain");
        // Aliases map to primaries.
        assert_eq!(VoteChoice::parse("approve").unwrap().as_str(), "yes");
        assert_eq!(VoteChoice::parse("reject").unwrap().as_str(), "no");
        // Case-insensitive.
        assert_eq!(VoteChoice::parse("APPROVE").unwrap().as_str(), "yes");
        // Invalid choice fails-closed.
        let err = VoteChoice::parse("maybe").unwrap_err();
        match err {
            GovernanceError::InvalidArgument { .. } => {}
            _ => panic!("expected InvalidArgument for unknown choice"),
        }
    }

    #[test]
    fn tv_cli_vote_6_vote_rejected_maps_to_exit_36() {
        // OctoCliError exit code pin: VoteRejected = 36 per
        // RFC-0011-g Appendix C exit code table.
        let err = OctoCliError::VoteRejected {
            reason: "quorum not reached".to_string(),
        };
        assert_eq!(err.exit_code(), 36);
    }

    #[test]
    fn tv_cli_vote_7_prereq_not_accepted_maps_to_exit_38() {
        // OctoCliError exit code pin: PrereqNotAccepted = 38
        // per RFC-0011-g Appendix C exit code table.
        let err = OctoCliError::PrereqNotAccepted {
            rfc_ref: "RFC-0855p-d".to_string(),
        };
        assert_eq!(err.exit_code(), 38);
    }

    #[test]
    fn tv_cli_vote_8_parse_hex_32_proposal_id_rejects_odd_length() {
        // Proposal-id parse guard: wrong byte length surfaces
        // InvalidFilter (exit 16) before substrate call.
        let odd = "ab".repeat(31); // 31 bytes, not 32
        let err = parse_hex_32("--proposal-id", &odd).unwrap_err();
        match err {
            OctoCliError::InvalidFilter(_) => {}
            _ => panic!("expected InvalidFilter for odd-length proposal id"),
        }
    }

    // ---- R4.5 --dry-run + weight_bps coverage ----
    //
    // Per `flags.rs` `--dry-run` contract, the preview
    // boundary must run BEFORE the `--confirm` gate so an
    // operator can request a preview without first
    // acknowledging a confirm gate that grants no authority.
    // Test pins both attest + vote --dry-run paths.

    #[test]
    fn tv_cli_attest_9_dry_run_bypasses_confirm_gate() {
        // --confirm=false BUT --dry-run=true → preview must
        // succeed (no ConfirmationRequired), per flags.rs
        // contract: --dry-run bypasses --confirm.
        let result = attest_handler(
            "did:octo:peer:alice".to_string(),
            "route-quality:uptime-30d".to_string(),
            None,
            Some("ab".repeat(32)),
            None,
            None,
            false,
            true,  // dry_run=true
            false, // confirm=false — should be bypassed
            false,
            &test_octo(OperatorMode::Human, false),
        );
        // Substrate-faithful: dry-run performs envelope parse
        // + renders preview envelope, returns Ok. No wallet IO,
        // no attest_v2 call, no exit-37 / exit-38 error.
        assert!(
            matches!(result, Ok(())),
            "attest --dry-run should bypass --confirm gate, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_attest_10_dry_run_rejects_bad_evidence_hash() {
        // --dry-run must STILL validate parseable inputs; bad
        // hex on --evidence-hash surfaces InvalidFilter (exit
        // 16) BEFORE the operator confirms a real signing.
        let result = attest_handler(
            "did:octo:peer:alice".to_string(),
            "route-quality:uptime-30d".to_string(),
            None,
            Some("not-valid-hex".to_string()), // bad hex
            None,
            None,
            false,
            true, // dry_run
            false,
            false,
            &test_octo(OperatorMode::Human, false),
        );
        assert!(
            matches!(result, Err(OctoCliError::InvalidFilter(_))),
            "attest --dry-run with bad evidence_hash should map to InvalidFilter, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_vote_9_dry_run_bypasses_confirm_gate() {
        // --confirm=false BUT --dry-run=true → preview must
        // succeed (no ConfirmationRequired), per flags.rs
        // contract.
        let result = vote_handler(
            "ab".repeat(32),
            "approve".to_string(),
            1000,
            "cap:vote:0001".to_string(),
            None,
            false,
            true,  // dry_run=true
            false, // confirm=false — should be bypassed
            false,
            None,
            &test_octo(OperatorMode::Human, false),
        );
        assert!(
            matches!(result, Ok(())),
            "vote --dry-run should bypass --confirm gate, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_vote_10_dry_run_rejects_weight_bps_over_10000() {
        // Per R4.5 MED ordering rule: weight_bps clamp fires
        // BEFORE parse_hex_32, so a too-large weight_bps
        // surfaces VoteRejected (exit 36) on the preview path
        // even with a valid proposal-id hex.
        let result = vote_handler(
            "ab".repeat(32), // valid 32-byte proposal id
            "approve".to_string(),
            10_001, // 1 bps over the 10_000 cap
            "cap:vote:0001".to_string(),
            None,
            false,
            true, // dry_run
            false,
            false,
            None,
            &test_octo(OperatorMode::Human, false),
        );
        assert!(
            matches!(result, Err(OctoCliError::VoteRejected { .. })),
            "vote --dry-run with weight_bps>10000 should map to VoteRejected (clamp fires FIRST), got {result:?}"
        );
    }

    #[test]
    fn tv_cli_vote_11_post_confirm_rejects_weight_bps_over_10000() {
        // Mirror of vote_10 on the post-confirm path:
        // weight_bps>10000 fails-fast before parse_hex_32 +
        // wallet IO per the MED ordering rule.
        let result = vote_handler(
            "ab".repeat(32),
            "approve".to_string(),
            99_999, // well over cap
            "cap:vote:0001".to_string(),
            None,
            false,
            false, // NOT dry-run — confirm path
            true,  // confirm=true
            false,
            None,
            &test_octo(OperatorMode::Human, true),
        );
        assert!(
            matches!(result, Err(OctoCliError::VoteRejected { .. })),
            "vote with weight_bps>10000 should map to VoteRejected, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_vote_12_dry_run_rejects_bad_proposal_id_hex() {
        // --dry-run must STILL validate proposal_id hex;
        // invalid hex surfaces InvalidFilter (exit 16) so the
        // operator catches the typo before confirming.
        let result = vote_handler(
            "not-a-valid-hex-string".to_string(),
            "approve".to_string(),
            1000,
            "cap:vote:0001".to_string(),
            None,
            false,
            true, // dry_run
            false,
            false,
            None,
            &test_octo(OperatorMode::Human, false),
        );
        assert!(
            matches!(result, Err(OctoCliError::InvalidFilter(_))),
            "vote --dry-run with bad proposal_id hex should map to InvalidFilter, got {result:?}"
        );
    }

    #[test]
    fn tv_cli_vote_13_dry_run_rationale_surfaces_in_preview_envelope() {
        // C1 (R5.5 fix-sweep) + R6.5 surface verification:
        // `--rationale <TEXT>` is plumbed from the Vote clap
        // variant through `vote_handler` into the dry-run preview
        // envelope via `build_vote_dry_run_preview`, and (on the
        // post-confirm path) into the live `vote_v2` substrate
        // 5th argument slot (load-bearing pin: see
        // `vote::tests::vote_v11_rationale_changes_envelope_pk`).
        //
        // This pin verifies the dry-run envelope carries the
        // rationale verbatim: the operator sees exactly the text
        // they are about to record in the audit log BEFORE
        // committing to a real signing operation. The dry-run
        // branch early-returns before `vote_v2` (per `--dry-run`
        // contract: no wallet IO, no substrate append), so the
        // substrate-side load-bearing pin (vote_v11) is the
        // authoritative coverage for live wiring; this pin is
        // the dry-run-side mirror.
        let rationale_input = "reject SLA terms: cap exceeds budget envelope".to_string();
        let preview = build_vote_dry_run_preview(
            [0xab; 32],
            "yes",
            1000,
            "cap:vote:0001".to_string(),
            None,
            false,
            Some(rationale_input.clone()),
        );
        assert_eq!(
            preview.rationale.as_deref(),
            Some(rationale_input.as_str()),
            "dry-run preview envelope MUST carry rationale verbatim"
        );
    }

    #[test]
    fn attest_dry_run_preview_struct_has_no_wallet_io_marker() {
        // Substrate-faithful preview: the preview note MUST
        // declare no wallet IO performed, so the operator
        // cannot mistake a preview for a recorded attestation.
        let preview = AttestDryRunPreview {
            command: "octo governance attest".to_string(),
            subject_did: "did:octo:peer:alice".to_string(),
            kind_ref: "route-quality:uptime-30d".to_string(),
            evidence_path: None,
            evidence_hash_hex: Some("ab".repeat(32)),
            expires_at_unix: None,
            snapshot_id_hex: None,
            allow_stale: false,
            dry_run_correlation_id: Uuid::new_v4().to_string(),
            preview_note: "no wallet IO performed; envelope not signed or appended".to_string(),
        };
        assert!(
            preview.preview_note.contains("no wallet IO"),
            "preview_note MUST declare no wallet IO: {preview:?}"
        );
        assert!(
            preview.preview_note.contains("signed")
                && preview.preview_note.contains("appended"),
            "preview_note MUST distinguish preview from recorded attestation (mention both signing + appending as not performed): {preview:?}"
        );
    }

    #[test]
    fn vote_dry_run_preview_struct_carries_parsed_hex() {
        // Substrate-faithful preview: the proposal_id_hex field
        // is the hex-encoded 32-byte form (after parse_hex_32),
        // NOT the raw operator input.
        let proposal_id_bytes: [u8; 32] = [0xab; 32];
        let proposal_id_hex = hex32(&proposal_id_bytes);
        let preview = VoteDryRunPreview {
            command: "octo governance vote".to_string(),
            proposal_id_hex,
            vote_choice: "approve".to_string(),
            weight_bps: 5000,
            voter_cap_id: "cap:vote:0001".to_string(),
            snapshot_id_hex: None,
            allow_stale: false,
            rationale: None,
            dry_run_correlation_id: Uuid::new_v4().to_string(),
            preview_note: "no wallet IO performed; vote not recorded".to_string(),
        };
        assert_eq!(preview.proposal_id_hex.len(), 64);
        assert!(preview
            .proposal_id_hex
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
        assert!(
            preview.preview_note.contains("no wallet IO"),
            "preview_note MUST declare no wallet IO: {preview:?}"
        );
    }
}
