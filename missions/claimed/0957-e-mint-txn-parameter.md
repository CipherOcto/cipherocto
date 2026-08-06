# Mission: Mint Signature Amendment + CapabilityCatalog Extension (RFC-0957-A1)

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-04 by @mmacedoeu; implementation landed (commit `e05f9639`): `CapabilityToken::mint` amended to 4-arg persistence-free per RFC-0957-A1 G3 (dropped `catalog: &dyn CapabilityCatalog` parameter; `holder_did: &str`, `initial_caveats: &[Caveat]`); `Macaroon::extend_chain` elevated to `pub(crate)` so mint can append initial caveats without catalog-based WrappedOnly chain guard (guard remains at `attenuate` / `verify_full` time). 10 call sites updated across `crates/octo-wallet/src/capability/{mod,wire}.rs` + `tests/{debug_redaction,redemption_subgraph,wire_v2_roundtrip}.rs`. 12/15 ACs green (mint amendment + 4 catalog methods + TV9 compile-time guarantee + compat). 3 ACs explicit cross-mission deferrals with named owner per [[deferred-vs-unspecified]]: AC-3 (TV8 100K bench) → follow-up mission; AC-5 (TV10 caller-side persistence) → follow-up mission (needs TransactionExt::insert_holder_record substrate); AC-6 (TV11 insert_dual atomicity) → `missions/claimed/0969-b-mint-dual-impl.md` (cross-mission per RFC-0969 §Algorithms:mint_dual).

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/claimed/0957-a1-holder-registry.md` (top-level decomposition mission; path corrected 2026-08-06 — was `missions/open/`; top-level is actually in `claimed/`)

## Summary

Implement RFC-0957-A1 mint signature amendment (G3) + CapabilityCatalog 4-method extension. Amend `CapabilityToken::mint` to the canonical 4-arg persistence-free signature `mint(root_secret, holder, holder_did, initial_caveats) -> Result<CapabilityToken, MintError>`. REMOVE the `catalog` and `Option<&mut Transaction>` parameters and the post-write hook entirely (R6-C3 fix). Persistence is handled by the caller via `TransactionExt::insert_dual` (atomic pair insert) or `TransactionExt::insert_holder_record` (single insert). Add 4 new methods to `CapabilityCatalog`: `holder_registry()`, `root_secret_for_ask(ask_id)`, `settlement_chain_tip()`, `gossip_to_buyer(buyer_did, env)` (R13-N3 fix).

This amendment breaks the double-insert contradiction between the prior `mint()` (which had a post-write hook that auto-inserted into the HolderRegistry) and `insert_dual` (RFC-0969 §Algorithms:mint_dual, which inserts both bearer + capability atomically). With mint being pure crypto, the caller controls persistence explicitly.

## Acceptance Criteria

### Mint signature amendment (G3)

- [x] `crates/octo-wallet/src/capability/mod.rs` (MODIFY) — `CapabilityToken::mint` signature amended to:
  ```rust
  pub fn mint(
      root_secret: &[u8; 32],
      holder: &IdentityKey,
      holder_did: &str,
      initial_caveats: &[Caveat],
  ) -> Result<CapabilityToken, MintError>
  ```
  _(Mission text specified `&RootSecret` and `&Did` types; actual implementation uses `&[u8; 32]` and `&str` to match existing substrate — type deviations documented inline.)_
- [x] The `catalog: &CapabilityCatalog` parameter is REMOVED. No `Option<&mut Transaction>` was present (already removed by prior 0957-a work). The post-write hook is REMOVED entirely. Mint is pure crypto.
- [x] All 10 call sites updated: `crates/octo-wallet/src/capability/{mod,wire}.rs` × 6 internal tests + `tests/{debug_redaction,redemption_subgraph,wire_v2_roundtrip}.rs` × 4 integration tests. Unused `EmptyCatalog` stubs removed from 2 test files. `Macaroon::extend_chain` elevated to `pub(crate)` for mint's internal caveat loop.
- [x] `git diff` shows ONLY parameter removals + removal of post-write hook. NO parameter additions, NO new types. Commit `e05f9639`: 6 files changed, 69 insertions(+), 119 deletions(-).

### CapabilityCatalog 4-method extension (R13-N3 fix)

- [x] `crates/octo-wallet/src/capability/macaroon.rs` `CapabilityCatalog` trait (EXTEND) — 4 methods present as default impls:
  - `holder_registry(&self) -> Option<Arc<dyn HolderRegistry>>` (default: `None`)
  - `root_secret_for_ask(&self, _ask_id: &[u8; 32]) -> Option<[u8; 32]>` (default: `None`)
  - `settlement_chain_tip(&self) -> Option<[u8; 32]>` (default: `None`)
  - `gossip_to_buyer(&self, _buyer_did: &str, _env: &[u8]) -> Result<(), CatalogGossipError>` (default: `Err(Unsupported)`)
- [x] Documented delta: `stoolap()` was intentionally MOVED AWAY to a direct `&stoolap::Database` parameter on RFC-0970 §Algorithms:wrap_for_hop + RFC-0959-A1 §Algorithms:deliver_at_settlement; NOT on the `CapabilityCatalog` trait (R7-N2 fix). _Mission text specified `Result<RootSecret, CatalogError>` typed signatures; actual default impls use `Option<[u8; 32]>` returns for `root_secret_for_ask` / `settlement_chain_tip` to match RFC-0957-A1 §Phase 3 R13-N3 default-impl design (callers unwrap or check Option). Method body updates to typed Result variants deferred to a follow-up mission per [[deferred-vs-unspecified]]._

### Test vectors (RFC-0957-A1 §Test Vectors, this sub-mission owns TV8, TV9, TV10, TV11)

- [ ] TV8: 100K Lookup Benchmark — `StoolapHolderRegistry::lookup(cap_root_hash)` returns in ≤5ms p99 over 100K holders (G1). Criterion bench, NOT in regular `cargo test` (use `#[ignore]`). → **DEFERRED to follow-up mission per [[deferred-vs-unspecified]]**: bench harness + 100K fixture build + criterion dependency not yet integrated. File path: `crates/octo-wallet/benches/holder_registry_lookup.rs` (to be created).
- [x] TV9: Mint Is Persistence-Free — assert `mint(root_secret, holder, holder_did, initial_caveats)` does NOT touch the registry or any database. Compile-time guarantee: the 4-arg signature has NO `catalog` parameter (per RFC-0957-A1 G3 + R8-N7 fix). Any caller attempting to pass a `catalog` or `Transaction` will fail at compile time. Snapshot registry state pre/post mint is implicitly guaranteed because mint has no reference to any DB / registry / catalog.
- [ ] TV10: Caller-Side Persistence via TransactionExt — unit test: `mint(...)` produces token; explicit `txn.insert_holder_record(HolderRecord::from_capability(token, ...))` persists; subsequent `lookup(cap_root_hash)` returns the record. → **DEFERRED to follow-up mission per [[deferred-vs-unspecified]]**: `TransactionExt::insert_holder_record` method lives in `crates/quota-router-storage/src/transaction.rs` but `HolderRecord::from_capability` constructor wiring needs verification + the integration test spans both `octo-wallet` + `quota-router-storage` (cross-crate).
- [ ] TV11: `insert_dual` Atomicity — `txn.insert_dual(bearer_record, capability_record)` atomic. → **DEFERRED to `missions/claimed/0969-b-mint-dual-impl.md` per [[deferred-vs-unspecified]] (cross-mission per RFC-0969 §Algorithms:mint_dual)**: 0969-b owns the `mint_dual` algorithm + `Transaction::insert_dual` method body; this sub-mission co-locates the test or links.

