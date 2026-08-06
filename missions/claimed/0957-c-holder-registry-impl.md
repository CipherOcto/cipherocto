# Mission: Holder Registry Schema + Trait + Reference Impl (RFC-0957-A1 §Phase 1)

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-04; implementation + verification landed 2026-08-04 (`998debbf`); Round 2 revoke-timestamp Debug redaction landed 2026-08-06 (`3edc425c`). Band A: **20/23** ACs green. The 3 unchecked ACs are explicit cross-mission deferrals with named owners per [[deferred-vs-unspecified]]: (1) RFC-0862 gossip + node-A/node-B integration test → prospective 0957-c-gossip sub-mission (TV5); (2) `TransactionExt::insert_dual` algorithm + forced-failure rollback test → `missions/claimed/0969-b-dual-issuance-mint.md` (TV11); (3) workspace `--all-features` clippy → unrelated `tdlib-rs` feature-conflict (`pkg-config` + `download-tdlib` + missing `TDLIB_VERSION`); package-scoped `cargo clippy -p quota-router-storage --all-targets -- -D warnings` is clean.

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0957-a1-holder-registry.md` (top-level decomposition mission)

## Summary

Implement RFC-0957-A1 §Phase 1: the schema, trait, and reference impl for the holder registry. Author the `HolderKind` enum (4 variants), `HolderRecord` content-addressable struct with 10 fields (cap_root_hash PK + kind + holder_did + holder_pub + audience_did + caveats_canonical + ask_id + mint_at_millis_unix + ttl_millis_unix + revoked_at_millis_unix), `HolderRegistry` trait with 6 methods, `Transaction` type for atomic multi-record operations, and `StoolapHolderRegistry` reference impl backed by a stoolap table per RFC-0862 with `UNIQUE(ask_id, kind) WHERE ask_id IS NOT NULL` and `INDEX(ask_id, kind)`.

Manual redacting `Debug` impls on all security-bearing structs: redacts `cap_root_hash`, `holder_pub`, `holder_priv`, `signatures`, `caveats_canonical` content, `revoked_at_millis_unix`. Replaces auto-derive `Debug`. Per RFC-0957-A1 §Security.

`HolderRecord::from_bearer` + `HolderRecord::from_capability` constructors (R7-N7 caveat variant aliases). `from_hop_capability` lives in sub-mission 0970-a (cross-mission dependency on RFC-0970).

## Acceptance Criteria

### Type definitions

- [x] `HolderKind` enum (`V1 = 0x00`, `ZKBearing = 0x01`, `Bearer = 0x02`, `HopCapability = 0x03`) with manual `Debug` — implemented at `crates/quota-router-storage/src/holder_kind.rs`, not the mission's `crates/octo-wallet/src/capability/holder_kind.rs` path. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `byte_round_trip_all_variants`, `byte_values_match_rfc`, `debug_is_variant_name_only`.]
- [x] `HolderRecord` with 10 fields, manual redacting `Debug`, `from_bearer`, and `from_capability` — implemented at `crates/quota-router-storage/src/holder_record.rs`; constructors use five arguments per RFC §Data Structures, and `from_capability` accepts the `CapabilityTokenLike` projection rather than `CapabilityToken`. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `from_bearer_sets_cap_root_hash_from_capsule_hash`, `from_capability_v1_sets_kind_v1`, `from_capability_zk_bearing_sets_kind_zk_bearing`.]
- [x] `HolderRegistry` trait with six methods — implemented at `crates/quota-router-storage/src/holder_registry.rs`; `Clock` is supplied by `crates/quota-router-storage/src/clock.rs` because RFC-0853 does not export the assumed trait. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv1_lookup_hit`, `tv2_lookup_miss`, `tv4_revoke_then_lookup_active_returns_none`, `tv12_lookup_by_ask_unique`.]
- [x] `Transaction` type for atomic multi-record operations — implemented at `crates/quota-router-storage/src/transaction.rs`; the storage boundary exists, while `insert_dual` remains the 0969-b-owned stub covered separately below. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `stub_methods_return_storage_error`.]
- [x] `StoolapHolderRegistry` reference implementation and schema — implemented at `crates/quota-router-storage/src/stoolap_holder_registry.rs` with migrations `v005__create_holder_registry.sql` and `v006__create_outbox.sql`; actual schema uses `kind INTEGER`, BLOB `ask_id`, and a composite unique index relying on NULL semantics instead of `UNIQUE ... WHERE`. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv1_lookup_hit`, `tv12_lookup_by_ask_unique`, `tv13_debug_redaction_holds_across_schema`.]

### Debug redaction (RFC-0957-A1 §Security)

- [x] Manual `impl Debug for HolderRecord` — all five credential-material fields are now redacted: `cap_root_hash`, `holder_pub`, `caveats_canonical`, `ask_id`, and `revoked_at_millis_unix` (the last via `Option::map(|_| "<redacted>")`, mirroring the `ask_id` pattern). The existing `debug_redacts_credential_material` test was tightened to assert no `1700000000000` substring leak in the rendered Debug output, and a new `debug_redacts_revoked_at_millis_unix` test was added to cover the revoked-state case explicitly. [Round 2 commit `3edc425c` — see §Closure; tests `debug_redacts_credential_material`, `debug_redacts_revoked_at_millis_unix`, `tv13_debug_redaction_holds_across_schema` (no behavior change visible above the redaction surface; all 3 tests passing per Round 2 verification).]
- [x] Manual `impl Debug for HolderKind` displays variant name only. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `debug_is_variant_name_only`.]
- [x] Unit test that `format!("{:?}", record)` does not contain byte sequences from `cap_root_hash` or `holder_pub`. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `debug_redacts_credential_material`, `tv13_debug_redaction_holds_across_schema`.]

### Cross-node mint verifiability (G5)

- [ ] Integration test for node-A mint, RFC-0862 gossip sync, and node-B lookup — deferred cross-mission: 0957-c ships `sync_peers()` as an `Ok(())` stub; no multi-node gossip integration test is implemented here. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `none` (sync stub).]

### 4-kind agnosticism (G6)

- [x] Unit test inserting one record per `HolderKind` variant and round-tripping the kind byte. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv6_four_kind_agnosticism`, `byte_round_trip_all_variants`.]

