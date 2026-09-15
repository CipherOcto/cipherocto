---
name: 0016-a-audit-write-path-substrate-rollout
description: RFC-0016-a acceptance rollout — §6.10 canonical-bytes-on-write invariant verification + §6.11 DOMAIN adapter paired-acceptance gate + ReceiptId(pub u64) verification
metadata:
  node_type: substrate-faithful-consumer
  type: post-acceptance-rollout
  originSessionId: RFC-0016-a promotion session (2026-09-15)
  created: 2026-09-15
  v: "1.1"
  pair: DOMAIN Adapter Paired-Acceptance
  depends_on:
    - RFC-0016-a
    - RFC-0014-v2
    - RFC-0012-v2
status: Open
---

# 0016-a-audit-write-path-substrate-rollout — RFC-0016-a acceptance rollout

**Status:** Open — post-acceptance rollout
**Substrate:** RFC-0016-a §6.10 (canonical-bytes-on-write invariant) + §6.11 (read-stalls-while-write invariant) + §6.4 (`ReceiptId(pub u64)` verification)
**Parent:** RFC-0016-a + RFC-0016
**Companion:** RFC-0014-v2 (paired-acceptance — `ReceiptId(pub u64)` newtype at `octo-settlement-core::receipt::ReceiptId`)

## §6.11 DOMAIN Adapter Paired-Acceptance gate

This mission is one of a pair of rollout missions for RFC-0016-a. See RFC-0016-a §6.11 Read-stalls-while-write invariant at `rfcs/accepted/process/0016-a-audit-receipt-write-path.md`. **RFC-0016-a must remain Accepted for §6.10 + §6.11 deliverables to remain claimable.** RFC-0014-v2 §S3 lands `ReceiptId(pub u64)` newtype at `octo-settlement-core` Layer A frozen — paired-acceptance gate ACTIVE for §6.4 verification. If RFC-0016-a drops back to Draft, this mission MUST defer (user-initiated only per BLUEPRINT.md §Mission Lifecycle Deferral procedure).

## Scope

Per RFC-0016-a §6.1 Public surface additions + §6.10 Acceptance Criterion + §6.11 Acceptance Criterion, the §6 substrate code is largely landed at the Layer B façade (`octo-audit-core` + `octo-audit`). This mission covers the **rollout verification** for the §6 substrate amendments — workspace-wide conformance that the invariants survive every consumer path the amendment does not own, plus the DOMAIN adapter paired-acceptance gate.

### Out-of-scope (already landed in paired implementation mission)

The following §6 surface landed in `missions/claimed/0015-b-substrate-defect-impl.md` (paired implementation, Claimed status, not yet archived to `archived/completed/`) + paired implementation work (substrate-side, pre-RFC-0016-a promotion):

- `ChainHash(pub [u8; 32])` newtype — `crates/octo-audit/src/audit_event_v2.rs` §ChainHash
- `append_audit_event(sink, event) -> Result<ChainHash, AuditError>` — Layer B façade
- `append_agent_transition_event(payload, transitioned_at_unix) -> Result<ChainHash, AuditError>` — `crates/octo-audit/src/audit_write.rs`
- `compute_chain_hash(event: &AuditEvent) -> [u8; 32]` — `crates/octo-audit-core/src/chain.rs` §compute_chain_hash
- `ReceiptSummary` projection — `crates/octo-audit/src/receipt_summary.rs`
- `redact_substrate_error(raw: &str) -> String` + scrub registry — `crates/octo-audit/src/scrub_newtypes.rs`
- `AuditError::ChainHashMismatch { event_id }` — `crates/octo-audit-core/src/error.rs`
- `AuditFilter` additive fields — `since_unix`, `until_unix`, `subject_did`, `status: Vec<StatusRef>`, `model`, `capability_root` — `crates/octo-audit/src/receipt_read.rs` §AuditFilter
- `WalletError::AlreadyInTransition(Uuid)` + `InvalidStateTransition { from, to }` + `AuditUnavailable(String)` — `crates/octo-wallet/src/error.rs`
- `OctoCliError::AlreadyInTransition` + `InvalidStateTransition` + `AuditSubstrateNotReady` — `crates/octo-cli/src/error.rs`
- RFC-0016-a §6.7 additive substrate variants `AuditError::AuditAppendFailed(reason)` + `ReceiptNotFound(decimal)` + `InvalidFilter(reason)` + `PermissionDenied(reason)` (collapse-group + 4 additive) — `crates/octo-audit-core/src/error.rs` + `crates/octo-cli/src/error.rs`

### Deliverables

