---
name: 0959-c1-wire-A-substrate-verify
description: Land RFC-0959 v2.0 wire-format amendment substrate per recon 2026-08-19: extend `SettlementEnvelope` with `cost_vault_id: Option<[u8; 32]>` + `chain_id: Option<[u8; 32]>` fields at `crates/quota-router-storage/src/ask.rs:984`; update `compute_settlement_hash()` canonical preimage; add `SettlementError::ChainMismatch { vault_id, vault_chain_id, envelope_chain_id }` + `SettlementError::CostVaultIdMissing` variants; create `crates/quota-router-storage/src/settlement_verify.rs` with `verify_settlement_chain_match(envelope, vault_lookup)` reusing `octo_cap_macaroon::VaultLookup`; add `octo-cap-macaroon` dep to `crates/quota-router-storage/Cargo.toml`; expose `settlement_verify` module via `lib.rs` `pub use`. Layer B intra-dep allowed per RFC-0957-A1 §Layer Discipline.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0959-c1-wire-format-amendment
    - 0900-d-chain-aware-slash-ledger
    - RFC-0959
    - RFC-0967-A1
    - RFC-0009
status: OPEN
---

# Mission `0959-c1-wire-A-substrate-verify` v1.0 — OPEN 2026-08-24

## Context

RFC-0959 v2.0 (canonical Accepted 2026-08-19) adds `cost_vault_id: Option<[u8; 32]>` + `chain_id: Option<[u8; 32]>` to `SettlementEnvelope` per review §8.4.1 + §20.7 + §8.5.1. The wire-format amendment enforces `envelope.chain_id == vault_row.chain_id` (cross-chain settlement reject). Substrate work partial: `cost_vault_id` column landed in `settlement_event_repo.rs:56` + migration v016 added columns per recon 2026-08-19; but `SettlementEnvelope` struct field extension + `settlement_verify.rs` module + Cargo dep + lib.rs pub use remain pending. Mission `0959-c1-wire-format-amendment` is OPEN and owns the 11-step recon scope; this mission owns the substrate-coding subset (steps 1, 2, 3, 4, 5, 9, 10, 11).

## Scope

### Step 1: `SettlementEnvelope` struct field extension

Edit `crates/quota-router-storage/src/ask.rs` SettlementEnvelope (around line 984):

```rust
pub struct SettlementEnvelope {
    pub version_tag: u8,                       // 0x02 for v2.0
    pub cost: Dqa,                             // DqaEncoding 16-byte BE scale 12
    pub cost_vault_id: Option<[u8; 32]>,       // NEW v2.0 field
    pub chain_id: Option<[u8; 32]>,            // NEW v2.0 field
    pub ask_id: [u8; 32],
    pub invocation_hash: [u8; 32],
    pub cap_root_hash: [u8; 32],
    pub canonical_axes_consumed: Vec<u8>,      // canonical_ser per RFC-0126
    pub settled_at_unix: u64,
    pub settlement_hash: [u8; 32],
    // ... existing fields preserved
}
```

### Step 2: Update `compute_settlement_hash()` canonical preimage

Preimage per RFC-0959 §Wire Format v2.0:

```
BLAKE3(
    version_tag              || 0x01  // 1 byte: 0x00=None, 0x01=Some
    || (cost_vault_id if Some)       // 32 bytes if present
    || 0x01                          // 1 byte presence tag
    || (chain_id if Some)            // 32 bytes if present
    || ask_id                        // 32 bytes
    || invocation_hash               // 32 bytes
    || cap_root_hash                 // 32 bytes
    || canonical_axes_consumed       // varint len + bytes
    || settled_at_unix               // 8 bytes BE
    || cost                          // 16 bytes BE DqaEncoding
)[:32]
```

`compute_settlement_hash` updated to read v2.0 format when `version_tag == 0x02`; v1.0 format preserved for legacy replay (`version_tag == 0x01`).

### Step 3: `SettlementError` variants

