---
name: 0012-v3-audit-substrate-amendment-rollout
description: RFC-0012-v3 Phase 4 acceptance rollout — workspace Display redaction regression + DOMAIN adapter conformance + §FW2 clippy lint prelude
metadata:
  node_type: substrate-faithful-consumer
  type: post-acceptance-rollout
  originSessionId: RFC-0012-v3 promotion session (2026-09-12)
  created: 2026-09-12
  v: "1.0"
  pair: 2-Cycle Atomic Promotion
  depends_on:
    - RFC-0012-v3
    - RFC-0014-v3
status: Open
---

> **§2-Cycle naming note:** RFC-side heading is `## 2-Cycle Atomic Promotion Tag` (the canonical marker per BLUEPRINT.md). Mission-side heading is `## 2-Cycle Atomic Promotion gate` (the consuming gate per BLUEPRINT.md §Mission Lifecycle). Both refer to the same gate; the rename disambiguates "what the RFC carries" from "what the mission enforces".

# 0012-v3-audit-substrate-amendment-rollout — workspace-wide adoption for RFC-0012-v3

**Status:** Open — post-acceptance rollout acceptance suite
**Substrate:** RFC-0012-v3 §S5.1 (scrubber) + §S6.2 (`TimestampOpaque`) + §S5.1.1 (DOMAIN adapter migration contract)
**Parent:** RFC-0012 + RFC-0012-v2 (substrate extensions)
**Companion:** RFC-0014-v3 (paired-acceptance — see §2-Cycle Atomic Promotion Tag)

## 2-Cycle Atomic Promotion gate

This mission is one of TWO paired-acceptance missions for the v3 amendment round; see §2-Cycle Atomic Promotion Tag at `rfcs/accepted/process/0012-v3-audit-substrate-amendment.md` + `rfcs/accepted/process/0014-v3-settlement-substrate-amendment.md`. **Both RFCs must remain Accepted for this mission to remain claimable.** If either drops back to Draft, this mission MUST defer (user-initiated only per BLUEPRINT.md §Mission Lifecycle Deferral procedure).

**§2-Cycle gate deviation note (user-acknowledged rationale):** BLUEPRINT.md §Mission Lifecycle §2-Cycle Atomic Promotion gate item #2 mandates "A single mission YAML MUST own both promotions" of a 2-cycle pair. This mission + sister mission `0014-v3-settlement-substrate-amendment-rollout` is a deliberate sister-mission split per `RFC-0012-v3 + RFC-0014-v3 multi-round DRY CLOSED 2026-09-12` precedent (RFC-0012/0013/0014 missions DRY CLOSED 2026-09-10: 9 NEW missions, 1 per RFC, with §2-Cycle gate metadata). Rationale: each rollout touches a distinct Layer B façade (`octo-audit` vs `octo-settlement`); consolidating into one mission would force cross-façade coupling for a rollout that is fundamentally audit-side vs settlement-side. The `pair: 2-Cycle Atomic Promotion` metadata block + cross-sibling `depends_on:` entries satisfy the gate's pairing invariant per item #5 (cite validation surfaces both RFC numbers).

**Sister mission:** `0014-v3-settlement-substrate-amendment-rollout`.

## Scope

Per RFC-0012-v3 §Implementation Phases Phases 1 + 2 + 3 are **DONE** at substrate-code level (commits `f33410ce` + `934242ce` + RFC promotion `2cd12a9e`). 132 tests PASS across the 5 affected crates (octo-audit 18, octo-audit-core 6, octo-settlement 13, octo-settlement-core 6, quota-router-sm-engine 89 — see sister-mission `0014-v3-settlement-substrate-amendment-rollout` §Scope for the settlement-side enumeration). This mission covers **Acceptance Rollout (post-RFC-promotion; no Phase 4 stub in parent RFC §Implementation Phases)** — workspace-wide verification that the substrate amendment posture (Display redaction + accessor surface) survives every consumer path the amendment does not own.

### Deliverables

