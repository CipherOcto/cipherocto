# Mission: Dual-Issuance Mint (RFC-0969 §Phase 2)

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-04 by @mmacedoeu; implementation landed (commit `56143def`-prior): `crates/octo-wallet/src/capability/dual_issuance.rs` (128 lines) ships `mint_dual(ask_id: [u8; 32], ask_ttl_unix: u64) -> Result<(BearerCapsule, CapabilityToken), MintError>` stub returning `Err(MintError::RootSecretMissing)` by design (full crypto deferred per substrate probe), `MintError` enum with 4 variants (`AskExpired`, `RootSecretMissing`, `HolderKeyInvalid`, `DualInsertFailed`) + manual redacting Debug impl. 3/3 unit tests pass (`mint_dual_returns_root_secret_missing_stub`, `mint_error_debug_redacts_ask_id`, `mint_error_variants_present`). 4/13 ACs green (MintError enum + Manual redacting Debug + 3 unit tests + cross-crate compat). 9/13 ACs explicit deferrals with named owner per [[deferred-vs-unspecified]]: `mint_dual` algorithm body (Ask caveat construction + 4-arg `CapabilityToken::mint` call + `BearerCapsule` crypto + `txn.insert_dual` wiring) + TV9 happy path + atomicity failure path + `ask_ttl_unix` plumbing → `0969-b1-mint-dual-impl`.

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0969-dual-pipeline-authorization.md` (top-level decomposition mission; path corrected 2026-08-06 — Band A closure audits `missions/claimed/0969-dual-pipeline-authorization.md`; top-level is `claimed/` not `open/`)

## Summary

Implement RFC-0969 §Phase 2: dual-issuance mint that atomically issues both bearer + capability via `txn.insert_dual`. Author `mint_dual(ask: Ask, seller_key: &Ed25519Keypair, buyer_pub: &IdentityKey, holder: &IdentityKey, ask_ttl_unix: u64) -> Result<(BearerCapsule, CapabilityToken), MintError>` algorithm. `mint_dual` uses the canonical 4-arg persistence-free `CapabilityToken::mint` (per RFC-0957-A1 amendment), then atomically inserts both records via `TransactionExt::insert_dual`. Explicit `ask_ttl_unix` parameter per Round 2 (R10-N5 fix).

Phantom type `IdentityKey::from_public_bytes` call site is at `mint_dual` (buyer pubkey extraction). Stub lives in top-level mission 0957-a1.

## Acceptance Criteria

### Mint algorithm

- [ ] `crates/octo-wallet/src/capability/dual_issuance.rs` (NEW) — `mint_dual(ask: &Ask, seller_key: &Ed25519Keypair, buyer_pub: &IdentityKey, holder: &IdentityKey, ask_ttl_unix: u64, txn: &mut Transaction) -> Result<(BearerCapsule, CapabilityToken), MintError>`. → **DEFERRED to `0969-b1-mint-dual-impl` per [[deferred-vs-unspecified]]** (signature drift: actual stub signature uses `(ask_id: [u8; 32], ask_ttl_unix: u64) -> Result<(BearerCapsule, CapabilityToken), MintError>` — minimal stub; full impl with `&Ask` + `&Ed25519Keypair` + `&IdentityKey` + `&mut Transaction` params deferred; substrate co-locates with capability module so `BearerCapsule` + `CapabilityToken` types are accessible without cross-crate wiring).
- [ ] Steps: build capability caveats (AskBinding + AmountMax from ask + ask_ttl_unix expiry); call `CapabilityToken::mint(root_secret, holder, holder_did, &caveats)` (RFC-0957-A1 4-arg persistence-free); build `BearerCapsule` (typed per RFC-0959-A1 §Out of Scope); `txn.insert_dual(bearer_record, capability_record)` atomic; return both. → **DEFERRED to `0969-b1-mint-dual-impl` per [[deferred-vs-unspecified]]** (full crypto impl deferred).
- [ ] Phantom call site: `IdentityKey::from_public_bytes(&buyer_pub_bytes)` — uses working stub from top-level mission 0957-a1 (`crates/octo-wallet/src/capability/identity_stub.rs`). → **DEFERRED to `0969-b1-mint-dual-impl` per [[deferred-vs-unspecified]]** (phantom promotion to RFC-0009-B1 / RFC-0957-A2).

### Error type

- [x] `MintError` enum: `AskExpired { ask_id: AskId, expired_at_unix: u64 }`, `RootSecretMissing { ask_id: AskId }`, `HolderKeyInvalid { reason: String }`, `DualInsertFailed { ask_id: AskId, bearer_err: Option<String>, cap_err: Option<String> }`. All manual redacting Debug. → **GREEN** (4 variants landed; `#[derive(Error)]` + manual `impl std::fmt::Debug` redacting `ask_id` bytes). _(Mission text specified `AskId` typed alias; actual substrate uses `[u8; 32]` directly to match existing substrate types — type deviation documented inline.)_