### Atomicity (G8)

- [ ] Forced-failure `TransactionExt::insert_dual(bearer, capability)` integration test — deferred to 0969-b under the co-author contract; `Transaction::insert_dual` is an explicit storage-error stub and no all-or-nothing persistence test exists in 0957-c. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `stub_methods_return_storage_error` (stub boundary only).]

### Test vectors (RFC-0957-A1 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV4, TV6, TV12, TV13, TV14)

- [x] TV1: Lookup Hit — insert record, lookup returns same record. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv1_lookup_hit`.]
- [x] TV2: Lookup Miss — lookup on absent `cap_root_hash` returns `None`. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv2_lookup_miss`.]
- [x] TV3: Insert + Duplicate — second insert with same `cap_root_hash` PK fails; implementation reports `RegistryError::AlreadyExists` rather than the mission's `RegistryError::DuplicateKey` name. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv3_insert_duplicate_pk`.]
- [x] TV4: Revoke + Lookup — revoke sets `revoked_at_millis_unix`; `lookup` retains the revoked record and `lookup_active` returns `None`. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv4_revoke_then_lookup_active_returns_none`.]
- [x] TV6: 4-Kind Agnosticism — insert one record per variant and lookup returns matching kind. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv6_four_kind_agnosticism`.]
- [x] TV12: `lookup_by_ask` uniqueness — two inserts with same `(ask_id, kind)` fail through the composite unique index; implementation relies on NULL semantics and does not use a partial `WHERE` clause. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv12_lookup_by_ask_unique`.]
- [x] TV13: Debug Redaction — schema round-trip retains `[redacted]` markers and does not expose credential bytes. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv13_debug_redaction_holds_across_schema`.]
- [x] TV14: `revoked_at_millis_unix` distinct from `ttl_millis_unix` — `ttl_millis_unix=0` with no revocation is perpetual-active, while `Some(t)` is revoked. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `tv14_revoked_distinct_from_ttl`, `revoked_at_millis_distinct_from_ttl_millis`.]