1. **§6.10 canonical-bytes-on-write invariant verification suite** — `crates/octo-audit/tests/canonical_bytes_invariant.rs` exercises every code path that emits an `AuditEvent` row and asserts:
   - `compute_chain_hash(event)` matches the value returned by `append_audit_event(sink, event) -> Ok(ChainHash)` for every `AuditEventKind` variant
   - No caller bypasses `compute_chain_hash` (grep `audit_event_v2.rs` + `audit_write.rs` for direct `blake3::hash(...)` invocations outside the façade)
   - Re-canonicalization yields identical bytes (idempotent re-append produces same chain hash for the same event)
   - Existing test surface stays green (paired implementation tests remain PASS)

2. **§6.11 read-stalls-while-write DOMAIN adapter paired-acceptance gate** — DOMAIN adapter work at the Layer B façade storage site (`crates/octo-audit/src/storage/stoolap.rs`, the canonical Stoolap DOMAIN adapter for `octo-audit` per RFC-0014-v2 §Specification DOMAIN adapter pattern). `crates/octo-audit-core/src/sink.rs` (Layer A frozen substrate trait `AppendOnlyAuditSink`) is read-only — DOMAIN adapter impl lives at Layer B-faithful `crates/octo-audit/src/storage/*.rs` (NOT Layer A frozen core). R/W primitive per RFC-0016-a §6.11 (each DOMAIN impl owns the choice — single shared mutex, `RwLock`, or sharded):
   - Reader-side: every `list_receipts` / `get_receipt` consumer acquires the chosen R/W primitive's read guard BEFORE the writer acquires the write guard
   - Writer-side: every `append_audit_event` / `append_agent_transition_event` path acquires the write guard with single-writer guarantee. `&mut self` on `AppendOnlyAuditSink::append` enforces type-level append-only
   - Invariant test: under N=8 concurrent reader threads + 1 writer thread, readers either see pre-write state OR post-write state atomically (never partial rows)
   - Stoolap DOMAIN adapter conformance: every DOMAIN adapter site (`crates/octo-audit/src/storage/*.rs`) routes through the gated adapter rather than direct table access

3. **`ReceiptId(pub u64)` verification** — substrate `ReceiptId(pub u64)` newtype already landed at `crates/octo-settlement-core/src/receipt.rs` per RFC-0014-v2 §S3 + RFC-0016-a §6.4 (paired-acceptance). No code migration required. Verification scope:
   - `ReceiptId` consumer-resolution grep: canonical substrate source is `octo_settlement::ReceiptId` (RFC-0014-v2 §S3 newtype at `octo-settlement-core::receipt::ReceiptId`). `octo-audit-core` carries NO `ReceiptId` (Layer A frozen, no `receipt_summary` module, no `ReceiptId` re-export at `octo-audit-core::lib`). The `ReceiptSummary` façade projection at `octo_audit::receipt_summary::ReceiptSummary` imports `octo_settlement::ReceiptId` directly (per `crates/octo-audit/src/receipt_summary.rs` §use statements) — no façade wrapper re-export exists
   - `ReceiptSummary::receipt_id` field already uses `ReceiptId` (paired form)
   - `list_receipts` substrate-faithful return type remains `Vec<u64>` (per §list_receipts in `crates/octo-audit/src/receipt_read.rs` — `Result<Vec<u64>, AuditError>`). CLI presentation layer wraps as `ReceiptId` for operator display (presentation-only, no substrate change)
   - Verification grep: no caller site uses raw `[u8; 32]` for `ReceiptId`. Every consumer resolves through `octo_settlement::ReceiptId` import path

4. **CLI §6.7 error variant wiring verification** — Two test files at `crates/octo-cli/tests/` exercise every §6.7 RFC-0016-a substrate error variant mapping to `OctoCliError`: `error_envelope_collapse_group.rs` (collapse-group) per AC-11a, `error_envelope_additive_variants.rs` (additive) per AC-11b:
   - Collapse-group (SequenceGap + AlreadyExists + SinkSpecific + ChainHashMismatch) → `OctoCliError::Internal(redacted_reason)` → exit 64 (per RFC-0016-a §6.7 + §OctoCliError mapping table in `crates/octo-cli/src/error.rs`)
   - `AuditError::AuditAppendFailed(reason)` → `OctoCliError::AuditSubstrateNotReady` → exit 52
   - `AuditError::ReceiptNotFound(decimal)` → `OctoCliError::ReceiptNotFound(redacted_id)` → exit 17
   - `AuditError::InvalidFilter(reason)` → `OctoCliError::InvalidFilter(redacted_reason)` → exit 16
   - `AuditError::PermissionDenied(reason)` → `OctoCliError::PermissionDenied(redacted_reason)` → exit 13
   - `redact_substrate_error` applied to every reason payload per RFC-0016-a §6.8

5. **Parent RFC cross-reference update** — append RFC-0016-a §Related RFCs note linking to RFC-0016 + RFC-0015-a + RFC-0014-v2 (paired-acceptance pointer, existing RFC-0014 unversioned row preserved). Verify RFC-0016 §Related RFCs row for RFC-0016-a is present (paired-amendment pointer, row already exists, verification only).

