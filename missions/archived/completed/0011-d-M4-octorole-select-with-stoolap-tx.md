---
name: 0011-d-M4-octorole-select-with-stoolap-tx
description: Implement `octo_role::select(role_id, operator_did, signer)` per RFC-0011-d §7.4 Substrate `[ADD]` signature; sync fn; `CapabilitySigner` from `octo-cap-macaroon`; Stoolap `BEGIN IMMEDIATE` envelope-build per §7.6; substrate-authoritative persistence; consumes `octo_wallet::next_nonce_counter` inside same tx (M5).
metadata:
  node_type: substrate-cli
  type: substrate-entrypoint-write
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  landing_commit: "3842e8c6"
  verified_by: "@mmacedoeu"
  review_commit: "f08d0ca9"
  closed: 2026-09-01
  depends_on:
    - RFC-0011-d
    - RFC-0900 substrate
    - mission 0011-d-M2-octorole-types-and-errors
    - mission 0011-d-M3-octorole-list-show
status: Closed
---

# 0011-d-M4-octorole-select-with-stoolap-tx — Write-side substrate entrypoint per RFC-0011-d §7.4

**Status:** Closed (2026-09-01). LANDED commit `3842e8c6` (feat: M4 select write-path + last-writer-wins) + review-loop substrate fixes `f08d0ca9`.

> **Retro-supersession (2026-09-01):** Mission landed in same substrate cycle as M1-M3 + M5. Substrate-truth deviations from original AC text documented inline below per M1 close-out pattern.

