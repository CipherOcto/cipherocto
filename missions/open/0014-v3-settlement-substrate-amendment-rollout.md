---
name: 0014-v3-settlement-substrate-amendment-rollout
description: RFC-0014-v3 Phase 4 acceptance rollout — workspace Display redaction regression + SinkSpecific cap monitoring + DOMAIN adapter conformance
metadata:
  node_type: substrate-faithful-consumer
  type: post-acceptance-rollout
  originSessionId: RFC-0014-v3 promotion session (2026-09-12)
  created: 2026-09-12
  v: "1.0"
  pair: 2-Cycle Atomic Promotion
  depends_on:
    - RFC-0014-v3
    - RFC-0012-v3
status: Open
---

> **§2-Cycle naming note:** RFC-side heading is `## 2-Cycle Atomic Promotion Tag` (the canonical marker per BLUEPRINT.md). Mission-side heading is `## 2-Cycle Atomic Promotion gate` (the consuming gate per BLUEPRINT.md §Mission Lifecycle). Both refer to the same gate; the rename disambiguates "what the RFC carries" from "what the mission enforces".

# 0014-v3-settlement-substrate-amendment-rollout — workspace-wide adoption for RFC-0014-v3

**Status:** Open — post-acceptance rollout acceptance suite
**Substrate:** RFC-0014-v3 §S5.1 (scrubber) + §S5.2 (`SettlementHashOpaque`) + §S5.3 (SinkSpecific payload cap posture) + §S5.1.1 (DOMAIN adapter contract)
**Parent:** RFC-0014 + RFC-0014-v2 (substrate extensions)
**Companion:** RFC-0012-v3 (paired-acceptance — see §2-Cycle Atomic Promotion Tag)

## 2-Cycle Atomic Promotion gate

This mission is one of TWO paired-acceptance missions for the v3 amendment round; see §2-Cycle Atomic Promotion Tag at `rfcs/accepted/process/0014-v3-settlement-substrate-amendment.md` + `rfcs/accepted/process/0012-v3-audit-substrate-amendment.md`. **Both RFCs must remain Accepted for this mission to remain claimable.** If either drops back to Draft, this mission MUST defer (user-initiated only per BLUEPRINT.md §Mission Lifecycle Deferral procedure).

**§2-Cycle gate deviation note (user-acknowledged rationale):** BLUEPRINT.md §Mission Lifecycle §2-Cycle Atomic Promotion gate item #2 mandates "A single mission YAML MUST own both promotions" of a 2-cycle pair. This mission + sister mission `0012-v3-audit-substrate-amendment-rollout` is a deliberate sister-mission split per `RFC-0012-v3 + RFC-0014-v3 multi-round DRY CLOSED 2026-09-12` precedent (RFC-0012/0013/0014 missions DRY CLOSED 2026-09-10: 9 NEW missions, 1 per RFC, with §2-Cycle gate metadata). Rationale: each rollout touches a distinct Layer B façade (`octo-audit` vs `octo-settlement`); consolidating into one mission would force cross-façade coupling for a rollout that is fundamentally audit-side vs settlement-side. The `pair: 2-Cycle Atomic Promotion` metadata block + cross-sibling `depends_on:` entries satisfy the gate's pairing invariant per item #5 (cite validation surfaces both RFC numbers).

**Sister mission:** `0012-v3-audit-substrate-amendment-rollout`.

## Scope

Per RFC-0014-v3 §Implementation Phases Phases 1 + 2 + 3 are **DONE** at substrate-code level (commits `f33410ce` + `934242ce` + RFC promotion `2cd12a9e`). 132 tests PASS across the 5 affected crates (octo-audit 18, octo-audit-core 6, octo-settlement 13, octo-settlement-core 6, quota-router-sm-engine 89 — see sister-mission `0012-v3-audit-substrate-amendment-rollout` §Scope for the audit-side enumeration). This mission covers **Acceptance Rollout (post-RFC-promotion; no Phase 4 stub in parent RFC §Implementation Phases)** — workspace-wide verification that the substrate amendment posture (Display redaction + hash accessor surface + substrate-faithful SinkSpecific cap) survives every consumer path.

### Deliverables

