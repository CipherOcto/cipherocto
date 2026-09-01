---
name: 0011-d-M4-octorole-select-with-stoolap-tx
description: Implement `octo_role::select(role_id, operator_did, signer)` per RFC-0011-d §7.4 Substrate `[ADD]` signature; sync fn; `CapabilitySigner` from `octo-cap-macaroon`; Stoolap `BEGIN IMMEDIATE` envelope-build per §7.6; substrate-authoritative persistence; consumes `octo_wallet::next_nonce_counter` inside same tx (M5).
metadata:
  node_type: substrate-cli
  type: substrate-entrypoint-write
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.1"
  depends_on:
    - RFC-0011-d
    - RFC-0900 substrate
    - mission 0011-d-M2-octorole-types-and-errors
    - mission 0011-d-M3-octorole-list-show
status: Open
---

# 0011-d-M4-octorole-select-with-stoolap-tx — Write-side substrate entrypoint per RFC-0011-d §7.4

**Status:** Open (2026-08-31) — unblocked.
**Substrate:** RFC-0011-d §7.4 Substrate `[ADD]` signature — `octo_role::select`
**Parent:** RFC-0011-d
**Depends on:** `0011-d-M2-octorole-types-and-errors` + `0011-d-M3-octorole-list-show` + RFC-0900 substrate

## Status

Open (2026-08-31). Fourth of 9 Phase 1 atomic missions. Lands the write-side substrate entrypoint that CLI `select` subcommand will call in M6. Substrate writes the role-binding record to the substrate's OWN store (slash ledger per RFC-0900); M5 adds the wallet's cached projection.

## Substrate (RFC-0011-d)

§7.4 Substrate `[ADD]` signature (canonical):

- `pub fn select(role_id: &str, operator_did: &Did, signer: &dyn CapabilitySigner) -> Result<RoleBinding, RoleError>` (sync; write-path)

§7.6 Role Select — HSM Signing Flow (canonicalize → read stake → sign → verify atomic; Stoolap `BEGIN IMMEDIATE` tx envelope-build).

§7.4 + §7.6 companion `[ADD]`: `pub fn next_nonce_counter(operator_did: &Did) -> Result<u64, WalletError>` — substrate consumes inside same `BEGIN IMMEDIATE` tx.

§7.4 substrate-truth note: the substrate `select` writes the role-binding record to the substrate's OWN store (slash ledger per RFC-0900) and creates the slash-ledger row. M5 only adds the wallet's cached projection.

## Parent

RFC-0011-d §7.4 (Substrate `[ADD]` signatures); §7.6 (Role Select HSM Signing Flow); §7.4 substrate-truth note (slash-ledger authoritative record); §Key Files row "Command dispatch".

## Depends on

- `0011-d-M2-octorole-types-and-errors` (RoleBinding + RoleError types)
- `0011-d-M3-octorole-list-show` (show() to validate role_id exists)
- RFC-0900 substrate (slash ledger PK + envelope + signature scheme)

## Acceptance Criteria

