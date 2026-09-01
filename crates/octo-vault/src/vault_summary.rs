//! `VaultSummary` CLI-facing substrate record (RFC-0011-e §Substrate
//! Additions + RFC-0960-v37 §2.1 canonical shape).
//!
//! Split from `vault_operations.rs` per Wave 1.5 hygiene callout — the
//! 767-line module crossed the per-module size threshold (per
//! [[cipherocto-design-principles]] §No god-objects).

use octo_cap_macaroon::{ChainId, VaultId};
use serde::{Deserialize, Serialize};

use crate::OwnerDid;

/// CLI-facing vault inventory record (RFC-0011-e §Substrate Additions +
/// RFC-0960-v37 §2.1 canonical shape).
///
/// Returned by [`crate::list_owned`]. The CLI wraps a `Vec<VaultSummary>`
/// into the `VaultListOutput` envelope (parent envelope
/// `schema_version = 3` per RFC-0011-e §Output Envelope divergence).
///
/// Field substrate-truth (per mission YAML §Type Coverage row 1 + parent
/// RFC VH v1.6 R1 fix L1285 — `vault_id: Hex32` → `vault_id: VaultId`):
///
/// - `vault_id` is the canonical 32-byte `VaultId` newtype (NOT
///   `Hex32`); the CLI envelope serializes via the standard `VaultId`
///   serde shape (RFC-0105 §3.11).
/// - `owner_did` is the substrate's [`OwnerDid`] (`String` alias) per
///   the workspace-cycle avoidance note in `vault_operations.rs`'s
///   "Substrate-truth deviation note" — RFC-0010 canonical form at the
///   wire level.
/// - `balance_projected` is the DQA canonical-form string per
///   RFC-0960-v36 §Wire Form (e.g. `"123.456789012345"`).
///
/// `#[non_exhaustive]` so future substrate columns (e.g. `policy_digest`,
/// `last_transfer_event_id`) can land without a semver-major break;
/// consumers must construct via field-by-field assignment or a builder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VaultSummary {
    /// Derived vault identifier (RFC-0960 §2.1 canonical 32-byte form).
    pub vault_id: VaultId,
    /// Owning chain (RFC-0105 §3.11 chain-binding).
    pub chain_id: ChainId,
    /// Owner DID canonical form (RFC-0010, TEXT wire form per
    /// review §20.3 schema sketch).
    pub owner_did: OwnerDid,
    /// Asset symbol (e.g. `OCTO`, `OCTO-W`) — CLI-side filter convenience;
    /// substrate authoritative source is `asset_id` resolved via
    /// `VaultAssetResolver` at projection time.
    pub asset_symbol: String,
    /// Last projected balance in DQA canonical wire form (RFC-0960-v36).
    pub balance_projected: String,
    /// Wall-clock timestamp of the last projection (unix seconds).
    /// `None` when the vault has zero events.
    pub last_updated_unix: Option<i64>,
}