### Out of scope (per RFC-0016-a §Future Work + paired-acceptance DEFERRED)

- §6.4 `ReceiptId(pub u64)` migration code already landed at `crates/octo-settlement-core/src/receipt.rs` per RFC-0014-v2 §S3. Mission verifies paired-acceptance via re-export grep + `ReceiptSummary::receipt_id` field shape. No substrate code change required.
- §6.11 DOMAIN adapter code: R/W primitive choice is per-DOMAIN-impl per RFC-0016-a §6.11 (NOT mandated to a specific primitive). If a DOMAIN impl violates the atomic pre/post-write invariant, AC-8 + AC-9 + AC-16 defer to a follow-on amendment round (user-initiated deferral per BLUEPRINT.md).
- Any new substrate amendments (v1.7+ after promotion) — out of scope.

## Acceptance criteria

- [ ] AC-1: `cargo build -p octo-audit -p octo-audit-core -p octo-wallet -p octo-cli --features full` succeeds with zero warnings
- [ ] AC-2: `cargo build --workspace --features full` succeeds (no regression)
- [ ] AC-3: `cargo test -p octo-audit --lib` passes (paired implementation tests stay green)
- [ ] AC-4: `cargo test -p octo-audit-core --lib` passes (paired)
- [ ] AC-5: `cargo test -p octo-wallet --lib` passes (transition_agent + AgentRecord tests stay green)
- [ ] AC-6: `cargo test -p octo-cli --lib` passes (error variant tests stay green)
- [ ] AC-7: NEW `crates/octo-audit/tests/canonical_bytes_invariant.rs` PASSES (≥10 tests covering each AuditEventKind variant + re-canonicalization idempotency)
- [ ] AC-8: NEW `crates/octo-audit/tests/read_stalls_while_write_invariant.rs` PASSES (N=8 concurrent readers + 1 writer. Readers see atomic pre/post-write states)
- [ ] AC-9: NEW Stoolap DOMAIN adapter conformance assertion — every DOMAIN adapter site (`crates/octo-audit/src/storage/*.rs`) routes through the gated adapter (no direct table access bypass). Atomic pre/post-write invariant holds regardless of R/W primitive choice
- [ ] AC-10: `ReceiptId(pub u64)` paired-acceptance verification — `octo-audit-core` consumer-resolution grep (no crate-root re-export, consumers resolve via `octo_audit` façade wrapper) + `ReceiptSummary::receipt_id` field shape + grep assertion that no caller site uses raw `[u8; 32]` for `ReceiptId`
- [ ] AC-11a: NEW `crates/octo-cli/tests/error_envelope_collapse_group.rs` PASSES (SequenceGap + AlreadyExists + SinkSpecific + ChainHashMismatch all map to `OctoCliError::Internal(redacted_reason)` → exit 64)
- [ ] AC-11b: NEW `crates/octo-cli/tests/error_envelope_additive_variants.rs` PASSES (4 additive substrate variants map to documented `OctoCliError` variants at exits 52/17/16/13 respectively)
- [ ] AC-12: RFC-0016-a §Related RFCs table appended with row pointing to RFC-0014-v2 (paired-acceptance for §6.4, existing RFC-0014 unversioned row is preserved) — row text: `RFC-0014-v2 — §S3 ReceiptId newtype paired with §6.4 verification`
- [ ] AC-13: RFC-0016 §Related RFCs table verification — RFC-0016-a row already present (paired-amendment pointer), no append required. Verification confirms row text reads `RFC-0016-a — Audit Receipt Write-Path Amendment (sibling; DEFERRED surface per §6.1 §Amendment Surface)`
- [ ] AC-14: Cite sweep clean for any RFC parent updates (`timeout 30 scripts/validate_cites.sh <parent-rfc-path>` returns 0 PHANTOM / 0 INVALID / 0 STALE per [[feedback-validate-cites-timeout]])
- [ ] AC-15: Prettier-clean on all new + edited `.md` files. `cargo fmt --all` clean on all new + edited `.rs` files
- [ ] AC-16: §6.11 DOMAIN adapter paired-acceptance gate sanity — atomic pre/post-write invariant holds in `crates/octo-audit/src/storage/*.rs` DOMAIN impls (R/W primitive agnostic per RFC-0016-a §6.11). If any DOMAIN impl violates atomicity, AC defers to a follow-on amendment round (user-initiated deferral per BLUEPRINT.md)

## Dependencies

