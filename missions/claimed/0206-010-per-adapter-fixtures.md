---
name: 0206-010-per-adapter-fixtures
description: Open 2026-08-20 v1.0; per-adapter fixture suites for the 5 adapter crates from mission 0206-009 — drop_table_negative.rs + namespace_guard.rs + 4 adversarial fixtures per adapter per -0206-A11 + TV-0206-A12 gates.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  supersedes: null
  depends_on:
    - 0206-009-adapter-crate-creation
phase: 1.8
layer: B
rfc_authority: RFC-0206
tvs:
  - TV-0206-A11
  - TV-0206-A12
status: done
---

# Mission `0206-010-per-adapter-fixtures` v1.0 — OPEN 2026-08-20

## Scope

Per + §Format Bypass Defense, each of the 5 adapter crates from mission `0206-009` needs a fixture suite:

- `tests/drop_table_negative.rs` — `DdlRegistered(DropTable(...))` → `SubstrateError::DdlNotInAllowlist` (TV-0206-A11)
- `tests/namespace_guard.rs` — workspace query outside adapter namespace → `SubstrateError::TableNotInNamespace` (TV-0206-A12)
- 4 adversarial fixtures per RFC §Format Bypass Defense:
  - `tests/adversarial_double_register.rs` — two adapters registering same `adapter_id` → second registration rejected
  - `tests/adversarial_empty_allowlist.rs` — adapter with empty `AdapterAllowlist` → all DDL rejected
  - `tests/adversarial_format_injection.rs` — `format!()` injection attempt in DdlRegistered template → `SubstrateError::DdlNotInAllowlist`
  - `tests/adversarial_escape_hatch_misuse.rs` — misuse of `From<Database> for stoolap::Database` escape hatch from a non-typed-query allowlist site → compile-time or runtime refusal

Total: 6 fixtures × 5 adapters = 30 NEW test files.

## Out of scope (deferred)

- Per the plan, the existing `register_roundtrip.rs` fixture (TV-0206-A10) was landed in mission `0206-009`.
- The 4 adversarial fixtures overlap with the substrate's `Format Bypass Defense` gates; they may be consolidated per-adapter or via cross-cutting fixtures once the substrate's runtime enforcement body lands (Phase 1.9 hook per `crates/octo-storage/src/lib.rs`).

## Dependencies

- `0206-009-adapter-crate-creation` (adapter crates must exist before fixture suites can target them)

## Version History

| Version | Date       | Change                                       |
| ------- | ---------- | -------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing; 6 fixtures × 5 adapters = 30 |
| v1.1    | 2026-08-22 | Phase 3.7 close-out per long-horizon plan v1.5 §Mission layout. AC verification per memory card `mission-0206-010-per-adapter-fixtures-status.md`: LANDED 5a337323 (2026-08-20). 20 NEW fixture files × 14 tests = 70 cases per-adapter (drop_table_negative + namespace_guard + 4 adversarial per adapter per Format Bypass Defense). TV-A11 + A12 PASS. RFC-only YAML edits per R10.5 scope discipline. Mission file git mv open → claimed + status open → done. |