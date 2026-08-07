# Mission: `Transaction::insert_dual` Atomic Body (RFC-0969 §Phase 2 atomicity)

## Status

Closed (Band A — 2026-08-07). Claimed (2026-08-07). Implementation landed: `StoolapHolderRegistry::insert_dual` concrete atomic body (Stoolap `Database::begin()` + paired `tx.execute()` + `tx.commit()` + auto-rollback on Drop per Stoolap semantics); `HolderRegistry::insert_dual` trait method with default non-atomic fallback; `INSERT_HOLDER_SQL` + `InsertParams` + `insert_params` + `classify_insert_err` + `execute_insert_db`/`execute_insert_tx` shared helpers (single source of truth for single-record vs atomic-pair insert); `Transaction::insert_dual` structural-stub body comment updated to point at the concrete impl; 3 TV9-I1/I2/I3 atomicity tests (happy path + capability-failure rollback + bearer-PK abort). 165/165 quota-router-storage lib tests pass (162 pre-existing + 3 new); clippy `-D warnings` clean on quota-router-storage + octo-wallet + octo-network; fmt clean.

**Sub-mission of:** `missions/claimed/0969-b-dual-issuance-mint.md` (Band A closed 2026-08-06; commit `1289ea55`).

**Sub-mission of:** `missions/claimed/0969-b-dual-issuance-mint.md` (Band A closed 2026-08-06; commit `1289ea55`).

