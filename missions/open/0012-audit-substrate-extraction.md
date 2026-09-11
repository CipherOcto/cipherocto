---
name: 0012-audit-substrate-extraction
description: Extract canonical `AuditEvent` + `AppendOnlyAuditSink` + `verify_chain` to `octo-audit-core` Layer A frozen crate + create `octo-audit` Layer B façade per RFC-0012
metadata:
  node_type: substrate-core
  type: layer-a-frozen-extraction
  originSessionId: RFC-0012 author session
  created: 2026-09-10
  v: "1.0"
  depends_on:
    - RFC-0012
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0012-audit-substrate-extraction — `octo-audit-core` + `octo-audit` per RFC-0012

**Status:** Open — substrate extraction (Layer A frozen core + Layer B façade)
**Substrate:** RFC-0012 §Specification (`octo-audit-core` + `octo-audit` modules)
**Parent:** RFC-0012

## Scope

Per RFC-0012 §Key Files to Modify Phase 1 — substrate extraction. Creates two new crates + a chain-integrity test suite.

### Deliverables

1. **`crates/octo-audit-core/` (NEW)** — Layer A frozen core.
   - `src/lib.rs` — module re-exports + crate docs (RFC-0012 §Module Layout)
   - `src/event.rs` — `AuditEvent` struct (`node_did: String` — raw canonical DID form; Layer A frozen per RFC-0010 + RFC-0009; domain converts DID → String at call boundary) + `AuditEventKind` enum (#[non_exhaustive]) + manual `Debug` impl redacting `cap_root_hash` + `prev_chain_hash` + `chain_hash`
   - `src/sink.rs` — `AppendOnlyAuditSink` trait (`&mut self` + `append` method only)
   - `src/chain.rs` — `verify_chain` + `compute_chain_hash` + `AuditChainError` enum (SequenceGap / HashMismatch / TimestampRegression)
   - `src/error.rs` — `AuditError` enum (cross-trait errors)
   - `Cargo.toml` — deps: `blake3`, `serde`, `thiserror` ONLY (NO `octo-ident`; per CLAUDE.md §Architectural Principles layer direction rule, Layer A frozen MUST NOT depend on Layer B substrate; DID canonical-form conversion lives at the domain call boundary)
2. **`crates/octo-audit/` (NEW)** — Layer B façade (~25 LoC).
   - `src/lib.rs` — `pub use octo_audit_core::*` + re-export any extension enums from `octo-wallet`
   - `Cargo.toml` — depends on `octo-audit-core` only
3. **Workspace registration** — add both crates to root `Cargo.toml` `members` list
4. **Byte-identical extraction claim** — `octo-wallet/capability/audit_log.rs` field shape (`event_id: u64, node_did: String, event_kind, cap_root_hash: [u8;32], at_millis_unix: u64, prev_chain_hash: [u8;32], chain_hash: [u8;32]`) preserved; `AuditEventKind { Insert, Revoke, Sync }` discriminants preserved; manual `Debug` redaction preserved verbatim per RFC-0957-A1 §F3

### Acceptance criteria

- [ ] AC-1: `cargo build -p octo-audit-core` succeeds with zero warnings (clippy `--all-features -- -D warnings`)
- [ ] AC-2: `cargo build -p octo-audit` succeeds with zero warnings
- [ ] AC-3: All 6 §Module Layout sections present in `octo-audit-core/src/lib.rs` per RFC-0012 §Key Files to Modify
- [ ] AC-4: `AuditEvent` + `AuditEventKind` `#[non_exhaustive]` per CLAUDE.md §Extension over enumeration
- [ ] AC-5: Manual `Debug` impl redacts the 3 hash fields (per existing `audit_log.rs` pattern)
- [ ] AC-6: `AppendOnlyAuditSink::append` is `&mut self`; no `delete` / `update` / `clear` method exists
- [ ] AC-7: Workspace `cargo build --workspace` succeeds after registration
- [ ] AC-8: RFC-0012 VH row appended documenting substrate extraction + cite-hygiene PASS

### Out of scope (separate missions)

- Migration of `octo-wallet/capability/audit_log.rs` to use substrate → `missions/open/0012-audit-wallet-migration.md`
- `StoolapAuditSink` impl + verify_chain test vectors → `missions/open/0012-audit-verify-chain-tests.md`
- Domain-level `StoolapAuditSink` impl (consumer of `AppendOnlyAuditSink`) → downstream mission

### Dependencies

- `RFC-0012` (accepted 2026-09-10) — canonical substrate spec
- `RFC-0957-A1` §F3 — source-of-truth for canonical `AuditEvent` field shape
- `RFC-0010` — canonical DID wire form (substrate holds `node_did: String`; domain does canonical-form conversion at call boundary)
- `RFC-0009` — `node_did` DID substrate (domain-side concern; substrate stays Layer A frozen)

### Risk

- **HIGH** — Layer A frozen core addition. Per CLAUDE.md §Architectural Principles + RFC-0012 §Security Considerations, the core MUST be RFC-frozen + semver-major only. Mitigation: explicit `Cargo.toml` comment pinning the frozen-core status + CLAUDE.md cross-link.
- **MEDIUM** — Manual `Debug` impl drift. Mitigation: cargo test asserts `format!("{:?}", event)` contains `<redacted 32 bytes>` for all 3 hash fields (RFC-0012 §Test Vector `debug-redaction`).
- **LOW** — Workspace member ordering. Mitigation: append (not insert) to `Cargo.toml` `members` list to avoid merge conflicts.

### Cross-RFC invariants preserved

- `AuditEvent` field shape byte-identical to RFC-0957-A1 §F3
- `AuditEventKind` discriminants preserved (Insert=0, Revoke=1, Sync=2)
- PQC migration blast radius confined to Layer A frozen core (RFC-0012 §Security Considerations)

### Test vectors (substrate-level, RFC-0012 §Test Vectors)

| ID | Scenario | Expected |
|----|----------|----------|
| `chain-empty` | Empty event sequence | `verify_chain(&[]) == Ok(())` |
| `chain-single` | Single event with `prev_chain_hash = [0;32]` | `verify_chain` accepts; `chain_hash` matches `BLAKE3(canonical_bytes)` |
| `chain-monotonic` | 10 events with strict `event_id` monotonicity + correct chaining | `verify_chain` accepts |
| `chain-gap` | 10 events with `event_id` skip 5 → 7 | `verify_chain` returns `AuditChainError::SequenceGap { event_id: 7, prev: 5, next: 7 }` |
| `chain-hash-mismatch` | Event with `chain_hash` field flipped by 1 byte | `verify_chain` returns `AuditChainError::HashMismatch` |
| `chain-timestamp-regression` | Two events with `at_millis_unix` decreasing | `verify_chain` returns `AuditChainError::TimestampRegression` |
| `debug-redaction` | `format!("{:?}", event)` with non-zero `cap_root_hash` | Output contains `<redacted 32 bytes>`; does NOT contain actual hash bytes |
