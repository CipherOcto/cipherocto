# Mission: Holder Registry Schema + Trait + Reference Impl (RFC-0957-A1 §Phase 1)

## Status

Claimed (2026-08-04)

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0957-a1-holder-registry.md` (top-level decomposition mission)

## Summary

Implement RFC-0957-A1 §Phase 1: the schema, trait, and reference impl for the holder registry. Author the `HolderKind` enum (4 variants), `HolderRecord` content-addressable struct with 10 fields (cap_root_hash PK + kind + holder_did + holder_pub + audience_did + caveats_canonical + ask_id + mint_at_millis_unix + ttl_millis_unix + revoked_at_millis_unix), `HolderRegistry` trait with 6 methods, `Transaction` type for atomic multi-record operations, and `StoolapHolderRegistry` reference impl backed by a stoolap table per RFC-0862 with `UNIQUE(ask_id, kind) WHERE ask_id IS NOT NULL` and `INDEX(ask_id, kind)`.

Manual redacting `Debug` impls on all security-bearing structs: redacts `cap_root_hash`, `holder_pub`, `holder_priv`, `signatures`, `caveats_canonical` content, `revoked_at_millis_unix`. Replaces auto-derive `Debug`. Per RFC-0957-A1 §Security.

`HolderRecord::from_bearer` + `HolderRecord::from_capability` constructors (R7-N7 caveat variant aliases). `from_hop_capability` lives in sub-mission 0970-a (cross-mission dependency on RFC-0970).

## Acceptance Criteria

### Type definitions

- [ ] `crates/octo-wallet/src/capability/holder_kind.rs` (NEW) — `HolderKind` enum: `V1 = 0x00`, `ZKBearing = 0x01`, `Bearer = 0x02`, `HopCapability = 0x03`. Manual Debug impl.
- [ ] `crates/octo-wallet/src/capability/holder_record.rs` (NEW) — `HolderRecord` struct + 10 fields. Manual redacting Debug impl per RFC-0957-A1 §Security. `from_bearer(b: BearerCapsule, mint_at_unix_ms: i64) -> Self` + `from_capability(t: CapabilityToken, mint_at_unix_ms: i64) -> Self` constructors.
- [ ] `crates/octo-wallet/src/capability/holder_registry.rs` (NEW) — `HolderRegistry` trait (6 methods): `lookup(cap_root_hash) -> Option<HolderRecord>`, `lookup_by_ask(ask_id, kind) -> Option<HolderRecord>`, `lookup_active(cap_root_hash, &dyn Clock) -> Option<HolderRecord>`, `insert(record) -> Result<(), RegistryError>`, `revoke(cap_root_hash, &dyn Clock) -> Result<(), RegistryError>`, `sync_peers() -> Result<(), RegistryError>`. (R24-N3 fix: `revoke` takes clock parameter per R15-N3 canonical signature; R26-N3 supersedes prior "uses internal clock" wording.)
- [ ] `crates/octo-wallet/src/capability/transaction.rs` (NEW) — `Transaction` type for atomic multi-record operations.
- [ ] `crates/octo-wallet/src/capability/stoolap_holder_registry.rs` (NEW) — `StoolapHolderRegistry` impl backed by `stoolap::Database`. Schema: `cap_root_hash BLOB PRIMARY KEY, kind TINYINT, holder_did TEXT, holder_pub BLOB, audience_did TEXT, caveats_canonical BLOB, ask_id TEXT NULL, mint_at_millis_unix BIGINT, ttl_millis_unix BIGINT, revoked_at_millis_unix BIGINT NULL`. UNIQUE constraint `UNIQUE(ask_id, kind) WHERE ask_id IS NOT NULL` (RFC-0957-A1 §Schema). INDEX `(ask_id, kind)`.

### Debug redaction (RFC-0957-A1 §Security)

- [ ] Manual `impl Debug for HolderRecord` — redact `cap_root_hash`, `holder_pub`, `caveats_canonical` (display `[REDACTED caveats]`), `revoked_at_millis_unix` (display `None` if Some, else `None`).
- [ ] Manual `impl Debug for HolderKind` — display variant name only (no payload).
- [ ] Unit test: `format!("{:?}", record)` does NOT contain any byte sequence from `cap_root_hash` or `holder_pub`.

### Cross-node mint verifiability (G5)

- [ ] Integration test: node A mints capability, inserts via `StoolapHolderRegistry::insert`, syncs to node B (RFC-0862 gossip), node B's `lookup(cap_root_hash)` returns the same `HolderRecord`. Cross-node mint verifiable end-to-end.

### 4-kind agnosticism (G6)

- [ ] Unit test inserting one record per `HolderKind` variant (V1, ZKBearing, Bearer, HopCapability); each round-trips; lookup returns the same kind byte.

### Atomicity (G8)

- [ ] Forced-failure integration test: `TransactionExt::insert_dual(bearer, capability)` where the capability insert fails — bearer record MUST NOT be persisted (all-or-nothing).

### Test vectors (RFC-0957-A1 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV4, TV6, TV12, TV13, TV14)

- [ ] TV1: Lookup Hit — insert record, lookup returns same record.
- [ ] TV2: Lookup Miss — lookup on absent `cap_root_hash` returns `None`.
- [ ] TV3: Insert + Duplicate — second insert with same `cap_root_hash` PK returns `RegistryError::DuplicateKey`.
- [ ] TV4: Revoke + Lookup — revoke sets `revoked_at_millis_unix`; subsequent `lookup` returns the revoked record; `lookup_active` returns `None` after revocation.
- [ ] TV6: 4-Kind Agnosticism — insert one per variant, lookup returns matching kind.
- [ ] TV12: `lookup_by_ask` UNIQUE — two inserts with same `(ask_id, kind)` second one fails UNIQUE constraint.
- [ ] TV13: Debug Redaction — `format!("{:?}", record)` contains `[REDACTED]` markers; grep test for credential material.
- [ ] TV14: `revoked_at_millis_unix` Distinct from `ttl_millis_unix` — assert field independence: a record with `ttl_millis_unix=0` and `revoked_at_millis_unix=None` is "active, no TTL expiry"; a record with `revoked_at_millis_unix=Some(t)` is "revoked at t".

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0862 — persistence + gossip substrate (Stoolap)
- RFC-0126 — canonical_ser for `caveats_canonical` column

**Requires (mission gates):**

- `missions/open/0957-a1-holder-registry.md` (top-level) — this is a sub-mission
- `missions/claimed/0957-a-capability-token-macaroon.md` (in progress) — BearerCapsule + CapabilityToken constructors MUST exist before `from_bearer` / `from_capability` can compile

**Not Requires:**

- RFC-0957-A1 §Phantom Types (IdentityKey stub) lives in top-level mission, not here

```yaml
depends_on:
  - 0957-a-capability-token-macaroon # BearerCapsule + CapabilityToken types
  - 0957-a1-holder-registry # top-level decomposition
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `HolderKind` enum (4 variants)
- `HolderRecord` struct (10 fields) + `from_bearer` + `from_capability` constructors
- `HolderRegistry` trait (6 methods)
- `Transaction` type
- `StoolapHolderRegistry` reference impl
- Manual redacting `Debug` impls