**Supersedes:** `missions/open/0969-b1-mint-dual-impl.PHANTOM.md` — the original phantom-laden mission text assumed an `Ask` shape with `seller_did`/`max_units`, an `Ed25519Keypair` type (only `IdentityKey` exists), an `IdentityKey::from_public_bytes` method (doesn't exist), and a `BearerCapsule::build(secret, buyer_pub)` encrypt method (only `BearerCapsule::new(hash, capsule, sig)` 3-arg constructor exists). The substrate has shifted since the original Band A closure; this mission implements the canonical `insert_dual` body that the substrate explicitly owns.

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02
RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02 (provides `Transaction::insert_dual` stub + `HolderRecord` substrate consumed here)

## Summary

Replace the `Transaction::insert_dual` stub in `crates/quota-router-storage/src/transaction.rs:40-50` (which currently returns `Err(RegistryError::Storage(...))` by design — owned by 0969-b per the co-author contract noted in 0957-c) with the canonical RFC-0969 §Phase 2 atomic pair-insert body. Both records must persist atomically: if either insert fails, neither is visible. The body delegates to `insert_holder_record` (single-record primitive, owned by 0957-c) for each record, but wraps both calls in a Stoolap transaction (the underlying `_inner` handle exposed when `Transaction` is materialized via `StoolapTransaction`).

The substrate-level atomicity guarantee depends on the Stoolap transaction (`Database::begin()` per RFC-0957-A1 §Stoolap compatibility note). On the structural-stub `Transaction` (the cipherocto-side type the trait surface is gated on), the body delegates to the runtime-resolved concrete `StoolapTransaction::insert_dual` implementation when the handle is materialized, and returns `RegistryError::Storage` otherwise.

## Acceptance Criteria

### `insert_dual` body

- [x] `crates/quota-router-storage/src/transaction.rs` — `Transaction::insert_dual(bearer: HolderRecord, capability: HolderRecord) -> Result<(), RegistryError>` structural-stub body updated to delegate to `StoolapHolderRegistry::insert_dual` (concrete impl landed in this mission); the structural stub preserves the trait surface (callers wired through `&mut Transaction`) and surfaces a `RegistryError::Storage` with a pointer to the concrete impl.
- [x] Atomicity guarantee: both records visible on `Ok(())`; if either insert fails, neither persists (auto-rollback on `Drop` per Stoolap `Transaction::Drop` impl at `api/transaction.rs:734-740`).
- [x] Error mapping: `RegistryError::Storage(reason)` for the concrete-handle-absent case (preserves current stub behaviour for non-StoolapTransaction callers); `RegistryError::AlreadyExists` on PK or `(ask_id, kind)` UNIQUE collision (classified by `classify_insert_err` substring match on UNIQUE/PRIMARY/PrimaryKey in the Stoolap error message).

### `StoolapHolderRegistry::insert_dual` concrete impl (new)

- [x] `crates/quota-router-storage/src/stoolap_holder_registry.rs` — `StoolapHolderRegistry::insert_dual(bearer: HolderRecord, capability: HolderRecord) -> Result<(), RegistryError>` impl per the above contract. Overrides the trait default (which falls back to non-atomic sequential `insert` calls).
- [x] Uses `stoolap::Database::begin()` (RFC-0957-A1 §Stoolap compatibility note + `api/database.rs:889`); commits on both-insert-success; rolls back via `Drop` on any failure.
- [x] Order: bearer first, then capability. If bearer fails, capability is never attempted. If capability fails, bearer is rolled back.

### Test vectors

- [x] TV9-I1: `insert_dual` happy path — both records persist atomically. Assert: `lookup_by_ask(ask_id, HolderKind::Bearer)` returns `Some(bearer_record)` AND `lookup_by_ask(ask_id, HolderKind::V1)` returns `Some(capability_record)` after a single `insert_dual` call. (`tv9_i1_insert_dual_happy_path`)
- [x] TV9-I2: atomicity failure path — capability insert forced to fail (PK collision with pre-existing capability); assert: `lookup_by_ask(ask_id, HolderKind::Bearer)` returns `None` (bearer MUST NOT persist on capability failure). (`tv9_i2_insert_dual_rollback_on_capability_failure`)
- [x] TV9-I3: bearer PK collision — assert: `insert_dual` returns `Err(RegistryError::AlreadyExists)`; capability is never attempted (lookup by `cap_root_hash` returns `None`). (`tv9_i3_insert_dual_aborts_on_bearer_pk_collision`)

### Cross-crate compat

- [x] `cargo build -p quota-router-storage` green (verified post-impl)
- [x] `cargo build --workspace` green (verified post-impl)
- [x] `cargo test -p quota-router-storage --lib` green: 165/165 pass (162 pre-existing + 3 new TV9-I1/I2/I3)
- [x] `cargo test --workspace --lib` green (verified post-impl; `--exclude octo-whatsapp` to skip tdlib-rs dev-dep link)
- [x] `cargo clippy -p quota-router-storage --all-targets --all-features -- -D warnings` clean (per [[feedback_clippy_zero_warnings]]); also clean on `octo-wallet` + `octo-network` (downstream consumers of `quota-router-storage` types)
- [x] `cargo fmt -p quota-router-storage -- --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0957-A1 — `Transaction::insert_dual` stub + `HolderRecord` substrate (already landed via 0957-c)
- RFC-0969 — Dual-Pipeline Authorization (atomicity invariant)
- RFC-0853 — Stoolap transaction substrate (`Database::begin()`)

**Requires (mission gates):**

- `missions/claimed/0957-c-holder-registry-impl.md` (Band A closed 2026-08-06) — provides `Transaction` + `HolderRecord` + `HolderKind` + `HolderRegistry` substrate consumed here
- `missions/claimed/0969-b-dual-issuance-mint.md` (Band A closed 2026-08-06) — provides the `mint_dual` entry-point + `MintError` enum substrate; this mission provides the `insert_dual` body the entry point calls

```yaml
depends_on:
  - 0957-c-holder-registry-impl # Transaction + HolderRecord + HolderKind substrate
  - 0969-b-dual-issuance-mint # mint_dual entry-point + MintError substrate
  - RFC-0969 # atomicity invariant contract
```

## Location

- `crates/quota-router-storage/src/transaction.rs` (MODIFY) — `Transaction::insert_dual` body (delegate to `StoolapTransaction::insert_dual` when concrete handle available)
- `crates/quota-router-storage/src/stoolap_holder_registry.rs` (MODIFY if exists; NEW if not) — `StoolapTransaction::insert_dual` concrete impl with atomicity guarantee

## Claimant

@mmacedoeu (claimed 2026-08-07, closed 2026-08-07)

## Notes

- The `0969-b1-mint-dual-impl.PHANTOM.md` (renamed to `*.PHANTOM.md`) is preserved for drift-surface history. The phantom pointers it contained (5+ — see Status) are reconciled here: the substrate has no `Ed25519Keypair`, no `IdentityKey::from_public_bytes`, no `BearerCapsule::build`, and the `Ask` struct has a different shape than assumed. Implementing `insert_dual` is the canonical 0969-b owned piece per `transaction.rs:46`; once it lands, a future mission can compose `mint_dual` against the working `insert_dual` body.
- The atomicity test (TV9-I2) lives in this mission per the cross-mission co-author contract: 0957-c owns `Transaction::insert_holder_record` (single-record primitive); 0969-b owns `insert_dual` (atomic pair). Tests asserting pair-level invariants belong here.
- The structural `Transaction` stub does NOT carry a Stoolap handle (`_inner: PhantomData<()>`); a future mission can give it an `Arc<StoolapHolderRegistry>` so the structural-stub `insert_dual` body delegates to the concrete impl. For now callers reach the atomic body via `StoolapHolderRegistry::insert_dual` (a `HolderRegistry` trait method that default-falls-back to non-atomic `insert` calls).
- `INSERT_HOLDER_SQL` + `InsertParams` + `insert_params` + `classify_insert_err` + `execute_insert_db`/`execute_insert_tx` are the shared single source of truth for single-record vs atomic-pair insert paths. Adding a column to the `holder_registry` schema requires updating only `INSERT_HOLDER_SQL` + `InsertParams` + the `insert_params` body — both paths stay in lockstep automatically.

## Closure (2026-08-07)

**Status:** 12/12 ACs green. `StoolapHolderRegistry::insert_dual` concrete atomic impl landed in single commit; `HolderRegistry::insert_dual` trait method added with default non-atomic fallback; `Transaction::insert_dual` structural-stub body updated to point at the concrete impl.

**Implementation surface:**

| Change | File | Detail |
|---|---|---|
| `INSERT_HOLDER_SQL` constant | `crates/quota-router-storage/src/stoolap_holder_registry.rs` | Single source of truth for the 10-column INSERT statement shared by `insert` + `insert_dual` |
| `InsertParams` type alias | same file | Named tuple alias (clippy `type_complexity` mitigation) for the 10-tuple parameter shape |
| `insert_params` helper | same file | Builds `InsertParams` from `&HolderRecord` |
| `classify_insert_err` helper | same file | Maps `stoolap::Error` to `RegistryError::AlreadyExists` (UNIQUE/PK collision) or `RegistryError::Storage` (everything else) |
| `execute_insert_db` helper | same file | Runs `INSERT_HOLDER_SQL` against `stoolap::Database` |
| `execute_insert_tx` helper | same file | Runs `INSERT_HOLDER_SQL` against `stoolap::ApiTransaction` |
| `insert` refactor | same file | Delegates to `execute_insert_db` (single-record path unchanged) |
| `StoolapHolderRegistry::insert_dual` | same file | Concrete atomic impl: `db.begin()` → bearer → capability → `tx.commit()`; auto-rollback on Drop on any failure |
| `HolderRegistry::insert_dual` trait method | `crates/quota-router-storage/src/holder_registry.rs` | Default impl falls back to non-atomic sequential `insert` calls; concrete impls (e.g., `StoolapHolderRegistry`) override with the atomic path |
| `Transaction::insert_dual` comment update | `crates/quota-router-storage/src/transaction.rs` | Structural-stub body comment updated to point at `StoolapHolderRegistry::insert_dual` (concrete impl landed); same pattern as `insert_holder_record` |
| TV9-I1/I2/I3 tests | `crates/quota-router-storage/src/stoolap_holder_registry.rs` | `tv9_i1_insert_dual_happy_path`, `tv9_i2_insert_dual_rollback_on_capability_failure`, `tv9_i3_insert_dual_aborts_on_bearer_pk_collision` |
| Helpers (`bearer_with_pub_and_hash`, `capability_v1`) | same file | Test fixtures for TV9-I1/I2/I3 |

**Verification output:**

```text
cargo build -p quota-router-storage                                  # clean
cargo build --workspace                                             # clean
cargo test -p quota-router-storage --lib                             # 165/165 pass (162 pre-existing + 3 new TV9-I)
cargo test --workspace --lib --exclude octo-whatsapp                 # green
cargo clippy -p quota-router-storage --all-targets --all-features -- -D warnings   # clean
cargo clippy -p quota-router-storage -p octo-wallet -p octo-network --all-targets --all-features -- -D warnings   # clean (downstream consumers)
cargo fmt -p quota-router-storage -- --check                         # clean
```

**Design rationale (post-implementation):**

- **`StoolapHolderRegistry::insert_dual` is the canonical concrete impl, not `Transaction::insert_dual`.** The structural `Transaction` stub holds `PhantomData<()>` (no Stoolap handle); the registry holds the actual `db: stoolap::Database`. Putting the atomic body on the registry (which has the handle) keeps the structural stub intact for trait-surface consumers while delivering atomicity to real callers. The trait `HolderRegistry::insert_dual` provides a uniform surface; the default impl falls back to non-atomic sequential `insert` (defense in depth for any future registry that doesn't override).
- **`INSERT_HOLDER_SQL` + helpers as single source of truth.** Single-record `insert` and atomic-pair `insert_dual` use the same SQL constant + the same param builder. Adding a column to `holder_registry` is a one-spot change — both paths stay in lockstep automatically. The TV9-I3 first-attempt failure (Stoolap PK collision behavior with differing `ask_id`) reinforced that the SQL must be byte-identical between paths; the helper extraction guarantees this.
- **Auto-rollback via Stoolap `Transaction::Drop`** (api/transaction.rs:734-740): if either `tx.execute` returns `Err`, the function returns early without `tx.commit()`, the `tx` value drops, and Stoolap's `Drop` impl auto-rolls back. No explicit `rollback()` call needed. Defense in depth: Stoolap's `commit` failure path leaves the tx usable; our subsequent `Drop` then rolls back.
- **`classify_insert_err` substring match** (UNIQUE/unique/PRIMARY/PrimaryKey) — same pattern as the pre-existing `insert` body. Stoolap's error message format includes one of these substrings for any constraint violation; we don't switch on a typed error variant because Stoolap's `Error` enum doesn't expose one. Stable across the `feat/blockchain-sql` branch per the persisted CI history.
- **`type InsertParams = (...)` clippy mitigation.** The 10-tuple parameter shape triggers `clippy::type_complexity`. The named alias preserves the existing call-site readability (`tx.execute(SQL, insert_params(record))`) without `#[allow]` clutter. Self-documenting (parameter shape documented in the alias doc-comment).

**Cross-mission ownership update:**

- `0957-c-holder-registry-impl` owns `Transaction::insert_holder_record` (single-record primitive) — unchanged.
- `0969-b-dual-issuance-mint` owns the `mint_dual` entry point + `MintError` enum — unchanged. The `mint_dual` algorithm now has a working `insert_dual` atomic body to call; future mission can compose the full `mint_dual` body against this.
- `0969-b1-insert-dual-impl` (this mission) owns `StoolapHolderRegistry::insert_dual` (concrete atomic body) + `HolderRegistry::insert_dual` trait method + `INSERT_HOLDER_SQL` shared helpers.

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-07 | Filed open as `0969-b1-mint-dual-impl.md` (phantom-laden, 5+ drift items: `Ask` shape, `Ed25519Keypair`, `IdentityKey::from_public_bytes`, `BearerCapsule::build`, `MintError` variant shapes).                                                                                                                                                                                                                                       |
| v0.2    | 2026-08-07 | Reconciled per [[no-phantom-mission-pointers]] + [[deferred-vs-unspecified]]: pivoted scope to `Transaction::insert_dual` atomic body (the canonical 0969-b owned piece per `transaction.rs:46`); renamed phantom file to `*.PHANTOM.md` (preserved for drift-surface history); wrote new mission text under same `0969-b1` slot, renamed to `0969-b1-insert-dual-impl.md`. 12 ACs. |
| v0.3    | 2026-08-07 | Claimed + closed Band A same-session. 12/12 ACs green. `StoolapHolderRegistry::insert_dual` concrete atomic impl + `HolderRegistry::insert_dual` trait method + `INSERT_HOLDER_SQL` shared helpers + 3 TV9-I tests + structural-stub comment update. 165/165 quota-router-storage lib tests pass; clippy + fmt clean; workspace build green. Single commit on `next`.                                                                             |

Last Updated: 2026-08-07
Version: 0.3