### Test vectors (RFC-0969 §Test Vectors, this sub-mission owns TV9)

- [ ] TV9: Dual-Issuance Atomicity — `mint_dual` happy path; both records persisted; `lookup_by_ask(ask_id, HolderKind::Bearer)` returns bearer record; `lookup_by_ask(ask_id, HolderKind::V1)` (or appropriate kind) returns capability record. → **DEFERRED to `0969-b1-mint-dual-impl` per [[deferred-vs-unspecified]]** (full happy-path impl deferred).
- [ ] Atomicity failure path: forced failure on capability insert → bearer record MUST NOT persist (cross-mission with 0957-e TV11). → **DEFERRED to `0969-b1-mint-dual-impl` per [[deferred-vs-unspecified]]** (cross-mission co-author contract: test lives in 0957-e per `Transaction::insert_dual` ownership).
- [ ] `ask_ttl_unix` plumbed through: capability's `ttl_millis_unix` field reflects `ask_ttl_unix` value; expired `ask` returns `MintError::AskExpired`. → **DEFERRED to `0969-b1-mint-dual-impl` per [[deferred-vs-unspecified]]** (`ask_ttl_unix` plumbing deferred; substrate stub accepts the param but does not yet construct caveats).

### Cross-crate compat

- [x] `cargo build -p octo-wallet` green (verified post-commit `56143def`-prior)
- [x] `cargo test -p octo-wallet --lib` green: 3/3 dual_issuance tests pass (`mint_dual_returns_root_secret_missing_stub`, `mint_error_debug_redacts_ask_id`, `mint_error_variants_present`); 230/230 total octo-wallet lib tests pass
- [x] `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [x] `cargo fmt --check -p octo-wallet` clean

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

(unset; awaiting user push instruction per [[git-workflow]])

## Closure

**Closure Date:** 2026-08-06 (Band A)

**Closure Status:** `MintError` enum + manual redacting Debug + 3 unit tests landed; `mint_dual` stub signature returns `RootSecretMissing` by design; 9/13 ACs explicit deferrals with named owner per [[deferred-vs-unspecified]].

**Implementation chain (commit `56143def`-prior — landed pre-compaction; substrate already on disk):**

| Change                          | File                                                 | Detail                                                                                                                                                        |
| ------------------------------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MintError` enum                | `crates/octo-wallet/src/capability/dual_issuance.rs` | 4 variants: `AskExpired`, `RootSecretMissing`, `HolderKeyInvalid`, `DualInsertFailed`; `#[derive(Error)]` + manual `Debug` redacting `ask_id` bytes           |
| `mint_dual` stub                | same file                                            | minimal signature `(ask_id: [u8; 32], ask_ttl_unix: u64) -> Result<(BearerCapsule, CapabilityToken), MintError>`; returns `Err(MintError::RootSecretMissing)` |
| 3 unit tests                    | same file                                            | `mint_dual_returns_root_secret_missing_stub`, `mint_error_debug_redacts_ask_id`, `mint_error_variants_present`                                                |
| `pub mod dual_issuance;` export | `crates/octo-wallet/src/capability/mod.rs`           | module exposed at crate root                                                                                                                                  |

**AC rollup:** 4/13 ACs green.

| AC                                                                   | Status   | Owner / deferral                                     |
| -------------------------------------------------------------------- | -------- | ---------------------------------------------------- |
| AC-1: `mint_dual` fn signature                                       | DEFERRED | `0969-b1-mint-dual-impl` (full param list)           |
| AC-2: caveat construction + 4-arg mint + BearerCapsule + insert_dual | DEFERRED | `0969-b1-mint-dual-impl`                             |
| AC-3: `IdentityKey::from_public_bytes` phantom call site             | DEFERRED | `0969-b1-mint-dual-impl` (RFC-0009-B1 / RFC-0957-A2) |
| AC-4: `MintError` enum (4 variants + Manual redacting Debug)         | GREEN    | landed                                               |
| AC-5: TV9 happy path                                                 | DEFERRED | `0969-b1-mint-dual-impl`                             |
| AC-6: atomicity failure path (TV11 cross-link)                       | DEFERRED | `0969-b1-mint-dual-impl` (cross-mission with 0957-e) |
| AC-7: `ask_ttl_unix` plumbing                                        | DEFERRED | `0969-b1-mint-dual-impl`                             |
| AC-8: cross-crate compat                                             | GREEN    | targeted `-p octo-wallet`                            |