- **RFC-0016-a** — canonical `## Status` body header + front-matter `Status` row declare status. Mission is claimable iff this RFC remains Accepted.
- **RFC-0016** — read-path substrate contract. This mission extends + verifies the read/write pairing.
- **RFC-0015-a** — write-path surface contract for agent transitions. `transition_agent` calls `append_agent_transition_event`.
- **RFC-0014-v2** — `ReceiptId(pub u64)` newtype at `octo-settlement-core::receipt` per §S3 (paired-acceptance for §6.4 verification). AC-10 active.
- **RFC-0012-v2** — substrate amendments for the audit chain. `compute_chain_hash` paired with §S5.
- **parent RFC-0016** (`rfcs/accepted/process/0016-audit-receipt-api.md`) — §Related RFCs row verification only (AC-13). No edit required since RFC-0016-a row is already present.
- **parent RFC-0016-a** (`rfcs/accepted/process/0016-a-audit-receipt-write-path.md`) — needs §Related RFCs table appended (AC-12).
- **paired impl mission** (`missions/claimed/0015-b-substrate-defect-impl.md`) — paired implementation that landed §6 substrate code (Claimed status, not yet archived).

## Risk

- **MEDIUM** — §6.11 DOMAIN adapter gate requires every DOMAIN impl at `crates/octo-audit/src/storage/*.rs` to enforce atomic pre/post-write observation (R/W primitive agnostic per RFC-0016-a §6.11). If any DOMAIN impl violates atomicity, AC-8 + AC-9 + AC-16 defer. Risk acknowledged in §Future Work. Deferral is user-initiated per BLUEPRINT.md.
- **LOW** — §6.4 `ReceiptId(pub u64)` verification is purely additive re-export + grep assertion (substrate already landed per RFC-0014-v2 §S3). AC-10 active. No substrate code change.
- **LOW** — §6.10 canonical-bytes verification suite is purely additive (no regression to paired implementation tests).
- **LOW** — RFC-0016-a + RFC-0016 cross-reference append is doc-only edit.

## Cross-RFC invariants preserved

- `compute_chain_hash` is the canonical substrate byte encoder (Layer A frozen). Every `append_*` façade funnels through it (no bypass paths).
- `ChainHashMismatch { event_id }` is canonical-error mapping at the write boundary. CLI surface via `OctoCliError::Internal` with `redact_substrate_error` scrub is enforced at the Layer B façade — see RFC-0016-a §6.8 + §6.10.
- `ReceiptSummary` projection preserves the 6-field canonical form per RFC-0014-v2 §S2.
- §6.11 atomic pre/post-write observation invariant preserved regardless of R/W primitive choice.

## Test vectors (mission-level)

| ID                                       | Scenario                                                                                                   | Expected                                                                         |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `cb-compute_chain_hash-each-variant`     | `compute_chain_hash(&event)` for every `AuditEventKind` variant                                            | returns 32-byte BLAKE3 digest matching `append_audit_event` return value         |
| `cb-no-bypass-compute_chain_hash`        | grep `audit_event_v2.rs` + `audit_write.rs` for direct `blake3::hash` invocations outside façade           | 0 matches                                                                        |
| `rsw-concurrent-readers-no-partial-rows` | N=8 reader threads + 1 writer thread, writer appends mid-read                                              | every reader sees either pre-write or post-write state atomically. Never partial |
| `rsw-single-writer-guarantee`            | 2 writer threads call `append_audit_event` concurrently                                                    | one succeeds, the other gets `AuditError::ChainHashMismatch` (or `WouldBlock`)   |
| `rsw-stoolap-domain-adapter-conformance` | grep `crates/octo-audit/src/storage/*.rs` for direct table access                                          | every flagged site routes through gated adapter (R/W primitive agnostic)         |
| `rid-paired-acceptance-verification`     | grep workspace for raw `[u8; 32]` use sites of `ReceiptId`                                                 | 0 matches. Every consumer resolves via `octo_settlement::ReceiptId` import path  |
| `cli-error-envelope-collapse-group`      | every collapse-group `AuditError` variant (SequenceGap + AlreadyExists + SinkSpecific + ChainHashMismatch) | maps to `OctoCliError::Internal(redacted_reason)` → exit 64                      |
| `cli-error-envelope-additive-variants`   | 4 §6.7 additive `AuditError` variants                                                                      | map to documented `OctoCliError` variants at exits 52/17/16/13 respectively      |

## Cross-references

- `missions/archived/completed/0016-a-audit-write-path-promotion.md` — RFC-0016-a promotion mission
- `missions/claimed/0015-b-substrate-defect-impl.md` — paired implementation (transition_agent + append_agent_transition_event + error variants), Claimed status
- RFC-0016 — read-path substrate contract
- RFC-0015-a — write-path surface contract
- RFC-0012-v2 — substrate amendment for audit chain
- RFC-0014-v2 — paired settlement-receipt newtype (§S3 paired-acceptance gate for §6.4)
- [[feedback_initiation_user_only]] + [[git-workflow]] — push + remote writes user-owned
- [[memory-is-never-status-ground-truth]] — status language describes CURRENT state at write-time
