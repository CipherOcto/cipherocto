# Mission: 0010-f2-registry-namespacing — Multi-Chain Registry Column

## Status

open (filed 2026-08-11). Builds on
`0010-f2-multi-chain-did-resolution` (commit `f6478bda`, RFC-0010
v1.4 ACCEPTED) + `0010-f8-rich-did-storage` (commit `269cf923`).

## Problem

RFC-0010 v1.4 introduced `ChainId` + `ChainNamespace` (the typed
canonical representation: 17 bytes = `[variant: u8 | tag: 15
bytes | length: u8]`). The `StoolapDidRegistry` `did_registry`
schema (v008) is keyed by `canonical_hash` only — multi-chain DIDs
that share the same `canonical_hash` collide on the PK. This
blocks RFC-0010 v1.4's intent that each `(chain_id, canonical_hash)`
pair is a unique registry row.

Recon:
- `octo-ident` (Layer B) — `ChainId` + `ChainNamespace` substrate
  LANDED (commit `f6478bda`). 17-byte canonical form is stable.
- `quota-router-storage` (Layer B-adjacent) — `did_registry`
  schema PK = `(canonical_hash)` only. No chain column.
- Stoolap fork supports `ALTER TABLE ADD COLUMN` (parser path
  confirmed at `stoolap/.../parser/statements.rs:2304` via
  `parse_column_definition`).
- Stoolap fork supports `CREATE UNIQUE INDEX IF NOT EXISTS`
  (parser path at `stoolap/.../parser/statements.rs:1927`).
- Stoolap fork supports `DROP PRIMARY KEY` — NOT VERIFIED; the
  v008 PK is preserved and a new composite UNIQUE INDEX is added
  for the (chain_id, canonical_hash) namespace.

## Fix

### Schema migration v011

```sql
ALTER TABLE did_registry ADD COLUMN chain_id BLOB NOT NULL
  DEFAULT x'0101000000000000000000000000000000000000';
```

The default is the 17-byte canonical encoding of
`CIPHEROCTO_MAINNET` (`ChainId::default()` →
`ChainNamespace::Rfc` + tag + length=18):
```
variant = 0x01 (Rfc)
tag     = 0xeb3071b5e113330c8763 09 54e3cc08 (CIPHEROCTO_MAINNET_TAG, 15 bytes)
length  = 0x12 (18 chars for "cipherocto-mainnet")
```

Hex literal expands to 34 hex chars = 17 bytes. Legacy v008
rows auto-backfill on migration.

```sql
CREATE UNIQUE INDEX IF NOT EXISTS did_registry_chain_hash_uidx
    ON did_registry (chain_id, canonical_hash);
```

Composite uniqueness across the new namespace. PK
`(canonical_hash)` preserved so legacy single-chain lookups still
hit the PK index.

### `DidRegistry` trait — additive evolution

Layer B (years-stable). Per
[[cipherocto-design-principles]] §Open/Closed Principle + §Stable
Abstractions Principle, the trait gains two ADDITIVE methods
with default impls (back-compat: existing impls work unchanged):

```rust
pub trait DidRegistry: Send + Sync + 'static {
    // ... existing 4 methods unchanged ...

    /// Register a DID on an explicit chain namespace. Default impl
    /// forwards to `register` (single-chain mode).
    fn register_in_chain(
        &self,
        _chain_id: &ChainId,
        canonical_hash: &[u8; 32],
        doc: DidDocument,
    ) -> Result<(), DidRegistryError> {
        self.register(canonical_hash, doc)
    }

    /// Resolve a DID on an explicit chain namespace. Default impl
    /// forwards to `resolve` (single-chain mode).
    fn resolve_in_chain(
        &self,
        _chain_id: &ChainId,
        canonical_hash: &[u8; 32],
    ) -> Result<Option<DidDocument>, DidRegistryError> {
        self.resolve(canonical_hash)
    }
}
```

`StoolapDidRegistry` overrides both: writes/reads the explicit
`chain_id` BLOB column. `InMemoryDidRegistry` keeps the default
impls (single-chain mode for tests; production uses stoolap).

### `StoolapDidRegistry` impl changes

- `register` (unchanged signature): SQL gains `chain_id` bind;
  internally writes `chain_id = MAINNET_CHAIN_ID_BYTES`.
- `resolve` (unchanged signature): SQL gains `WHERE chain_id = ?
  AND canonical_hash = ?`.
