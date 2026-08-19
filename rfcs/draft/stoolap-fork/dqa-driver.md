# RFC (Stoolap Fork): Dqa Driver Surface

## Status

**Version:** 1.0 (2026-08-19)
**Status:** Draft

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: Stoolap steward team (per RFC-0205)
- Co-maintainer: octo-determin owner

## Summary

Formalizes the Stoolap fork `Dqa` driver surface — `impl FromValue
for octo_determin::Dqa` + `impl ToParam for octo_determin::Dqa` —
that exposes `r.get::<Dqa>(idx)` and `tx.set::<Dqa>(idx, value)` on
the cipherocto fork `feat/blockchain-sql`. Wire form matches the
canonical 16-byte BE `DqaEncoding` defined in `octo_determin::Dqa::
to_bytes()` (value(i64) | scale(u8) | reserved[7]). Closes
CipherOcto mission 0900-d2 AC-1, AC-2, AC-3, AC-6.

## Dependencies

**Requires:**

- RFC-0105 (Numeric): Deterministic Quant Arithmetic — Dqa
  substrate (octo_determin::Dqa)
- RFC-0205 (Storage): Stoolap fork stability certification —
  defines the fork pin policy

**Optional:**

- RFC-0900 (Economics): Chain-aware slash ledger — depends on
  fork Dqa driver for native DQA column reads (deferred to
  follow-on cipherocto AC-5 migration)

> **Dependency Validation Rules:** All upstream RFCs Accepted
> (RFC-0105, RFC-0205 Draft). This RFC documents an existing fork
> surface; no new substrate is introduced.

## Design Goals

| Goal | Target                | Metric                                            |
| ---- | --------------------- | ------------------------------------------------- |
| G1   | Byte-exact wire form  | `Dqa::to_bytes()` round-trip via column           |
| G2   | Mirror existing codec | `r.get::<Dqa>` shape matches `r.get::<i64>`       |
| G3   | Orphan-rule compliant | Impl in fork (fork owns `FromValue`/`ToParam`)    |
| G4   | No new storage types  | Reuses existing `Value::Extension(Quant)` payload |

## Motivation

The Stoolap fork on `feat/blockchain-sql` exposes native value
types via `r.get::<T>(idx)` for `i64`, `String`, `Vec<u8>`, `bool`,
etc. (see `src/api/database.rs:1104+`). The fork had no codec
surface for `octo_determin::Dqa`, forcing cipherocto consumers to
cross the i64 bridge at scale=0 with `dqa_to_i64`/`i64_to_dqa`
helpers (LANDED in 0900-d at commit `58c4c2ce`).

Three problems with the bridge:

1. **Substrate invariant leakage** — every consumer that writes
   amount-bearing columns must remember to apply the bridge.
2. **Wire form stranding** — canonical `DqaEncoding` 16-byte BE
   wire form is defined in `octo_determin::Dqa::to_bytes()`. The
   bridge serializes to i64 text, which is a different encoding.
3. **Composability cost** — any future column at non-zero scale
   (e.g., `DQA(12)` for fractional fees) must re-invent the bridge.

**Solution:** Adopt the fork `Dqa` driver surface. Two small trait
impls delegate to the existing `Value::quant()` / `Value::as_dqa()`
codec path (`src/core/value.rs:212` + `:504`), which already serializes
the 16-byte BE `DqaEncoding` under the `DataType::Quant` extension
tag (9). Wire form matches `Dqa::to_bytes()` byte-for-byte.

## Roles and Authorities

1. **Stoolap steward team** — owns the fork; merges driver changes
   back to `feat/blockchain-sql`.
2. **octo-determin owner** — gates `Dqa` API surface changes that
   could break the fork driver.
3. **RFC reviewer** — signs off on wire-form changes.

| Role                | Identifier                          | Authority Scope                          | Lifecycle                 |
| ------------------- | ----------------------------------- | ---------------------------------------- | ------------------------- |
| Stoolap steward     | GitHub team `@stoolap-stewards`     | Fork maintenance, codec surface approval | Active until role revoked |
| octo-determin owner | GitHub team `@octo-determin-owners` | Dqa API stability                        | Active until role revoked |
| RFC reviewer        | RFC process role                    | Wire-form change approval                | Per-RFC                   |

## Specification

### Driver Surface

**`impl FromValue for octo_determin::Dqa`** in `src/api/database.rs`:

```rust
impl FromValue for Dqa {
    fn from_value(value: &Value) -> Result<Self> {
        value.as_dqa().ok_or_else(|| Error::TypeConversion {
            from: format!("{:?}", value),
            to: "Dqa".to_string(),
        })
    }
}
```

**`impl ToParam for octo_determin::Dqa`** in `src/api/params.rs`:

```rust
impl ToParam for octo_determin::Dqa {
    fn to_param(&self) -> Value {
        Value::quant(*self)
    }
}
```

### Wire Form

Both impls delegate to the existing `Value` codec surface:

- **Encoding path** (`ToParam`): `Value::quant(*self)` constructs
  `Value::Extension(CompactArc<[u8]>)` with leading tag byte
  `DataType::Quant as u8` (= 9), followed by 16 bytes
  `value::i64 || scale::u8 || reserved::[u8; 7]`. Matches
  `Dqa::to_bytes()` byte-for-byte (the `to_bytes()` impl produces
  exactly the same 16-byte payload).

- **Decoding path** (`FromValue`): `Value::as_dqa()` reads the
  extension tag, copies `data[1..9]` to `i64::from_be_bytes`,
  copies `data[9]` to `scale`, and constructs `Dqa::new(value,
scale)`. Returns `None` if the payload is not a `Quant` extension
  OR if the payload is too short (< 10 bytes total).

### Orphan-Rule Compliance

The fork owns the `FromValue` and `ToParam` traits (defined in
`src/api/database.rs:1097` and `src/api/params.rs` respectively).
cipherocto owns the `octo_determin::Dqa` type. The orphan rule permits
`impl FromValue for Dqa` **in the fork crate** because the fork is
the local crate of the trait. cipherocto _cannot_ directly impl
`stoolap::FromValue for octo_determin::Dqa` — the impl must live in
the fork.

### Error Handling

| Error                         | Detection                                        | Recovery                                  |
| ----------------------------- | ------------------------------------------------ | ----------------------------------------- |
| `Value::from_value` non-Quant | `FromValue::from_value` returns `TypeConversion` | Caller MUST pre-validate column type      |
| Payloads < 10 bytes total     | `Value::as_dqa` returns `None`                   | Schema migration OR column type assertion |

## Performance Targets

| Metric                       | Target  | Notes                                           |
| ---------------------------- | ------- | ----------------------------------------------- |
| Encode/`to_param` path       | < 50 ns | Single `Value::quant` call + 17-byte allocation |
| Decode/`from_value` path     | < 50 ns | Tag check + 16-byte read + `Dqa::new`           |
| Round-trip (insert + select) | < 5 µs  | End-to-end via integration test                 |

## Implicit Assumptions Audit

| Assumption                                        | Where Relied Upon | Blast Radius if False                    | Mitigation                            |
| ------------------------------------------------- | ----------------- | ---------------------------------------- | ------------------------------------- |
| `Dqa::to_bytes()` is canonical 16-byte BE         | §Wire Form        | Round-trip byte mismatch with serde path | AC-1 mandates wire-form match         |
| `Value::Extension(Quant)` is stable               | §Wire Form        | Fork upgrade breaks round-trip           | RFC-0205 pin policy + monthly re-cert |
| Fork always re-exports `octo_determin`            | §Driver Surface   | Orphan-rule bypass fails                 | Fork Cargo.toml:55 git dep (frozen)   |
| `Dqa::new(i64, u8) -> Result` accepts scale=0..18 | §Wire Form        | Scale boundary violation silently fails  | Validated at fork `Value::as_dqa`     |

### Categories to Audit

- **Operator trust** — Stoolap steward team trusted to maintain
  codec surface; compromise → consume-side crashes. Mitigation:
  RFC reviewer co-sign on changes.
- **Platform trust** — fork availability per RFC-0205.
- **Time source** — none; codec is pure.
- **Network partition** — none; codec is local.
- **Upgrade safety** — codec surface is additive; removing it
  requires fork RFC major.
- **Configuration** — none; default fork pin.
- **Identity stability** — steward GitHub team stable.
- **Resource availability** — 17 bytes per Quant value.

## Security Considerations

- **Wire-form forgery** — attacker forges `Value::Extension(Quant)`
  tag with arbitrary `value`/`scale`. Mitigation: fork only
  constructs `Quant` via `Value::quant(Dqa)` which validates
  `Dqa::new` returns `Ok`.
- **Cross-version regression** — fork codec changes
  simultaneously. Mitigation: monthly re-cert per RFC-0205.

## Adversary Analysis

| Decision                 | Q1 Beneficiary           | Q2 Cost to Attacker        | Q3 Gain if Successful             | Q4 Defense                           | Q5 Residual Risk            |
| ------------------------ | ------------------------ | -------------------------- | --------------------------------- | ------------------------------------ | --------------------------- |
| Two-impl driver surface  | Compromised fork release | Steward account compromise | Inject malformed codec            | AC-6 round-trip TV + reviewer audit  | LOW — TV catches byte-drift |
| Leak `Dqa` wire form     | None directly            | N/A                        | N/A                               | N/A                                  | NONE — wire form is public  |
| Reuse `Value::Extension` | Compromised codec        | Low                        | Coerce decode-via-other-extension | Tag check + length check in `as_dqa` | LOW — defensive codec       |