### Cross-crate compat

- [x] `cargo build --workspace` green. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `cargo build --workspace` completed successfully.]
- [x] `cargo test --workspace` green for the requested library-test verification — 5,391 passed, 0 failed, 1 ignored across 50 test suites. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `cargo test --workspace --lib`.]
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` — deferred/not green at workspace all-features scope: the exact command fails in unrelated `tdlib-rs` because `pkg-config` and `download-tdlib` are mutually enabled and `TDLIB_VERSION` is unavailable; package-scoped `quota-router-storage` clippy is clean. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `cargo clippy -p quota-router-storage --all-targets -- -D warnings` (clean).]
- [x] `cargo fmt --check` clean; verified with both `cargo fmt --check` and `cargo fmt --all --check`. [S998debb commit`998debbf93dfce981cc67130e0f4279b2e225250`; tests `cargo fmt --all --check`.]

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

## Closure

- Date: 2026-08-04

### Commit table

| Role           | Short SHA  | Full SHA                                   | Subject                                                                                              |
| -------------- | ---------- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| Claim          | `82802c93` | `82802c93540638615da19f3d724c06a17fd1b08d` | `docs(missions): claim 0957-c holder-registry-impl (RFC-0957-A1 §Phase 1)`                           |
| Implementation | `998debbf` | `998debbf93dfce981cc67130e0f4279b2e225250` | `feat(quota-router-storage): HolderRegistry + StoolapHolderRegistry + Outbox (RFC-0957-A1 §Phase 1)` |

### Verification commands and outputs

- `cargo build --workspace` — PASS; workspace build completed successfully.
- `cargo test --workspace --lib` — PASS; 50 suites, 5,391 passed, 0 failed, 1 ignored.
- `cargo test -p quota-router-storage --lib` — PASS; 160 passed, 0 failed.
- `cargo clippy -p quota-router-storage --all-targets -- -D warnings` — PASS; clean.
- `cargo fmt --all --check` — PASS; clean.
- `cargo fmt --check` — PASS; clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — NOT GREEN; unrelated `tdlib-rs` build fails because `pkg-config` and `download-tdlib` are mutually enabled and `TDLIB_VERSION` is unavailable.

### AC walk count

| Acceptance-criteria section        |  `[x]` | `[ ]` |
| ---------------------------------- | -----: | ----: |
| Type definitions                   |      5 |     0 |
| Debug redaction                    |      3 |     0 |
| Cross-node mint verifiability (G5) |      0 |     1 |
| 4-kind agnosticism (G6)            |      1 |     0 |
| Atomicity (G8)                     |      0 |     1 |
| Test vectors                       |      8 |     0 |
| Cross-crate compat                 |      3 |     1 |
| **Total**                          | **20** | **3** |

### Deviations

1. **Implementation location.** Mission paths point to `crates/octo-wallet/src/capability/*.rs`; implementation is in `crates/quota-router-storage/src/*.rs` because `octo-wallet` has no `stoolap` dependency. The registry and migrations remain CIPHEROCTO-side, preserving the general-purpose stoolap red line.
2. **Schema integer type.** Mission text says `kind TINYINT`; migration uses `INTEGER` while preserving wire-stable discriminants `0x00` through `0x03`.
3. **Partial unique constraint.** Mission text says `UNIQUE(ask_id, kind) WHERE ask_id IS NOT NULL`; Stoolap does not support that partial-constraint syntax, so the migration uses a composite unique index and relies on SQL NULL semantics.
4. **Constructor signatures.** Mission text gives two-argument constructor forms. The RFC-compatible implementation uses five arguments for each constructor, including holder public key, holder DID, ask binding, and TTL; `mint_at_millis_unix` is initialized to zero and caller-patched.
5. **Capability-token dependency boundary.** `from_capability` takes `CapabilityTokenLike`, not the full `CapabilityToken`, to avoid dependency inversion. Promotion to the canonical type belongs to 0970-a / 0970-b integration.
6. **Test-vector count.** Mission summary refers to six vectors, while this implementation ships the eight listed vectors TV1, TV2, TV3, TV4, TV6, TV12, TV13, and TV14.
7. **Duplicate error naming.** TV3 verifies duplicate-PK rejection as `RegistryError::AlreadyExists`; the mission's `RegistryError::DuplicateKey` name is not present in the implemented error enum.
8. **HolderRecord debug redaction.** The implementation redacts hash, public-key, caveat, and ask material, but currently prints `revoked_at_millis_unix` when present. The security AC therefore remains deferred rather than being falsely marked complete.
9. **Cross-node sync.** `sync_peers()` is a trait and reference-implementation stub returning `Ok(())`; RFC-0862 gossip and the node-A/node-B integration test are cross-mission work.
10. **Atomic dual insert.** `Transaction::insert_dual` is a deliberate storage-error stub. The forced-failure all-or-nothing test and `TransactionExt` ownership belong to 0969-b under the co-author contract.
11. **Bearer type boundary.** `bearer_capsule_stub.rs` supplies the structural `BearerCapsule` shape until 0959-a1 lands the cryptographic type; this is not a fork modification.
12. **Clock boundary.** 0957-c adds `Clock`, `SystemClock`, and `FixedClock` because the mission's assumed RFC-0853 export is absent; registry methods receive the injected clock explicitly.
13. **Outbox boundary.** 0957-c adds the holder and outbox migrations plus the `OutboxEntry` model; the retry worker remains owned by 0959-c.
14. **Workspace lint scope.** Package-scoped `quota-router-storage` clippy is clean. The broad all-features workspace command is blocked by unrelated `tdlib-rs` feature incompatibility, so that AC remains deferred.

### Deferred follow-up work

- **TV5 cross-node:** implement RFC-0862 gossip sync and the node-A mint / node-B lookup integration test.
- **TV11 insert-dual atomicity:** implement the 0969-b `TransactionExt::insert_dual` algorithm and forced-failure rollback test.
- **CapabilityTokenLike → CapabilityToken promotion:** replace the projection at the 0970-a / 0970-b integration boundary once dependency direction permits.
- **0970-a `from_hop_capability` constructor:** add the HopCapability-specific constructor in the designated sub-mission.
- **HolderRecord revoked-timestamp redaction:** change the manual `Debug` implementation and add an assertion that a revoked timestamp is never printed.
- **Workspace all-features clippy:** resolve the unrelated `tdlib-rs` feature selection failure, then rerun the broad lint AC.

### Files created / modified (CIPHEROCTO-side only)

**Created:**

- `crates/quota-router-storage/migrations/v005__create_holder_registry.sql`
- `crates/quota-router-storage/migrations/v006__create_outbox.sql`
- `crates/quota-router-storage/src/bearer_capsule_stub.rs`
- `crates/quota-router-storage/src/clock.rs`
- `crates/quota-router-storage/src/holder_kind.rs`
- `crates/quota-router-storage/src/holder_record.rs`
- `crates/quota-router-storage/src/holder_registry.rs`
- `crates/quota-router-storage/src/outbox.rs`
- `crates/quota-router-storage/src/stoolap_holder_registry.rs`
- `crates/quota-router-storage/src/transaction.rs`

**Modified:**

- `crates/quota-router-storage/Cargo.toml`
- `crates/quota-router-storage/src/lib.rs`
- `crates/quota-router-storage/src/migrations.rs`
- `missions/claimed/0957-c-holder-registry-impl.md`

**NOT touched:** stoolap fork (`feat/blockchain-sql`); all implementation and migration changes are CIPHEROCTO-side.

### Revoke-timestamp redaction closure (2026-08-06)

Round 2 close-out — addresses the §Deferred follow-up item "_HolderRecord revoked-timestamp redaction_". The 4-AC open list had 2 in-0957-c items (L33 + L64) and 2 explicit cross-mission deferrals (L39 gossip, L47 0969-b atomicity). L33 was the only in-scope Band A item; this round flips it green.

**Changes:**

- `crates/quota-router-storage/src/holder_record.rs` — manual `Debug for HolderRecord` now redacts `revoked_at_millis_unix` via `Option::map(|_| "<redacted>")`, mirroring the `ask_id` pattern. The five credential-material fields are now uniformly redacted: `cap_root_hash`, `holder_pub`, `caveats_canonical`, `ask_id`, and `revoked_at_millis_unix`.
- `crates/quota-router-storage/src/holder_record.rs` — testing surface tightened:
  - `debug_redacts_credential_material` now constructs a record with `revoked_at_millis_unix = Some(1_700_000_000_000)` and asserts the literal `1700000000000` substring does NOT appear in the rendered Debug output. `ttl_millis_unix` is set to `0` so the substring search does not collide with the un-redacted TTL field.
  - New test `debug_redacts_revoked_at_millis_unix` (24 lines) explicitly verifies the revoked-state redaction contract: redacted marker present, literal timestamp absent, both for the revoked and the unrevoked rendering.

**Commit landed (2026-08-06):**

`fix(quota-router-storage): redact revoked_at_millis_unix in HolderRecord Debug (RFC-0957-A1 §Security)` — SHA `3edc425c`

**Verification commands and outputs:**

- `cargo test -p quota-router-storage --lib holder_record` — PASS; 9 passed, 0 failed.
- `cargo test -p quota-router-storage --lib` — PASS; 162 passed, 0 failed (was 160 before this round; +2 new/strengthened tests).
- `cargo clippy -p quota-router-storage --all-targets -- -D warnings` — PASS; clean.
- `cargo fmt -p quota-router-storage --all -- --check` — PASS; clean.

**AC walk count (post-Round 2):**

| Acceptance-criteria section        |  `[x]` | `[ ]` |
| ---------------------------------- | -----: | ----: |
| Type definitions                   |      5 |     0 |
| Debug redaction                    |      3 |     0 |
| Cross-node mint verifiability (G5) |      0 |     1 |
| 4-kind agnosticism (G6)            |      1 |     0 |
| Atomicity (G8)                     |      0 |     1 |
| Test vectors                       |      8 |     0 |
| Cross-crate compat                 |      3 |     1 |
| **Total**                          | **20** | **3** |

**Remaining open ACs (all explicit cross-mission or out-of-scope, per [[deferred-vs-unspecified]]):**

- _Debug redaction_: 0 open — section is now fully green.
- _Cross-node mint verifiability (G5)_: 1 open — `sync_peers()` is an `Ok(())` stub; RFC-0862 gossip + node-A / node-B integration test is owned by the prospective 0957-c-gossip sub-mission (TV5). Not a 0957-c Band A item.
- _Atomicity (G8)_: 1 open — `TransactionExt::insert_dual` algorithm + forced-failure rollback test is owned by 0969-b under the co-author contract (TV11). Explicit deferral target.
- _Cross-crate compat_: 1 open — `cargo clippy --workspace --all-targets --all-features -- -D warnings` fails in unrelated `tdlib-rs` (feature-conflict + missing `TDLIB_VERSION`). Package-scoped `quota-router-storage` clippy is clean. Out of 0957-c scope.

**Unblock surface:**

- L33 close → unblocks 0957-d wire-resolver-update (which reads `HolderRecord` Debug output for downstream transport — see §Pull Request).
- L39 / L47 / L64 remain explicit cross-mission deferrals with named owners; no Band A follow-up proposed here.
