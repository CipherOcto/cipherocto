---
name: 0014-settlement-verify-chain-tests
description: 21 substrate-level property tests for verify_receipt_chain + receipt_id_for + domain separator byte-pin + state machine transitions, split RFC canonical vs mission-defined supplementary per substrate-faithful test policy
metadata:
  node_type: substrate-tests
  type: chain-property-tests
  originSessionId: RFC-0014 author session
  created: 2026-09-10
  v: "1.1"
  depends_on:
    - RFC-0014
    - mission 0014-settlement-substrate-extraction
    - mission 0014-settlement-sm-engine-migration
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
completed_at: 2026-09-13
---

# 0014-settlement-verify-chain-tests — substrate-level property tests for `verify_receipt_chain`

**Status:** Completed — substrate-level property tests LANDED 2026-09-13
**Substrate:** RFC-0014 §Test Vectors (canonical IDs) + substrate-faithful supplementary surface
**Parent:** RFC-0014

## Scope

Per RFC-0014 §Test Vectors, this mission lands 21 substrate-level property tests for the chain-integrity helpers `verify_receipt_chain` + `receipt_id_for` + domain separator byte-pin + state machine type surface. Per the substrate-faithful test policy, each test file splits into **RFC canonical** (vectors with real canonical RFC-0014 §Test Vectors IDs) and **Mission-defined supplementary** (vectors exercising substrate code paths without a canonical RFC ID; clearly labeled with `mission-defined:` prefix).

### Deliverables

1. **`crates/octo-settlement-core/tests/chain_verify.rs`** — 11 vectors: 6 RFC canonical (chain-empty, chain-monotonic, chain-settlement-hash-mismatch × 2 position variants, receipt-compute-receipt-id-stable, receipt-canonical-bytes-stable) + 5 mission-defined supplementary (single-receipt boundary, 50-receipt monotonic stress, sequence gap, leading nonzero receipt_id accepted at verifier, duplicate id collapsing to SequenceGap)
2. **`crates/octo-settlement-core/tests/domain_separator.rs`** — 2 mission-defined supplementary vectors (CHAIN_DOMAIN_SEPARATOR byte-pin + receipt_id_for consumes separator proof via external blake3 recompute)
3. **`crates/octo-settlement-core/tests/ask_state_machine.rs`** — 5 mission-defined supplementary vectors (InvalidTransition variant constructible, 4 ReservationState transitions including invalid Pending→Redeemed reject via `can_transition_to` substrate helper). AskState discriminant byte-pin lives at `sql_strings::mission_01_ask_state_discriminant_byte_pinning` (single canonical substrate surface for SQL byte-stability).
4. **`crates/octo-settlement-core/tests/sql_strings.rs`** — 3 vectors: 2 RFC canonical (`ask-state-sql-roundtrip`, `ask-state-unknown-sql` — 5 SQL-value assertions inside vector_02) + 1 mission-defined (`ask_state_discriminant_byte_pinning` — single canonical substrate surface for discriminant byte-pin)

### Acceptance criteria

- [x] AC-1: `cargo test -p octo-settlement-core --test chain_verify` passes all 11 vectors — verified 2026-09-13 (11/11 PASS: 6 RFC canonical + 5 mission-defined supplementary)
- [x] AC-2: `cargo test -p octo-settlement-core --test domain_separator` passes — verified 2026-09-13 (2/2 PASS: byte-pin + receipt_id_for-uses-separator external recompute proof)
- [x] AC-3: `cargo test -p octo-settlement-core --test ask_state_machine` passes — verified 2026-09-13 (5/5 PASS: 1 InvalidTransition variant + 4 ReservationState transitions including invalid Pending→Redeemed reject via `can_transition_to` substrate helper per RFC-0960 §2.3)
- [x] AC-4: `cargo test -p octo-settlement-core --test sql_strings` passes — verified 2026-09-13 (3/3 PASS: 2 RFC canonical + 1 mission-defined supplementary)
- [x] AC-5: `verify_receipt_chain` rejects any `receipt_id` sequence gap (SequenceGap variant) and any `settlement_hash` mismatch (ChainIntegrity variant) — verified 2026-09-13 (chain_verify vectors exercise both rejection paths; substrate-faithful: `verify_receipt_chain` does NOT enforce timestamp monotonicity — RFC-0014 §chain-timestamp-regression is DEFERRED per RFC)
- [x] AC-6: `receipt_id_for` is deterministic across calls (same input → same output) — verified 2026-09-13 (`chain_verify::vector_05_receipt_compute_receipt_id_stable` + `chain_verify::vector_06_receipt_canonical_bytes_stable`)
- [x] AC-7: Workspace `cargo test --workspace` green — verified 2026-09-13 (no failures across all crates)
- [x] AC-8: RFC-0014 VH row appended documenting verify_chain tests — landed 2026-09-13

### Dependencies

- `RFC-0014` — canonical substrate spec
- `mission 0014-settlement-substrate-extraction` — substrate crates must exist
- `mission 0014-settlement-sm-engine-migration` — sm-engine consumer migrated
- `RFC-0959` §Data Structures + §State Machine — AskState source-of-truth
- `RFC-0960` §2.3 — ReservationState source-of-truth

