# 0871b-storage-backend — DidRegistry impls + resolver-node wiring

**Status:** ready-to-claim (RFC-0010 v1.3 landed commit `2ace15e9`)
**Substrate:** RFC-0010 v1.3 §Storage Extension (Accepted 2026-08-10)
**Parent:** 0871b-identity-resolver-node (claimed `3b1767d6`) per
`missions/claimed/0871b-identity-resolver-node.md`

## Scope

Mission `0871b-identity-resolver-node` landed Phase 1 MVP with a
placeholder `public_key` derivation (`RawDid::hash`) because the
storage backend substrate was missing. Mission `0871b-storage-backend`
ships the production storage wiring:

1. **`crates/octo-ident/src/registry.rs`** (NEW) — `DidRegistry` trait
   - `DidDocument` + `DidRegistryError` per RFC-0010 v1.3 §Storage
     Extension §Data Structures.
2. **`crates/octo-ident/src/in_memory_did_registry.rs`** (NEW) —
   `InMemoryDidRegistry` impl per RFC-0010 v1.3 §Storage Extension
   §InMemoryDidRegistry. `parking_lot::RwLock<HashMap<String, DidDocument>>`
   thread-safe wrapper.
3. **`crates/quota-router-storage/src/stoolap_did_registry.rs`** (NEW)
   — `StoolapDidRegistry` impl per RFC-0010 v1.3 §Storage Extension
   §StoolapDidRegistry. Cipherocto-side migration v008 per
   [[stoolap-general-purpose-db]].
4. **`crates/quota-router-storage/migrations/v008__create_did_registry.sql`**
   (NEW) — `did_registry` table + `did_registry_updated_at` index per
   RFC-0010 v1.3 §Storage Extension §StoolapDidRegistry.
5. **`crates/octo-identity-resolver-node/src/handlers/resolve.rs`**
   (MODIFIED) — swap placeholder `RawDid::hash` derivation for
   `DidRegistry::resolve(canonical_did)`. `IdentityResolverNodeConfig`
   gains `registry: Arc<dyn DidRegistry>` slot. `ResolveHandler` no
   longer self-derives the public key — calls `registry.resolve(&wire)`.

### Why this is unblocked now

RFC-0010 v1.3 amendment (commit `2ace15e9`) defined the substrate:
`DidRegistry` trait lives in `crates/octo-ident/` (Layer B years-stable),
`StoolapDidRegistry` lives in `crates/quota-router-storage/` (Layer
B-adjacent storage), mirror to `StoolapSpendLedger` pattern (mission
`0871e-phase5b-stoolap-ledger`). Cross-instance coordination (F7) +
`ResolverBackend` typed view (F6) + multi-chain resolution (F2) + rich
DID Documents explicitly OUT of scope per RFC-0010 v1.3 §Storage
Extension §Out of scope.

### Layer direction (per [[cipherocto-design-principles]])

