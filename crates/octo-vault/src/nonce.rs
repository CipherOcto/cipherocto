//! Substrate-handle derivation (handle_id + nonce) per RFC-0011-e
//! §Substrate Additions.
//!
//! Split from `vault_operations.rs` per Wave 1.5 hygiene callout. Both
//! functions live here so the deterministic BLAKE3 derivations stay
//! co-located and the substrate envelope builder (`initiate_transfer`)
//! consumes them via plain fn calls.
//!
//! ## Layer model
//!
//! Routes through `octo_cap_macaroon::blake3_hash` (Layer A frozen
//! substrate per `cipherocto-design-principles`) — no direct `blake3`
//! crate dep in this crate's production source. The input bytes are
//! concatenated in the canonical order (domain-separator-prefixed
//! raw concatenation, no length prefixes) and hashed in one shot.

use octo_cap_macaroon::{blake3_hash, AssetId, VaultId};

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
    // Concatenate inputs in canonical order — same byte order the
    // streaming Hasher would feed to `update()`.
    let mut input: Vec<u8> =
        Vec::with_capacity(b"octo:transfer-handle:v1:".len() + 32 + 32 + 8 + 32 + 32);
    input.extend_from_slice(b"octo:transfer-handle:v1:");
    input.extend_from_slice(vault_id.as_bytes());
    input.extend_from_slice(dest_vault_id.as_bytes());
    input.extend_from_slice(&amount_dqa_micros.to_be_bytes());
    input.extend_from_slice(asset_id.as_bytes());
    input.extend_from_slice(nonce);
    blake3_hash(&input)
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
    let mut input: Vec<u8> =
        Vec::with_capacity(b"octo:transfer-nonce:v1:".len() + 32 + 32 + 8 + 32 + 8);
    input.extend_from_slice(b"octo:transfer-nonce:v1:");
    input.extend_from_slice(vault_id.as_bytes());
    input.extend_from_slice(dest_vault_id.as_bytes());
    input.extend_from_slice(&amount_dqa_micros.to_be_bytes());
    input.extend_from_slice(asset_id.as_bytes());
    input.extend_from_slice(&current_unix_seconds.to_be_bytes());
    blake3_hash(&input)
}