Edit `crates/quota-router-storage/src/ask.rs` SettlementError enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SettlementError {
    #[error("settlement hash mismatch: expected {expected:?}, computed {actual:?}")]
    HashMismatch { expected: [u8; 32], actual: [u8; 32] },
    #[error("cost_vault_id missing on v2.0 envelope (chain_id present requires cost_vault_id)")]
    CostVaultIdMissing,
    #[error("cross-chain settlement rejected: vault_id {vault_id:?} lives on chain {vault_chain_id:?}, envelope claims chain {envelope_chain_id:?}")]
    ChainMismatch {
        vault_id: [u8; 32],
        vault_chain_id: [u8; 32],
        envelope_chain_id: [u8; 32],
    },
    // ... existing variants preserved
}
```

### Step 4: Create `settlement_verify.rs`

New file `crates/quota-router-storage/src/settlement_verify.rs`:

```rust
//! Settlement-time vault-row chain-match verification per RFC-0959 v2.0 §Settlement-Time Vault Row Lookup.
//!
//! Reuses `octo_cap_macaroon::VaultLookup` (Layer B extension trait, shared with capability
//! verify-time path per RFC-0957-A1 §Verify-Time Extension). No shadow impl.

use octo_cap_macaroon::VaultLookup;
use crate::ask::SettlementEnvelope;
use crate::consumed_receipt_repo::SettlementError;