### Severity Classification

| Severity     | Definition                                | Action                                  |
| ------------ | ----------------------------------------- | --------------------------------------- |
| **CRITICAL** | Wire form diverges from `Dqa::to_bytes()` | MUST mitigate before Accept (TV-6)      |
| **HIGH**     | Driver silently fails on non-Quant        | SHOULD mitigate (caller pre-validation) |
| **MEDIUM**   | Fork pin drifts                           | SHOULD mitigate (RFC-0205 monthly)      |
| **LOW**      | Tag-format ambiguity                      | MAY accept (documented; tested)         |

## Economic Analysis

No new tokens or stake. Cost: ~0.1 FTE for fork steward (already
covered by RFC-0205 monthly re-cert).

## Compatibility

- **Backward:** existing cipherocto code using `dqa_to_i64`/`i64_to_dqa`
  bridge continues to work; fork does not remove `i64` codec.
- **Forward:** codec surface is additive; upstream merge will
  preserve all `FromValue`/`ToParam` impls.

## Test Vectors

Byte-exact round-trip TV in fork `tests/dqa_driver_test.rs`:

1. **TV-DQA-DRV-01:** `Dqa::new(900_000, 0)` → column → `Dqa` byte-exact
2. **TV-DQA-DRV-02:** `Dqa::new(900, 5)` → column → `Dqa` scale=5 preserved
3. **TV-DQA-DRV-03:** `Dqa::new(0, 0)` → column → `Dqa` zero edge
4. **TV-DQA-DRV-04:** `Dqa::new(-1, 0)` → column → `Dqa` negative edge
5. **TV-DQA-DRV-05:** `Dqa::new(i64::MAX, 0)` → column → `Dqa` max value edge

All 5 green at fork commit `dfc5b71` (LANDED 2026-08-19).

## Alternatives Considered

| Approach                                                          | Pros                                                               | Cons                                                                |
| ----------------------------------------------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------- |
| Option A: cipherocto-owned codec (fork newtype)                   | Fork stays minimal                                                 | Violates orphan rule; fork can't impl fork trait on cipherocto type |
| Option B: BIGINT-column view                                      | No migration needs                                                 | Loses scale semantics; non-zero-scale future columns need re-do     |
| **Option C: Fork-side `FromValue`/`ToParam` for `Dqa` (adopted)** | Reuses existing extension codec; byte-exact; orphan-rule compliant | Requires fork commit                                                |

## Implementation Phases

### Phase 1: Fork Surface (LANDED 2026-08-19)

- [x] Task 1: Add `impl FromValue for octo_determin::Dqa` in fork
- [x] Task 2: Add `impl ToParam for octo_determin::Dqa` in fork
- [x] Task 3: Add `tests/dqa_driver_test.rs` with 5 AC-6 round-trip cases
- [x] Task 4: 5/5 tests green; fmt clean; pre-existing-clippy-zero-hits-in-new-files

### Phase 2: CipherOcto Migration (DEFERRED — multi-session)

- [ ] Task 5: `v017__dqa_columns.sql` migration (additive `DQA(0)` columns side-by-side)
- [ ] Task 6: Drop `dqa_to_i64`/`i64_to_dqa` bridge helpers in `slash_store.rs` + `stoolap_spend_ledger.rs`
- [ ] Task 7: Switch `r.get::<i64>` to `r.get::<Dqa>` for DQA columns
- [ ] Task 8: Register v017 in `migrations.rs`
- [ ] Task 9: `cargo test -p quota-router-storage` + `cargo test -p octo-vault` AC-7 gates

## Key Files Modified

| File                              | Change                                  |
| --------------------------------- | --------------------------------------- |
| `src/api/database.rs` (fork)      | `impl FromValue for octo_determin::Dqa` |
| `src/api/params.rs` (fork)        | `impl ToParam for octo_determin::Dqa`   |
| `tests/dqa_driver_test.rs` (fork) | NEW — 5 AC-6 round-trip cases           |

## Future Work

- Cipherocto `v017__dqa_columns.sql` migration (Phase 2 Task 5)
- Cipherocto bridge helper removal (Phase 2 Tasks 6-7)
- Merge-to-upstream decision (deferred per RFC-0205 §Future Work)

## Version History

| Version | Date       | Author     | Changes                                                                                            |
| ------- | ---------- | ---------- | -------------------------------------------------------------------------------------------------- |
| 1.0     | 2026-08-19 | @mmacedoeu | Initial draft. Documents fork `dfc5b71` driver surface; 5 AC-6 TV; Phase 2 deferred to cipherocto. |
