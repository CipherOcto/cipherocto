---
name: 0012-audit-wallet-migration
description: Migrate `octo-wallet/capability/audit_log.rs` to consume `octo-audit-core` canonical `AuditEvent` per RFC-0012 §Key Files to Modify Phase 2
metadata:
  node_type: substrate-consumer
  type: domain-migration
  originSessionId: RFC-0012 author session
  created: 2026-09-10
  v: "1.1"
  depends_on:
    - RFC-0012
    - mission 0012-audit-substrate-extraction
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
completed_at: 2026-09-13
---

# 0012-audit-wallet-migration — `octo-wallet/capability/audit_log.rs` substrate migration

**Status:** Completed — domain migration to substrate (LANDED 2026-09-13)
**Substrate:** RFC-0012 §Key Files to Modify Phase 2 (`octo-wallet/src/capability/audit_log.rs`)
**Parent:** RFC-0012

## Scope

Per RFC-0012 §Key Files to Modify SUBSTRATE row 2, the canonical `AuditEvent` field shape lives in `octo-audit-core` (Layer A frozen). The wallet-domain `audit_log.rs` re-exports the canonical types via the Layer B façade `pub use octo_audit::{verify_chain, AuditChainError, AuditEvent, AuditEventKind}` (correct layer direction: wallet Layer B → octo-audit Layer B façade → octo-audit-core Layer A frozen — the wallet module consumes the façade, not the frozen core directly, to preserve façade-only stable consumer surface) and implements the domain-specific `AuditEventKind` label helper per CLAUDE.md §Extension over enumeration.

### Deliverables

1. **`crates/octo-wallet/src/capability/audit_log.rs`** — replace local `AuditEvent` struct + `AuditEventKind` enum with `pub use octo_audit::{verify_chain, AuditChainError, AuditEvent, AuditEventKind}` (Layer B façade re-export). Local `CapabilityAuditEventKind` enum (wrapping `AuditEventKind` per substrate extension) becomes a free function helper `audit_event_kind_as_str` with `_ => "unknown"` sentinel (typed-discriminator pattern, NOT central enum).
2. **Manual `Debug` impl** — preserved verbatim (per RFC-0957-A1 §F3 + RFC-0012 §Module Layout §event). Wallet-domain does NOT redefine manual `Debug`; substrate canonical owns it.
3. **Field shape invariant test** — `cargo test -p octo-wallet capability::audit_log::tests::audit_event_struct_exposes_canonical_fields` asserts the migrated shape matches the canonical substrate spec (7 named fields reachable + Serialize/Deserialize round-trip).
4. **CapabilityMint / CapabilityAttenuate enum variants** — preserved as substrate-extension variants (per CLAUDE.md §Extension over enumeration: typed-discriminator pattern with `_ => "unknown"` sentinel, NOT central enum).

### Acceptance criteria

- [x] AC-1: `cargo build -p octo-wallet` succeeds with zero warnings (clippy `--all-features -- -D warnings`) — verified `cargo clippy -p octo-wallet --all-targets --all-features -- -D warnings` clean
- [x] AC-2: `crates/octo-wallet/src/capability/audit_log.rs` no longer defines local `AuditEvent` struct; uses Layer B façade re-export `pub use octo_audit::{verify_chain, AuditChainError, AuditEvent, AuditEventKind}` (correct B→B direction: wallet Layer B → octo-audit Layer B façade → octo-audit-core Layer A frozen; wallet consumes the façade, NOT the frozen core, per layer model) — line 33
- [x] AC-3: Local `CapabilityAuditEventKind` enum extension preserved via substrate-canonical extension pattern (typed-discriminator, NOT central enum) — substrate `AuditEventKind` is `#[non_exhaustive]` per RFC-0012 §Extension over enumeration; wallet `audit_event_kind_as_str` free function uses `_ => "unknown"` sentinel pattern (the typed-discriminator extension pattern, NOT central enum)
- [x] AC-4: Manual `Debug` impl absent in wallet-domain; substrate canonical owns it — module-level comment at lines 20-21 confirms; no manual `impl Debug` in wallet file
- [x] AC-5: Field-shape invariant test added and PASSES — `audit_event_struct_exposes_canonical_fields` at line 96-122 (constructs via canonical field order, reads all 7 fields, asserts `Serialize`/`Deserialize` round-trip)
- [x] AC-6: Workspace `cargo build --workspace` succeeds — verified clean (note: `--all-features` exposes pre-existing `octo-reputation/bin/reputation-parity` issue unrelated to this mission)
- [x] AC-7: Workspace `cargo test -p octo-wallet --lib capability::audit_log::` passes — 9 tests PASS (audit_event_struct_exposes_canonical_fields, audit_event_debug_redacts_hash_fields, broken_prev_link_detected, chain_hash_is_deterministic, chain_verify_accepts_genesis_prev_zero, empty_chain_verifies, event_kind_labels_stable, insert_then_revoke_emits_two_audit_entries, tampering_with_log_breaks_chain_check)
- [x] AC-8: RFC-0012 VH row appended documenting wallet migration — VH row "1.2 2026-09-13 Mission 0012-audit-wallet-migration LANDED" appended

### Dependencies

- `RFC-0012` — canonical substrate spec
- `mission 0012-audit-substrate-extraction` — must complete first (this mission consumes the substrate)
- `RFC-0957-A1` §F3 — source-of-truth for byte-identical field shape
- `RFC-0009` — `node_did` DID canonical form

### Risk

- **MEDIUM** — Domain migration can break downstream consumers if `pub use` chain breaks. Mitigation: workspace `cargo test --workspace` after migration.
- **LOW** — Manual `Debug` impl drift if wallet-domain accidentally re-defines it. Mitigation: AC-4 + cargo clippy lint `manual_debug_impl` on `audit_log` path.

### Cross-RFC invariants preserved

- `AuditEvent` field shape byte-identical to RFC-0957-A1 §F3 (event_id, node_did, event_kind, cap_root_hash, at_millis_unix, prev_chain_hash, chain_hash)
- `AuditEventKind` discriminants byte-identical (Insert, Revoke, Sync)
- Manual `Debug` redaction (3 hash fields → `<redacted 32 bytes>`)
- `CapabilityAuditEventKind` extension preserved as substrate-extension typed-discriminator

### Test vectors (domain-level)

| ID                                      | Scenario                                                                      | Expected                                                                   |
| --------------------------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `wallet-field-shape-invariant`          | Migrated `AuditEvent` field shape vs canonical substrate                      | byte-identical (rustc struct layout assertion)                             |
| `wallet-capability-mint-extension`      | `CapabilityAuditEventKind::CapabilityMint` → `AuditEventKind` conversion      | succeeds; event_kind byte matches canonical substrate                      |
| `wallet-capability-attenuate-extension` | `CapabilityAuditEventKind::CapabilityAttenuate` → `AuditEventKind` conversion | succeeds; event_kind byte matches canonical substrate                      |
| `wallet-debug-redaction`                | `format!("{:?}", event)` in wallet-domain                                     | output identical to substrate canonical (manual Debug redaction preserved) |