/// Verify `envelope.cost_vault_id` → vault row exists → `vault.chain_id == envelope.chain_id`.
///
/// Returns `Ok(())` on chain match. Returns `Err(SettlementError::CostVaultIdMissing)` if v2.0
/// envelope lacks `cost_vault_id`. Returns `Err(SettlementError::ChainMismatch { ... })` if
/// vault row exists but `chain_id` diverges. Returns `Err(SettlementError::VaultLookup(...))`
/// if `cost_vault_id` not present in vault lookup (404 case).
pub async fn verify_settlement_chain_match(
    envelope: &SettlementEnvelope,
    vault_lookup: &dyn VaultLookup,
) -> Result<(), SettlementError> {
    let cost_vault_id = envelope.cost_vault_id
        .ok_or(SettlementError::CostVaultIdMissing)?;

    let vault_row = vault_lookup.lookup_vault(cost_vault_id).await
        .ok_or(SettlementError::VaultLookupNotFound { vault_id: cost_vault_id })?;

    let envelope_chain_id = envelope.chain_id
        .ok_or(SettlementError::CostVaultIdMissing)?; // chain_id required when cost_vault_id present

    if vault_row.chain_id != envelope_chain_id {
        return Err(SettlementError::ChainMismatch {
            vault_id: cost_vault_id,
            vault_chain_id: vault_row.chain_id,
            envelope_chain_id,
        });
    }

    Ok(())
}
```

### Step 5: Cargo dep + lib.rs pub use

Edit `crates/quota-router-storage/Cargo.toml` — add `octo-cap-macaroon` dep:

```toml
# Layer B intra-dep (allowed per RFC-0957-A1 §Layer Discipline; both crates in Layer B).
octo-cap-macaroon = { path = "../octo-cap-macaroon" }
```

Edit `crates/quota-router-storage/src/lib.rs` — expose new module:

```rust
pub mod settlement_verify;
pub use settlement_verify::{verify_settlement_chain_match, SettlementError};
```

### Step 9 + Step 10: Update `PersistedSettlementEvent` + `SettlementEventInsert` DAO

The DAO struct at `crates/quota-router-storage/src/settlement_event_repo.rs` already carries `cost_vault_id: Option<[u8; 32]>` (line 56) + `chain_id` (line 160) per migration v016. Need to:

- Update `SettlementEventInsert` (the input struct) to carry the new fields
- Update `INSERT INTO settlement_events` SQL bindings (line 167-170 already partially updated)
- Verify `PersistedSettlementEvent` deserialization matches migration v016 column ordering

## Acceptance Criterion

- `SettlementEnvelope` struct extended with `cost_vault_id: Option<[u8; 32]>` + `chain_id: Option<[u8; 32]>` fields
- `compute_settlement_hash()` updated for v2.0 preimage (presence tag + conditional 32B)
- `SettlementError::ChainMismatch` + `SettlementError::CostVaultIdMissing` variants added
- `crates/quota-router-storage/src/settlement_verify.rs` exists with `verify_settlement_chain_match` function
- Cargo.toml `octo-cap-macaroon` dep added (intra-Layer B)
- `lib.rs` `pub use settlement_verify` exposes module
- `PersistedSettlementEvent` + `SettlementEventInsert` carry the new fields + SQL bindings match
- AC gate: `rg 'pub.*cost_vault_id.*Option.*\[u8; 32\]' crates/quota-router-storage/src/ask.rs` ≥ 1 hit (SettlementEnvelope field)
- AC gate: `rg 'pub enum SettlementError' crates/quota-router-storage/src/ask.rs` → 1 hit (enum def)
- AC gate: `rg 'ChainMismatch' crates/quota-router-storage/src/settlement_verify.rs` → ≥1 hit (variant used)
- AC gate: `rg 'verify_settlement_chain_match' crates/quota-router-storage/src/lib.rs` → 1 hit (pub use)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green
- `cargo fmt --all -- --check` green
- Per RFC-0206 §4: NO destructive migration to existing tables (`settlement_events` schema unchanged)

## Files / Artifacts

- Edit: `crates/quota-router-storage/src/ask.rs` (SettlementEnvelope field extension + compute_settlement_hash + SettlementError variants)
- New: `crates/quota-router-storage/src/settlement_verify.rs`
- Edit: `crates/quota-router-storage/src/lib.rs` (pub use settlement_verify)
- Edit: `crates/quota-router-storage/Cargo.toml` (octo-cap-macaroon dep)
- Edit: `crates/quota-router-storage/src/settlement_event_repo.rs` (SettlementEventInsert + SQL bindings)

## Cross-references

- RFC-0959 (wire-format v2.0 §Wire Format + §Settlement-Time Vault Row Lookup + §Cross-Chain Settlement Reject)
- RFC-0957 (VaultLookup trait reuse per §Verify-Time Extension)
- RFC-0967-A1 §2.5 (policy_kind_authority substrate for Layer B intra-deps)
- RFC-0009 (HSM-routable VaultLookup substrate)
- RFC-0206 §4 (Layer B additive-only migration rule)
- Mission `0959-c1-wire-format-amendment` (parent — owns 11-step recon scope; this mission owns substrate coding subset)
- Mission `0900-d-chain-aware-slash-ledger` (sibling — chain_id canonical mapping)

## Out of scope

- RFC-0959 v2.0 amendment filing (owned by sibling mission `0959-c1-wire-B-rfc-tv`)
- migrations/v016__settlement_chain_vault.sql (already LANDED via `0900-d` chain-aware slash ledger work; recon step 6)
- `tv_0959_settlement_wire.rs` 25 byte-exact fixtures (owned by sibling `0959-c1-wire-B-rfc-tv`)
- `kind_uuid_registry` 30-UUIDv5 namespace seeding (separate future mission)
- Live DID provisioning for treasury + corp_admin signers (separate onboarding flow)
- Shadow VaultLookup impl (REJECTED per RFC-0959 v2.0 §Settlement-Time Vault Row Lookup: must reuse capability verify-time `VaultLookup` trait)

## Dependencies

- `0959-c1-wire-format-amendment` (parent — 11-step recon)
- `0900-d-chain-aware-slash-ledger` (sibling — chain_id canonical substrate)
- RFC-0959 v2.0 (canonical Accepted — wire-format amendment)
- RFC-0967-A1 v1.9.2 (Layer B intra-dep justification)
- RFC-0009 (HSM-routable VaultLookup substrate)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                      |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per RFC-0959 v2.0 + recon 2026-08-19 audit. Substrate coding subset (steps 1, 2, 3, 4, 5, 9, 10, 11 of recon) for `SettlementEnvelope` v2.0 wire format + `verify_settlement_chain_match` algorithm. Sibling to `-B-rfc-tv`. |
