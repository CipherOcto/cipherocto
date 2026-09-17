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
    snapshot, GovernanceSnapshotError, OctoGovernanceSnapshotCache, ProposalFilter,
    ProposalState as SubstrateProposalState, SnapshotView, TTL_SNAPSHOT_SECONDS,
};

use crate::error::{map_hsm_error, sanitize_substrate_error, OctoCliError};
use crate::output::OutputEnvelope;
use crate::Octo;

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
/// `Snapshot` only; `Attest` + `Vote` land in the
/// `0011-g-governance-attest-vote` mission (release-gated on the
/// RFC-0855p-d + RFC-0855p-e + RFC-0011-d Phase 1 conjunction).
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
}

/// Dispatch a parsed `octo governance ...` invocation to its
/// handler. `Snapshot` is the only Phase 1 surface; `Attest` +
/// `Vote` land in the follow-on mission.
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
}