- `octo-ident` (Layer B) — `DidRegistry` trait + `InMemoryDidRegistry`
- `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
- `octo-identity-resolver-node` (Layer C) — consumer only, no
  substrate ownership

No reverse dependency (`octo-identity-resolver-node` does NOT add a
dep on `quota-router-storage` — registry is injected via `Arc<dyn
DidRegistry>` per the trait-object dispatch pattern established in
RFC-0957-A1 §HolderRegistry).

## Acceptance Criteria

### Top-level: Storage trait + impls

- [ ] `crates/octo-ident/src/registry.rs` defines `DidRegistry` trait +
      `DidDocument` + `DidRegistryError` exactly per RFC-0010 v1.3
      §Storage Extension §Data Structures
- [ ] `crates/octo-ident/src/in_memory_did_registry.rs` defines
      `InMemoryDidRegistry` impl; re-exports at `octo_ident::InMemoryDidRegistry`
- [ ] `crates/quota-router-storage/src/stoolap_did_registry.rs` defines
      `StoolapDidRegistry` impl with raw-byte-slice API (NOT `WireDid`
      typed wrapper) to avoid cyclic crate dep — same pattern as
      `StoolapSpendLedger`
- [ ] `crates/quota-router-storage/migrations/v008__create_did_registry.sql`
      applies `did_registry` table + `did_registry_updated_at` index
- [ ] `crates/quota-router-storage/src/migrations.rs` registers v008

### Resolver-node wiring

- [ ] `IdentityResolverNodeConfig` gains `registry: Arc<dyn DidRegistry>` slot
- [ ] `IdentityResolverNode::new(config)` validates `config.registry.is_some()`
      in debug builds; production default = `Arc::new(InMemoryDidRegistry::default())`
- [ ] `ResolveHandler::handle` calls `config.registry.resolve(&wire)`
      instead of deriving placeholder from `RawDid::hash`
- [ ] `ResolveResponse.public_key` populated from `DidDocument.public_key`
      (still 32 bytes; wire shape byte-exact across cutover)
- [ ] `octo-identity-resolver-node` gains `octo-ident` dep (already
      present per closure record); does NOT gain `quota-router-storage`
      dep (registry injected via trait object)

### Test vectors (per RFC-0010 v1.3 §Storage Extension §Compatibility)

- [ ] `register_resolve_round_trip` — register `(canonical_did,
    DidDocument{public_key, revoked:false})` → resolve returns
      `Some(DidDocument)` with same `public_key`
- [ ] `register_upsert_overwrites_existing` — register twice, second
      registration overwrites `DidDocument` (upsert semantics)
- [ ] `resolve_unknown_returns_none` — resolve unregistered DID returns
      `Ok(None)` (NOT error)
- [ ] `revoke_marks_resolve_none` — register → revoke → resolve returns
      `Ok(None)` (fail-closed)
- [ ] `revoke_unknown_errors` — revoke unregistered DID returns
      `Err(DidRegistryError::UnknownDid)`
- [ ] `register_after_revoke_errors` — register DID that was revoked
      returns `Err(DidRegistryError::AlreadyRevoked)`
- [ ] `list_returns_all_active_dids` — register 3 DIDs → list returns 3;
      revoke one → list returns 2
- [ ] `register_resolve_concurrent_load` (20 threads × 1000 ops each,
      exactly 0 races) — matches `StoolapSpendLedger` atomicity TV
- [ ] `resolve_handler_uses_registry` (resolver-node integration) —
      `IdentityResolverNode` configured with custom `DidRegistry`
      (returns distinct `public_key` per DID); `ResolveHandler` returns
      that `public_key` (NOT the placeholder hash)
- [ ] `wire_shape_byte_exact_across_cutover` — pre-cutover
      `ResolveResponse` (placeholder hash) byte-equals post-cutover
      `ResolveResponse` when registry's `DidDocument.public_key` IS the
      placeholder hash (regression guard for the cutover)

## Type Coverage

| RFC Type / Section                             | Implemented By                                                                        |
| ---------------------------------------------- | ------------------------------------------------------------------------------------- |
| `DidRegistry` trait (RFC-0010 v1.3 §Storage)   | This mission — `crates/octo-ident/src/registry.rs`                                    |
| `DidDocument` (RFC-0010 v1.3 §Storage)         | This mission — same file                                                              |
| `DidRegistryError` (RFC-0010 v1.3 §Storage)    | This mission — same file                                                              |
| `InMemoryDidRegistry` (RFC-0010 v1.3 §Storage) | This mission — `crates/octo-ident/src/in_memory_did_registry.rs`                      |
| `StoolapDidRegistry` (RFC-0010 v1.3 §Storage)  | This mission — `crates/quota-router-storage/src/stoolap_did_registry.rs`              |
| `did_registry` table (migration v008)          | This mission — `crates/quota-router-storage/migrations/v008__create_did_registry.sql` |
| `IdentityResolverNodeConfig.registry` slot     | This mission — `crates/octo-identity-resolver-node/src/node.rs`                       |
| `ResolveHandler` calls `registry.resolve`      | This mission — `crates/octo-identity-resolver-node/src/handlers/resolve.rs`           |

## Dependencies

**Requires:**

- RFC-0010 v1.3 (Accepted 2026-08-10) — `DidRegistry` trait substrate
- RFC-0010 v1.2 — canonical DID codec substrate
- `crates/octo-ident` — Layer B codec crate (already in workspace)
- `crates/quota-router-storage` — Layer B-adjacent storage crate (already in workspace)
- `crates/octo-identity-resolver-node` — Layer C consumer (Phase 1 MVP landed)

**Mission gates (sequential):**

- RFC-0010 v1.3 MUST land first (DONE — commit `2ace15e9`)
- Mission `0871b-identity-resolver-node` MUST complete first (DONE — commit `3b1767d6`)

**Not Requires (explicitly OUT of scope per RFC-0010 v1.3):**

- Cross-instance DID write coordination (RFC-0862 amendment — separate)
- `ResolverBackend` typed view (RFC-0871 §Future Work — mission
  `0871b-cross-domain-resolution` follows this mission)
- Multi-chain DID resolution (RFC-0010 F2 — separate)
- Rich DID Documents (future amendment — separate)

## Implementation Guide

### Step 1: Trait + test impl in `octo-ident`

`crates/octo-ident/src/registry.rs` — defines `DidRegistry` trait +
`DidDocument` + `DidRegistryError` per RFC-0010 v1.3 §Storage Extension
§Data Structures. Add `pub use` re-export at `crates/octo-ident/src/lib.rs`.

`crates/octo-ident/src/in_memory_did_registry.rs` — defines
`InMemoryDidRegistry` per RFC-0010 v1.3 §Storage Extension
§InMemoryDidRegistry. Uses `parking_lot::RwLock<HashMap<String,
DidDocument>>` for thread safety. Add `parking_lot` to
`crates/octo-ident/Cargo.toml` dev-deps (test only — production uses
`StoolapDidRegistry`).

Add unit tests per RFC-0010 v1.3 §Storage Extension §Compatibility.

### Step 2: Production impl in `quota-router-storage`

`crates/quota-router-storage/migrations/v008__create_did_registry.sql`
per RFC-0010 v1.3 §Storage Extension §StoolapDidRegistry. Register
v008 in `crates/quota-router-storage/src/migrations.rs`.

`crates/quota-router-storage/src/stoolap_did_registry.rs` — defines
`StoolapDidRegistry` per RFC-0010 v1.3. Uses raw byte slices (NOT
`WireDid` typed wrapper) to avoid cyclic crate dep — same pattern as
`StoolapSpendLedger` (`crates/quota-router-storage/src/stoolap_spend_ledger.rs`).
The `canonical_did` parameter is the decoded 32-byte hash
(`RawDid::hash`), NOT the wire form. A glue crate
(`crates/octo-identity-resolver-node` boundary adapter) converts
`WireDid` ↔ `RawDid::hash` at the consumer boundary.

Add unit tests per RFC-0010 v1.3 §Storage Extension §Compatibility +
atomicity TV (`register_resolve_concurrent_load`).

### Step 3: Resolver-node wiring

`crates/octo-identity-resolver-node/src/node.rs` — add
`IdentityResolverNodeConfig.registry: Arc<dyn DidRegistry>` slot.
Update `IdentityResolverNode::new(config)` to validate registry presence.

`crates/octo-identity-resolver-node/src/handlers/resolve.rs` — replace
placeholder `RawDid::hash` derivation with
`config.registry.resolve(&wire)`. The new error variant
`IdentityResolveError::Storage(String)` propagates from
`DidRegistryError::Storage`. Update
`crates/octo-identity-resolver-node/src/handlers/mod.rs` to add
the new variant + `From<DidRegistryError>` conversion.

Add integration tests per RFC-0010 v1.3 §Storage Extension §Compatibility
(`resolve_handler_uses_registry`, `wire_shape_byte_exact_across_cutover`).

### Step 4: Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

Per [[feedback_stoolap_test_performance]] use `--lib` (full build
compiles ~150 test files = 2+ min).

## Backward compat

- `ResolveResponse` wire shape byte-exact across placeholder/real-registry
  cutover (still `public_key: [u8; 32]`). Consumers unchanged.
- `IdentityResolverNode::new` adds `Arc<dyn DidRegistry>` to config;
  Phase 1 MVP callers (tests) MUST update their config literal to pass
  `Arc::new(InMemoryDidRegistry::default())`. Production default in
  `IdentityResolverNode::new` covers deployment configs.
- `crates/octo-ident` does NOT gain `quota-router-storage` dep
  (registry trait object is dyn-compatible; consumers wire storage at
  the consumer boundary).

## Test Vector Discipline

10 new TV per RFC-0010 v1.3 §Storage Extension §Compatibility:
`register_resolve_round_trip`, `register_upsert_overwrites_existing`,
`resolve_unknown_returns_none`, `revoke_marks_resolve_none`,
`revoke_unknown_errors`, `register_after_revoke_errors`,
`list_returns_all_active_dids`, `register_resolve_concurrent_load`,
`resolve_handler_uses_registry`, `wire_shape_byte_exact_across_cutover`.

## Cross-references

- [[mission-0871b-identity-resolver-node]] — Phase 1 MVP predecessor
- [[mission-0871b-cross-domain-resolution]] — F6 follow-on (gated on
  this mission + RFC-0871 §Future Work)
- [[wave-3-plan-correction-2026-08-10]] — drift context (RFC-0010
  v1.3 unblocking this mission)
- [[cipherocto-design-principles]] — Layer B additive-only rule
- [[stoolap-general-purpose-db]] — cipherocto-side migration convention
- [[feedback_clippy_zero_warnings]] — clippy invariant
- [[feedback_stoolap_test_performance]] — `--lib` test convention

## Claimant

@unassigned

## Pull Request

#
