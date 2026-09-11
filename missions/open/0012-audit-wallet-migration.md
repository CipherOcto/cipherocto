---
name: 0012-audit-wallet-migration
description: Migrate `octo-wallet/capability/audit_log.rs` to consume `octo-audit-core` canonical `AuditEvent` per RFC-0012 §Key Files to Modify Phase 2
metadata:
  node_type: substrate-consumer
  type: domain-migration
  originSessionId: RFC-0012 author session
  created: 2026-09-10
  v: "1.0"
  depends_on:
    - RFC-0012
    - mission 0012-audit-substrate-extraction
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-10
---

# 0012-audit-wallet-migration — `octo-wallet/capability/audit_log.rs` substrate migration

**Status:** Claimed — domain migration to substrate
**Substrate:** RFC-0012 §Key Files to Modify Phase 2 (`octo-wallet/src/capability/audit_log.rs`)
**Parent:** RFC-0012

## Scope

Per RFC-0012 §Key Files to Modify SUBSTRATE row 2, the canonical `AuditEvent` field shape lives in `octo-audit-core` (Layer A frozen). The wallet-domain `audit_log.rs` re-exports the canonical type via `pub use octo_audit_core::AuditEvent` and implements the domain-specific `AuditEventKind` extensions (CapabilityMint / CapabilityAttenuate) per CLAUDE.md §Extension over enumeration.

### Deliverables

1. **`crates/octo-wallet/src/capability/audit_log.rs`** — replace local `AuditEvent` struct + `AuditEventKind` enum with `pub use octo_audit_core::{AuditEvent, AuditEventKind, AppendOnlyAuditSink, verify_chain}`. Local `CapabilityAuditEventKind` enum (wrapping `AuditEventKind` per substrate extension) becomes a `pub use` of the substrate-canonical extension shape.
2. **Manual `Debug` impl** — preserved verbatim (per RFC-0957-A1 §F3 + RFC-0012 §Module Layout §event). Wallet-domain does NOT redefine manual `Debug`; substrate canonical owns it.
3. **Field shape invariant test** — `cargo test -p octo-wallet capability::audit_log::tests::audit_event_field_shape_byte_identical_to_rfc_0957_a1_f3` asserts the migrated shape matches the canonical substrate spec.
4. **CapabilityMint / CapabilityAttenuate enum variants** — preserved as substrate-extension variants (per CLAUDE.md §Extension over enumeration: typed-discriminator pattern, NOT central enum).

### Acceptance criteria

- [ ] AC-1: `cargo build -p octo-wallet` succeeds with zero warnings (clippy `--all-features -- -D warnings`)
- [ ] AC-2: `crates/octo-wallet/src/capability/audit_log.rs` no longer defines local `AuditEvent` struct; uses `pub use octo_audit_core::AuditEvent`
- [ ] AC-3: Local `CapabilityAuditEventKind` enum extension preserved via substrate-canonical extension pattern (typed-discriminator, NOT central enum)
- [ ] AC-4: Manual `Debug` impl absent in wallet-domain; substrate canonical owns it
- [ ] AC-5: Field-shape invariant test added and PASSES
- [ ] AC-6: Workspace `cargo build --workspace` succeeds
- [ ] AC-7: Workspace `cargo test -p octo-wallet --lib capability::audit_log::` passes (existing tests still green post-migration)
- [ ] AC-8: RFC-0012 VH row appended documenting wallet migration

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

| ID | Scenario | Expected |
|----|----------|----------|
| `wallet-field-shape-invariant` | Migrated `AuditEvent` field shape vs canonical substrate | byte-identical (rustc struct layout assertion) |
| `wallet-capability-mint-extension` | `CapabilityAuditEventKind::CapabilityMint` → `AuditEventKind` conversion | succeeds; event_kind byte matches canonical substrate |
| `wallet-capability-attenuate-extension` | `CapabilityAuditEventKind::CapabilityAttenuate` → `AuditEventKind` conversion | succeeds; event_kind byte matches canonical substrate |
| `wallet-debug-redaction` | `format!("{:?}", event)` in wallet-domain | output identical to substrate canonical (manual Debug redaction preserved) |
