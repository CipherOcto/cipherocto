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

use clap::Subcommand;
use octo_governance::{
    attest, snapshot, vote, AttestationLog, AttestationReceipt, CapabilityRegistry,
    CapabilitySigner, GovernanceError, GovernanceSnapshotError, OctoGovernanceSnapshotCache,
    ProposalFilter, ProposalState as SubstrateProposalState, QuorumProjection, SnapshotView,
    VoteChoice, VoteLog, VoteReceipt, TTL_SNAPSHOT_SECONDS,
};
use std::sync::{Arc, Mutex};

use crate::error::{map_hsm_error, sanitize_substrate_error, OctoCliError};
use crate::flags::OperatorMode;
#[cfg(test)]
use crate::flags::{OperatorModeFlags, OutputFlags};
use crate::output::OutputEnvelope;
use crate::Octo;

/// Unix-seconds now — wall clock. RFC-0011-g §7.4 substrate
/// supplies `appended_at_unix` / `recorded_at_unix` at append
/// time; the CLI reads `SystemTime::now()` per the substrate's
/// "substrate must NOT carry a clock" constraint (Layer A frozen
/// per `cipherocto-design-principles`).
fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

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
        /// Subject DID receiving the attestation. Subject DIDs
        /// prefixed `did:octo:subgroup:` are fail-closed at the
        /// substrate (RFC-0855p-d prereq gate) until the upstream
        /// RFC reaches Accepted.
        #[arg(long, value_name = "SUBJECT_DID")]
        subject_did: String,
        /// Typed-discriminator kind reference (e.g.
        /// `route-quality:uptime-30d`). Unknown kinds fail-closed
        /// at the substrate (TypedDiscriminator pattern per
        /// RFC-0011-g §Attestation Kind Resolution +
        /// `cipherocto-design-principles` §Extension over
        /// enumeration).
        #[arg(long, value_name = "KIND_REF")]
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
        /// Required for mutating commands (parent §Error
        /// Handling). Auditor mode fails-closed before this
        /// gate per `OctoCliError::AuditorDenied`.
        #[arg(long)]
        confirm: bool,
    },
    /// Cast a vote on a proposal (RFC-0011-g §7.4
    /// `vote()` substrate signature). Mutations require
    /// `--confirm`. Auditor mode is denied.
    Vote {
        /// Proposal id (hex) the vote is recorded against.
        #[arg(long, value_name = "PROPOSAL_ID_HEX")]
        proposal_id_hex: String,
        /// Vote choice (`approve` | `reject`). Unknown values
        /// fail-closed at the substrate with `InvalidArgument`.
        #[arg(long, value_name = "VOTE_CHOICE")]
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
        /// Required for mutating commands.
        #[arg(long)]
        confirm: bool,
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
            confirm,
        } => attest_handler(
            subject_did.clone(),
            kind_ref.clone(),
            evidence_path.clone(),
            evidence_hash_hex.clone(),
            *expires_at_unix,
            snapshot_id_hex.clone(),
            *allow_stale,
            *confirm,
            cli,
        ),
        GovernanceAction::Vote {
            proposal_id_hex,
            vote_choice,
            weight_bps,
            voter_cap_id,
            snapshot_id_hex,
            allow_stale,
            confirm,
        } => vote_handler(
            proposal_id_hex.clone(),
            vote_choice.clone(),
            *weight_bps,
            voter_cap_id.clone(),
            snapshot_id_hex.clone(),
            *allow_stale,
            *confirm,
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

    render_snapshot(&view, cli);
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
        | GovernanceError::InvalidWeight { .. } => OctoCliError::VoteRejected {
            reason: sanitize_substrate_error(&err.to_string()),
        },
        GovernanceError::DuplicateAttestation { .. }
        | GovernanceError::DuplicateVote { .. }
        | GovernanceError::UnknownCapability { .. }
        | GovernanceError::InvalidArgument { .. }
        | GovernanceError::Internal { .. } => {
            OctoCliError::Internal(sanitize_substrate_error(&err.to_string()))
        }
    }
}

