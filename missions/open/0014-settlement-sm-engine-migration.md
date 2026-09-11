---
name: 0014-settlement-sm-engine-migration
description: Migrate `quota-router-sm-engine` to consume `octo-settlement-core` canonical types + implement `SettlementStore` + `AppendOnlyReceiptSink` for `StoolapStore` per RFC-0014 §Key Files to Modify Phase 2
metadata:
  node_type: substrate-consumer
  type: domain-migration
  originSessionId: RFC-0014 author session
  created: 2026-09-10
  v: "1.0"
  depends_on:
    - RFC-0014
    - mission 0014-settlement-substrate-extraction
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0014-settlement-sm-engine-migration — `quota-router-sm-engine` substrate migration

**Status:** Claimed — domain migration to substrate (storage adapter implementation)
**Substrate:** RFC-0014 §Key Files to Modify Phase 2 (`quota-router-sm-engine/src/lib.rs` + `store.rs`)
**Parent:** RFC-0014

## Scope

Per RFC-0014 §Key Files to Modify SUBSTRATE row 2, the canonical `Receipt` + `AskState` + `Reservation` + `ReservationState` + `SettlementError` types live in `octo-settlement-core` (Layer A frozen). The domain `quota-router-sm-engine` re-exports the canonical types via `pub use octo_settlement_core::*` and implements BOTH `SettlementStore` (canonical CRUD trait) and `AppendOnlyReceiptSink` (type-level append-only enforcement trait) for `StoolapStore`.

### Deliverables

1. **`crates/quota-router-sm-engine/src/lib.rs`** — replace local `Receipt` + `AskState` + `Reservation` + `ReservationState` + `SettlementError` with `pub use octo_settlement_core::*`. Existing `as_sql` / `from_sql` helpers become re-exports of substrate canonical helpers.
2. **`crates/quota-router-sm-engine/src/store.rs`** — `StoolapStore` impl `SettlementStore` (now substrate trait, was crate-internal). Implement `AppendOnlyReceiptSink` for `StoolapStore` (NEW per RFC-0014 §Trait).
3. **`StoolapStore` struct location** — `StoolapStore` lives in `crates/quota-router-sm-engine/src/store.rs` (domain-owned storage adapter per RFC-0014 §Rationale). Its `SettlementStore` impl + new `AppendOnlyReceiptSink` impl land there.
4. **Domain separator invariant** — `cipherocto/reservation/v1/` preserved in `AppendOnlyReceiptSink::append` per RFC-0014 §Domain Separator.
5. **Append-only enforcement** — `AppendOnlyReceiptSink::append(&mut self, receipt: &Receipt) -> Result<(), SettlementError>` validates idempotency + atomic write; returns `SettlementError::AlreadyConsumed` on duplicate.

### Acceptance criteria

- [ ] AC-1: `cargo build -p quota-router-sm-engine` succeeds with zero warnings
- [ ] AC-2: `crates/quota-router-sm-engine/src/lib.rs` no longer defines local `Receipt` / `AskState` / `Reservation` / `ReservationState` / `SettlementError`; uses `pub use octo_settlement_core::*`
- [ ] AC-3: `StoolapStore` impl `SettlementStore` (4 methods: `mint`, `settle`, `consume`, `get` — all `&self`)
- [ ] AC-4: `StoolapStore` impl `AppendOnlyReceiptSink` (NEW; `append` method, `&mut self`)
- [ ] AC-5: Domain separator `cipherocto/reservation/v1/` preserved verbatim in `AppendOnlyReceiptSink::append`
- [ ] AC-6: Workspace `cargo build --workspace` succeeds
- [ ] AC-7: Workspace `cargo test -p quota-router-sm-engine --lib` passes (existing tests still green post-migration)
- [ ] AC-8: Workspace `cargo test --workspace` green
- [ ] AC-9: RFC-0014 VH row appended documenting sm-engine migration

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

| ID | Scenario | Expected |
|----|----------|----------|
| `sm-engine-field-shape-invariant` | Migrated `Receipt` field shape vs canonical substrate | byte-identical (rustc struct layout assertion) |
| `sm-engine-settlement-store-mint` | `StoolapStore::mint(&self, &Ask)` | succeeds; `Receipt` row persisted atomically |
| `sm-engine-settlement-store-settle` | `StoolapStore::settle(&self, ask_id, &Receipt)` | succeeds; receipt state transitions Minted → Settled |
| `sm-engine-settlement-store-consume` | `StoolapStore::consume(&self, ask_id)` | succeeds; receipt state transitions Settled → Consumed |
| `sm-engine-settlement-store-get` | `StoolapStore::get(&self, ask_id)` | returns canonical `Receipt` (or `Err(AskNotFound)`) |
| `sm-engine-sink-append-success` | `StoolapStore::append(&mut self, &Receipt)` | succeeds; `settlement_hash` persisted atomically |
| `sm-engine-sink-append-idempotent` | Same receipt appended twice | First `Ok(())`; second returns `Err(SettlementError::AlreadyConsumed)` |
| `sm-engine-domain-separator-byte-pin` | Domain separator string in `append` | `b"cipherocto/reservation/v1/"` byte-identical to canonical |
