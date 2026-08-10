# 0010-f2-multi-chain-did-resolution — Cross-chain DID resolution

**Status:** unassigned (wave 5; absorbed from RFC-0010 §Future Work F2 on 2026-08-10)
**Substrate:** RFC-0010 v1.3 §Storage Extension (`DidRegistry` trait)
**Parent:** RFC-0010 §Future Work F2 per `missions/open/0871b-storage-backend.md`

## Scope

Multi-chain DID resolution: a single canonical DID resolves to
**multiple storage backends** keyed by chain identifier (e.g.,
`cipherocto-mainnet`, `ethereum-attestation`, `bitcoin-ordinals`).
v1.3 ships single-chain resolution (`DidRegistry` keyed by canonical
DID only); F2 adds chain-keyed multi-tenancy so a DID can have
distinct documents per chain.

### Why this is the next F-row after v1.3

v1.3 §Storage Extension explicitly defers F2 to a future amendment —
`DidRegistry::resolve` returns one `DidDocument`. Multi-chain needs
either:

- **(a) Typed chain tag in the registry** — `DidRegistry::resolve(canonical_did, chain: ChainId) -> Option<DidDocument>`; preserves trait shape, adds parameter.
- **(b) Per-chain `DidRegistry` instances** — registry keyed by `ChainId`, dispatch at the resolver-node boundary; mirrors `CompositeCapabilityCatalog` pattern (mission `0959-c4`).
- **(c) Composite `DidRegistry` facade** — `CompositeDidRegistry { registries: HashMap<ChainId, Arc<dyn DidRegistry>> }`; Option B evolved.

### Mission scope (decision pending)

1. **Approach pick** (a / b / c) — see §Implementation Guide §Step 1.
2. New `ChainId` type in `crates/octo-ident/src/chain.rs` — typed
   wrapper around a `String` namespace (e.g., `"cipherocto-mainnet"`,
   `"ethereum-attestation"`). RFC-allocated namespace + user
   extension range per [[cipherocto-design-principles]] §Extension
   over enumeration.
3. `DidRegistry` trait amended (v1.4 in-place amendment to RFC-0010) —
   add `chain: &ChainId` parameter to `register` / `resolve` / `revoke`
   / `list`. Default `chain = ChainId::default()` for backward-compat.
4. `CompositeDidRegistry` impl (Option C) — `crates/octo-ident/src/composite_did_registry.rs`.
5. Multi-chain integration TV: register DID under 2 chains → resolve
   returns distinct documents; revoke on one chain does NOT affect
   other chains; composite dispatch correctly routes by chain.

### RFC amendment (v1.4)

In-place additive amendment to RFC-0010 (mirrors v1.2 → v1.3 pattern):

- Add `ChainId` type + `DidRegistry` `chain` parameter
- Add `CompositeDidRegistry` impl
- §Future Work F2 status moved to "closed (landed in v1.4)"
- New F8: chain federation consistency (CRDT-style merge of
  per-chain `DidDocument` revisions)

## Test Vectors (preview)

- 5 new TV: register-on-multiple-chains; resolve-by-chain-dispatch;
  revoke-on-one-chain-isolates; composite-empty-chain-returns-none;
  chain-namespace-validation (rejects unknown / unallocated chains).

## Layer direction

- `octo-ident` (Layer B) — `ChainId` + `CompositeDidRegistry`
- `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
  gains `chain_id` column in migration v009
- `octo-identity-resolver-node` (Layer C) — `IdentityResolverConfig`
  gains `chains: Vec<ChainId>` slot

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

## Cross-references

- [[rfc-0010-v13-storage-extension]] — v1.3 substrate
- [[mission-0871b-storage-backend]] — substrate mission (DAG predecessor)
- [[cipherocto-design-principles]] — Extension over enumeration
- [[mission-0959-c4-composite-catalog]] — `CompositeCapabilityCatalog`
  pattern mirror

## Claimant

@unassigned

## Pull Request

#
