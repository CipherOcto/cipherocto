---
name: 0014-settlement-verify-chain-tests
description: 12 canonical verify_receipt_chain test vectors per RFC-0014 §Test Vectors + domain separator byte-pin
metadata:
  node_type: substrate-tests
  type: chain-property-tests
  originSessionId: RFC-0014 author session
  created: 2026-09-10
  v: "1.0"
  depends_on:
    - RFC-0014
    - mission 0014-settlement-substrate-extraction
    - mission 0014-settlement-sm-engine-migration
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0014-settlement-verify-chain-tests — 12 canonical verify_receipt_chain test vectors

**Status:** Claimed — substrate-level property tests for verify_receipt_chain
**Substrate:** RFC-0014 §Test Vectors (12 canonical vectors)
**Parent:** RFC-0014

## Scope

Per RFC-0014 §Test Vectors, 12 substrate-level property tests for the chain-integrity helper `verify_receipt_chain` + `receipt_id_for` + domain separator byte-pin + state machine transitions.

### Deliverables

1. **`crates/octo-settlement-core/tests/chain_verify.rs` (NEW)** — 12 canonical chain-integrity test vectors per RFC-0014 §Test Vectors
2. **`crates/octo-settlement-core/tests/domain_separator.rs` (NEW)** — domain separator byte-pin test
3. **`crates/octo-settlement-core/tests/ask_state_machine.rs` (NEW)** — `AskState` transitions + `ReservationState` transitions (8 variants)
4. **`crates/octo-settlement-core/tests/sql_strings.rs` (NEW)** — `AskState::as_sql` / `from_sql` byte-stability test (frozen SQL strings per RFC-0959 §State Machine)

### Acceptance criteria

- [ ] AC-1: `cargo test -p octo-settlement-core --test chain_verify` passes all 12 canonical vectors
- [ ] AC-2: `cargo test -p octo-settlement-core --test domain_separator` passes (`cipherocto/reservation/v1/` byte-pin)
- [ ] AC-3: `cargo test -p octo-settlement-core --test ask_state_machine` passes (Mint → Settled → Consumed transitions; invalid transitions return `SettlementError::InvalidTransition`)
- [ ] AC-4: `cargo test -p octo-settlement-core --test sql_strings` passes (SQL string byte-stability)
- [ ] AC-5: `verify_receipt_chain` rejects any sequence gap, hash mismatch, or timestamp regression
- [ ] AC-6: `receipt_id_for` is deterministic across calls (same input → same output)
- [ ] AC-7: Workspace `cargo test --workspace` green
- [ ] AC-8: RFC-0014 VH row appended documenting verify_chain tests

### Dependencies

- `RFC-0014` — canonical substrate spec
- `mission 0014-settlement-substrate-extraction` — substrate crates must exist
- `mission 0014-settlement-sm-engine-migration` — sm-engine consumer migrated

### Risk

- **MEDIUM** — Domain separator drift. Mitigation: AC-2 + byte-pinned test (matches existing TV-0862-19 byte-pin precedent).
- **LOW** — `AskState::as_sql` string drift. Renaming SQL strings would require SQL migration. Mitigation: AC-4 explicit byte-stability test.
- **LOW** — State machine transition drift. Mitigation: AC-3 explicit invalid-transition test + `SettlementError::InvalidTransition` error variant.

### Cross-RFC invariants preserved

- Domain separator `cipherocto/reservation/v1/` preserved verbatim
- `AskState::as_sql` strings: `'Minted'`, `'Settled'`, `'Consumed'` (RFC-0959 §State Machine frozen)
- `ReservationState` 8-variant state machine (RFC-0960 §2.3 frozen)
- BLAKE3-256 chain integrity (RFC-0014 §Chain Helpers)

### Test vectors (12 canonical, RFC-0014 §Test Vectors)

| ID | Scenario | Expected |
|----|----------|----------|
| `chain-empty` | Empty receipt sequence | `verify_receipt_chain(&[]) == Ok(())` |
| `chain-single` | Single receipt with `prev_settlement_hash = [0;32]` | `verify_receipt_chain` accepts |
| `chain-monotonic` | 10 receipts with strict `timestamp_unix` monotonicity + correct `prev_settlement_hash` chaining | `verify_receipt_chain` accepts |
| `chain-gap` | 10 receipts with sequence gap (skip 5 → 7) | `verify_receipt_chain` returns `SettlementError::ChainIntegrity("sequence gap")` |
| `chain-hash-mismatch` | Receipt with `settlement_hash` field flipped by 1 byte | `verify_receipt_chain` returns `SettlementError::ChainIntegrity("hash mismatch")` |
| `chain-timestamp-regression` | Two receipts with `timestamp_unix` decreasing | `verify_receipt_chain` returns `SettlementError::ChainIntegrity("timestamp regression")` |
| `append-only-success` | `StoolapAppendOnlyReceiptSink::append` with valid receipt | `Ok(())`; `settlement_hash` persisted atomically |
| `append-only-idempotent` | Same receipt appended twice | First `Ok(())`; second returns `Err(AlreadyConsumed)` |
| `ask-state-mint-to-settle` | `AskState::Minted` → `Settled` transition | succeeds |
| `ask-state-settle-to-consume` | `AskState::Settled` → `Consumed` transition | succeeds |
| `ask-state-invalid-mint-to-consume` | `AskState::Minted` → `Consumed` (skip Settled) | returns `SettlementError::InvalidTransition` |
| `reservation-state-pending-to-active` | `ReservationState::Pending` → `Active` transition | succeeds |
| `reservation-state-active-to-redeemed` | `ReservationState::Active` → `Redeemed` transition | succeeds |
| `reservation-state-active-to-expired` | `ReservationState::Active` → `Expired` transition | succeeds |
| `reservation-state-invalid-pending-to-redeemed` | `ReservationState::Pending` → `Redeemed` (skip Active) | returns `SettlementError::InvalidTransition` |
| `domain-separator-byte-pin` | Domain separator string | `b"cipherocto/reservation/v1/"` byte-identical to canonical |
