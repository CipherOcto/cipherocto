---
name: rfc-0855p-c-drift-fix-2026-09-03
description: RFC-0855p-c Doc-vs-Substrate drift-fix closed INLINE 2026-09-03
metadata:
  node_type: memory
  type: project
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  modified: 2026-09-03T23:00:00.000Z
---

RFC-0855p-c Doc-vs-Substrate drift-fix ALL CLOSED 2026-09-03 at `next` `205f1434`. 4 files / +244 / -219. Same single-change-set style as `8e788920` 0x0100 fix.

## Drift scope (3 fictional types, 0 substrate matches)

| RFC text                                                                                                                                                                          | Substrate reality                                                                                                                                                                                                                                                                                                                                 | Fix                                                                                                                                                                                                |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `enum DomainCoordinatorLifecycle` (§1 heading + L156 code)                                                                                                                        | `pub enum CoordinatorLifecycle` at `octo_coordinator_types::state.rs:72` (no Domain prefix; canonical)                                                                                                                                                                                                                                            | §1 heading renamed to `CoordinatorLifecycle (RFC-0855p-b reuse, platform-event-driven)`; canonical path added                                                                                      |
| `struct DomainCoordinatorRecord { base, mission_id, domain_id, group_jid, platform, platform_admin_id, last_platform_check_epoch, adapter_connected, record_hash }` (§2 L184-215) | 4/7 fields exist via Layer C `GroupBinding` (`mission_id`, `domain_id`, `group_jid`, `platform`) at `octo_network::dot::group_registry.rs:64-127`; 3/7 spec-only (`platform_admin_id`, `last_platform_check_epoch`, `adapter_connected`); canonical `CoordinatorRecord` at `octo_coordinator_types::state.rs:289`                                 | §2 rewritten as 3-layer split: Layer B `CoordinatorRecord` (RFC-0855p-b reuse unchanged) + Layer C `GroupBinding` + Layer D per-adapter local state surfaced via `PlatformEvent` envelopes per §5a |
| `enum DomainCoordinatorError` (§10 L580-588, 9 variants)                                                                                                                          | 0 matches; typed errors split across `octo-network` + per-adapter crates: `DcSlashError` (`dc/slash.rs:120`), `PlatformAdminAttestError` (`dc/admin_attest.rs:122`), `HandoverError` (`dot/handover.rs:1031`), `PlatformAdapterError` (`dot/error.rs:72`), `BindingError` (`dot/binding.rs:648`), `DotError::SignatureInvalid` (`dot/error.rs:8`) | §10 error table re-keyed: each cell `DomainCoordinatorError::Variant` → actual substrate path; Notes block layer-model rationale                                                                   |

## Files updated (4 files, 1 commit)

- `rfcs/accepted/networking/0855p-c-domain-coordinator-role.md` — §1 heading + §2 rewrite + §10 error table + §Adversary Analysis + IA-DC-13/14/16 + Roles-and-Authorities line 110 + Phase 1 + Integration Order step 5 + TV-1/TV-2/TV-5 + "RFC-0855p-b Integration" prose + Key Files to Modify table + Summary + Design Goals G1 + Version History v0.1.4
- `crates/octo-mesh/src/lib.rs:7` — module doc drift fix
- `crates/octo-mesh/src/trust_level.rs:21/25/38/45` — 4 doc-comment refs to fictional `DomainCoordinatorRecord` replaced with `GroupBinding` + `CoordinatorRecord` substrate signals + cross-ref to RFC-0855p-c §2
- `crates/octo-mesh/Cargo.toml:7` — description drift fix

## Verification

- `cargo fmt --all --check`: clean
- `cargo clippy -p octo-mesh --all-targets --all-features -- -D warnings`: clean
- `cargo test -p octo-mesh --lib`: 11/11 pass
- `bash scripts/validate_cites.sh rfcs/accepted/networking/0855p-c-domain-coordinator-role.md`: 147/147 VALID, 0 INVALID/STALE/PHANTOM
- `npx prettier --check rfcs/accepted/networking/0855p-c-domain-coordinator-role.md`: clean
- Pre-commit hook (Guard 2 §-cite validator): PASS
- Pre-existing clippy issue in `dot/subgroup_state.rs:230`: untouched (user-owned per [[feedback_initiation_user_only]])

## `DomainCoordinator` identifier that's REAL (kept in RFC)

- `pub enum CoordinatorRole { DomainCoordinator = 0x01, ... }` at `crates/octo-network/src/dot/handover.rs:266/276` — this is the canonical enum-variant identifier for the role, not the fictional struct.

## Layer-model rationale

Per CLAUDE.md §Architectural Principles §"Layer model" + §"Extension over enumeration":

- Layer B (years-stable, RFC-0855p-b): `CoordinatorRecord` + `CoordinatorLifecycle` + canonical `SlashReasonCode` enum
- Layer C (per-RFC, RFC-0855p-c + RFC-0850p-c): `GroupBinding` carries the (mission, domain, group, platform) tuple
- Layer D (per-adapter, RFC-0850p-a + RFC-0850ab-a + ...): adapter-local platform state, surfaced as `PlatformEvent` envelopes

No single struct spans Layer B+C+D — substrate correctly does NOT centralize, RFC was wrong to suggest a centralized `DomainCoordinatorRecord`. Fix brings RFC text in line with the layer model without touching the substrate (which was already correct).

## Combined with prior session

Two inline fixes for RFC-0855p-c now land as `8e788920` (0x0100 collision) + `205f1434` (drift-fix). Both follow the "in place, not separated amendments" user directive. Cross-RFC invariants preserved:

- `RecorderDid` canonical keying
- `HARD_THRESHOLD = 5` byte-identical
- `SlashReasonCode::Extension(0x0100)` for cross-domain slash
- §-cite hygiene (Guard 2 passes; Guard 1 §-letter phantom-token pre-existing across repo is a known false-positive)

**Why:** Doc-vs-Substrate drift closed INLINE per user "do it inplace" directive; 3 fictional types removed from RFC text; substrate correctness reaffirmed; layer model honored.

**How to apply:** User owns `git push origin next` + PR `next → main` for both fix commits. NO PUSH from agent.

Related: [[cipherocto-design-principles]] (§Layer model + §Extension over enumeration), [[rfc-0855p-c-4drift-mission-closure-2026-09-03]] (prior session), [[feedback_initiation_user_only]], [[git-workflow]], [[never-guess]], [[docs-audits-scratchpad]].