`HolderRecord::from_hop_capability` constructor lives in sub-mission 0970-a (cross-mission dependency on RFC-0970).

## Location

- `crates/octo-wallet/src/capability/holder_kind.rs` (NEW)
- `crates/octo-wallet/src/capability/holder_record.rs` (NEW)
- `crates/octo-wallet/src/capability/holder_registry.rs` (NEW)
- `crates/octo-wallet/src/capability/transaction.rs` (NEW)
- `crates/octo-wallet/src/capability/stoolap_holder_registry.rs` (NEW)
- `crates/octo-wallet/src/capability/mod.rs` (MODIFY) — add module exports

## Claimant

@mmacedoeu (CipherOcto-side implementation; cipherocto-side migration only per [[stoolap-general-purpose-db]] red line)

## Pull Request

(unset)

## Notes

- The `Clock` trait used in `lookup_active(cap_root_hash, &dyn Clock)` and `revoke(cap_root_hash, &dyn Clock)` already exists from RFC-0853; this sub-mission consumes it.
- The `stoolap` dep is the CipherOcto fork at `feat/blockchain-sql` per [[feedback_stoolap-persistence]]. Schema migration path documented in RFC-0957-A1 §Appendix A.
- TV11 (`insert_dual` atomicity) crosses into sub-mission 0969-b (RFC-0969 §Algorithms:mint_dual). Co-author contract: 0969-b owns the algorithm; this sub-mission owns the trait method `Transaction::insert_dual(...)` if it lives on `Transaction`, or 0969-b owns it if it lives as a free function. Pre-RFC-0957-A2 split: default `Transaction::insert_dual`.
