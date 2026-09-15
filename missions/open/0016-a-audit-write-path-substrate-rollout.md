---
name: 0016-a-audit-write-path-substrate-rollout
description: RFC-0016-a acceptance rollout — §6.10 canonical-bytes-on-write invariant verification + §6.11 DOMAIN adapter paired-acceptance gate + ReceiptId(pub [u8;32]) migration
metadata:
  node_type: substrate-faithful-consumer
  type: post-acceptance-rollout
  originSessionId: RFC-0016-a promotion session (2026-09-15)
  created: 2026-09-15
  v: "1.0"
  pair: DOMAIN Adapter Paired-Acceptance
  depends_on:
    - RFC-0016-a
    - RFC-0014-v2
    - RFC-0012-v2
status: Open
---

# 0016-a-audit-write-path-substrate-rollout — RFC-0016-a acceptance rollout

**Status:** Open — post-acceptance rollout
**Substrate:** RFC-0016-a §6.10 (canonical-bytes-on-write invariant) + §6.11 (read-stalls-while-write invariant) + §6.4 (`ReceiptId(pub [u8;32])` migration)
**Parent:** RFC-0016-a (Accepted 2026-09-15) + RFC-0016 (Accepted 2026-09-14)
**Companion:** RFC-0014-v2 (paired-acceptance — `ReceiptId(pub [u8;32])` lands when RFC-0014-v2 §6.3 settlement-receipt newtype is Accepted)

## §6.11 DOMAIN Adapter Paired-Acceptance gate

This mission is one of a pair of rollout missions for RFC-0016-a; see RFC-0016-a §6.11 Read-stalls-while-write invariant at `rfcs/accepted/process/0016-a-audit-receipt-write-path.md`. **RFC-0016-a + RFC-0014-v2 must both remain Accepted for §6.11 + §6.4 deliverables to remain claimable.** If either drops back to Draft, this mission MUST defer (user-initiated only per BLUEPRINT.md §Mission Lifecycle Deferral procedure).

## Scope

Per RFC-0016-a §6.1 Public surface additions + §6.10 Acceptance Criterion + §6.11 Acceptance Criterion, the §6 substrate code is largely landed at the Layer B façade (`octo-audit-core` + `octo-audit`). This mission covers the **rollout verification** for the §6 substrate amendments — workspace-wide conformance that the invariants survive every consumer path the amendment does not own, plus the DOMAIN adapter paired-acceptance gate.

### Out-of-scope (already landed in paired implementation mission)

The following §6 surface landed in `missions/claimed/0015-b-substrate-defect-impl.md` + paired implementation work (substrate-side, pre-RFC-0016-a promotion):

- `ChainHash(pub [u8; 32])` newtype — `crates/octo-audit/src/audit_event_v2.rs` §ChainHash
- `append_audit_event(sink, event) -> Result<ChainHash, AuditError>` — Layer B façade
- `append_agent_transition_event(payload, transitioned_at_unix) -> Result<[u8; 32], AuditError>` — `crates/octo-audit/src/audit_write.rs`
- `compute_chain_hash(event: &AuditEvent) -> [u8; 32]` — `crates/octo-audit-core/src/chain.rs` §compute_chain_hash
- `ReceiptSummary` projection — `crates/octo-audit/src/receipt_summary.rs`
- `redact_substrate_error(raw: &str) -> String` + 18-pattern registry — `crates/octo-audit/src/scrub_newtypes.rs`
- `AuditError::ChainHashMismatch { event_id }` — `crates/octo-audit-core/src/error.rs`
- `AuditFilter` additive fields — `since_unix`, `until_unix`, `subject_did`, `status: Vec<StatusRef>`, `model`, `capability_root` — `crates/octo-audit/src/receipt_read.rs` §AuditFilter
- `WalletError::AlreadyInTransition(Uuid)` + `InvalidStateTransition { from, to }` + `AuditUnavailable(String)` — `crates/octo-wallet/src/error.rs`
- `OctoCliError::AlreadyInTransition` + `InvalidStateTransition` + `AuditSubstrateNotReady` — `crates/octo-cli/src/error.rs`

### Deliverables

1. **§6.10 canonical-bytes-on-write invariant verification suite** — `crates/octo-audit/tests/canonical_bytes_invariant.rs` exercises every code path that emits an `AuditEvent` row and asserts:
   - `compute_chain_hash(event)` matches the value returned by `append_audit_event(sink, event) -> Ok(ChainHash)` for every `AuditEventKind` variant
   - No caller bypasses `compute_chain_hash` (grep `audit_event_v2.rs` + `audit_write.rs` for direct `BLAKE3::hash(...)` or `compute_chain_hash_on(event)` invocations outside the façade)
   - Re-canonicalization yields identical bytes (idempotent re-append produces same chain hash for the same event)
   - Existing test surface stays green (paired implementation tests remain PASS)

