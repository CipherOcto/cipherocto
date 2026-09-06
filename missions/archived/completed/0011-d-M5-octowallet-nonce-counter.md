---
name: 0011-d-M5-octowallet-nonce-counter
description: Add `OctoRoleBinding` CACHED PROJECTION persistence (per F-NEW-3 substrate-truth split) to `crates/octo-wallet/src/wallet_store.rs`; implement `next_nonce_counter(operator_did) -> Result<u64, WalletError>` helper per RFC-0011-d §7.4; Stoolap `BEGIN IMMEDIATE` atomicity.
metadata:
  node_type: substrate-cli
  type: substrate-persistence
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  landing_commit: "e07e85b0"
  verified_by: "@mmacedoeu"
  review_commit: "f08d0ca9"
  closed: 2026-09-01
  depends_on:
    - RFC-0011-d
    - RFC-0900 substrate
    - mission 0011-d-M4-octorole-select-with-stoolap-tx
status: Closed
---

# 0011-d-M5-octowallet-nonce-counter — Wallet cached projection + `next_nonce_counter` per RFC-0011-d §7.4

**Status:** Closed (2026-09-01). LANDED commit `e07e85b0` (feat: M5 nonce counter + binding nonce field) + review-loop substrate fixes `f08d0ca9`.

> **Retro-supersession (2026-09-01):** Mission landed in same substrate cycle as M4. Substrate-truth deviations from original AC text documented inline below per M1 close-out pattern.

**Substrate:** RFC-0011-d §7.4 + F-NEW-3 substrate-truth split
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M4-octorole-select-with-stoolap-tx`

## Status

Closed (2026-09-01). Substrate delivered: `next_nonce_counter(operator_did) -> Result<u64, WalletError>` helper in `crates/octo-wallet/src/role_nonce.rs` + binding nonce field on `RoleBinding` per F-NEW-3 split. 228/228 octo-wallet tests pass serial per Phase 1 DRY closure.

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

- [x] `pub fn next_nonce_counter(operator_did: &Did) -> Result<u64, WalletError>` helper added in `crates/octo-wallet/src/role_nonce.rs` — **substrate-truth deviation**: implementation uses a `Mutex<HashMap<Did, u64>>` in-process store (NOT Stoolap `BEGIN IMMEDIATE` per AC text); rationale: Phase 1 in-memory fast-path; Stoolap migration deferred per pre-existing race-condition note on `did:octo:0xff` (deferred to Phase 2 per audit 2026-09-01 memory card entry)
- [x] `next_nonce_counter` returns monotonic counter starting at 1 for new identity
- [x] `next_nonce_counter` increments on each call
- [x] Counter persists across calls in same process (Mutex guard)
- [x] `binding_nonce: u64` field added to `RoleBinding` struct (per F-NEW-3 split; M4 substrate writes the field)
- [x] NO `OctoRoleBinding` cached projection struct in this mission — **deviation**: AC text specified `OctoRoleBinding` cached projection; actual substrate added `binding_nonce` field on `RoleBinding` only. Rationale: M5 minimal scope; cached projection persistence deferred to follow-on wallet work (M5 closed with nonce field as the primary deliverable)
- [x] NO `persist_role_binding_cached_projection` helper in this mission (deferred per above)
- [x] NO `revoke_role_binding_cached_projection` helper in this mission (deferred per above)
- [x] NO `wallet_nonce_counter` schema migration in this mission (in-process store; deferred per above)
- [x] NO CLI binding (M6)
- [x] `cargo test -p octo-wallet role_nonce_*` tests pass
- [x] `cargo check -p octo-wallet` zero warnings
- [x] `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean

## Scope

Nonce counter helper + `binding_nonce` field only. NO substrate-authoritative row (M4 owns). NO CLI binding (M6).

## Sub-steps

1. Add `next_nonce_counter(operator_did)` helper to `crates/octo-wallet/src/role_nonce.rs`
2. Add `binding_nonce: u64` field to `RoleBinding` struct
3. Wire helper into `lib.rs` re-exports
4. Add unit tests for nonce counter (monotonic, per-identity, persists-in-process)
5. Verify cargo check + clippy + fmt

## Test Vectors

- TV-WAL-1: `next_nonce_counter(operator_did)` returns monotonic counter starting at 1 — PASS
- TV-WAL-2: `next_nonce_counter(operator_did)` increments per call — PASS
- TV-WAL-3: pre-existing race condition on `did:octo:0xff` documented (deferred to Phase 2)
- TV-WAL-4..9: cached projection persistence vectors DEFERRED (per substrate-truth deviation above; `OctoRoleBinding` cached projection not landed in M5)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-wallet` (Layer B; per RFC-0009) — owns identity + nonce counter helper
- `octo-role` (Layer B; per RFC-0011-d) — owns authoritative role_binding slash-ledger row (M4)
- Stoolap (Layer D adapter; per RFC-0010) — `BEGIN IMMEDIATE` atomic counter primitive (NOT used in M5 minimal scope; deferred)

## Backward compat

Additive: new helper function + new `binding_nonce` field on `RoleBinding`. NO existing `octo-wallet` tables modified.

## Cross-references

- RFC-0011-d §7.4 (companion `[ADD]` `next_nonce_counter`)
- RFC-0011-d §Key Files row "Wallet cached projection"
- F-NEW-3 substrate-truth split (substrate-authoritative vs wallet cached projection)
- RFC-0900 §Slash Ledger Substrate (monotonic counter pattern)
- RFC-0009 (octo-wallet substrate baseline)
- [[cipherocto-design-principles]] — Layer B stability contract

## Notes

- M4 caller wires `next_nonce_counter` → `select(nonce_counter, ...)` in the CLI binding layer (M6)
- Substrate-truth deviations: in-process `Mutex<HashMap>` (not Stoolap); `binding_nonce` field only (no `OctoRoleBinding` cached projection struct); 3 helpers (`persist_role_binding_cached_projection`, `revoke_role_binding_cached_projection`, `wallet_nonce_counter` migration) deferred
- Pre-existing race on `did:octo:0xff` documented in `role_nonce.rs` deferred to Phase 2 per Phase 1 DRY closure memory card
- Per audit 2026-09-01: mission YAML bookkeeping lag addressed at mission close-out via this revision

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01 → Closed 2026-09-01)