---
name: 0011-d-M5-octowallet-nonce-counter
description: Add `OctoRoleBinding` CACHED PROJECTION persistence (per F-NEW-3 substrate-truth split) to `crates/octo-wallet/src/wallet_store.rs`; implement `next_nonce_counter(operator_did) -> Result<u64, WalletError>` helper per RFC-0011-d §7.4; Stoolap `BEGIN IMMEDIATE` atomicity.
metadata:
  node_type: substrate-cli
  type: substrate-persistence
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011-d
    - RFC-0900 substrate
    - mission 0011-d-M4-octorole-select-with-stoolap-tx
status: Claimed
---

# 0011-d-M5-octowallet-nonce-counter — Wallet cached projection + `next_nonce_counter` per RFC-0011-d §7.4

**Status:** Claimed 2026-09-01 by @mmacedoeu — unblocked.
**Substrate:** RFC-0011-d §7.4 + F-NEW-3 substrate-truth split
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M4-octorole-select-with-stoolap-tx`

## Status

Claimed (2026-09-01) by @mmacedoeu. Fifth of 9 Phase 1 atomic missions. Per F-NEW-3 substrate-truth split: M4 substrate writes the slash-ledger row (authoritative); M5 adds the WALLET'S CACHED PROJECTION of that record (NOT a new authoritative table).

## Substrate (RFC-0011-d)

§7.4 companion `[ADD]`: `pub fn next_nonce_counter(operator_did: &Did) -> Result<u64, WalletError>` (atomic).

F-NEW-3 substrate-truth split: substrate (M4) owns the authoritative `role_binding` record in the slash ledger. The wallet's `OctoRoleBinding` is a CACHED PROJECTION for fast operator-UX lookup; it is derived FROM the substrate row, never the other way around.

§Non-monicity cross-reference: RFC-0900 §Slash Ledger Substrate (monotonic counter pattern).

## Parent

RFC-0011-d §7.4 (companion `[ADD]` `next_nonce_counter`); F-NEW-3 substrate-truth split; §Key Files row "Wallet cached projection".

## Depends on

- `0011-d-M4-octorole-select-with-stoolap-tx` (substrate-authoritative `role_binding` row; M5 derives cached projection)
- RFC-0900 substrate (slash ledger authoritative record)

## Acceptance Criteria

- [ ] Extend `crates/octo-wallet/src/wallet_store.rs` (NOT new module; cached projection lives alongside existing `OctoIdentity` + `OctoCapability` cached projections)
- `OctoRoleBinding` cached projection struct added (mirrors `OctoIdentity` + `OctoCapability` cached projection pattern): `(identity_did, role_id, role_kind_uuid, role_binding_hash, chain_id, created_at_unix, revoked_at_unix: Option<i64>)`
- `OctoRoleBinding` cached projection derives `Serialize` + `Deserialize` (wallet store JSON persistence)
- `OctoRoleBinding` is a CACHED PROJECTION per F-NEW-3 (NOT authoritative; do NOT add UNIQUE constraint that competes with substrate)
- `next_nonce_counter(operator_did: &Did) -> Result<u64, WalletError>` helper added to `octo-wallet`
- `next_nonce_counter` uses Stoolap `BEGIN IMMEDIATE` + `UPDATE wallet_nonce_counter SET counter = counter + 1 WHERE identity_did = ? RETURNING counter` (atomic; no race)
- `wallet_nonce_counter` table migration added (idempotent `CREATE TABLE IF NOT EXISTS`)
- Counter PERSISTS ACROSS RESTART (wallet store on disk; Layer B; parent RFC-0011 §Implicit Assumptions Audit row "Monotonic nonce counter persists across process restarts")
- `persist_role_binding_cached_projection(role_binding: &RoleBinding) -> Result<(), WalletError>` helper added (idempotent on `role_binding_hash` UNIQUE within the cached projection)
- `revoke_role_binding_cached_projection(role_binding_hash: &Hex32, revoked_at_unix: i64) -> Result<(), WalletError>` helper added
- NO CLI binding (M6)
- `cargo test -p octo-wallet next_nonce_counter_increments`
- `cargo test -p octo-wallet next_nonce_counter_concurrent_safe`
- `cargo test -p octo-wallet next_nonce_counter_persists_across_restart`
- `cargo test -p octo-wallet persist_role_binding_cached_projection_idempotent_on_hash`
- `cargo test -p octo-wallet revoke_role_binding_cached_projection_updates_timestamp`
- `cargo test -p octo-wallet wallet_schema_migration_idempotent`
- `cargo check -p octo-wallet` zero warnings
- `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean

## Scope

Wallet cached projection + nonce counter only. NO substrate-authoritative row (M4 owns). NO CLI binding (M6).

## Sub-steps

