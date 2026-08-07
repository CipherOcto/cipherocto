# Mission: Full `mint_dual` Implementation (RFC-0969 §Phase 2 follow-up)

## Status

Open (filed 2026-08-06 by mission `0969-b-dual-issuance-mint.md` Band A closure). Per [[deferred-vs-unspecified]] named-owner rule, this follow-up mission owns the deferred full `mint_dual` algorithm body (caveat construction + 4-arg `CapabilityToken::mint` call + `BearerCapsule` crypto + `txn.insert_dual` atomic wiring) + TV9 happy path + atomicity failure path + `ask_ttl_unix` plumbing + `IdentityKey::from_public_bytes` phantom promotion.

**Sub-mission of:** `missions/claimed/0969-b-dual-issuance-mint.md` (Band A closed 2026-08-06; commit `1289ea55`).

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02 (provides 4-arg `CapabilityToken::mint` consumed here)

## Summary

Replace the `mint_dual` stub in `crates/octo-wallet/src/capability/dual_issuance.rs` (which currently returns `Err(MintError::RootSecretMissing)` by design) with the full algorithm. The stub signature `(ask_id: [u8; 32], ask_ttl_unix: u64) -> Result<(BearerCapsule, CapabilityToken), MintError>` becomes the canonical RFC-0969 §Phase 2 signature `(ask: &Ask, seller_key: &Ed25519Keypair, buyer_pub: &IdentityKey, holder: &IdentityKey, ask_ttl_unix: u64, txn: &mut Transaction) -> Result<(BearerCapsule, CapabilityToken), MintError>`.

Algorithm steps (RFC-0969 §Phase 2):

1. Build capability caveats (`AskBinding { ask_id, seller_did }` + `AmountMax { max_units }` from `ask` + `Expiry { ttl_millis_unix: ask_ttl_unix * 1000 }`).
2. Call canonical 4-arg persistence-free `CapabilityToken::mint(root_secret, holder, holder_did, &caveats)` (RFC-0957-A1 amendment).
3. Build `BearerCapsule` (typed per RFC-0959-A1 §Out of Scope) — X25519 ECDH + ChaCha20-Poly1305 encrypt of the cap_root_secret for the buyer's `buyer_pub`.
4. Call `txn.insert_dual(bearer_record, capability_record)` atomic insertion (RFC-0957-A1 amendment).
5. Return both `(BearerCapsule, CapabilityToken)`.

Phantom promotion: `IdentityKey::from_public_bytes(&buyer_pub_bytes)` at the `buyer_pub: &IdentityKey` extraction point. Stub lives in `0957-a1`; this mission promotes to RFC-0009-B1 / RFC-0957-A2.

## Acceptance Criteria

### `mint_dual` algorithm

- [ ] `crates/octo-wallet/src/capability/dual_issuance.rs` (MODIFY) — replace stub with full impl. New signature: `pub fn mint_dual(ask: &Ask, seller_key: &Ed25519Keypair, buyer_pub: &IdentityKey, holder: &IdentityKey, ask_ttl_unix: u64, txn: &mut Transaction) -> Result<(BearerCapsule, CapabilityToken), MintError>`.
- [ ] Caveat construction: `AskBinding { ask_id: ask.ask_id, seller_did: ask.seller_did }`, `AmountMax { max_units: ask.max_units }`, `Expiry { ttl_millis_unix: ask_ttl_unix * 1000 }`.
- [ ] `CapabilityToken::mint(root_secret, holder, holder_did, &caveats)` call (4-arg persistence-free per RFC-0957-A1).
- [ ] `BearerCapsule::build(cap_root_secret, buyer_pub)` (typed per RFC-0959-A1).
- [ ] `txn.insert_dual(bearer_record, capability_record)` atomic insertion.
- [ ] Return `Ok((BearerCapsule, CapabilityToken))`.
- [ ] All existing `MintError` variants (`AskExpired`, `RootSecretMissing`, `HolderKeyInvalid`, `DualInsertFailed`) wired correctly:
  - `MintError::AskExpired { ask_id, expired_at_unix }` — if `ask.expires_at_unix < current_unix_time` (or equivalent ask-ttl check)
  - `MintError::RootSecretMissing { ask_id }` — if `seller_key` cannot derive root_secret (placeholder; removed once substrate lands)
  - `MintError::HolderKeyInvalid { reason }` — if `holder` is malformed
  - `MintError::DualInsertFailed { ask_id, bearer_err, cap_err }` — if `txn.insert_dual` returns error (propagate both failure modes)

### Phantom type promotion

