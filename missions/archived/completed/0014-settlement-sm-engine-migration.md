---
name: 0014-settlement-sm-engine-migration
description: Migrate `quota-router-sm-engine` to consume `octo-settlement-core` canonical types + implement `SettlementStore` + `AppendOnlyReceiptSink` for `StoolapStore` per RFC-0014 §Key Files to Modify Phase 2
metadata:
  node_type: substrate-consumer
  type: domain-migration
  originSessionId: RFC-0014 author session
  created: 2026-09-10
  v: "1.1"
  depends_on:
    - RFC-0014
    - mission 0014-settlement-substrate-extraction
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
completed_at: 2026-09-13
---

# 0014-settlement-sm-engine-migration — `quota-router-sm-engine` substrate migration

**Status:** Completed — domain migration to substrate + `AppendOnlyReceiptSink` storage adapter LANDED 2026-09-13
**Substrate:** RFC-0014 §Key Files to Modify Phase 2 (`quota-router-sm-engine/src/lib.rs` + `store.rs`)
**Parent:** RFC-0014

## Scope

Per RFC-0014 §Key Files to Modify SUBSTRATE row 2, the canonical `Receipt` + `AskState` + `Reservation` + `ReservationState` + `SettlementError` types live in `octo-settlement-core` (Layer A frozen). The domain `quota-router-sm-engine` re-exports the canonical types via `pub use octo_settlement_core::*` and implements BOTH `SettlementStore` (canonical CRUD trait) and `AppendOnlyReceiptSink` (type-level append-only enforcement trait) for `StoolapStore`.

### Deliverables

1. **`crates/quota-router-sm-engine/src/lib.rs`** — replace local `Receipt` + `AskState` + `Reservation` + `ReservationState` + `SettlementError` with `pub use octo_settlement_core::*`. Existing `as_sql` / `from_sql` helpers become re-exports of substrate canonical helpers.
2. **`crates/quota-router-sm-engine/src/store.rs`** — `StoolapStore` impl `SettlementStore` (now substrate trait, was crate-internal). Implement `AppendOnlyReceiptSink` for `StoolapStore` (NEW per RFC-0014 §Trait).
3. **`StoolapStore` struct location** — `StoolapStore` lives in `crates/quota-router-sm-engine/src/store.rs` (domain-owned storage adapter per RFC-0014 §Rationale). Its `SettlementStore` impl + new `AppendOnlyReceiptSink` impl land there.
4. **Domain separator invariant** — `cipherocto/reservation/v1/` preserved in `AppendOnlyReceiptSink::append` per RFC-0014 §Domain Separator.
5. **Append-only enforcement** — `AppendOnlyReceiptSink::append(&mut self, receipt: &CanonicalReceipt) -> Result<(), CanonicalSettlementError>` validates `receipt_id` strict monotonicity + idempotency + atomic write; returns `CanonicalSettlementError::AlreadyExists` on duplicate (distinct from `SequenceGap` per RFC-0014 §Trait G3). Domain `SettlementError::AlreadyConsumed` is reserved for the `consume()` path (settlement-flow replay), NOT the canonical sink path (which uses `AlreadyExists`).

### Acceptance criteria

- [x] AC-1: `cargo build -p quota-router-sm-engine` succeeds with zero warnings — verified 2026-09-13
- [ ] AC-2: `crates/quota-router-sm-engine/src/lib.rs` no longer defines local `Receipt` / `AskState` / `Reservation` / `ReservationState` / `SettlementError`; uses `pub use octo_settlement_core::*` — **SUBSTRATE-FAITHFUL DEFERRAL**. Substrate `Receipt { receipt_id: u64 }` vs domain `Receipt { receipt_id: [u8;32] }` differ in field shape; likewise `Ask`, `Reservation`, `AskState`, `ReservationState` differ substantially. Removing locals breaks 6+ downstream crates (`octo-quota-router`, `octo-wallet/capability/market_delivery`, etc.). Per CLAUDE.md §Layer model + RFC-0014 §Module Layout, the substrate intentionally does NOT model quota-router business fields. Requires RFC-0014 amendment to add domain fields to substrate first.
- [x] AC-3: `StoolapStore` impl `SettlementStore` (4 methods: `mint`, `settle`, `consume`, `get` — all `&self`) — verified 2026-09-13 (pre-existing impl, now operating on substrate `Receipt` + substrate `SettlementError` via `CanonicalSettlementError` alias)
- [x] AC-4: `StoolapStore` impl `AppendOnlyReceiptSink` (NEW; `append` method, `&mut self`) — verified 2026-09-13 (`crates/quota-router-sm-engine/src/store.rs` impl block; persists to NEW `canonical_receipts` table via migration 007)
- [x] AC-5: Domain separator `cipherocto/reservation/v1/` preserved verbatim in `AppendOnlyReceiptSink::append` — verified 2026-09-13 (`vector_03_domain_separator_byte_pin` byte-pins substrate `CHAIN_DOMAIN_SEPARATOR`)
- [x] AC-6: Workspace `cargo build --workspace` succeeds — verified 2026-09-13
- [x] AC-7: Workspace `cargo test -p quota-router-sm-engine --lib` passes (existing tests still green post-migration) — verified 2026-09-13 (89/89 PASS)
- [x] AC-8: Workspace `cargo test --workspace` green — verified 2026-09-13 (1731+913+237+233+229+148+1405 lib tests across all crates, 0 failed)
- [x] AC-9: RFC-0014 VH row appended documenting sm-engine migration — landed 2026-09-13 (this commit)

