---
name: 0014-settlement-substrate-extraction
description: Extract canonical `Receipt` + `AskState` + `ReservationState` + `SettlementStore` + `AppendOnlyReceiptSink` to `octo-settlement-core` Layer A frozen crate + create `octo-settlement` Layer B façade per RFC-0014
metadata:
  node_type: substrate-core
  type: layer-a-frozen-extraction
  originSessionId: RFC-0014 author session
  created: 2026-09-10
  v: "1.0"
  completed: 2026-09-10
  commit: b8e0e454
  depends_on:
    - RFC-0014
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0014-settlement-substrate-extraction — `octo-settlement-core` + `octo-settlement` per RFC-0014

**Status:** Completed — substrate extraction landed at commit `b8e0e454` on `next`. Two new crates (`octo-settlement-core` Layer A frozen + `octo-settlement` Layer B façade) + 6 lib unit tests pass + clippy clean. Domain separator `cipherocto/reservation/v1/` byte-pinned via `domain_separator_pinned` unit test. **Inline bug fix:** `receipt_id_for` initially included `settlement_hash` in the BLAKE3 input (which IS the output) — made hash depend on itself. Removed field from input; comment warns future contributors. `repr(u8)` discriminants byte-identical to RFC-0959 §State Machine (AskState) + RFC-0960 §2.3 (ReservationState). DRY review R1=2 LOW (other crates) → R2 fixes → R3=0 → DRY CLOSED per `docs/audits/2026-09-10-0012-0013-0014-substrate-extraction-r3-dry-closure.md`. Push + PR + RFC VH row append user-owned per [[feedback_initiation_user_only]] + [[git-workflow]].
**Substrate:** RFC-0014 §Specification (`octo-settlement-core` + `octo-settlement` modules)
**Parent:** RFC-0014

## Scope

Per RFC-0014 §Key Files to Modify Phase 1 — substrate extraction. The substrate owns canonical `Receipt` + `AskState` + `Reservation` + `ReservationState` + `SettlementError` + `SettlementStore` trait + `AppendOnlyReceiptSink` trait + chain-integrity helpers (`receipt_id_for`, `verify_receipt_chain`). Domain crate `quota-router-sm-engine` implements both traits via `StoolapStore`.

### Deliverables

1. **`crates/octo-settlement-core/` (NEW)** — Layer A frozen core.
   - `src/lib.rs` — module re-exports + crate docs (RFC-0014 §Module Layout)
   - `src/receipt.rs` — `Receipt` struct (RFC-0959 §Data Structures canonical; 6 fields: `receipt_id`, `ask_id`, `settlement_hash`, `router_id`, `router_sig`, `timestamp_unix`)
   - `src/ask.rs` — `Ask` struct + `AskState` enum (3 variants: Minted, Settled, Consumed) + `as_sql` / `from_sql` helpers
   - `src/reservation.rs` — `Reservation` struct + `ReservationState` enum (8 variants per RFC-0960 §2.3)
   - `src/store.rs` — `SettlementStore` trait (`mint(&self, &Ask)`, `settle(&self, &[u8;32], &Receipt)`, `consume(&self, &[u8;32])`, `get(&self, &[u8;32])` — all `&self` per `Arc<Mutex<Database>>` interior mutability pattern)
   - `src/sink.rs` — `AppendOnlyReceiptSink` trait (`&mut self` + `append` method only; type-level append-only enforcement)
   - `src/chain.rs` — `verify_receipt_chain` + `receipt_id_for` + chain helpers
   - `src/error.rs` — `SettlementError` enum (AskNotFound, AlreadyConsumed, InvalidTransition, ChainIntegrity, etc.)
   - `Cargo.toml` — deps: `serde`, `thiserror`, `blake3` (NO IO deps; NO storage deps)
2. **`crates/octo-settlement/` (NEW)** — Layer B façade (~30 LoC).
   - `src/lib.rs` — `pub use octo_settlement_core::*` + re-export any extension types from `quota-router-sm-engine`
   - `Cargo.toml` — depends on `octo-settlement-core` only