**Substrate:** RFC-0011-d §7.4 Substrate `[ADD]` signature — `octo_role::select`
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M2-octorole-types-and-errors` + `0011-d-M3-octorole-list-show` + RFC-0900 substrate

## Status

Closed (2026-09-01). Substrate delivered: `pub fn select(role_id, operator_did, signer, now_unix_seconds) -> Result<RoleBinding, RoleError>` (SYNC) + last-writer-wins semantics + `select_with_chain_id` variant in `crates/octo-role/src/select.rs` + `crates/octo-role/src/default_store.rs`. 406/406 tests pass serial per Phase 1 DRY closure.

## Substrate (RFC-0011-d)

§7.4 Substrate `[ADD]` signature (canonical):

- `pub fn select(role_id: &str, operator_did: &Did, signer: &dyn CapabilitySigner) -> Result<RoleBinding, RoleError>` (sync; write-path) — **deviation**: actual signature `pub fn select(role_id, operator_did, signer, now_unix_seconds: i64) -> Result<RoleBinding, RoleError>`; explicit `now_unix_seconds: i64` parameter added per RFC-0008 Class A determinism (substrate MUST NOT call `SystemTime::now()` directly).

§7.6 Role Select — HSM Signing Flow (canonicalize → read stake → sign → verify atomic; Stoolap `BEGIN IMMEDIATE` tx envelope-build).

§7.4 + §7.6 companion `[ADD]`: `pub fn next_nonce_counter(operator_did: &Did) -> Result<u64, WalletError>` — substrate consumes inside same `BEGIN IMMEDIATE` tx.

§7.4 substrate-truth note: the substrate `select` writes the role-binding record to the substrate's OWN store (slash ledger per RFC-0900) and creates the slash-ledger row. M5 only adds the wallet's cached projection.

## Parent

RFC-0011-d §7.4 (Substrate `[ADD]` signatures); §7.6 (Role Select HSM Signing Flow); §7.4 substrate-truth note (slash-ledger authoritative record); §Key Files row "Command dispatch".

## Acceptance Criteria

- [x] `pub fn select(role_id, operator_did, signer, now_unix_seconds)` SYNC implemented in `crates/octo-role/src/select.rs` — **substrate-truth deviation**: extra `now_unix_seconds: i64` parameter added per RFC-0008 Class A determinism contract; substrate MUST NOT call `SystemTime::now()` directly
- [x] `operator_did: &Did` parameter REQUIRED per RFC §7.4 (slash-ledger PK + last-writer-wins + signer-invariant substrate check)
- [x] `signer: &dyn CapabilitySigner` (per RFC §7.4 + parent RFC-0011 §Subcommand Taxonomy entry #10; trait lives in `octo-cap-macaroon`)
- [x] Stoolap `BEGIN IMMEDIATE` transaction (write lock; per §7.6 footnote)
- [x] Substrate writes role-binding record to substrate's OWN store (slash ledger per RFC-0900)
- [x] Substrate consumes `octo_wallet::next_nonce_counter(operator_did)` INSIDE same `BEGIN IMMEDIATE` tx
- [x] Canonicalize → read stake → sign → verify atomic sequence per §7.6 sequence diagram
- [x] `body_hash` = BLAKE3-256 over canonical `RoleBinding` serialization (RFC-0104 canonical encoding)
- [x] Returns `RoleError::RoleNotFound { role_id }` if role_id missing in registry
- [x] Returns `RoleError::StakeInsufficient { required, available }` if stake check fails
- [x] Returns `RoleError::SignerMismatch` if `signer.did()` ≠ `operator_did`
- [x] Re-select with same `(operator_did, role_id, chain_id)` UPDATES existing row (last-writer-wins per RFC-0011-d §Security 2); NO `RoleBindingConflict` variant surfaced
- [x] `RoleBinding.role_binding_hash: Hex32` computed as BLAKE3-256 over canonical serialization
- [x] Returns `RoleBinding` with `signature_proof` field redacted as `RedactedHex` (CLI redaction boundary)
- [x] NO wallet persistence in this mission (M5 owns `OctoRoleBinding` CACHED PROJECTION persistence)
- [x] `cargo test -p octo-role select_*` 7 tests pass (creates_signed_envelope, returns_role_not_found_on_missing, returns_stake_insufficient_rolls_back_tx, returns_signer_mismatch_rolls_back_tx, overwrites_existing_binding_last_writer_wins, uses_begin_immediate_transaction, consumes_nonce_counter_inside_tx)
- [x] `cargo check -p octo-role` zero warnings
- [x] `cargo clippy -p octo-role --all-targets -- -D warnings` clean

## Scope

Substrate write-side only. NO wallet cached projection persistence (M5). NO CLI binding (M6).

## Sub-steps

1. Add `octo-cap-macaroon` dep usage for `CapabilitySigner` trait
2. Add `octo-slash-ledger` dep usage for envelope signing
3. Add `pub fn select(role_id, operator_did, signer, now_unix_seconds)` SYNC to lib.rs
4. Wire Stoolap `BEGIN IMMEDIATE` transaction
5. Wire `next_nonce_counter(operator_did)` call INSIDE same tx
6. Wire canonicalize → read stake → sign → verify atomic sequence
7. Wire slash-ledger row creation (substrate-authoritative per §7.4 substrate-truth note)
8. Wire stake check (`requires_octo_min` vs available)
9. Wire signer DID check (`signer.did() == operator_did`)
10. Wire envelope signing via `octo-slash-ledger::sign_envelope`
11. Wire `body_hash` + `role_binding_hash` computation
12. Wire redaction boundary on `signature_proof` return
13. Add 7 unit tests
14. Verify cargo check + clippy + fmt

## Test Vectors

Per RFC-0011-d §11:

- TV-RX-1: `select(role_id, operator_did, signer, now)` with valid inputs → returns `RoleBinding`; Stoolap tx commits
- TV-RX-2: `select(role_id, operator_did, signer, now)` with missing role_id → returns `RoleError::RoleNotFound`; tx rolls back
- TV-RX-3: `select(role_id, operator_did, signer, now)` with insufficient stake → returns `RoleError::StakeInsufficient`; tx rolls back
- TV-RX-4: `select(role_id, operator_did, signer, now)` with signer mismatch → returns `RoleError::SignerMismatch`; tx rolls back
- TV-RX-5: `select(role_id, operator_did, signer, now)` called twice with same key → second OVERWRITES (last-writer-wins); tx commits
- TV-RX-6: Stoolap `BEGIN IMMEDIATE` tx commits on success, rolls back on error
- TV-RX-7: `next_nonce_counter(operator_did)` consumed INSIDE same tx
- TV-RX-8: `role_binding_hash` deterministic for same input
- TV-RX-9: `signature_proof` field redacted in return value

## Layer direction (per [[cipherocto-design-principles]])

- `octo-role` (Layer B; per RFC-0011-d §7.4) — owns the write-path substrate + slash-ledger row creation
- `octo-cap-macaroon` (Layer B; per RFC-0957) — owns `CapabilitySigner` trait
- `octo-slash-ledger` (Layer B; per RFC-0900) — owns envelope signing substrate
- `octo-wallet` (Layer B; per RFC-0009) — owns `next_nonce_counter` (M5 wires persistence)
- Stoolap (Layer D adapter; per RFC-0010) — `BEGIN IMMEDIATE` transaction primitive

## Backward compat

Additive: new public sync fn. No existing substrate crates modified. No `schema_version` bump.

## Cross-references

- RFC-0011-d §7.4 (Substrate `[ADD]` signatures; substrate-truth note)
- RFC-0011-d §7.6 (Role Select HSM Signing Flow; sequence diagram)
- RFC-0957 (CapabilitySigner trait substrate)
- RFC-0900 (slash ledger substrate)
- RFC-0104 (DFP canonical encoding)
- RFC-0008 §Class A determinism (now_unix_seconds parameter pattern)
- F-NEW-3 substrate-truth split (substrate-authoritative vs wallet cached projection)

## Notes

- SYNC fn — per RFC §7.4 substrate signature
- M4 substrate creates slash-ledger row (authoritative per F-NEW-3)
- M5 adds wallet cached projection (per F-NEW-3)
- M6 adds CLI binding
- M8 adds CLI integration test vectors
- Substrate-truth deviation: `now_unix_seconds: i64` parameter added for RFC-0008 Class A determinism contract
- Per audit 2026-09-01: mission YAML bookkeeping lag addressed at mission close-out via this revision

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01 → Closed 2026-09-01)