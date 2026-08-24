---
name: 0959-c1-wire-B-rfc-tv
description: Land RFC-0959 v2.0 wire-format amendment documentation + test-vector subset per recon 2026-08-19: file RFC-0959 v2.0 amendment (Status header v1.0 → v2.0, §Wire Format v2.0 subsection, NEW §Settlement-Time Vault Row Lookup subsection, NEW §Cross-Chain Settlement Reject subsection, §Version History v2.0 row); create `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` with 25 byte-exact fixtures covering v1.0/v2.0 envelope hash preimage + chain-match accept/reject + cost_vault_id_missing rejection. Per RFC-0206 §4 Layer B additive-only rule, v015 slot already taken by `v015__chain_aware_slash_ledger.sql` per recon; this mission verifies v016 already exists or creates if not.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0959-c1-wire-format-amendment
    - 0959-c1-wire-A-substrate-verify
    - 0900-d-chain-aware-slash-ledger
    - RFC-0959
    - RFC-0126
status: OPEN
---

# Mission `0959-c1-wire-B-rfc-tv` v1.0 — OPEN 2026-08-24

## Context

RFC-0959 v2.0 wire-format amendment landed 2026-08-19 in canonical Accepted file (`rfcs/accepted/economics/0959-ask-settlement-chain.md` Status header), but the substrate work (per recon 2026-08-19) is split across 11 steps. This mission owns the documentation + test-vector subset (steps 6, 7, 8 of recon):

- Step 6: `migrations/v016__settlement_chain_vault.sql` — verify existence (recon says LANDED via 0900-d follow-on); create if absent
- Step 7: `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` — 25 byte-exact TV
- Step 8: RFC-0959 v2.0 amendment (Status header + VH v2.0 row + §Wire Format v2.0 + §Settlement-Time Vault Row Lookup + §Cross-Chain Settlement Reject)

## Scope

### Step 6: migrations/v016 verification

Check `crates/quota-router-storage/migrations/v016__settlement_chain_vault.sql` exists. Per recon 2026-08-19: column type unchanged (16-byte BE DqaEncoding via scale=0 i64-bridge per RFC-0960 §Vault Substrate + 0900-d AC-2 sub-clause); NEW column `cost_vault_id BLOB(32) NULL` per §20.7; NEW column `chain_id BLOB(32) NULL` per §20.7; NEW `CREATE UNIQUE INDEX idx_se_cost_vault_id ON settlement_events(cost_vault_id)` per §20.7 audit query; backfill legacy v004 rows with `NULL`.

If absent, create per above spec. If present, verify content matches spec.

### Step 7: 25 byte-exact TV

Create `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs`:

| #     | TV name                            | Coverage                                                                                                                                                                               |
| ----- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1-5   | v1.0 envelope hash preimage        | 5 fixtures: minimal envelope + 4 corner-case variants (zero cost, max cost, varint axes lengths)                                                                                       |
| 6-10  | v2.0 envelope hash preimage        | 5 fixtures: minimal v2.0 (cost_vault_id present, chain_id present) + 4 variants (None/None edge, mixed presence tags, RFC-0010 §2 chain_id canonical form)                             |
| 11-15 | chain-match accept                 | 5 fixtures: vault row lookup returns row with matching chain_id → `Ok(())`                                                                                                             |
| 16-20 | chain-match reject (ChainMismatch) | 5 fixtures: vault row lookup returns row with divergent chain_id → `Err(SettlementError::ChainMismatch)`                                                                               |
| 21-25 | edge cases                         | 5 fixtures: cost_vault_id missing on v2.0 → `Err(CostVaultIdMissing)`; vault_lookup returns None → `Err(VaultLookupNotFound)`; v1.0 envelope replay vs v2.0 hash → `Err(HashMismatch)` |

### Step 8: RFC-0959 v2.0 amendment

Edit `rfcs/accepted/economics/0959-ask-settlement-chain.md`:

- Status header: `v1.0 (2026-07-20)` → add v2.0 row documenting wire-format amendment
- §Version History: add v2.0 row (2026-08-19) citing missions `0959-c1-wire-A-substrate-verify` + `0959-c1-wire-B-rfc-tv`
- §Wire Format v2.0 subsection (replaces v1.0 §Wire Format) — field list with byte offsets + `DqaEncoding` 16-byte BE encoding + `cost_vault_id` derivation cross-ref to RFC-0960 + `chain_id` derivation cross-ref to RFC-0010 v1.6
- §Settlement-Time Vault Row Lookup subsection (NEW) — 3-step algorithm: `cost_vault_id` present → `vault_lookup.lookup_vault(cost_vault_id)` → `vault.chain_id == envelope.chain_id` else `SettlementError::ChainMismatch`
- §Cross-Chain Settlement Reject subsection (NEW) — explicit reject invariant per §20.7
- Cross-ref to RFC-0957 §Verify-Time Extension (same `VaultLookup` trait shared between capability + settlement paths)