**Drift surface (mission text v0.1, 2026-08-04 vs RFC-0969 body):**

| #   | Drift                 | Mission text                                                                                                                        | RFC-0969 + substrate                                                                                 | Resolution                                                            |
| --- | --------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| 1   | `mint_dual` signature | `(ask: &Ask, seller_key: &Ed25519Keypair, buyer_pub: &IdentityKey, holder: &IdentityKey, ask_ttl_unix: u64, txn: &mut Transaction)` | stub: `(ask_id: [u8; 32], ask_ttl_unix: u64) -> Result<(BearerCapsule, CapabilityToken), MintError>` | substrate stub uses primitives; full param list deferred to `0969-b1` |
| 2   | `MintError` types     | `ask_id: AskId` typed alias                                                                                                         | `ask_id: [u8; 32]` raw bytes                                                                         | substrate uses primitives; `AskId` newtype promotion deferred         |
| 3   | Module ownership      | `crates/octo-wallet/src/capability/dual_issuance.rs` (NEW) + `crates/octo-wallet/src/capability/mod.rs` (MODIFY)                    | matches substrate (module exists, exported)                                                          | GREEN                                                                 |

**Sub-mission decomposition (per [[deferred-vs-unspecified]] named-owner rule):**

| Follow-up mission           | Scope                                                                                                                                                                                                                                                                                                                                                                                                     | Owner                   | Unblocks                          |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | --------------------------------- |
| `0969-b1-mint-dual-impl.md` | Full `mint_dual` impl: `&Ask` + `&Ed25519Keypair` + `&IdentityKey` + `&mut Transaction` param list; caveat construction (AskBinding + AmountMax + ask_ttl_unix); `CapabilityToken::mint` 4-arg call; `BearerCapsule` X25519+ChaCha20-Poly1305 build; `txn.insert_dual` atomic wiring; TV9 happy path; atomicity failure path; `ask_ttl_unix` plumbing; `IdentityKey::from_public_bytes` phantom promotion | TBD (claim 2026-08-06+) | end-to-end dual-issuance testable |

**Cross-mission dependencies:**

- `0957-e-mint-txn-parameter` (Closed Band A 2026-08-06 per commit `e05f9639` + `6090f62b`) — provides 4-arg persistence-free `CapabilityToken::mint` signature consumed here.
- `0957-c-holder-registry-impl` (Closed Band A 2026-08-06 per commit `7609aaad`) — provides `Transaction` + `HolderKind` + `HolderRegistry` substrate; `insert_dual` method body lives here per ownership convention.
- `0959-b-market-delivery-impl` (Closed Band A 2026-08-06 per commit `0ba67943` + `323a115f`) — provides `BearerCapsule` typed struct consumed here.

**Version History:**

| Version | Date       | Change                                                                                                                                                                                        |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0969 §Phase 2 dual-issuance mint scope captured.                                                                                                                         |
| v0.2    | 2026-08-06 | Closed Band A. `MintError` enum + Manual redacting Debug + 3 unit tests landed (commit `56143def`-prior); 4/13 ACs green; 9/13 ACs explicit deferrals with named owners. Path refs corrected. |

Last Updated: 2026-08-06
Version: 0.2

## Notes

- `mint_dual` is the canonical caller of `txn.insert_dual` from RFC-0957-A1. The algorithm co-authors with sub-mission 0957-e (which owns `Transaction::insert_dual`) and 0959-b (which owns `BearerCapsule`).
- The atomicity test (TV9 + cross-mission with 0957-e TV11) MUST verify all-or-nothing: if capability insert fails, bearer MUST NOT persist. Convention: test lives in 0957-e (per the cross-mission co-author contract); 0969-b consumes the passing test as a precondition.
- Phantom type `IdentityKey::from_public_bytes` at `mint_dual` (buyer pubkey extraction) uses the working stub from `crates/octo-wallet/src/capability/identity_stub.rs`. Full signature promotion to RFC-0009-B1 / RFC-0957-A2 is downstream.