1. **Workspace-wide Display redaction regression suite** — `crates/octo-audit/tests/workspace_redaction_regression.rs` exercises every public Display-emitting path reachable from `octo-audit` types (`AuditError`, `AuditChainError`, `AuditEvent`, per-variant Display). Asserts no Display output contains raw timestamp / hash / hex digest bytes that the substrate's redaction policy would have redacted. Coverage: octo-audit (18 tests already), octo-audit-core (6 tests), every DOMAIN consumer of `format!("{e}")` or `tracing::error!("{}", err)`.
2. **`TimestampOpaque` accessor round-trip suite** — verify every consumer site that constructs `TimestampOpaque::new(u64)` is paired with an `as_millis_unix()` consumer for programmatic access. Audit grep `verify_chain` + any test pattern; assert NO consumer uses raw `.0` field access (would bypass accessor).
3. **DOMAIN adapter contract conformance** — every `crates/*/src/storage/*.rs::format!` chain in workspace flagged via grep + added to a registry assertion test (the test asserts every flagged site is paired with `scrub_adapter_error_with`). New sites from outside `octo-audit` must opt in to the registry.
4. **§FW2 clippy lint prelude** — adds a workspace-level `clippy.toml` rule + a custom lint module `crates/octo-clippy-extensions/src/no_raw_format_err.rs` (NEW Layer E per-extension crate per CLAUDE.md §User extensibility — LintModuleRegistry pattern, NOT inside `octo-audit` which is Layer B façade). Gates on workspace feature flag `lint-no-raw-format-err` (default off per RFC-0012-v3 §FW2, v3.x activation deferred). The crate follows the per-extension registry pattern so future sibling lints (settlement-side `no_raw_to_string` per sister-mission Deliverable 6) register without forcing cross-façade coupling.
5. **Parent RFC `RFC-0012` cross-reference update** — append §Cross-References note linking to RFC-0012-v2 + RFC-0012-v3 + RFC-0014-v3 (paired). Adds §Substrate-Faithful Amendment Trail table parent RFC.
6. **Test surface delta** — additions land in `octo-audit/tests/`, `octo-audit-core/tests/`, `octo-clippy-extensions/tests/` (NEW Layer E per-extension crate). New tests must be additive — NO regressions in the 132 tests already PASS.

### Out of scope (per RFC §Future Work)

- **§FW1 cross-RFC scrubber shared utility** (extract to `octo-foundation::scrub`) — DEFERRED to v2.1+; out of scope for this mission. Flag for next-round.
- Any new substrate amendments (v3.x+) — out of scope.

### Acceptance criteria

- [ ] AC-1: `cargo build -p octo-audit-core -p octo-audit` succeeds with zero warnings
- [ ] AC-2: `cargo build --workspace` succeeds (no regression)
- [ ] AC-3: `cargo test -p octo-audit --lib` passes (existing 18 tests stay green)
- [ ] AC-4: `cargo test -p octo-audit-core --lib` passes (existing 6 tests stay green)
- [ ] AC-5: `cargo test -p octo-settlement --lib` passes (paired — sister mission)
- [ ] AC-6: `cargo test -p quota-router-sm-engine --lib` passes (paired — sister mission; 89 tests stay green)
- [ ] AC-7: NEW `crates/octo-audit/tests/workspace_redaction_regression.rs` PASSES (≥20 tests covering each Display variant + DOMAIN adapter call site)
- [ ] AC-8: NEW `crates/octo-audit/tests/timestamp_opaque_accessor_round_trip.rs` PASSES (asserts every `verify_chain` consumer uses accessor pattern)
- [ ] AC-9: NEW `crates/octo-audit/tests/domain_adapter_contract_registry.rs` PASSES (asserts every flagged `format!("{e}")` site in workspace `storage/*.rs` modules gets scrubbed)
- [ ] AC-10: NEW `crates/octo-clippy-extensions/src/no_raw_format_err.rs` PLUS `clippy.toml` rule registered (gated on `--features lint-no-raw-format-err`, default off). Compiles.
- [ ] AC-11: RFC-0012 §Cross-References table appended with v2 + v3 sibling refs
- [ ] AC-12: Cite sweep clean for any RFC parent updates (`scripts/validate_cites.sh <parent-rfc-path>` returns 0 PHANTOM / 0 INVALID / 0 STALE)
- [ ] AC-13: Prettier-clean on all new + edited files
- [ ] AC-14: §2-Cycle gate sanity: sister mission `0014-v3-settlement-substrate-amendment-rollout` still `Open` in `missions/open/`; if it deferred, this mission defers too (user-initiated only)

