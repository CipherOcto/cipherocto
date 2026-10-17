//! `initiate_transfer` — substrate envelope builder (RFC-0011-e
//! §Substrate Additions).
//!
//! Split from `vault_operations.rs` per Wave 1.5 hygiene callout.
//!
//! Sibling modules:
//! - [`crate::nonce`] — `handle_id` + `handle_nonce` BLAKE3 derivations.
//! - [`crate::transfer_handle`] — `TransferHandle` + `TransferStatus`.

use octo_cap_macaroon::{AssetId, Dqa, VaultId};

use crate::nonce::{handle_id, handle_nonce};
use crate::transfer_handle::{TransferHandle, TransferStatus};
use crate::vault_owner::VaultError;

/// Build the transfer envelope substrate handle (RFC-0011-e §Substrate
/// Additions).
///
/// Canonical substrate signature: `(vault_id, dest, amount_dqa_micros,
/// asset) -> Result<TransferHandle, VaultError>`. The substrate here
/// builds the envelope structure (handle_id, nonce derivation per
/// substrate inputs, asset validation against the substrate vault
/// registry) and returns `TransferHandle { status: Pending, .. }`.
///
/// **Signing boundary:** the substrate does NOT sign here. The chain
/// adapter (Layer D) holds the HSM-bound signing key and signs the
/// broadcast envelope at submission time. The substrate `handle_id` is
/// deterministic so a re-derivation at the chain adapter produces the
/// same id (cross-validation hook).
///
/// **Replay protection:** the substrate-handle nonce is BLAKE3-derived
/// from the substrate inputs (vault_id, dest, amount, asset,
/// current_unix_seconds). The chain-time nonce derived from
/// `max_occurred_at_unix` per RFC-0011-e §Security: Transfer Replay is
/// the chain adapter's responsibility — the substrate reserves the
/// substrate-handle nonce for envelope identification only.
///
/// `current_unix_seconds` is supplied by the CLI caller so the
/// derivation is deterministic (RFC-0008 Class A); the substrate does
/// NOT call `SystemTime::now()` itself.
pub fn initiate_transfer(
    vault_id: &VaultId,
    dest: &VaultId,
    amount_dqa_micros: i64,
    asset: &AssetId,
    current_unix_seconds: i64,
) -> Result<TransferHandle, VaultError> {
    // Asset quantity validation: amount must be positive (DQA micros,
    // scale 0). The substrate fails-closed on non-positive amounts.
    if amount_dqa_micros <= 0 {
        return Err(VaultError::Substrate);
    }

    // DQA invariant check — substrate fails-closed on out-of-scale
    // amounts (Dqa::new uses scale 0 for micros).
    let _ = Dqa::new(amount_dqa_micros, 0).map_err(|_| VaultError::Substrate)?;

    let nonce = handle_nonce(
        vault_id,
        dest,
        amount_dqa_micros,
        asset,
        current_unix_seconds,
    );
    let handle = handle_id(vault_id, dest, amount_dqa_micros, asset, &nonce);

    Ok(TransferHandle {
        handle_id: handle,
        vault_id: *vault_id,
        dest_vault_id: *dest,
        amount_dqa_micros,
        asset_id: *asset,
        nonce,
        status: TransferStatus::Pending,
    })
}