## Acceptance Criterion

- `migrations/v016__settlement_chain_vault.sql` exists with `cost_vault_id BLOB(32) NULL` + `chain_id BLOB(32) NULL` columns + `idx_se_cost_vault_id` unique index
- `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` exists with 25 fixtures
- RFC-0959 Status header documents v2.0 wire-format amendment (2026-08-19)
- RFC-0959 §Wire Format v2.0 subsection replaces v1.0 §Wire Format (preimage spec)
- RFC-0959 §Settlement-Time Vault Row Lookup subsection (NEW) — 3-step algorithm
- RFC-0959 §Cross-Chain Settlement Reject subsection (NEW) — explicit reject invariant
- AC gate: `rg 'cost_vault_id.*BLOB\(32\)' crates/quota-router-storage/migrations/v016*` → ≥1 hit
- AC gate: `rg 'cost_vault_id.*Option.*\[u8; 32\]' rfcs/accepted/economics/0959-ask-settlement-chain.md` → ≥1 hit (RFC §Wire Format v2.0 mentions field)
- AC gate: `rg 'fn tv_0[1-9]|fn tv_1[0-9]|fn tv_2[0-5]' crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` → ≥20 hits (25 TV functions)
- AC gate: `rg 'Settlement-Time Vault Row Lookup' rfcs/accepted/economics/0959-ask-settlement-chain.md` → 1 hit (NEW subsection)
- AC gate: `rg 'Cross-Chain Settlement Reject' rfcs/accepted/economics/0959-ask-settlement-chain.md` → 1 hit (NEW subsection)
- `cargo test -p quota-router-storage --test tv_0959_settlement_wire` → 25/25 green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green
- `cargo fmt --all -- --check` green
- Prettier formatting clean (RFC markdown file)
- Guard 2 cite validation PASS

## Files / Artifacts

- New/Verify: `crates/quota-router-storage/migrations/v016__settlement_chain_vault.sql`
- New: `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` (25 byte-exact fixtures)
- Edit: `rfcs/accepted/economics/0959-ask-settlement-chain.md` (Status + VH + §Wire Format v2.0 + §Settlement-Time Vault Row Lookup + §Cross-Chain Settlement Reject)

## Cross-references

- RFC-0959 v2.0 (canonical Accepted; this mission extends spec with 3 NEW subsections)
- RFC-0010 v1.6 (chain_id canonical 32-byte form for §Wire Format cross-ref)
- RFC-0960 (vault substrate for cost_vault_id derivation cross-ref)
- RFC-0957 (VaultLookup trait reuse cross-ref in §Settlement-Time Vault Row Lookup)
- RFC-0126 (canonical_ser for canonical_axes_consumed encoding)
- RFC-0206 §4 (Layer B additive-only migration rule for v016 ownership)
- Mission `0959-c1-wire-format-amendment` (parent — 11-step recon)
- Mission `0959-c1-wire-A-substrate-verify` (sibling — substrate coding)
- Mission `0900-d-chain-aware-slash-ledger` (sibling — chain_id canonical substrate)

## Out of scope

- Substrate code changes (owned by sibling `0959-c1-wire-A-substrate-verify`)
- New SettlementError variants (owned by sibling `0959-c1-wire-A-substrate-verify`)
- `settlement_verify.rs` module creation (owned by sibling `0959-c1-wire-A-substrate-verify`)
- Cargo.toml dep + lib.rs pub use (owned by sibling `0959-c1-wire-A-substrate-verify`)
- `kind_uuid_registry` 30-UUIDv5 namespace seeding (separate future mission per RFC-0967-A1 §2.6)
- Live DID provisioning (separate onboarding flow)

## Dependencies

- `0959-c1-wire-format-amendment` (parent — 11-step recon)
- `0959-c1-wire-A-substrate-verify` (sibling — substrate coding, must land first or in lockstep)
- `0900-d-chain-aware-slash-ledger` (sibling — chain_id canonical substrate)
- RFC-0959 v2.0 (canonical Accepted)
- RFC-0126 (canonical_ser substrate)

## Version History

| Version | Date       | Change                                                                                                                                                                                                     |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per RFC-0959 v2.0 + recon 2026-08-19 audit. RFC + TV + migrations verification subset (steps 6, 7, 8 of recon) for `SettlementEnvelope` v2.0 wire format. Sibling to `-A-substrate-verify`. |
