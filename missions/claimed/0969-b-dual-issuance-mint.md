# Mission: Dual-Issuance Mint (RFC-0969 §Phase 2)

## Status

Claimed (2026-08-04)

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0969-dual-pipeline-authorization.md` (top-level decomposition mission)

## Summary

Implement RFC-0969 §Phase 2: dual-issuance mint that atomically issues both bearer + capability via `txn.insert_dual`. Author `mint_dual(ask: Ask, seller_key: &Ed25519Keypair, buyer_pub: &IdentityKey, holder: &IdentityKey, ask_ttl_unix: u64) -> Result<(BearerCapsule, CapabilityToken), MintError>` algorithm. `mint_dual` uses the canonical 4-arg persistence-free `CapabilityToken::mint` (per RFC-0957-A1 amendment), then atomically inserts both records via `TransactionExt::insert_dual`. Explicit `ask_ttl_unix` parameter per Round 2 (R10-N5 fix).

Phantom type `IdentityKey::from_public_bytes` call site is at `mint_dual` (buyer pubkey extraction). Stub lives in top-level mission 0957-a1.

## Acceptance Criteria

### Mint algorithm

- [ ] `crates/octo-wallet/src/capability/dual_issuance.rs` (NEW) — `mint_dual(ask: &Ask, seller_key: &Ed25519Keypair, buyer_pub: &IdentityKey, holder: &IdentityKey, ask_ttl_unix: u64, txn: &mut Transaction) -> Result<(BearerCapsule, CapabilityToken), MintError>`.
- [ ] Steps: build capability caveats (AskBinding + AmountMax from ask + ask_ttl_unix expiry); call `CapabilityToken::mint(root_secret, holder, holder_did, &caveats)` (RFC-0957-A1 4-arg persistence-free); build `BearerCapsule` (typed per RFC-0959-A1 §Out of Scope); `txn.insert_dual(bearer_record, capability_record)` atomic; return both.
- [ ] Phantom call site: `IdentityKey::from_public_bytes(&buyer_pub_bytes)` — uses working stub from top-level mission 0957-a1 (`crates/octo-wallet/src/capability/identity_stub.rs`).

### Error type

- [ ] `MintError` enum: `AskExpired { ask_id: AskId, expired_at_unix: u64 }`, `RootSecretMissing { ask_id: AskId }`, `HolderKeyInvalid { reason: String }`, `DualInsertFailed { ask_id: AskId, bearer_err: Option<String>, cap_err: Option<String> }`. All manual redacting Debug.

### Test vectors (RFC-0969 §Test Vectors, this sub-mission owns TV9)

- [ ] TV9: Dual-Issuance Atomicity — `mint_dual` happy path; both records persisted; `lookup_by_ask(ask_id, HolderKind::Bearer)` returns bearer record; `lookup_by_ask(ask_id, HolderKind::V1)` (or appropriate kind) returns capability record.
- [ ] Atomicity failure path: forced failure on capability insert → bearer record MUST NOT persist (cross-mission with 0957-e TV11).
- [ ] `ask_ttl_unix` plumbed through: capability's `ttl_millis_unix` field reflects `ask_ttl_unix` value; expired `ask` returns `MintError::AskExpired`.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0009 — seller signature substrate (`seller_key: &Ed25519Keypair`) + buyer encryption pubkey
- RFC-0903 — bearer path substrate
- RFC-0957 — capability path substrate
- RFC-0957-A1 — canonical 4-arg `CapabilityToken::mint` + `Transaction::insert_dual` + `CapabilityCatalog` extensions
- RFC-0959-A1 — `BearerCapsule` typed struct (cross-mission: BearerCapsule defined in 0959-b, consumed here)

**Requires (mission gates):**

- `missions/open/0969-dual-pipeline-authorization.md` (top-level)
- `missions/open/0957-e-mint-txn-parameter.md` — 4-arg `CapabilityToken::mint` signature amendment MUST land first
- `missions/open/0957-c-holder-registry-impl.md` — `Transaction::insert_dual` MUST exist (owned by 0957-c per the convention)
- `missions/open/0959-b-market-delivery-impl.md` — `BearerCapsule` typed struct MUST exist

```yaml
depends_on:
  - 0957-e-mint-txn-parameter # 4-arg mint signature
  - 0957-c-holder-registry-impl # Transaction::insert_dual
  - 0959-b-market-delivery-impl # BearerCapsule type
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `mint_dual` algorithm
- `MintError` enum
- `ask_ttl_unix` explicit parameter on `mint_dual`
- Manual redacting Debug impls on `MintError`

## Location

- `crates/octo-wallet/src/capability/dual_issuance.rs` (NEW)
- `crates/octo-wallet/src/capability/mod.rs` (MODIFY) — module exports

## Claimant

@mmacedoeu (algorithm stub + MintError type; full crypto deferred)

## Pull Request

(unset)

## Notes

- `mint_dual` is the canonical caller of `txn.insert_dual` from RFC-0957-A1. The algorithm co-authors with sub-mission 0957-e (which owns `Transaction::insert_dual`) and 0959-b (which owns `BearerCapsule`).
- The atomicity test (TV9 + cross-mission with 0957-e TV11) MUST verify all-or-nothing: if capability insert fails, bearer MUST NOT persist. Convention: test lives in 0957-e (per the cross-mission co-author contract); 0969-b consumes the passing test as a precondition.
- Phantom type `IdentityKey::from_public_bytes` at `mint_dual` (buyer pubkey extraction) uses the working stub from `crates/octo-wallet/src/capability/identity_stub.rs`. Full signature promotion to RFC-0009-B1 / RFC-0957-A2 is downstream.