2. **§6.11 read-stalls-while-write DOMAIN adapter paired-acceptance gate** — extend `crates/octo-audit-core/src/append_only_sink.rs` (or paired DOMAIN adapter crate) with:
   - Reader-side: every `list_receipts` / `get_receipt` consumer acquires a read lock (`RwLock::read`) BEFORE the writer acquires the write lock
   - Writer-side: every `append_audit_event` / `append_agent_transition_event` path acquires the write lock (`RwLock::write`) with a single-writer guarantee
   - Invariant test: under N concurrent reader threads + 1 writer thread, readers either see pre-write state OR post-write state atomically (never partial rows)
   - Stoolap DOMAIN adapter conformance: every DOMAIN adapter (`crates/octo-settlement/src/storage/*.rs`, `crates/octo-audit/src/storage/*.rs`) routes through the gated `RwLock` adapter rather than direct table access

3. **`ReceiptId(pub [u8;32])` migration (paired with RFC-0014-v2 §6.3)** — DEFERRED until RFC-0014-v2 Accepted. When RFC-0014-v2 lands:
   - Add `ReceiptId(pub [u8; 32])` newtype to `crates/octo-audit-core/src/receipt_id.rs` (paired with `crates/octo-settlement-core/src/receipt.rs`)
   - Type alias `pub type ReceiptId = [u8; 32]` (or explicit newtype) used in `ReceiptSummary::receipt_id` field
   - `list_receipts` return type migrates from `Vec<u64>` to `Vec<ReceiptId>` (additive per RFC-0016-a §6.4 paired-acceptance contract)
   - Migration test surface: `Vec<u64>` → `Vec<ReceiptId>` conversion via `receipt_id_for_digest` is deterministic + invertible

4. **CLI §6.7 error variant wiring verification** — `crates/octo-cli/tests/error_envelope_audit_write_path.rs` exercises every `OctoCliError` variant reachable from the audit write-path surface:
   - `AlreadyInTransition(Uuid)` → exit 43
   - `InvalidStateTransition { from, to }` → exit 43
   - `AuditSubstrateNotReady` → exit 52
   - `AuditError::ChainHashMismatch { event_id }` → `OctoCliError::Internal(reason)` (redacted reason per RFC-0016-a §6.8 scrub helper) → exit 64

5. **Parent RFC cross-reference update** — append RFC-0016-a §Related RFCs note linking to RFC-0016 + RFC-0015-a + RFC-0016 §Substrate-Faithful Amendment Trail table.

### Out of scope (per RFC-0016-a §Future Work + §paired-acceptance DEFERRED)

- §6.4 `ReceiptId(pub [u8;32])` migration code lands ONLY when RFC-0014-v2 §6.3 is Accepted (paired-acceptance gate). Until then, the rollout mission verifies the `Vec<u64>` path remains stable + the paired migration test surface is prepared but skipped.
- §6.11 DOMAIN adapter code requires the Stoolap DOMAIN adapter (Layer B submodule, NOT Layer D adapter) to expose the `RwLock` reader/writer surface. If the Stoolap DOMAIN adapter does not yet expose that surface, this deliverable is DEFERRED to a follow-on amendment round.
- Any new substrate amendments (v1.7+ after promotion) — out of scope.

## Acceptance criteria

- [ ] AC-1: `cargo build -p octo-audit -p octo-audit-core -p octo-wallet -p octo-cli --features full` succeeds with zero warnings
- [ ] AC-2: `cargo build --workspace --features full` succeeds (no regression)
- [ ] AC-3: `cargo test -p octo-audit --lib` passes (paired implementation tests stay green)
- [ ] AC-4: `cargo test -p octo-audit-core --lib` passes (paired)
- [ ] AC-5: `cargo test -p octo-wallet --lib` passes (transition_agent + AgentRecord tests stay green)
- [ ] AC-6: `cargo test -p octo-cli --lib` passes (error variant tests stay green)
- [ ] AC-7: NEW `crates/octo-audit/tests/canonical_bytes_invariant.rs` PASSES (≥10 tests covering each AuditEventKind variant + re-canonicalization idempotency)
- [ ] AC-8: NEW `crates/octo-audit-core/tests/read_stalls_while_write_invariant.rs` PASSES (concurrent readers + 1 writer; readers see atomic pre/post-write states)
- [ ] AC-9: NEW Stoolap DOMAIN adapter conformance assertion — every DOMAIN adapter site routes through the `RwLock` adapter (no direct table access bypass)
- [ ] AC-10: `ReceiptId(pub [u8;32])` paired migration surface prepared (test-only — DEFERRED until RFC-0014-v2 Accepted)
- [ ] AC-11: NEW `crates/octo-cli/tests/error_envelope_audit_write_path.rs` PASSES (every §6.7 variant returns documented exit code)
- [ ] AC-12: RFC-0016-a §Related RFCs table appended with RFC-0016 + RFC-0015-a + RFC-0014-v2 paired pointer
- [ ] AC-13: RFC-0016 `D. Cross-references` table appended with v1 amendment pointer
- [ ] AC-14: Cite sweep clean for any RFC parent updates (`scripts/validate_cites.sh <parent-rfc-path>` returns 0 PHANTOM / 0 INVALID / 0 STALE)
- [ ] AC-15: Prettier-clean on all new + edited files
- [ ] AC-16: §6.11 DOMAIN adapter paired-acceptance gate sanity: if Stoolap DOMAIN adapter does not expose `RwLock` surface, this AC defers to a follow-on amendment round (user-initiated deferral per BLUEPRINT.md)