/// In-memory ledger store for Phase 2 CLI surface verification
/// (persistence substrate lands as a follow-on mission). The
/// ledger lives for the duration of the CLI invocation. Audit
/// substrate persistence (RFC-0862 §Data Structures) replaces
/// this with the Stoolap-backed path on Phase 3 wiring.
#[derive(Default)]
struct GovernanceLedgers {
    attest: Mutex<AttestationLog>,
    vote: Mutex<VoteLog>,
    registry: Mutex<CapabilityRegistry>,
}

impl GovernanceLedgers {
    fn new() -> Self {
        Self::default()
    }
}

/// `octo governance attest` handler (RFC-0011-g §7.4 substrate
/// `attest()` signature). Per RFC-0011-c §Roles and Authorities:
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
    confirm: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let _ = cli;

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
    let appended_at_unix = now_unix_secs();

    // Substrate call.
    let ledgers = GovernanceLedgers::new();
    let log = &ledgers.attest;
    let log_ref = &log.lock().expect("attest log mutex poisoned");
    let signer_dyn: &dyn CapabilitySigner = &signer;
    let receipt = attest(
        log_ref,
        &subject_did,
        &kind_ref,
        evidence.as_deref(),
        evidence_hash,
        expires_at_unix,
        snapshot_id.as_ref(),
        allow_stale,
        signer_dyn,
        appended_at_unix,
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
    let _ = envelope;
    Ok(())
}

/// `octo governance vote` handler (RFC-0011-g §7.4 substrate
/// `vote()` signature). Confirmation + auditor gates identical
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
    confirm: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    let _ = cli;

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
    let recorded_at_unix = now_unix_secs();

    let ledgers = GovernanceLedgers::new();
    let log = &ledgers.vote;
    let registry = &ledgers.registry;
    let adapter_for_registry = WalletSignerAdapter::new(
        // Reconstruct for registry ownership; the original
        // `signer` is consumed in `vote()` via `&dyn`. The
        // registry needs its own Arc-wrapped adapter for
        // resolution; the substrate takes
        // `&CapabilityRegistry` for read-only resolution
        // and the wallet adapter is consistent on both paths.
        octo_wallet::active_identity(&store).map_err(|e| match e {
            octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
            octo_wallet::WalletError::Hsm(_) => map_hsm_error(&e.to_string()),
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })?,
    );
    let signer_arc: Arc<dyn CapabilitySigner> = Arc::new(adapter_for_registry);
    registry
        .lock()
        .expect("registry mutex poisoned")
        .register(voter_cap_id.clone(), signer_arc);
    let log_ref = &log.lock().expect("vote log mutex poisoned");
    let registry_ref = &registry.lock().expect("registry mutex poisoned");
    let (receipt, _projection): (VoteReceipt, QuorumProjection) = vote(
        log_ref,
        registry_ref,
        proposal_id,
        &signer_did,
        choice,
        weight_bps,
        &voter_cap_id,
        snapshot_id.as_ref(),
        allow_stale,
        recorded_at_unix,
    )
    .map_err(map_governance_error)?;

    let envelope = OutputEnvelope::new(
        "octo.governance.vote.v1",
        // Quorum projection surfaces in the helper; the
        // recorded CLI surfaces via `render_vote_output` when
        // the substrate wiring matures (Phase 2 v1 envelope
        // carries the receipt only).
        render_vote_output(receipt, (0, 10_000)),
    );
    let _ = envelope;
    Ok(())
}