1. **Workspace-wide Display redaction regression suite** — `crates/octo-settlement/tests/workspace_redaction_regression.rs` exercises every public Display-emitting path reachable from `octo-settlement` types (`SettlementError` 8-variant shadow at quota-router-sm-engine + canonical substrate `SettlementError`). Asserts no Display output contains raw 32-byte hashes or hex bytes that the substrate's redaction policy would have redacted. Coverage: octo-settlement (13 tests), octo-settlement-core (6 tests), quota-router-sm-engine shadow enum (8 Display variants × Display-derived tests).
2. **`SettlementHashOpaque` accessor round-trip suite** — verify every consumer site that constructs `SettlementHashOpaque::new([u8; 32])` is paired with an `as_bytes()` consumer (or `Clone`/`PartialEq`). Audit grep `quota-router-sm-engine/src/store.rs` + `settlement_event.rs`; assert no raw `.0` field access bypasses accessor.
3. **SinkSpecific payload monitoring infra** — substrate-faithful = no cap at substrate (per §S5.3 + AC-11); cap lives at scrubber. Add workspace-level log-capture test that exercises `SettlementError::SinkSpecific("x".repeat(10_000))` through DOMAIN adapter wrapper and asserts scrubber-side cap fires (sentinel `<redacted-too-long>` present). Production-side: NO runtime enforcement added (would violate substrate-faithful); only verification of cap-at-scrubber posture.
4. **DOMAIN adapter contract conformance (settlement-side)** — every `crates/quota-router-sm-engine/**/*.rs::format!("{e}")` and `.to_string()` chain in workspace flagged + scrubbed via `scrub_adapter_error_with(s, ADAPTER_TYPES)`. Verify all 24 sites from R48-s defect 1b closure remain scrubbed; add 4 bare `scrub_adapter_error(` callsites to a registry assertion test (no-registry entry points for adapter types outside DOMAIN registry).
5. **Parent RFC `RFC-0014` cross-reference update** — append §Cross-References note linking to RFC-0014-v2 + RFC-0014-v3 + RFC-0012-v3 (paired). Adds §Substrate-Faithful Amendment Trail table to parent RFC.
6. **§FW4 clippy lint prelude (settlement-side)** — extend the workspace clippy rule from sister mission to also flag raw `.to_string()` chains on `SettlementError` in DOMAIN-boundary modules (gated off-by-default per RFC-0014-v3 §FW4). Sister-mission audit-side lint lives at `RFC-0012-v3 §FW2`. The audit-side and settlement-side lints detect SEMANTICALLY DIFFERENT patterns (`format!("{e}")` vs `e.to_string()`) and require two distinct lint registrations in the shared registry — the layer model is Layer E per-extension crate (`octo-clippy-extensions`) hosting both registrations, per CLAUDE.md §User extensibility Registry pattern.

### Out of scope (per RFC §Future Work)

- **§FW1 cross-RFC scrubber shared utility** (extract to `octo-foundation::scrub`) — DEFERRED to v2.1+; out of scope.
- Substrate-level payload cap on `SinkSpecific(String)` — **DEFERRED — lands at acceptance** per §S5.3 + AC-11. Mission captures acceptance behavior; runtime cap enforcement is a separate mission (when/if 4 KiB scrubber cap proves insufficient).
- Any new substrate amendments (v3.x+) — out of scope.

### Acceptance criteria

- [ ] AC-1: `cargo build -p octo-settlement-core -p octo-settlement -p quota-router-sm-engine` succeeds with zero warnings
- [ ] AC-2: `cargo build --workspace` succeeds (no regression)
- [ ] AC-3: `cargo test -p octo-settlement --lib` passes (existing 13 tests stay green)
- [ ] AC-4: `cargo test -p octo-settlement-core --lib` passes (existing 6 tests stay green)
- [ ] AC-5: `cargo test -p quota-router-sm-engine --lib` passes (existing 89 tests stay green)
- [ ] AC-6: `cargo test -p octo-audit --lib` passes (paired — sister mission)
- [ ] AC-7: NEW `crates/octo-settlement/tests/workspace_redaction_regression.rs` PASSES (≥20 tests covering 8-variant settlement shadow enum + canonical substrate variants)
- [ ] AC-8: NEW `crates/octo-settlement/tests/hash_opaque_accessor_round_trip.rs` PASSES (asserts every consumer of `SettlementHashOpaque` uses accessor pattern; 0 `.0` field accesses)
- [ ] AC-9: NEW `crates/octo-settlement/tests/sink_specific_payload_cap_at_scrubber.rs` PASSES (asserts scrubber-side cap fires on 10K-char `SinkSpecific` payload; substrate Display remains verbatim)
- [ ] AC-10: NEW `crates/octo-settlement/tests/domain_adapter_contract_registry.rs` PASSES (asserts every flagged `.to_string()` site in workspace `quota-router-sm-engine/**` + `octo-settlement/**` modules gets scrubbed)
- [ ] AC-11: RFC-0014 §Cross-References table appended with v2 + v3 sibling refs
- [ ] AC-12: RFC-0014 §Substrate-Faithful Amendment Trail table added (per Deliverable 5; enumerates v2 + v3 amendment round + substrate commit refs)
- [ ] AC-13: Cite sweep clean for any RFC parent updates
- [ ] AC-14: Prettier-clean on all new + edited files
- [ ] AC-15: §2-Cycle gate sanity: sister mission `0012-v3-audit-substrate-amendment-rollout` still `Open` in `missions/open/`; if it deferred, this mission defers too (user-initiated only)

### Dependencies