### Dependencies

- **RFC-0012-v3** — Accepted (status header verified at `rfcs/accepted/process/0012-v3-audit-substrate-amendment.md` Status line 5: `Accepted`). Mission is claimable iff this RFC remains Accepted.
- **RFC-0014-v3** — Accepted (paired). Required for the §2-Cycle gate. Sister mission `0014-v3-settlement-substrate-amendment-rollout` covers substrate roll-out on the settlement side.
- **RFC-0012-v2 §FW6** — Cross-RFC consensus-invariance scrubber patterns (substrate-side companion at `### §FW6 — Cross-RFC consensus-invariance scrubber patterns`). NOT the canonical pattern list — settlement-side owns the canonical list (audit-side depends on settlement-side canonical per RFC-0014-v2 §FW6 single-source-of-truth contract).
- **RFC-0014-v2 §FW6** — Canonical scrubber pattern list (single source of truth for §S5.1 per the heading `### §FW6 — Canonical Scrubber Patterns`).
- **parent RFC-0012** (`rfcs/accepted/process/0012-audit-substrate.md`) — needs `D. Cross-references` table appended (AC-11).

### Risk

- **LOW** — Substrate code is shipped + tests pass (132 PASS); this is a rollout verification + lint-prelude + cross-RFC refactor. No new schema, no new variant.
- **LOW** — §FW2 clippy lint is gated off by default; adds compile-time only, no runtime regression.
- **LOW** — RFC-0012 cross-reference append is doc-only edit.

### Cross-RFC invariants preserved

- `TimestampOpaque` Display emits `<redacted-timestamp>` (symmetric with Debug per RFC-0012-v3 §Security Considerations).
- `as_millis_unix()` retains raw u64 for programmatic chain-integrity verification.
- Scrubber Pattern 1-5 + 5b-5e + 6 registry unchanged from RFC-0012-v3 §S5.1.
- DOMAIN adapter contract: every raw `format!("{e}")` chain in `storage/*.rs` MUST go through `scrub_adapter_error_with(s, ADAPTER_TYPES)` (Pattern 6 registry).
- 132 test surface stays PASS (no regression).

### Test vectors (mission-level)

| ID                                          | Scenario                                                                                    | Expected                                                                                         |
| ------------------------------------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `audit-display-redaction-each-variant`      | `format!("{}", err)` for every `AuditError` variant + every `AuditChainError` variant       | no raw `u64` / `hex` / `[u8; 32]` bytes leaked; sentinel `<redacted-*>` present where applicable |
| `audit-debug-symmetric-redaction`           | `format!("{:?}", TimestampOpaque::new(123))`                                                | `<redacted-timestamp>` (symmetric with Display per RFC-0012-v3 §Security Considerations)         |
| `audit-accessor-round-trip`                 | `TimestampOpaque::new(t).as_millis_unix() == t` for t ∈ {0, 1, u64::MAX, 1_700_000_000_000} | returns raw u64 verbatim                                                                         |
| `audit-no-raw-zero-field-access`            | grep `\.0\b` on `TimestampOpaque` consumers in `crates/octo-audit-core/**/*.rs`             | 0 matches (only `as_millis_unix()` + `new()` accessors)                                          |
| `audit-domain-adapter-registry-conformance` | Every `format!("{e}")` chain in `crates/*/src/storage/*.rs` modules                         | every flagged site wrapped via `scrub_adapter_error_with`                                        |
| `audit-scrubber-pattern-coverage`           | Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e, 6                                                   | every pattern produces expected sentinel; existing 14 octo-audit scrubber tests stay PASS        |
| `audit-fw2-lint-prelude-compiles`           | `cargo build -p octo-audit --features lint-no-raw-format-err`                               | success; lint module compiles, gate is off-by-default                                            |