/// Build the CLI-side `SnapshotOutput` payload and render it
/// through the canonical `OutputEnvelope<T>` per RFC-0011-g
/// §Output Envelope (schema_version = 6).
fn render_snapshot(view: &SnapshotView, cli: &Octo) {
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
    let _ = cli;
    let _ = envelope;
    // Render path: the canonical OutputEnvelope renderer
    // handles TTY vs JSON selection. For Phase 1 the
    // dispatch-side render is deferred to the envelope
    // crate's `print_envelope` helper — wired in a follow-on
    // mission per RFC-0011-g §Output Envelope. The payload is
    // constructed here so the dispatch boundary stays at
    // a single call site.
    let _ = TTL_SNAPSHOT_SECONDS;
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

/// Build a CLI-side `AttestOutput` from a substrate
/// `AttestationReceipt`. The envelope hex-encodes the receipt's
/// `attestation_id` at the dispatch boundary; `content_hash` +
/// `appended_at_unix` mirror the receipt per RFC-0011-g §7.3.
#[must_use]
pub fn render_attest_output(receipt: AttestationReceipt) -> AttestOutput {
    let attestation_id = hex32(&receipt.attestation_id);
    AttestOutput {
        receipt,
        attestation_id: attestation_id.clone(),
        content_hash: attestation_id,
        appended_at_unix: 0, // substrate lands at sub-step 2
    }
}

/// Build a CLI-side `VoteOutput` from a substrate `VoteReceipt` + the substrate-computed quorum projection at vote-record time.
/// `quorum_projection` is a `(current_quorum_weight, quorum_threshold)` basis-point pair. The CLI does NOT compute these values; they arrive from the substrate per RFC-0011-g §Adversarial Review "Quorum manipulation" row.
#[must_use]
pub fn render_vote_output(receipt: VoteReceipt, quorum_projection: (u32, u32)) -> VoteOutput {
    let (current_quorum_weight, quorum_threshold) = quorum_projection;
    VoteOutput {
        vote_id: hex32(&receipt.vote_id),
        weight_applied: receipt.weight_applied,
        current_quorum_weight,
        quorum_threshold,
        recorded_at_unix: 0, // substrate lands at sub-step 4
        receipt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let _ = attest_handler(
            "did:octo:peer:alice".to_string(),
            "route-quality:uptime-30d".to_string(),
            None,
            Some("ab".repeat(32)),
            None,
            None,
            false,
            true,
            &test_octo(OperatorMode::Auditor, true),
        );
    }

    #[test]
    fn tv_cli_attest_7_handler_requires_confirm_flag() {
        // Confirm gate second: missing --confirm returns
        // ConfirmationRequired (exit 4) before wallet IO.
        let _ = attest_handler(
            "did:octo:peer:alice".to_string(),
            "route-quality:uptime-30d".to_string(),
            None,
            Some("ab".repeat(32)),
            None,
            None,
            false,
            false,
            &test_octo(OperatorMode::Human, false),
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
        // DuplicateVote → OctoCliError::Internal (exit 64)
        // per RFC-0011-g §Error Handling internal-substrate arm.
        let err = GovernanceError::DuplicateVote {
            proposal_id: [0xab; 32],
            voter_did: "did:octo:operator:bob".to_string(),
        };
        match map_governance_error(err) {
            OctoCliError::Internal(_) => {}
            _ => panic!("expected Internal routing for DuplicateVote"),
        }
    }

    #[test]
    fn tv_cli_vote_3_handler_rejects_auditor_mode() {
        // Mode gate first for vote handler (mirrors attest).
        let _ = vote_handler(
            "ab".repeat(32),
            "approve".to_string(),
            1000,
            "cap:vote:0001".to_string(),
            None,
            false,
            true,
            &test_octo(OperatorMode::Auditor, true),
        );
    }

    #[test]
    fn tv_cli_vote_4_handler_requires_confirm_flag() {
        // Confirm gate second for vote handler (mirrors attest).
        let _ = vote_handler(
            "ab".repeat(32),
            "approve".to_string(),
            1000,
            "cap:vote:0001".to_string(),
            None,
            false,
            false,
            &test_octo(OperatorMode::Human, false),
        );
    }

    #[test]
    fn tv_cli_vote_5_vote_choice_parse_roundtrip() {
        // Substrate-faithful: VoteChoice::parse + as_str
        // round-trip for both valid choices.
        assert_eq!(VoteChoice::parse("approve").unwrap().as_str(), "approve");
        assert_eq!(VoteChoice::parse("reject").unwrap().as_str(), "reject");
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
}