- `revoke` (unchanged signature): same SQL change as `register`.
- `list` (unchanged signature): filter on `chain_id = MAINNET_CHAIN_ID_BYTES`.
- `register_in_chain` / `resolve_in_chain` (new methods): SQL
  uses the caller-provided `ChainId.canonical_bytes()` (17 bytes).
- `register_in_chain` SELECT/UPDATE/INSERT branches: chain_id
  bind param.
- `MAINNET_CHAIN_ID_BYTES: [u8; 17]` const = the precomputed
  canonical encoding of `CIPHEROCTO_MAINNET`. Stored as a const
  in the module (not derived at runtime — the constant is
  verified in TV against `ChainId::default().namespace()`).

## Acceptance criteria

- [ ] NEW: `migrations/v011__add_chain_id_namespace.sql` with
      the ADD COLUMN + UNIQUE INDEX statements.
- [ ] `src/migrations.rs` static catalog gains v011 entry.
- [ ] `StoolapDidRegistry` impl updated:
  - `register` SQL gains chain_id bind (mainnet).
  - `resolve` SQL filters on chain_id = mainnet.
  - `revoke` SQL filters on chain_id = mainnet.
  - `list` SQL filters on chain_id = mainnet.
  - NEW `register_in_chain` + `resolve_in_chain` override the
    default impls with explicit chain_id SQL.
  - `MAINNET_CHAIN_ID_BYTES` const matches
    `ChainId::default().namespace().unwrap().canonical_bytes()`.
- [ ] `octo-ident::DidRegistry` trait gains
  `register_in_chain` + `resolve_in_chain` with default impls.
- [ ] NEW TV `stoolap_chain_namespace.rs` (1 TV):
  `register_in_chain_isolates_dids_across_chains`: register same
  `canonical_hash` on two distinct `ChainId` values (e.g.,
  `cipherocto-mainnet` + a user-extension chain); both resolve
  independently under their respective chain.
- [ ] Existing TV in `tests/stoolap_rich_did.rs` +
  `tests/stoolap_migration_chain.rs` + `tests/stoolap_idempotent_alter.rs`
  still pass (no regression; chain_id = mainnet for existing tests).
- [ ] Migration chain test gains `migration_chain_reaches_v011_on_fresh_db`
  + `migration_chain_creates_chain_id_column`.

## Files

- `crates/octo-ident/src/registry.rs` — trait gains 2 new methods
  with default impls.
- `crates/octo-ident/src/in_memory_did_registry.rs` — no override
  needed (default impls; in-memory tests use single-chain mode).
- `crates/quota-router-storage/migrations/v011__add_chain_id_namespace.sql`
  (NEW) — ADD COLUMN + UNIQUE INDEX.
- `crates/quota-router-storage/src/migrations.rs` — catalog entry
  v11.
- `crates/quota-router-storage/src/stoolap_did_registry.rs` —
  MAINNET_CHAIN_ID_BYTES const + SQL filter on chain_id in 4
  methods + 2 new chain-aware overrides.
- `crates/quota-router-storage/tests/stoolap_chain_namespace.rs`
  (NEW) — 1 TV.

## Layer discipline

- `octo-ident` (Layer B) — additive trait methods only (no
  breaking signature change). Default impls preserve back-compat.
- `quota-router-storage` (Layer B-adjacent) — schema migration +
  impl update.
- `octo-identity-resolver-node` (Layer C) — UNCHANGED. The
  chain-aware resolution path lands in
  `0010-f2-multi-chain-routing` (follow-on).

## Defer (explicit)

- `list_in_chain` / `revoke_in_chain` methods — out of scope;
  the default `list` / `revoke` signatures cover the single-chain
  use case. Multi-chain list/revoke lands when
  `0010-f2-multi-chain-routing` is filed.
- `IdentityResolverConfig.chains: Vec<ChainId>` (per chain.rs:29
  follow-on) — NOT in scope; resolver-node config change.

## Cross-references

- RFC-0010 v1.4 §ChainId Namespace Extension
- `0010-f2-multi-chain-did-resolution` (commit `f6478bda`) — typed
  `ChainId` + `ChainNamespace` substrate
- `0010-f8-rich-did-storage` (commit `269cf923`) — previous
  migration in the same chain
- `crates/octo-ident/src/chain.rs:55` —
  `CIPHEROCTO_MAINNET_TAG` const (15-byte tag)
- `crates/octo-ident/src/chain.rs:236` —
  `ChainNamespace::canonical_bytes()` (17-byte form)