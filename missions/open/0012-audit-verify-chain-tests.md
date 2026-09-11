---
name: 0012-audit-verify-chain-tests
description: Implement `StoolapAuditSink` + `AppendOnlyAuditSink` impl + 10 verify_chain test vectors per RFC-0012 §Test Vectors + §Key Files to Modify Phase 3
metadata:
  node_type: substrate-storage-adapter
  type: sink-impl-and-tests
  originSessionId: RFC-0012 author session
  created: 2026-09-10
  v: "1.0"
  depends_on:
    - RFC-0012
    - mission 0012-audit-substrate-extraction
    - mission 0012-audit-wallet-migration
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0012-audit-verify-chain-tests — `StoolapAuditSink` + verify_chain test vectors

**Status:** Claimed — storage adapter + chain-integrity test suite
**Substrate:** RFC-0012 §Test Vectors + §Key Files to Modify Phase 3
**Parent:** RFC-0012

## Scope

Per RFC-0012 §Test Vectors (10 canonical vectors) + §Key Files to Modify test files, this mission implements the Stoolap-backed storage adapter for `AppendOnlyAuditSink` and exercises the 10 chain-integrity test vectors end-to-end.

### Deliverables

1. **`crates/octo-audit/src/storage/stoolap.rs` (NEW)** — `StoolapAuditSink` struct implementing `AppendOnlyAuditSink`. Lives in DOMAIN crate `octo-audit` (Layer B façade), NOT in `octo-audit-core` (Layer A frozen). Per CLAUDE.md §Architectural Principles + RFC-0012 §Security Considerations, Layer A frozen core MUST NOT couple to a specific DB implementation; storage adapters are domain-owned (matches 0013 + 0014 storage adapter pattern).
   - Wraps `Arc<Mutex<Database>>` (interior mutability; matches existing `StoolapStore` pattern in `quota-router-sm-engine`)
   - `append(&mut self, event: &AuditEvent) -> Result<(), AuditError>` — validates `event_id` monotonicity against last-persisted event, computes `chain_hash` via `BLAKE3(canonical_bytes)`, persists atomically
   - `last_event_id(&self) -> Result<Option<u64>, AuditError>` — read accessor (does NOT violate append-only invariant; reads do not mutate)
2. **`crates/octo-audit/tests/chain_verify.rs` (NEW)** — 10 chain-integrity test vectors per RFC-0012 §Test Vectors (run against substrate `verify_chain` directly, NOT via storage adapter)
3. **`crates/octo-audit/tests/sink_stoolap.rs` (NEW)** — StoolapAdapter smoke tests (round-trip append + last_event_id + idempotent-append rejection; integration with substrate `AppendOnlyAuditSink` trait)

### Acceptance criteria

- [ ] AC-1: `cargo test -p octo-audit --test chain_verify` passes all 10 test vectors (`chain-empty`, `chain-single`, `chain-monotonic`, `chain-gap`, `chain-hash-mismatch`, `chain-timestamp-regression`, `append-success`, `append-idempotent`, `extension-enum`, `debug-redaction`)
- [ ] AC-2: `StoolapAuditSink::append` enforces `event_id` monotonicity; non-monotonic sequence returns `AuditError::SequenceGap`
- [ ] AC-3: `StoolapAuditSink::append` is idempotent — same `event_id` re-append returns `AuditError::SequenceGap` (per RFC-0012 TV `append-idempotent`)
- [ ] AC-4: `StoolapAuditSink::append` computes `chain_hash` via `BLAKE3(canonical_bytes)` where `canonical_bytes` = stable RFC-0012 §Canonical Serialization form
- [ ] AC-5: `StoolapAuditSink` impl lives in DOMAIN crate `octo-audit` (storage adapter is domain-owned per CLAUDE.md §Architectural Principles; substrate `octo-audit-core` owns canonical types + traits only, NOT storage adapters; matches 0013 + 0014 storage adapter pattern)
- [ ] AC-6: `cargo clippy -p octo-audit --all-targets --all-features -- -D warnings` clean
- [ ] AC-7: `cargo fmt --all -- --check` clean
- [ ] AC-8: Workspace `cargo test --workspace` green

### Dependencies

- `RFC-0012` — canonical substrate spec
- `mission 0012-audit-substrate-extraction` — substrate crates must exist
- `mission 0012-audit-wallet-migration` — wallet-domain consumer migrated
- `quota-router-storage` (Layer D) — Stoolap DB handle

### Risk

- **HIGH** — `StoolapAuditSink::append` must be atomic. If a partial write leaves the chain inconsistent, `verify_chain` will reject all subsequent events. Mitigation: Stoolap `Transaction` wrapper in `append`; new `AuditError::PersistenceAtomic { path, reason }` variant on failure.
- **MEDIUM** — `BLAKE3(canonical_bytes)` byte-form must be stable across substrate + wallet-domain consumers. Mitigation: per RFC-0012 §Canonical Serialization, the canonical form is `[event_id (BE u64) | node_did (UTF-8) | event_kind (tag byte) | cap_root_hash (32 bytes) | at_millis_unix (BE u64) | prev_chain_hash (32 bytes)]`. Documented in `octo-audit-core/src/event.rs` module-level doc.
- **LOW** — Test flakiness if Stoolap DB path collision between parallel test runs. Mitigation: `tempfile::TempDir` per test.

### Cross-RFC invariants preserved

- `AppendOnlyAuditSink::append` is `&mut self`; no `delete` / `update` / `clear` method exists (RFC-0012 §Trait G3)
- `verify_chain` rejects any sequence gap, hash mismatch, or timestamp regression (RFC-0012 §Design Goals G5)
- BLAKE3-256 over canonical serialization (RFC-0012 §Design Goals G5)

### Test vectors (10 canonical, RFC-0012 §Test Vectors)

| ID | Scenario | Expected |
|----|----------|----------|
| `chain-empty` | Empty event sequence | `verify_chain(&[]) == Ok(())` |
| `chain-single` | Single event with `prev_chain_hash = [0;32]` | `verify_chain` accepts; `chain_hash` matches `BLAKE3(canonical_bytes)` |
| `chain-monotonic` | 10 events with strict `event_id` monotonicity + correct `prev_chain_hash` chaining | `verify_chain` accepts |
| `chain-gap` | 10 events with `event_id` skip 5 → 7 | `verify_chain` returns `AuditChainError::SequenceGap { event_id: 7, prev: 5, next: 7 }` |
| `chain-hash-mismatch` | Event with `chain_hash` field flipped by 1 byte | `verify_chain` returns `AuditChainError::HashMismatch` |
| `chain-timestamp-regression` | Two events with `at_millis_unix` decreasing | `verify_chain` returns `AuditChainError::TimestampRegression` |
| `append-success` | `StoolapAuditSink::append` with valid event | Returns `Ok(())`; `chain_hash` field set to `compute_chain_hash()` |
| `append-idempotent` | Same event appended twice | First `Ok(())`; second returns `AuditChainError::SequenceGap` |
| `extension-enum` | `CapabilityAuditEventKind = CapabilityMint \| CapabilityAttenuate` wraps `AuditEventKind` | Conversion to `AuditEventKind` succeeds for both variants |
| `debug-redaction` | `format!("{:?}", event)` with non-zero `cap_root_hash` | Output contains `<redacted 32 bytes>`; does NOT contain actual hash bytes |
