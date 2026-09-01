//! `TransferHandle` + `TransferStatus` envelope substrate
//! (RFC-0011-e §Substrate Additions).
//!
//! Split from `vault_operations.rs` per Wave 1.5 hygiene callout — the
//! 767-line module crossed the per-module size threshold (per
//! [[cipherocto-design-principles]] §No god-objects).

use octo_cap_macaroon::{AssetId, VaultId};
use serde::{Deserialize, Serialize};

/// Transfer envelope substrate handle (RFC-0011-e §Substrate Additions +
/// RFC-0960 transfer envelope).
///
/// Returned by [`crate::initiate_transfer`]. The CLI wraps a
/// `TransferHandle` into the `VaultTransferOutput` envelope.
///
/// `#[non_exhaustive]` so future substrate fields (e.g.
/// `broadcast_chain_id`, `signed_envelope_hash`) can land without a
/// semver-major break.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TransferHandle {
    /// Substrate-handle identifier — `BLAKE3("octo:transfer-handle:v1:"
    /// || vault_id || dest_vault_id || amount_dqa_micros_be ||
    /// asset_id || nonce)`. Deterministic per substrate inputs so a
    /// replay of the same inputs surfaces the same handle id (idempotent
    /// handle creation).
    pub handle_id: [u8; 32],
    /// Source vault.
    pub vault_id: VaultId,
    /// Destination vault.
    pub dest_vault_id: VaultId,
    /// Transfer amount (DQA micros, scale 0).
    pub amount_dqa_micros: i64,
    /// Asset identifier.
    pub asset_id: AssetId,
    /// Substrate-handle nonce (BLAKE3-derived from substrate inputs).
    /// The chain-time nonce (derived from `max_occurred_at_unix` per
    /// RFC-0011-e §Security: Transfer Replay) is computed by the chain
    /// adapter (Layer D) at broadcast time, NOT here — the substrate
    /// builds the envelope structure only.
    pub nonce: [u8; 32],
    /// Current transfer status (substrate-internal).
    pub status: TransferStatus,
}

/// Transfer lifecycle status (RFC-0011-e §Substrate Additions +
/// §Output Envelope — `DryRun | Pending | Confirmed | Failed`).
///
/// `Broadcast` is an internal substrate phase, NOT a `TransferStatus`
/// variant (per RFC-0011-e Appendix D state machine).
///
/// `#[non_exhaustive]` so future substrate phases (e.g. `Reorged`,
/// `RolledBack`) can land without a semver-major break; downstream
/// consumers MUST add a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[repr(u8)]
pub enum TransferStatus {
    /// Envelope built; awaiting broadcast confirmation.
    Pending = 0,
    /// Broadcast confirmed by chain adapter.
    Confirmed = 1,
    /// Broadcast failed (chain rejection, IO, or substrate validation).
    Failed = 2,
    /// Envelope built and substrate-validated, but `--dry-run`
    /// suppressed both HSM signing and broadcast (terminal state per
    /// RFC-0011-e Appendix D).
    ///
    /// **Set by Layer C, never produced by the substrate.**
    /// [`crate::initiate_transfer`] always returns
    /// [`TransferStatus::Pending`]; the CLI (Layer C) rewrites the
    /// handle status to `DryRun` at envelope-build time when the
    /// operator passes `--dry-run`. The variant lives here — not in a
    /// parallel CLI-side enum — because RFC-0011-e §Output Envelope
    /// pins `TransferStatus` as the single carrier of dry-run state
    /// (`preview_only` was dropped from the envelope), and a duplicate
    /// Layer C enum would be a parallel abstraction per
    /// [[cipherocto-design-principles]].
    DryRun = 3,
}