- [ ] `IdentityKey::from_public_bytes(&buyer_pub_bytes)` call site at `mint_dual` (line ~150-160 in `dual_issuance.rs`) — uses working stub from `crates/octo-wallet/src/capability/identity_stub.rs`.
- [ ] File follow-up RFC-0009-B1 / RFC-0957-A2 promotion mission (or absorb into this mission if scope permits).

### Test vectors (RFC-0969 §Test Vectors)

- [ ] TV9: Dual-Issuance Atomicity happy path — `mint_dual` with valid `ask` + `seller_key` + `buyer_pub` + `holder` returns `Ok((BearerCapsule, CapabilityToken))`. Both records persisted (verify via `lookup_by_ask(ask_id, HolderKind::Bearer)` returns bearer record; `lookup_by_ask(ask_id, HolderKind::V1)` returns capability record).
- [ ] Atomicity failure path (cross-mission with `0957-e` TV11): forced failure on capability insert → bearer record MUST NOT persist. Test lives in `0957-e` per `Transaction::insert_dual` ownership convention.
- [ ] `ask_ttl_unix` plumbing: capability's `ttl_millis_unix` field reflects `ask_ttl_unix * 1000`; expired `ask` returns `MintError::AskExpired { ask_id, expired_at_unix }`.
- [ ] Manual redacting Debug on `MintError` (already GREEN from `0969-b` Band A closure) — verify all 4 new error paths redacted correctly.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace --lib` green (existing 234+ tests + 3 new TV9 tests = 237+ total)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]); workspace-level pre-existing `tdlib-rs` build error excluded from this AC
- [ ] `cargo fmt --check --workspace` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0009 — Identity substrate (`IdentityKey` + `Ed25519Keypair` types)
- RFC-0853 — Overlay Cryptography (X25519 + ChaCha20-Poly1305 for `BearerCapsule` build)
- RFC-0903 — Bearer path substrate
- RFC-0957-A1 — Capability path substrate (4-arg `CapabilityToken::mint` + `Transaction::insert_dual`)
- RFC-0959-A1 — `BearerCapsule` typed struct
- RFC-0969 — Dual-Pipeline Authorization (the algorithm itself)

**Requires (mission gates):**

- `missions/claimed/0969-b-dual-issuance-mint.md` (Band A closed 2026-08-06) — provides `MintError` enum + 3 stub tests + cross-crate compat substrate
- `missions/claimed/0957-e-mint-txn-parameter.md` (Band A closed 2026-08-06) — provides 4-arg `CapabilityToken::mint` signature + `Transaction::insert_dual` consumed here
- `missions/claimed/0957-c-holder-registry-impl.md` (Band A closed 2026-08-06) — provides `HolderKind::Bearer` + `HolderKind::V1` consumed by lookup assertions
- `missions/claimed/0959-b-market-delivery-impl.md` (Band A closed 2026-08-06) — provides `BearerCapsule` typed struct consumed here

```yaml
depends_on:
  - 0969-b-dual-issuance-mint # MintError enum + stub substrate
  - 0957-e-mint-txn-parameter # 4-arg CapabilityToken::mint + Transaction::insert_dual
  - 0957-c-holder-registry-impl # HolderKind::Bearer + HolderKind::V1
  - 0959-b-market-delivery-impl # BearerCapsule typed struct
  - 0957-a1 # IdentityKey::from_public_bytes working stub
```

## Location

- `crates/octo-wallet/src/capability/dual_issuance.rs` (MODIFY) — full `mint_dual` impl + 3 new TV9 tests
- `crates/octo-wallet/src/capability/identity_stub.rs` (EXTEND) — `IdentityKey::from_public_bytes` working stub (already GREEN per `0957-a1`)

## Claimant

TBD (claim 2026-08-06+)

## Notes

- The stub `mint_dual(ask_id, ask_ttl_unix) -> Err(RootSecretMissing)` is the canonical Band A "honest gate" pattern — substrate ships with the type surface + error variants + 3 unit tests, full crypto deferred to this follow-up mission with named owner.
- Cross-mission co-author contract: `0957-e` owns `Transaction::insert_dual`; this mission owns the `mint_dual` algorithm. The atomicity failure-path test lives in `0957-e` (per ownership convention); this mission consumes it as a precondition for the happy path.
- Phantom type `IdentityKey::from_public_bytes` promotion to RFC-0009-B1 / RFC-0957-A2 is a separate cleanup mission; this mission uses the working stub from `0957-a1`.
- `MintError::RootSecretMissing` will likely be removed once the real impl lands (no longer reachable in normal operation); kept as a safety net for `seller_key` extraction failures (e.g., corrupted vault slot).