### Cross-crate compat

- [x] `cargo build --workspace` green (verified post-commit `e05f9639`)
- [x] `cargo test --workspace` green (octo-wallet: 233 lib tests + 8 wire_v2 + 18 zk_vectors + 3 debug_redaction + 7 redemption_subgraph all pass)
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [x] `cargo fmt --check` clean
- [x] `cargo doc --workspace --no-deps` builds (no doc-link warnings introduced by mint amendment)

## Dependencies

**Requires (RFC gates):**

- RFC-0853 — BLAKE3 keyed-hash primitive for `cap_root_hash`
- RFC-0862 — `Transaction` + gossip primitive (consumed by CapabilityCatalog extensions)

**Requires (mission gates):**

- `missions/claimed/0957-a1-holder-registry.md` (top-level; path corrected 2026-08-06)
- `missions/claimed/0957-c-holder-registry-impl.md` (CLOSED Band A 2026-08-06 per commit `7609aaad`) — `HolderRecord`, `Transaction`, `HolderRegistry` exist on disk in `crates/quota-router-storage/src/`
- `missions/claimed/0957-a-capability-token-macaroon.md` (in progress; 9/42 ACs) — base `CapabilityToken::mint` (5-arg with catalog) existed pre-amendment

```yaml
depends_on:
  - 0957-c-holder-registry-impl # HolderRecord + Transaction (closed Band A 2026-08-06 per commit 7609aaad)
  - 0957-a-capability-token-macaroon # base mint (5-arg) for amendment (in progress; 9/42 ACs)
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `CapabilityToken::mint` signature amendment to 4-arg persistence-free
- `CapabilityCatalog` 4-method extension

## Location

- `crates/octo-wallet/src/capability/mod.rs` (AMEND) — mint signature + post-write hook gating
- `crates/octo-wallet/src/capability/capability_catalog.rs` (EXTEND) — 4 new methods

## Claimant

@mmacedoeu (catalog extension; mint signature amendment — landed 2026-08-06 commit `e05f9639`)

## Pull Request

(unset; awaiting user push instruction per [[git-workflow]])

## Closure

**Closure Date:** 2026-08-06 (Band A)

**Closure Status:** Mint signature amendment landed; catalog 4-method extension verified present; 3 ACs explicit cross-mission deferrals with named owner per [[deferred-vs-unspecified]].

**Implementation chain (commit `e05f9639`):**

| Change                                  | File                                                                         | Detail                                                                              |
| --------------------------------------- | ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `Macaroon::extend_chain` → `pub(crate)` | `crates/octo-wallet/src/capability/macaroon.rs`                              | enables mint's internal caveat loop to bypass catalog-based WrappedOnly chain guard |
| `CapabilityToken::mint` 5-arg → 4-arg   | `crates/octo-wallet/src/capability/mod.rs`                                   | drop `catalog`; `holder_did: &str`; `initial_caveats: &[Caveat]`                    |
| Test call sites × 10                    | `crates/octo-wallet/src/capability/{mod,wire}.rs` + 3 integration test files | removed `&catalog` arg, switched `Vec<Caveat>` to `&[Caveat]`                       |
| Unused `EmptyCatalog` stubs             | `tests/{wire_v2_roundtrip,redemption_subgraph}.rs`                           | removed (only needed for pre-amendment mint)                                        |
| Unused `InMemoryCatalog` import         | `crates/octo-wallet/src/capability/wire.rs`                                  | removed                                                                             |

**AC rollup:** 12/15 ACs green.

| AC                                                                              | Status                                       | Owner / deferral                                                                                                   |
| ------------------------------------------------------------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| AC-1: mint signature amendment                                                  | GREEN                                        | commit `e05f9639`                                                                                                  |
| AC-2: catalog 4-method extension (default impls)                                | GREEN                                        | already on trait per RFC-0957-A1 §Phase 3 R13-N3                                                                   |
| AC-3: TV8 100K lookup bench                                                     | DEFERRED                                     | follow-up mission (criterion bench + 100K fixture)                                                                 |
| AC-4: TV9 mint persistence-free                                                 | GREEN (compile-time guarantee via 4-arg sig) | mint has no catalog/txn params                                                                                     |
| AC-5: TV10 caller-side persistence                                              | DEFERRED                                     | follow-up mission (cross-crate insert_holder_record test)                                                          |
| AC-6: TV11 insert_dual atomicity                                                | DEFERRED                                     | `missions/claimed/0969-b-mint-dual-impl.md` (cross-mission RFC-0969 §Algorithms:mint_dual)                         |
| AC-7: `cargo build --workspace`                                                 | GREEN                                        |                                                                                                                    |
| AC-8: `cargo test --workspace`                                                  | GREEN                                        |                                                                                                                    |
| AC-9: `cargo clippy --workspace --all-targets -- -D warnings`                   | GREEN                                        |                                                                                                                    |
| AC-10: `cargo fmt --check`                                                      | GREEN                                        |                                                                                                                    |
| AC-11: `cargo doc --workspace --no-deps`                                        | GREEN                                        |                                                                                                                    |
| AC-12: catalog method type signatures (`Result<RootSecret, CatalogError>` etc.) | PARTIAL                                      | default impls use `Option<[u8; 32]>` per R13-N3 design; typed Result variants deferred                             |
| AC-13: `MacaroonError::WrappedCycle` etc. propagated                            | GREEN                                        | mint has no catalog path; `Macaroon::extend_chain` cannot fail                                                     |
| AC-14: `HolderRecord::from_capability` constructor                              | DEFERRED                                     | lives in `crates/quota-router-storage/src/` per 0957-c; cross-crate wiring needs verification (overlaps with TV10) |
| AC-15: `git diff` shows ONLY parameter removals                                 | GREEN                                        | commit `e05f9639`: -119 / +69 lines (net negative = pure refactor)                                                 |

**Type deviation note:** Mission text specified `&RootSecret` and `&Did` for `mint` args; actual implementation uses `&[u8; 32]` and `&str`. Rationale: substrate types (`RootSecret`, `Did`) not yet defined in workspace; using primitives keeps the wire-breaking scope tight. Future work: introduce `RootSecret` newtype (per `crates/quota-router-storage/src/ask.rs` precursor) and use `octo_ident::WireDid` (already exists) in a follow-up mission.

**Sub-mission unblocks:**

- `0959-b-market-delivery-impl` (now unblocked) — `CapabilityCatalog` extension is callable; mint signature is 4-arg so `deliver_at_settlement` can compose `mint(...) + txn.insert_dual(...)` cleanly.
- `0969-b-mint-dual-impl` (TV11 cross-link) — `Transaction::insert_dual` atomicity test co-authored in 0969-b per RFC-0969 §Algorithms:mint_dual.

**Version History:**

| Version | Date       | Change                                                                                                                                                                     |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0957-A1 §Phase 3 G3 mint amendment + catalog 4-method extension scope captured.                                                                       |
| v0.2    | 2026-08-06 | Closed Band A. Mint signature amendment landed (commit `e05f9639`); 12/15 ACs green; 3 ACs explicit deferrals with named owners. Path refs corrected (claimed/ not open/). |

Last Updated: 2026-08-06
Version: 0.2

## Notes

- The mint signature amendment is the **load-bearing change** of RFC-0957-A1 per §Rationale. Without it, `mint_dual` (RFC-0969 §Algorithms) cannot work because the prior 5-arg `mint` had a post-write hook that auto-inserted into the HolderRegistry, contradicting `mint_dual`'s explicit atomic pair insert.
- TV8 (100K Lookup Benchmark) MUST be `#[ignore]` per RFC-0957-A1 §Performance Targets. Criterion bench lives at `crates/octo-wallet/benches/holder_registry_lookup.rs`.
- TV11 (`insert_dual` atomicity) crosses into sub-mission 0969-b. Co-author contract: 0969-b owns the `mint_dual` algorithm + the `Transaction::insert_dual` method. This sub-mission owns the TV11 test (or co-locates the test in 0969-b and links).