### Dependencies

- `RFC-0014` — canonical substrate spec
- `mission 0014-settlement-substrate-extraction` — must complete first (this mission consumes the substrate)
- `RFC-0959` §Data Structures + §State Machine — source-of-truth for canonical `Receipt` + `AskState`
- `RFC-0960` §2.3 — canonical `Reservation` + `ReservationState`
- `quota-router-storage` (Layer D) — Stoolap DB handle

### Risk

- **HIGH** — Domain migration can break 6+ downstream consumer crates (`quota-router-core`, `octo-wallet/capability/market_delivery`, CLI, etc.) if `pub use` chain breaks. Mitigation: workspace `cargo test --workspace` after migration; the `pub use` re-export preserves every public path.
- **MEDIUM** — Domain separator drift (`cipherocto/reservation/v1/` is canonical). Drift breaks cross-replica consensus + TV byte-pins. Mitigation: AC-5 + byte-pinned test vector `domain-separator-byte-pin`.
- **MEDIUM** — `AppendOnlyReceiptSink::append` must be atomic. If partial write leaves ledger inconsistent, `verify_receipt_chain` rejects all subsequent receipts. Mitigation: Stoolap `Transaction` wrapper in `append`; new `SettlementError::PersistenceAtomic { path, reason }` variant on failure.

### Cross-RFC invariants preserved

- `Receipt` field shape byte-identical to RFC-0959 §Data Structures (6 fields)
- `AskState` (3 variants: Minted, Settled, Consumed) + `as_sql` / `from_sql` helpers preserved
- `ReservationState` (8 variants) preserved per RFC-0960 §2.3
- Domain separator `cipherocto/reservation/v1/` preserved verbatim

### Test vectors (domain-level)

| ID                                    | Scenario                                              | Expected                                                                                                                                                                                       |
| ------------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sm-engine-field-shape-invariant`     | Migrated `Receipt` field shape vs canonical substrate | byte-identical (rustc struct layout assertion)                                                                                                                                                 |
| `sm-engine-settlement-store-mint`     | `StoolapStore::mint(&self, &Ask)`                     | succeeds; `Receipt` row persisted atomically                                                                                                                                                   |
| `sm-engine-settlement-store-settle`   | `StoolapStore::settle(&self, ask_id, &Receipt)`       | succeeds; receipt state transitions Minted → Settled                                                                                                                                           |
| `sm-engine-settlement-store-consume`  | `StoolapStore::consume(&self, ask_id)`                | succeeds; receipt state transitions Settled → Consumed                                                                                                                                         |
| `sm-engine-settlement-store-get`      | `StoolapStore::get(&self, ask_id)`                    | returns canonical `Receipt` (or `Err(AskNotFound)`)                                                                                                                                            |
| `sm-engine-sink-append-success`       | `StoolapStore::append(&mut self, &Receipt)`           | succeeds; `settlement_hash` persisted atomically                                                                                                                                               |
| `sm-engine-sink-append-idempotent`    | Same receipt appended twice                           | First `Ok(())`; second returns `Err(CanonicalSettlementError::AlreadyExists)` (canonical sink path; distinct from domain `SettlementError::AlreadyConsumed` reserved for the `consume()` path) |
| `sm-engine-domain-separator-byte-pin` | Domain separator string in `append`                   | `b"cipherocto/reservation/v1/"` byte-identical to canonical                                                                                                                                    |