1. Extend `crates/octo-wallet/src/wallet_store.rs` (NOT new module; mirrors existing `OctoIdentity` + `OctoCapability` cached projection pattern)
2. Add `OctoRoleBinding` cached projection struct
3. Add `wallet_nonce_counter` schema migration (idempotent)
4. Add `next_nonce_counter` helper (atomic SQL update via Stoolap `BEGIN IMMEDIATE`)
5. Add `persist_role_binding_cached_projection` helper (idempotent on `role_binding_hash` UNIQUE within the cached projection)
6. Add `revoke_role_binding_cached_projection` helper
7. Wire re-exports in `octo-wallet/src/lib.rs`
8. Add 6 unit tests listed in Acceptance Criteria (incl. cross-restart persistence test)
9. Verify cargo check + clippy + fmt

## Test Vectors

- TV-WAL-1: `next_nonce_counter(operator_did)` returns monotonic counter starting at 1
- TV-WAL-2: `next_nonce_counter(operator_did)` called concurrently from 2 tasks → both return distinct counters (no race; Stoolap `BEGIN IMMEDIATE` serializes)
- TV-WAL-3: `next_nonce_counter(operator_did)` PERSISTS ACROSS RESTART (close wallet store, reopen, counter resumes from persisted value)
- TV-WAL-4: `persist_role_binding_cached_projection(rb)` first call → success
- TV-WAL-5: `persist_role_binding_cached_projection(rb)` second call with same `role_binding_hash` → idempotent success (no duplicate row in cached projection)
- TV-WAL-6: `revoke_role_binding_cached_projection(rb_hash, ts)` sets `revoked_at_unix = ts` in cached projection
- TV-WAL-7: `wallet_nonce_counter` migration runs cleanly on fresh wallet
- TV-WAL-8: `wallet_nonce_counter` migration is no-op on already-migrated wallet
- TV-WAL-9: `OctoRoleBinding` cached projection is read-only projection of substrate slash-ledger row; write-path goes through M4 substrate (NOT through cached projection)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-wallet` (Layer B; per RFC-0009) — owns identity + nonce + role_binding CACHED PROJECTION (NOT authoritative)
- `octo-role` (Layer B; per RFC-0011-d) — owns authoritative role_binding slash-ledger row (M4)
- Stoolap (Layer D adapter; per RFC-0010) — `BEGIN IMMEDIATE` atomic counter primitive

## Backward compat

Additive: new cached projection + new migration + 3 new public functions. NO existing `octo-wallet` tables modified. NO `schema_version` bump (migration is additive).

## Risk

- **Cached projection vs authoritative confusion**: implementer may treat `OctoRoleBinding` as authoritative. Mitigation: explicit F-NEW-3 framing in AC; test TV-WAL-9 enforces read-only projection pattern; code comments.
- **Concurrent nonce counter**: `BEGIN IMMEDIATE` + `UPDATE ... RETURNING` is atomic in Stoolap; concurrent calls serialize via write lock. Mitigation: explicit `BEGIN IMMEDIATE` test (TV-WAL-2).
- **Migration ordering**: `wallet_nonce_counter` table must exist before `OctoRoleBinding` cached projection. Mitigation: dependency in migration runner; test fixture creates schema in order.
- **`role_binding_hash` UNIQUE collision within cached projection**: SHA-256-bits collision is astronomically unlikely; BLAKE3-256 same. Mitigation: rely on cryptographic uniqueness; document in code comment.
- **Cross-restart persistence**: wallet store JSON on disk. Mitigation: explicit persistence test (TV-WAL-3); Layer B monotonicity contract per parent RFC-0011 §Implicit Assumptions Audit.

## Notes

- M4 caller wires `next_nonce_counter` → `select(nonce_counter, ...)` → `persist_role_binding_cached_projection(result)` in the CLI binding layer (M6)
- OctoRoleBinding cached projection revocation: lands in this mission (NOT M10/M11; revocation is core substrate, not Phase 2)
- Tests TV-WAL-2 uses concurrent tasks; Stoolap single-writer means second call blocks until first commits
- Cached projection is READ-ONLY projection of substrate slash-ledger row (F-NEW-3 substrate-truth split); write-path ALWAYS goes through M4 substrate

## Cross-references

- RFC-0011-d §7.4 (companion `[ADD]` `next_nonce_counter`)
- RFC-0011-d §Key Files row "Wallet cached projection"
- F-NEW-3 substrate-truth split (substrate-authoritative vs wallet cached projection)
- RFC-0900 §Slash Ledger Substrate (monotonic counter pattern)
- RFC-0009 (octo-wallet substrate baseline)
- [[cipherocto-design-principles]] — Layer B stability contract; no premature coupling (cached projection ≠ authoritative)

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