### Risk

- **MEDIUM** — Domain separator drift. Mitigation: AC-2 + byte-pinned test (matches existing TV-0862-19 byte-pin precedent).
- **LOW** — `AskState::as_sql` string drift. Renaming SQL strings would require SQL migration. Mitigation: AC-4 explicit byte-stability test.
- **LOW** — State machine transition drift. Mitigation: AC-3 explicit invalid-transition test via substrate `can_transition_to` helper.

### Cross-RFC invariants preserved

- Domain separator `cipherocto/reservation/v1/` preserved verbatim (byte-pinned)
- `AskState` 3-variant discriminant (Minted=0, Settled=1, Consumed=2) preserved per RFC-0959 §State Machine
- `ReservationState` 8-variant state machine (RFC-0960 §2.3 frozen)
- BLAKE3-256 chain integrity (RFC-0014 §Chain Helpers)
- `verify_receipt_chain` substrate surface scoped to `receipt_id` monotonicity + `settlement_hash` chain only — timestamp monotonicity enforcement is DEFERRED per RFC-0014 §chain-timestamp-regression and lives at the domain sink layer, not the substrate

### Test vectors

#### RFC canonical (RFC-0014 §Test Vectors)

| ID                                    | Scenario                                                                                          | Expected                                                                |
| ------------------------------------- | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `chain-empty`                         | Empty receipt sequence                                                                            | `verify_receipt_chain(&[]) == Ok(())`                                   |
| `chain-monotonic`                     | 10-receipt sequence with strict `receipt_id` successor                                            | `verify_receipt_chain` accepts                                          |
| `chain-settlement-hash-mismatch` (×2) | `settlement_hash` flipped by 1 byte (interior + first-position variants)                          | `verify_receipt_chain` returns `SettlementError::ChainIntegrity { .. }` |
| `receipt-compute-receipt-id-stable`   | Same `Receipt` input yields identical `settlement_hash` across calls                              | hashes equal                                                            |
| `receipt-canonical-bytes-stable`      | Two structurally identical Receipts yield identical `settlement_hash` (cross-replica determinism) | hashes equal                                                            |
| `ask-state-sql-roundtrip`             | Every canonical AskState variant round-trips via `as_sql` + `from_sql`                            | recovers same variant                                                   |
| `ask-state-unknown-sql`               | SQL discriminant outside canonical set returns `None` (fail-closed)                               | `from_sql` returns `None`                                               |

#### Mission-defined supplementary (no canonical RFC ID; documented per substrate-faithful policy)

| ID                                                                            | Scenario                                                                           | Expected                                                             |
| ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `chain_verify::mission_01_single_receipt`                                     | 1-receipt boundary of `chain-monotonic`                                            | accepted (boundary)                                                  |
| `chain_verify::mission_02_chain_monotonic_50`                                 | 50-receipt stress variant of `chain-monotonic`                                     | accepted                                                             |
| `chain_verify::mission_03_sequence_gap`                                       | Verifier's own gap-detection path (substrate-faithful: monotonicity is sink layer) | `verify_receipt_chain` returns `SettlementError::SequenceGap { .. }` |
| `chain_verify::mission_04_leading_nonzero_id`                                 | Verifier boundary: `receipt_id != 0` accepted (canonical first-id rule is sink)    | accepted                                                             |
| `chain_verify::mission_05_duplicate_id`                                       | Duplicate `receipt_id` collapses to SequenceGap (substrate-faithful)               | `verify_receipt_chain` returns `SettlementError::SequenceGap { .. }` |
| `domain_separator::mission_01_domain_separator_byte_pin`                      | `CHAIN_DOMAIN_SEPARATOR == b"cipherocto/reservation/v1/"` (length 26)              | byte-equal                                                           |
| `domain_separator::mission_02_receipt_id_for_consumes_domain_separator`       | External blake3 recompute over `sep \|\| canonical_receipt_bytes` matches          | hashes equal                                                         |
| `ask_state_machine::mission_01_invalid_transition_variant_constructible`      | `SettlementError::InvalidTransition` variant constructible                         | constructible, `Display` mentions "invalid state transition"         |
| `ask_state_machine::mission_02_reservation_state_pending_to_active`           | `ReservationState::Pending → Active` (admin approves)                              | `can_transition_to` true                                             |
| `ask_state_machine::mission_03_reservation_state_active_to_redeemed`          | `ReservationState::Active → Redeemed` (ask settled)                                | `can_transition_to` true                                             |
| `ask_state_machine::mission_04_reservation_state_active_to_expired`           | `ReservationState::Active → Expired` (lock expires)                                | `can_transition_to` true                                             |
| `ask_state_machine::mission_05_reservation_state_invalid_pending_to_redeemed` | `ReservationState::Pending → Redeemed` (skips Active)                              | `can_transition_to` false                                            |
| `sql_strings::mission_01_ask_state_discriminant_byte_pinning`                 | `AskState::Minted as i64 == 0` etc.                                                | byte-equal                                                           |
