//! Substrate-handle derivation (handle_id + nonce) per RFC-0011-e
//! §Substrate Additions.
//!
//! Split from `vault_operations.rs` per Wave 1.5 hygiene callout. Both
//! functions live here so the deterministic BLAKE3 derivations stay
//! co-located and the substrate envelope builder (`initiate_transfer`)
//! consumes them via plain fn calls.

use blake3::Hasher;
use octo_cap_macaroon::{AssetId, VaultId};

/// Derive the substrate-handle identifier (deterministic per inputs).
///
/// `handle_id = BLAKE3("octo:transfer-handle:v1:" || vault_id ||
/// dest_vault_id || amount_dqa_micros_be || asset_id || nonce)`. Same
/// inputs produce the same handle id; the chain adapter (Layer D) may
/// re-derive at broadcast time for cross-validation.
#[must_use]
pub fn handle_id(
    vault_id: &VaultId,
    dest_vault_id: &VaultId,
    amount_dqa_micros: i64,
    asset_id: &AssetId,
    nonce: &[u8; 32],
) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(b"octo:transfer-handle:v1:");
    h.update(vault_id.as_bytes());
    h.update(dest_vault_id.as_bytes());
    h.update(&amount_dqa_micros.to_be_bytes());
    h.update(asset_id.as_bytes());
    h.update(nonce);
    let bytes = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_bytes());
    out
}

/// Derive the substrate-handle nonce (BLAKE3 of substrate inputs).
///
/// `nonce = BLAKE3("octo:transfer-nonce:v1:" || vault_id ||
/// dest_vault_id || amount_dqa_micros_be || asset_id ||
/// current_unix_seconds)`. The chain adapter (Layer D) may re-derive
/// using the chain-time `max_occurred_at_unix` per RFC-0011-e
/// §Security: Transfer Replay; the substrate here computes an envelope
/// identification nonce so the handle is substrate-deterministic.
#[must_use]
pub fn handle_nonce(
    vault_id: &VaultId,
    dest_vault_id: &VaultId,
    amount_dqa_micros: i64,
    asset_id: &AssetId,
    current_unix_seconds: i64,
) -> [u8; 32] {
    let mut h = Hasher::new();
    h.update(b"octo:transfer-nonce:v1:");
    h.update(vault_id.as_bytes());
    h.update(dest_vault_id.as_bytes());
    h.update(&amount_dqa_micros.to_be_bytes());
    h.update(asset_id.as_bytes());
    h.update(&current_unix_seconds.to_be_bytes());
    let bytes = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_bytes());
    out
}