## Dependencies

- **RFC-0016-a** — Accepted (canonical `## Status` body header + front-matter `Status` row both declare Accepted). Mission is claimable iff this RFC remains Accepted.
- **RFC-0016** — Accepted (read-path substrate contract; this mission extends + verifies the read/write pairing).
- **RFC-0015-a** — Accepted (write-path surface contract for agent transitions; `transition_agent` calls `append_agent_transition_event`).
- **RFC-0014-v2** — DEFERRED for §6.4 `ReceiptId(pub [u8;32])` migration (paired-acceptance gate). Until RFC-0014-v2 lands, AC-10 is test-only (skip).
- **RFC-0012-v2** — Accepted (substrate amendments for the audit chain; `compute_chain_hash` paired with §S5).
- **parent RFC-0016** (`rfcs/accepted/process/0016-audit-receipt-api.md`) — needs `D. Cross-references` table appended (AC-13).
- **parent RFC-0016-a** (`rfcs/accepted/process/0016-a-audit-receipt-write-path.md`) — needs §Related RFCs table appended (AC-12).

## Risk

- **MEDIUM** — §6.11 DOMAIN adapter gate requires Stoolap DOMAIN adapter (Layer B submodule) to expose `RwLock` reader/writer surface. If the adapter does not yet expose this, AC-9 + AC-16 defer. Risk acknowledged in §Future Work; deferral is user-initiated per BLUEPRINT.md.
- **LOW** — §6.4 `ReceiptId(pub [u8;32])` migration is paired with RFC-0014-v2 (DEFERRED). AC-10 is test-only until RFC-0014-v2 lands.
- **LOW** — §6.10 canonical-bytes verification suite is purely additive (no regression to paired implementation tests).
- **LOW** — RFC-0016-a + RFC-0016 cross-reference append is doc-only edit.

## Cross-RFC invariants preserved

- `compute_chain_hash` is the canonical substrate byte encoder (Layer A frozen); every `append_*` façade funnels through it (no bypass paths).
- `ChainHashMismatch { event_id }` is the canonical error at the write boundary; CLI surfaces it via `OctoCliError::Internal` with `redact_substrate_error` scrub applied (per RFC-0016-a §6.8).
- `ReceiptSummary` projection preserves the 6-field canonical form (RFC-0014-v2 §S2 paired).
- §6.11 reader/writer lock discipline preserves read-stalls-while-write invariant — no reader sees partial rows (atomic pre/post-write observation).

## Test vectors (mission-level)

| ID                                       | Scenario                                                                                         | Expected                                                                         |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| `cb-compute_chain_hash-each-variant`     | `compute_chain_hash(&event)` for every `AuditEventKind` variant                                  | returns 32-byte BLAKE3 digest matching `append_audit_event` return value         |
| `cb-no-bypass-compute_chain_hash`        | grep `audit_event_v2.rs` + `audit_write.rs` for direct `BLAKE3::hash` invocations outside façade | 0 matches                                                                        |
| `cb-recanonicalization-idempotent`       | re-call `compute_chain_hash` on same `AuditEvent`                                                | returns identical 32 bytes                                                       |
| `rsw-concurrent-readers-no-partial-rows` | N=8 reader threads + 1 writer thread, writer appends mid-read                                    | every reader sees either pre-write or post-write state atomically; never partial |
| `rsw-single-writer-guarantee`            | 2 writer threads call `append_audit_event` concurrently                                          | one succeeds, the other gets `AuditError::ChainHashMismatch` (or `WouldBlock`)   |
| `rsw-stoolap-domain-adapter-conformance` | grep `crates/octo-*/src/storage/*.rs` for direct table access                                    | every flagged site routes through `RwLock` adapter                               |
| `rid-paired-migration-prepared`          | `Vec<u64>` → `Vec<ReceiptId>` via `receipt_id_for_digest`                                        | deterministic + invertible (paired test-only until RFC-0014-v2 lands)            |
| `cli-error-envelope-each-variant`        | every §6.7 OctoCliError variant returns documented exit code                                     | matches RFC-0016-a §6.7 exit code table                                          |

## Cross-references

- `missions/archived/completed/0016-a-audit-write-path-promotion.md` — RFC-0016-a promotion mission (CLOSED 2026-09-15)
- `missions/archived/completed/0015-b-substrate-defect-impl.md` — paired implementation (transition_agent + append_agent_transition_event + error variants)
- RFC-0016 (Accepted 2026-09-14) — read-path substrate contract
- RFC-0015-a (Accepted 2026-09-14) — write-path surface contract
- RFC-0012-v2 (Accepted) — substrate amendment for audit chain
- RFC-0014-v2 (DRAFT/DEFERRED) — paired settlement-receipt newtype (paired-acceptance gate for §6.4)
- [[feedback_initiation_user_only]] + [[git-workflow]] — push + remote writes user-owned
- [[memory-is-never-status-ground-truth]] — status language describes CURRENT state at write-time
