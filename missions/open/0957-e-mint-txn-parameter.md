# Mission: Mint Signature Amendment + CapabilityCatalog Extension (RFC-0957-A1)

## Status

Open

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0957-a1-holder-registry.md` (top-level decomposition mission)

## Summary

Implement RFC-0957-A1 mint signature amendment (G3) + CapabilityCatalog 4-method extension. Amend `CapabilityToken::mint` to the canonical 4-arg persistence-free signature `mint(root_secret, holder, holder_did, initial_caveats) -> Result<CapabilityToken, MintError>`. REMOVE the `catalog` and `Option<&mut Transaction>` parameters and the post-write hook entirely (R6-C3 fix). Persistence is handled by the caller via `TransactionExt::insert_dual` (atomic pair insert) or `TransactionExt::insert_holder_record` (single insert). Add 4 new methods to `CapabilityCatalog`: `holder_registry()`, `root_secret_for_ask(ask_id)`, `settlement_chain_tip()`, `gossip_to_buyer(buyer_did, env)` (R13-N3 fix).

This amendment breaks the double-insert contradiction between the prior `mint()` (which had a post-write hook that auto-inserted into the HolderRegistry) and `insert_dual` (RFC-0969 §Algorithms:mint_dual, which inserts both bearer + capability atomically). With mint being pure crypto, the caller controls persistence explicitly.

## Acceptance Criteria

### Mint signature amendment (G3)

- [ ] `crates/octo-wallet/src/capability/mod.rs` (MODIFY) — `CapabilityToken::mint` signature amended to:
  ```rust
  pub fn mint(
      root_secret: &RootSecret,
      holder: &IdentityKey,
      holder_did: &Did,
      initial_caveats: &[Caveat],
  ) -> Result<CapabilityToken, MintError>
  ```
- [ ] The `catalog: &CapabilityCatalog` and `Option<&mut Transaction>` parameters are REMOVED. The post-write hook is REMOVED entirely. Mint is pure crypto.
- [ ] All call sites updated: replace `mint(root_secret, holder, holder_did, catalog, txn_opt, initial_caveats)` with `mint(root_secret, holder, holder_did, initial_caveats)` + explicit `txn.insert_holder_record(...)` or `txn.insert_dual(...)` call.
- [ ] `git diff` shows ONLY parameter removals + removal of post-write hook. NO parameter additions, NO new types.

### CapabilityCatalog 4-method extension (R13-N3 fix)

- [ ] `crates/octo-wallet/src/capability/capability_catalog.rs` (EXTEND) — add 4 methods:
  - `pub fn holder_registry(&self) -> Arc<dyn HolderRegistry>`
  - `pub fn root_secret_for_ask(&self, ask_id: AskId) -> Result<RootSecret, CatalogError>`
  - `pub fn settlement_chain_tip(&self) -> Result<ChainTip, CatalogError>`
  - `pub fn gossip_to_buyer(&self, buyer_did: Did, env: &MarketDeliveryEnvelope) -> Result<(), CatalogError>`
- [ ] Documented delta: `stoolap()` was intentionally MOVED AWAY to a direct `&stoolap::Database` parameter on RFC-0970 §Algorithms:wrap_for_hop + RFC-0959-A1 §Algorithms:deliver_at_settlement; NOT on the `CapabilityCatalog` trait (R7-N2 fix).

### Test vectors (RFC-0957-A1 §Test Vectors, this sub-mission owns TV8, TV9, TV10, TV11)

- [ ] TV8: 100K Lookup Benchmark — `StoolapHolderRegistry::lookup(cap_root_hash)` returns in ≤5ms p99 over 100K holders (G1). Criterion bench, NOT in regular `cargo test` (use `#[ignore]`).
- [ ] TV9: Mint Is Persistence-Free — assert `mint(root_secret, holder, holder_did, initial_caveats)` does NOT touch the registry or any database. Snapshot the registry state pre/post mint; assert identical. No `Transaction` parameter is accepted; compile-time guarantee (R8-N7 fix: prior 6-arg TV rewritten).
- [ ] TV10: Caller-Side Persistence via TransactionExt — unit test: `mint(...)` produces token; explicit `txn.insert_holder_record(HolderRecord::from_capability(token, ...))` persists; subsequent `lookup(cap_root_hash)` returns the record (R8-N7 fix: prior 6-arg TV rewritten).
- [ ] TV11: `insert_dual` Atomicity — `txn.insert_dual(bearer_record, capability_record)` atomic. Forced-failure test: capability insert fails → bearer insert rolls back. (Cross-mission: lives also in sub-mission 0969-b `mint_dual` algorithm; co-author the test.)

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] `cargo doc --workspace --no-deps` builds without broken-doc-link warnings

## Dependencies

**Requires (RFC gates):**

- RFC-0853 — BLAKE3 keyed-hash primitive for `cap_root_hash`
- RFC-0862 — `Transaction` + gossip primitive (consumed by CapabilityCatalog extensions)

**Requires (mission gates):**

- `missions/open/0957-a1-holder-registry.md` (top-level)
- `missions/open/0957-c-holder-registry-impl.md` — `HolderRecord`, `Transaction`, `HolderRegistry` MUST exist
- `missions/claimed/0957-a-capability-token-macaroon.md` (in progress) — base `CapabilityToken::mint` (5-arg with catalog) MUST exist before amendment

```yaml
depends_on:
  - 0957-c-holder-registry-impl # HolderRecord + Transaction
  - 0957-a-capability-token-macaroon # base mint (5-arg) for amendment
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `CapabilityToken::mint` signature amendment to 4-arg persistence-free
- `CapabilityCatalog` 4-method extension

## Location

- `crates/octo-wallet/src/capability/mod.rs` (AMEND) — mint signature + post-write hook gating
- `crates/octo-wallet/src/capability/capability_catalog.rs` (EXTEND) — 4 new methods

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- The mint signature amendment is the **load-bearing change** of RFC-0957-A1 per §Rationale. Without it, `mint_dual` (RFC-0969 §Algorithms) cannot work because the prior 5-arg `mint` had a post-write hook that auto-inserted into the HolderRegistry, contradicting `mint_dual`'s explicit atomic pair insert.
- TV8 (100K Lookup Benchmark) MUST be `#[ignore]` per RFC-0957-A1 §Performance Targets. Criterion bench lives at `crates/octo-wallet/benches/holder_registry_lookup.rs`.
- TV11 (`insert_dual` atomicity) crosses into sub-mission 0969-b. Co-author contract: 0969-b owns the `mint_dual` algorithm + the `Transaction::insert_dual` method. This sub-mission owns the TV11 test (or co-locates the test in 0969-b and links).