3. **Workspace registration** — add both crates to root `Cargo.toml` `members` list
4. **Byte-identical extraction claim** — `crates/quota-router-sm-engine/src/lib.rs` canonical `Receipt` + `AskState` + `SettlementError` types preserved verbatim per RFC-0959 §Data Structures + §State Machine; `Reservation` + `ReservationState` preserved per RFC-0960 §2.3
5. **Domain separator** — `cipherocto/reservation/v1/` preserved verbatim in `AppendOnlyReceiptSink`

### Acceptance criteria

- [ ] AC-1: `cargo build -p octo-settlement-core` succeeds with zero warnings (clippy `--all-features -- -D warnings`)
- [ ] AC-2: `cargo build -p octo-settlement` succeeds with zero warnings
- [ ] AC-3: All 7 §Module Layout sections present in `octo-settlement-core/src/lib.rs`
- [ ] AC-4: `Receipt` + `AskState` + `ReservationState` all `#[non_exhaustive]` per CLAUDE.md §Extension over enumeration
- [ ] AC-5: `SettlementStore` trait methods take `&self` (interior mutability pattern; matches existing `quota-router-sm-engine::store::StoolapStore`)
- [ ] AC-6: `AppendOnlyReceiptSink::append` is `&mut self`; no `delete` / `update` / `clear` method exists
- [ ] AC-7: Domain separator `cipherocto/reservation/v1/` preserved verbatim in `AppendOnlyReceiptSink`
- [ ] AC-8: Workspace `cargo build --workspace` succeeds after registration
- [ ] AC-9: RFC-0014 VH row appended documenting substrate extraction + cite-hygiene PASS

### Out of scope (separate missions)

- Migration of `quota-router-sm-engine` to consume substrate (`SettlementStore` impl + `AppendOnlyReceiptSink` impl) → `missions/open/0014-settlement-sm-engine-migration.md`
- 12 verify_receipt_chain test vectors → `missions/open/0014-settlement-verify-chain-tests.md`

### Dependencies

- `RFC-0014` (accepted 2026-09-10) — canonical substrate spec
- `RFC-0959` §Data Structures + §State Machine — source-of-truth for canonical `Receipt` + `AskState` field shape
- `RFC-0960` §2.3 — canonical `Reservation` + `ReservationState` (8 variants)

### Risk

- **HIGH** — Layer A frozen core addition. Per CLAUDE.md §Architectural Principles + RFC-0014 §Security Considerations, the core MUST be RFC-frozen + semver-major only. Mitigation: explicit `Cargo.toml` comment pinning the frozen-core status + CLAUDE.md cross-link.
- **MEDIUM** — Domain separator drift. `cipherocto/reservation/v1/` is the canonical separator (per RFC-0959 + RFC-0014 §Test Vectors). Drift breaks cross-replica consensus. Mitigation: AC-7 + byte-pinned test vector (TV-`domain-separator-byte-pin` per RFC-0014 §Test Vectors).
- **MEDIUM** — `SettlementStore::mint`/`settle`/`consume` use `&self` not `&mut self` (interior mutability via `Arc<Mutex<Database>>`). Type-level append-only enforcement lives on the separate `AppendOnlyReceiptSink` trait (`&mut self` requirement). The two-trait split keeps the canonical CRUD interface ergonomic while preserving append-only guarantee on the raw ledger path. Documented in RFC-0014 §Rationale §Why `SettlementStore::mint`/`settle`/`consume` use `&self` (not `&mut self`).

### Cross-RFC invariants preserved

- `Receipt` field shape byte-identical to RFC-0959 §Data Structures
- `AskState` (3 variants: Minted, Settled, Consumed) + `as_sql` / `from_sql` helpers preserved
- `ReservationState` (8 variants) preserved per RFC-0960 §2.3
- Domain separator `cipherocto/reservation/v1/` preserved
- PQC migration blast radius confined to Layer A frozen core

### Test vectors (substrate-level, RFC-0014 §Test Vectors 12 vectors)

See `missions/open/0014-settlement-verify-chain-tests.md` for the canonical 12 test vectors.