- [ ] `pub fn select(role_id: &str, operator_did: &Did, signer: &dyn CapabilitySigner) -> Result<RoleBinding, RoleError>` implemented in `crates/octo-role/src/lib.rs` (SYNC per RFC §7.4; NOT async)
- `operator_did: &Did` parameter REQUIRED per RFC §7.4 (slash-ledger PK + last-writer-wins + signer-invariant substrate check)
- `signer: &dyn CapabilitySigner` (NOT `OctoSigner`; per RFC §7.4 + parent RFC-0011 §Subcommand Taxonomy entry #10; trait lives in `octo-cap-macaroon`)
- Stoolap `BEGIN IMMEDIATE` transaction (write lock; per §7.6 footnote)
- Substrate writes the role-binding record to substrate's OWN store (slash ledger per RFC-0900); creates slash-ledger row (per §7.4 substrate-truth note)
- Substrate consumes `octo_wallet::next_nonce_counter(operator_did)` INSIDE same `BEGIN IMMEDIATE` tx (per §7.4 + §7.6 sequence diagram; companion `[ADD]` per §7.4)
- Canonicalize → read stake → sign → verify atomic sequence per §7.6 sequence diagram
- `body_hash` = BLAKE3-256 over canonical `RoleBinding` serialization (RFC-0104 canonical encoding)
- Returns `RoleError::RoleNotFound { role_id }` if role_id missing in registry
- Returns `RoleError::StakeInsufficient { required, available }` if stake check fails (per RoleRecord `requires_octo_min`)
- Returns `RoleError::SignerMismatch` if `signer.did()` ≠ `operator_did` (slash-ledger PK invariant)
- Re-select with same `(operator_did, role_id, chain_id)` UPDATES existing row (last-writer-wins per RFC-0011-d §Security 2; substrate enforces atomically inside envelope-build step); NO `RoleBindingConflict` variant surfaced — duplicate inserts are guarded by Stoolap schema UNIQUE constraint as defense-in-depth only, never raised to substrate caller
- `RoleBinding.role_binding_hash: Hex32` computed as BLAKE3-256 over `body_bytes || signature_proof`
- Returns `RoleBinding` with `signature_proof` field redacted as `RedactedHex` (CLI redaction boundary per RFC-0011-d §Key Files)
- NO wallet persistence in this mission (M5 owns `OctoRoleBinding` CACHED PROJECTION persistence to wallet store per F-NEW-3 substrate-truth split)
- `cargo test -p octo-role select_creates_signed_envelope`
- `cargo test -p octo-role select_returns_role_not_found_on_missing`
- `cargo test -p octo-role select_returns_stake_insufficient_rolls_back_tx`
- `cargo test -p octo-role select_returns_signer_mismatch_rolls_back_tx`
- `cargo test -p octo-role select_overwrites_existing_binding_last_writer_wins`
- `cargo test -p octo-role select_uses_begin_immediate_transaction`
- `cargo test -p octo-role select_consumes_nonce_counter_inside_tx`
- `cargo check -p octo-role` zero warnings
- `cargo clippy -p octo-role --all-targets -- -D warnings` clean

## Scope

Substrate write-side only. NO wallet cached projection persistence (M5). NO CLI binding (M6).

## Sub-steps

1. Add `octo-cap-macaroon` dep usage for `CapabilitySigner` trait
2. Add `octo-slash-ledger` dep usage for envelope signing
3. Add `pub fn select(role_id, operator_did, signer)` SYNC to lib.rs
4. Wire Stoolap `BEGIN IMMEDIATE` transaction
5. Wire `next_nonce_counter(operator_did)` call INSIDE same tx (companion `[ADD]` per §7.4)
6. Wire canonicalize → read stake → sign → verify atomic sequence per §7.6
7. Wire slash-ledger row creation (substrate-authoritative per §7.4 substrate-truth note)
8. Wire stake check (`requires_octo_min` vs available)
9. Wire signer DID check (`signer.did() == operator_did`)
10. Wire binding conflict check (UNIQUE constraint query)
11. Wire envelope signing via `octo-slash-ledger::sign_envelope`
12. Wire `body_hash` + `role_binding_hash` computation
13. Wire redaction boundary on `signature_proof` return
14. Add 7 unit tests listed in Acceptance Criteria
15. Verify cargo check + clippy + fmt

## Test Vectors

Per RFC-0011-d §11:

- TV-RX-1: `select(role_id, operator_did, signer)` with valid role_id + valid stake + matching signer.did() + matching operator_did → returns `RoleBinding`; Stoolap tx commits
- TV-RX-2: `select(role_id, operator_did, signer)` with role_id missing in registry → returns `RoleError::RoleNotFound`; tx rolls back
- TV-RX-3: `select(role_id, operator_did, signer)` with stake < `requires_octo_min` → returns `RoleError::StakeInsufficient`; tx rolls back
- TV-RX-4: `select(role_id, operator_did, signer)` with `signer.did() ≠ operator_did` → returns `RoleError::SignerMismatch`; tx rolls back
- TV-RX-5: `select(role_id, operator_did, signer)` called twice with same (operator_did, role_id, chain_id) → second call OVERWRITES (last-writer-wins per RFC-0011-d §Security 2); returns updated `RoleBinding`; tx commits; binding row replaced atomically inside envelope-build step
- TV-RX-6: Stoolap `BEGIN IMMEDIATE` tx commits on success, rolls back on substrate error
- TV-RX-7: `next_nonce_counter(operator_did)` consumed INSIDE same tx (assert via test fixture)
- TV-RX-8: `role_binding_hash` deterministic for same input (RFC-0104 DFP)
- TV-RX-9: `signature_proof` field redacted in return value (assert via test)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-role` (Layer B; per RFC-0011-d §7.4) — owns the write-path substrate + slash-ledger row creation
- `octo-cap-macaroon` (Layer B; per RFC-0957) — owns `CapabilitySigner` trait
- `octo-slash-ledger` (Layer B; per RFC-0900) — owns envelope signing substrate
- `octo-wallet` (Layer B; per RFC-0009) — owns `next_nonce_counter` (M5 wires persistence)
- Stoolap (Layer D adapter; per RFC-0010) — `BEGIN IMMEDIATE` transaction primitive

## Backward compat

Additive: new public sync fn. No existing substrate crates modified. No `schema_version` bump (slash-ledger schema already supports `role_binding` table).

## Risk

- **Concurrent select on same identity**: Stoolap `BEGIN IMMEDIATE` serializes. Mitigation: schema UNIQUE constraint as second line of defense.
- **Nonce counter drift**: M5 provides wallet-backed monotonic counter; until M5 lands, `next_nonce_counter` reads substrate-only. Mitigation: explicit atomic SQL update; M5 adds wallet cached projection.
- **Stake oracle**: `requires_octo_min` check requires live stake view. Mitigation: substrate reads via `octo-stake-oracle` (pre-existing crate; not in this mission scope but flagged as dependency).
- **Body hash canonicalization**: RFC-0104 DFP serialization. Mitigation: use existing canonical encoding helper from `cipherocto-encoding`.
- **Substrate-authoritative vs wallet cached projection**: M4 owns substrate; M5 owns cached projection. Mitigation: F-NEW-3 substrate-truth split documented in M5.

## Notes

- SYNC fn — per RFC §7.4 substrate signature (NOT async)
- M4 substrate creates slash-ledger row (authoritative per F-NEW-3)
- M5 adds wallet cached projection (per F-NEW-3)
- M6 adds CLI binding
- M8 adds CLI integration test vectors

## Cross-references

- RFC-0011-d §7.4 (Substrate `[ADD]` signatures; substrate-truth note)
- RFC-0011-d §7.6 (Role Select HSM Signing Flow; sequence diagram)
- RFC-0957 (CapabilitySigner trait substrate)
- RFC-0900 (slash ledger substrate)
- RFC-0104 (DFP canonical encoding)
- F-NEW-3 substrate-truth split (substrate-authoritative vs wallet cached projection)

## Claimant

@unassigned (mission lifecycle: Open)