- **RFC-0014-v3** — Accepted (canonical `## Status` body header + front-matter `Status` row both declare Accepted). Mission is claimable iff this RFC remains Accepted.
- **RFC-0012-v3** — Accepted (paired). Required for the §2-Cycle gate. Sister mission `0012-v3-audit-substrate-amendment-rollout` covers substrate roll-out on the audit side.
- **RFC-0014-v2 §FW6** — Canonical scrubber pattern list (single source of truth for §S5.1 per the heading `### §FW6 — Canonical Scrubber Patterns`).
- **RFC-0012-v2 §FW6** — Cross-RFC consensus-invariance scrubber patterns (substrate-side companion at `### §FW6 — Cross-RFC consensus-invariance scrubber patterns`). NOT the canonical pattern list (audit-side depends on settlement-side canonical, NOT vice versa).
- **parent RFC-0014** (`rfcs/accepted/process/0014-settlement-substrate.md`) — needs `D. Cross-references` table appended (AC-11).

### Risk

- **LOW** — Substrate code is shipped + tests pass (132 PASS); this is a rollout verification + monitoring + cross-RFC refactor. No new schema, no new variant.
- **LOW** — Sister-mission §FW2 (audit-side) clippy lint is gated off by default; settlement-side §FW4 extension reuses same lint module.
- **LOW** — RFC-0014 cross-reference append is doc-only edit.
- **LOW** — SinkSpecific payload-cap-at-scrubber is already enforced per R48-s defect 3 closure; this mission VERIFIES the posture (not enforcement).

### Cross-RFC invariants preserved

- `SettlementHashOpaque` Display emits `<redacted-hash>` (symmetric with Debug per RFC-0014-v3 §Security Considerations).
- `as_bytes()` retains raw `[u8; 32]` for programmatic chain-integrity verification.
- `SinkSpecific(String)` substrate-faithful posture preserved — no `debug_assert!` discipline at substrate. Cap at scrubber (4 KiB input + 4 KiB output).
- Scrubber Pattern 1-5 + 5b-5e + 6 registry unchanged from RFC-0014-v3 §S5.1.
- DOMAIN adapter contract: every raw `.to_string()` chain in `quota-router-sm-engine/**/*.rs` MUST go through `scrub_adapter_error_with(s, ADAPTER_TYPES)` (Pattern 6 registry).
- 132 test surface stays PASS (no regression).

### Test vectors (mission-level)

| ID                                                | Scenario                                                                                                                                                                                                                                                                            | Expected                                                                                                                                                            |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `settle-display-redaction-shadow-each-variant`    | `format!("{}", err)` for every shadow `SettlementError` variant at `crates/quota-router-sm-engine/src/lib.rs` (8 variants: AskNotFound, AlreadyConsumed, InvalidTransition, ReservationNotFound, ReservationExpired, InvalidReservationTransition, SettlementHashMismatch, Storage) | no raw `[u8; 32]` / hex bytes leaked; sentinel `<redacted-hash>` present where applicable; Storage variant wrapped via scrubber sentinels                           |
| `settle-display-redaction-substrate-each-variant` | `format!("{}", err)` for every canonical `SettlementError` variant at `crates/octo-settlement-core/src/error.rs` (7 variants: AskNotFound, AlreadyConsumed, InvalidTransition, SequenceGap, ChainIntegrity, AlreadyExists, SinkSpecific)                                            | no raw `[u8; 32]` / hex bytes leaked; sentinel `<redacted-hash>` present where applicable; SinkSpecific payload cap-at-scrubber sentinel fires for > 4 KiB payloads |
| `settle-debug-symmetric-redaction`                | `format!("{:?}", SettlementHashOpaque::new([0xab; 32]))`                                                                                                                                                                                                                            | `SettlementHashOpaque(<redacted-hash>)` (symmetric with Display per RFC-0014-v3 §Security Considerations)                                                           |
| `settle-accessor-round-trip`                      | `SettlementHashOpaque::new(h).as_bytes() == &h` for h = [0;32], [0xff;32], mixed                                                                                                                                                                                                    | returns raw `[u8; 32]` slice verbatim                                                                                                                               |
| `settle-no-raw-zero-field-access`                 | grep `SettlementHashOpaque\.0\b` in `crates/quota-router-sm-engine/**/*.rs`                                                                                                                                                                                                         | 0 matches (only `as_bytes()` + `new()` accessors)                                                                                                                   |
| `settle-domain-adapter-registry-conformance`      | Every `.to_string()` site in quota-router-sm-engine `format!` chains                                                                                                                                                                                                                | every flagged site paired with `scrub_adapter_error_with` or `scrub_adapter_error`                                                                                  |
| `settle-sinkspecific-cap-at-scrubber`             | `SettlementError::SinkSpecific("x".repeat(10_000))::to_string()` (substrate-faithful = no cap)                                                                                                                                                                                      | full 10K chars emitted at substrate; wrapped via scrubber → `<redacted-too-long>` sentinel                                                                          |
| `settle-scrubber-pattern-coverage`                | Patterns 1, 2, 3, 4, 5, 5b, 5c, 5d, 5e, 6                                                                                                                                                                                                                                           | every pattern produces expected sentinel; existing 13 octo-settlement scrubber tests stay PASS                                                                      |
