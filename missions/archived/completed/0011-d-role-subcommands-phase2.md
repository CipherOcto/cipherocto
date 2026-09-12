---
name: 0011-d-role-subcommands-phase2
description: Implement `octo role select` Phase 2 (coordinator + domain-coordinator) per RFC-0011-d; gated on RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e Accepted
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-d author session
  created: 2026-08-31
  v: "1.3"
  completed: 2026-09-03
  release_gate:
    require: "RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e reach Accepted"
    released_version: TBD
  depends_on:
    - RFC-0011-d
    - mission 0011-d-role-subcommands-phase1
    - RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) (must be Accepted)
    - RFC-0855p-e (must be Accepted)
    - mission 0011-d-M10-phase2-coordinator-domain-coordinator
    - mission 0011-d-M11-phase2-domain-coordinator-platform-binding
status: Completed
---

# 0011-d-role-subcommands-phase2 — Role subcommands Phase 2 (select coordinator + domain-coordinator)

**Status:** Completed 2026-09-03 (aggregate) — Both Phase 2 atomic missions (M10 coordinator + domain-coordinator substrate + CLI, M11 domain-coordinator platform binding) closed at `missions/archived/completed/0011-d-M10-*.md` + `missions/archived/completed/0011-d-M11-*.md` per audit `docs/audits/2026-09-03-0011-d-M10-M11-mission-closeout.md`. Landing commit `96bccc4b` + drift-fix commit `c6f9ab6c` (10 drift findings closed INLINE per user direction `in place, not separated amendments`). R1 hygiene fix `c0e86dc8` (`parse_hash32_hex` canonical lowercase alphabet + 4 regression tests + cross-reference doc comments). Prereq RFCs (RFC-0855p-d + RFC-0855p-e) reached Accepted 2026-09-02 (gate CLEARED). Phase 2 fully closed.
**Substrate:** RFC-0011-d §Specification §7.2 `octo role select <role>` (Phase 2 row); §Implementation Phases Phase 2; §Compatibility partial-prereq caveat
**Parent:** RFC-0011-d
**Depends on:**

- Mission `0011-d-role-subcommands-phase1` — Phase 1 substrate (role registry + `[ADD]` substrate entrypoints + CLI dispatch + envelope types + 4 `OctoCliError` variants + HSM gate + filter parser)
- RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) — Sub-Domain / Sub-Group Nesting substrate (REQUIRED for `domain-coordinator`; flat-domain coordinator is also gated since the role binding requires the sub-group substrate to participate in mission-level handover)
- RFC-0855p-e — HandoverRequest Envelope & Coordinator Term Handover substrate (REQUIRED for both `coordinator` and `domain-coordinator` role bindings; the handover ceremony is the substrate for the role binding)

## Status

Claimed (2026-09-01) by @mmacedoeu — release-gated on RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e Accepted. Until both prereq RFCs reach Accepted, `role select coordinator` and `role select domain-coordinator` return exit 33 (`RoleNotSelectable` + prereq RFC names in error message) per RFC-0011-d §Implementation Phases Phase 2 + §Compatibility partial-prereq caveat + Appendix E Partial-prereq flow.

## Atomic Decomposition (claim units)

This phase-aggregate mission decomposes per RFC-0011-d §Mission Decomposition into 2 atomic claim units (GATED; do NOT claim until RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e reach Accepted):

| #   | Mission file                                               | Substrate scope                                                          | Gate                                                                                          |
| --- | ---------------------------------------------------------- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| M10 | `0011-d-M10-phase2-coordinator-domain-coordinator.md`      | `octo coordinator domain-coordinator {bind,unbind,list,show}` clap       | RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) + RFC-0855p-e Accepted |
| M11 | `0011-d-M11-phase2-domain-coordinator-platform-binding.md` | `octo_coordinator::{bind,unbind,list,show}_domain_coordinator` substrate | RFC-0855p-e Accepted                                                                          |

**Workflow:** Both atomics are GATED. Sub-step 1 of each mission is the gate verification (`git log` of Accepted RFC files). Do not claim M10/M11 until both prereq RFCs are Accepted. Per [[deferred-vs-unspecified]], deferred (not unspecified).

**Ordering:** M11 (substrate) should land BEFORE M10 (CLI binding) so M10 has substrate to call. Per RFC-0011 substrate-first pattern.

**Cross-reference:** Phase 1 atomics live under `0011-d-M1..M9-*.md` (unblocked). This aggregate remains Open as the Phase 2 summary; do not claim this aggregate — claim M10 + M11 atomics.

## Substrate (RFC-0011-d)

RFC-0011-d §7.2 Subcommand Taxonomy `octo role select <role>` table (Phase 2 row: `coordinator` and `domain-coordinator` are reserved for Phase 2); §7.4 Substrate `[ADD]` signatures (`octo_role::select` extended with `coordinator` + `domain-coordinator`); §7.5 Role Summary canonical role → role-token mapping (Phase 2 rows: `coordinator` uses OCTO-O; `domain-coordinator` uses OCTO-O); §Implementation Phases Phase 2; §Compatibility partial-prereq caveat; §Test Vectors Phase 2 (+3 vectors); Appendix E Partial-prereq flow.

## Parent

RFC-0011-d (process — `octo role` provisioning subcommands; Phase 2 follow-on per §Implementation Phases).

## Depends on

See YAML frontmatter `depends_on` block + `release_gate` block above. Hard sequencing: Phase 1 lands first (unblocked); Phase 2 lands when BOTH RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e reach Accepted.

## Acceptance Criteria

- [ ] `octo role select coordinator` implemented + unit-tested (TV-RC-1 per RFC-0011-d §Test Vectors — coordinator success post-0855p-e Accept)
- [ ] `octo role select domain-coordinator` implemented + unit-tested (TV-RDC-1 per RFC-0011-d §Test Vectors — domain-coordinator success post-0855p-d + 0855p-e Accept)
- [ ] Domain-coordinator binding updates `RFC-0855p-c` `DomainCoordinatorRecord.platform_admin_id` per RFC-0011-d §7.2 side effects row
- [ ] Partial-prereq guard ENFORCED until release_gate unblocks: `coordinator` + `domain-coordinator` return exit 33 + prereq RFC names in error message (TV-RP-1 carries forward from Phase 1 + TV-RDC-2 per RFC-0011-d §Test Vectors — domain-coordinator blocked on RFC-0855p-e only)
- [ ] Mission-level coordination surface wired (RFC-0855p-b `CoordinatorLifecycle` state machine; handover ceremony substrate)
- [ ] Sub-group nesting respected for `domain-coordinator` role binding (RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) sub-DC authority)
- [ ] HandoverRequest Envelope substrate wired into role binding flow (RFC-0855p-e handover ceremony)
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy -p octo-cli -p octo-role --all-targets -- -D warnings clean
- [ ] Cargo test -p octo-cli -p octo-role --lib --tests green
- [ ] No new INVALID cites introduced (cite validator runs clean per `docs/07-developers/octo-cli-implementation-guide.md` Guard 2)
- [ ] **release_gate verification** — verify BOTH RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e have reached Accepted status (per `git log` of `rfcs/accepted/` directory + VH table) BEFORE claiming this mission per [[feedback_no_guess_hard_check]]

### Type Coverage

| RFC-0011-d type                                       | Sub-step                          | Notes                                                                                                                                                                      |
| ----------------------------------------------------- | --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `OctoRoleBinding { role_id: "coordinator" }`          | Sub-step 1 (Phase 2 binding)      | Layer B; `role_kind_uuid` derives from RFC-0855-namespaced UUIDv5 of `urn:octo:role:0855:coordinator`; uses OCTO-O role token                                              |
| `OctoRoleBinding { role_id: "domain-coordinator" }`   | Sub-step 1 (Phase 2 binding)      | Layer B; `role_kind_uuid` derives from RFC-0855-namespaced UUIDv5 of `urn:octo:role:0855:domain-coordinator`; uses OCTO-O role token; requires sub-group nesting substrate |
| `RoleBinding.platform_admin_id`                       | Sub-step 2 (side effect)          | Layer B; updates `RFC-0855p-c` `DomainCoordinatorRecord.platform_admin_id` (RFC-0011-d §7.2 side effects row); mirror RFC-0855p-c platform-mediated handover pattern       |
| `RoleBinding.handover_envelope_hash`                  | Sub-step 3 (handover ceremony)    | Layer B; BLAKE3-256 of canonical HandoverRequest Envelope per RFC-0855p-e; substrate-truth reference for audit                                                             |
| `RoleError::RoleNotSelectable { reason }`             | Sub-step 4 (Phase 2 prereq block) | Layer B; CLI surfaces prereq RFC names verbatim (RFC-0011-d §Implementation Phases Phase 2 + §Compatibility partial-prereq caveat + Appendix E Partial-prereq flow)        |
| `OctoCliError::RoleNotSelectable { role_id, reason }` | Sub-step 5 (CLI error)            | Layer C; exit 33; reused from Phase 1; carries forward with new `reason` payloads                                                                                          |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Role Subcommands Phase 2 (companion guide amended per RFC-0011-d §Key Files to Modify + §Implementation Phases Phase 2). Rust snippets + clap wiring patterns mirror Phase 1 `role select` flow with two substrate-specific adaptations: (1) sub-group nesting dispatch (RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3)) and (2) HandoverRequest Envelope ceremony (RFC-0855p-e).

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Risk

- **HSM downgrade via env var manipulation** — same as Phase 1; mitigated by `InMemorySigner` gated to `OCTO_ENV == "development"` (RFC-0011-d §Security 1).
- **Cross-chain role replay** — same as Phase 1; mitigated by `chain_id: [u8; 32]` in binding envelope (RFC-0010 ChainId).
- **HandoverRequest Envelope signature replay** — NEW Phase 2 risk; the HandoverRequest Envelope substrate (RFC-0855p-e) MUST canonicalize the envelope bytes before signing; substrate owns canonical form per DCS pattern.
- **Sub-group nesting ambiguity** — NEW Phase 2 risk; `domain-coordinator` role binding must specify which sub-group it binds to; ambiguous binding surfaces `RoleError::RoleNotSelectable { reason: "ambiguous sub-group" }` (substrate-truth).
- **Mission-level coordination race** — NEW Phase 2 risk; multiple operators binding `coordinator` for the same mission concurrently could race; substrate uses RFC-0855p-b `CoordinatorLifecycle` state machine for arbitration; last-writer-wins per `(chain_id, operator_did)` (RFC-0011-d §Security 2; no `RoleBindingConflict` variant — substrate atomic); race window closed by `BEGIN IMMEDIATE` tx serialization (M4).
- **Pastejacking on `--role` argument** — same as Phase 1; mitigated by `--confirm-acknowledge` two-step gate.
- **Stake slashing risk undisclosed to operator** — same as Phase 1; mitigated by `--dry-run` showing slashing rules table.

## Notes

> **CRITICAL — Partial-prereq caveat (RFC-0011-d §Compatibility + §Implementation Phases + Appendix E):**
>
> This mission is **release-gated** on BOTH RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e reaching Accepted status. Until both prereq RFCs land, `role select coordinator` and `role select domain-coordinator` MUST return exit 33 (`RoleNotSelectable`) with the prereq RFC numbers named verbatim in the error message. The error message format is:
>
> - `RoleNotSelectable { role_id: "coordinator", reason: "Phase 2 requires RFC-0855p-e" }`
> - `RoleNotSelectable { role_id: "domain-coordinator", reason: "Phase 2 requires RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) and RFC-0855p-e" }`
>
> The substrate `octo_role::select` returns `RoleError::RoleNotSelectable` directly (substrate-truth per RFC-0011-d §7.4 + §Implementation Phases Phase 2 critical callout). The CLI's `RoleAction::Select` dispatch enforces the same gate; this is defense in depth (substrate-truth AND CLI-level pre-check).
>
> Operators see the prereq RFC names in the error message so they can track upstream RFC status. This is a UX requirement per RFC-0011-d §Compatibility partial-prereq caveat.
>
> **Verification per [[feedback_no_guess_hard_check]]:** Before claiming this mission, verify BOTH RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e have reached Accepted status (per `git log` of `rfcs/accepted/` directory + VH table). Substrate status ≠ RFC status; never assume.

- `RoleKind` discriminator for `coordinator` and `domain-coordinator` follows the same RFC-0855-namespaced UUIDv5 pattern as Phase 1 (`urn:octo:role:0855:coordinator` and `urn:octo:role:0855:domain-coordinator`); integration test asserts distinct UUIDs across all 9 role slugs (7 Phase 1 + 2 Phase 2).
- `domain-coordinator` role binding updates `RFC-0855p-c` `DomainCoordinatorRecord.platform_admin_id` per RFC-0011-d §7.2 side effects row; the platform_admin_id is the operator's DID (RFC-0009).
- Dual-stake sufficiency check for `coordinator` + `domain-coordinator` is substrate-authoritative; both require 100 OCTO-O role-token stake + 1,000 OCTO global stake (per RFC-0011-d §7.5 Role Summary Phase 2 rows).
- Mission-level coordination surface is wired via RFC-0855p-b `CoordinatorLifecycle` state machine + RFC-0855p-e HandoverRequest Envelope ceremony; the role binding IS the election witness for the coordinator role (per RFC-0855p-b + RFC-0855p-e).

## Scope

Extend `octo role select` for `coordinator` + `domain-coordinator` per RFC-0011-d §Implementation Phases Phase 2. OUT OF SCOPE: any new subcommand group; any new substrate crate beyond `[ADD]` extensions to `octo-role` + `octo-slash-ledger` + `octo-coordinator` + `octo-adapter-{whatsapp,matrix,telegram}` (per RFC-0855p-c platform-binding substrate).

## Sub-steps

1. **Verify release_gate** — verify BOTH RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e have reached Accepted status (per `git log` of `rfcs/accepted/` directory + VH table); cite the Accepted commits in the landing commit message.
2. **Phase 2 role binding scaffolding** — extend `RoleBinding` substrate type with `platform_admin_id: Option<Did>` + `handover_envelope_hash: Option<Hex32>`; extend `octo_role::select` to handle `coordinator` + `domain-coordinator` slugs (currently returns `RoleError::RoleNotSelectable`).
3. **Sub-group nesting dispatch** — wire RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) sub-DC authority into `octo_role::select` for `domain-coordinator`; substrate owns the sub-group ambiguity check (returns `RoleError::RoleNotSelectable { reason: "ambiguous sub-group" }`).
4. **HandoverRequest Envelope ceremony** — wire RFC-0855p-e HandoverRequest Envelope substrate into the role binding flow; canonicalize envelope bytes per DCS; sign via `CapabilitySigner`; persist `handover_envelope_hash` to `RoleBinding`.
5. **DomainCoordinator side effect** — on `domain-coordinator` select, update `RFC-0855p-c` `DomainCoordinatorRecord.platform_admin_id` per RFC-0011-d §7.2 side effects row; substrate owns the update (substrate-truth).
6. **Partial-prereq guard tests** — verify the partial-prereq guard (TV-RP-1 from Phase 1 + TV-RDC-2) is REPLACED by the success cases when release_gate unblocks; keep the guard tests as regression tests for the fallback path (defense in depth).
7. **Test vectors** — 3 additional test vectors per RFC-0011-d §Test Vectors Phase 2 (TV-RC-1 coordinator success post-0855p-e Accept; TV-RDC-1 domain-coordinator success post-0855p-d + 0855p-e Accept; TV-RDC-2 domain-coordinator blocked on RFC-0855p-e only); assert exit codes + envelope shape + redaction + sub-group dispatch + handover ceremony.
8. **Layer direction audit** — verify no reverse deps; CLI depends on substrate; substrate does NOT depend on CLI.
9. **Release gate clearance** — when both prereq RFCs reach Accepted, remove the partial-prereq guard from the success path; keep the guard tests as regression tests.

### Cargo deps

```toml
# crates/octo-cli/Cargo.toml (Layer C; Phase 2 extension)
# Existing deps from Phase 1:
# octo-role, octo-slash-ledger (already added in Phase 1)

# Phase 2 additions:
# Coordinator substrate (Layer B; RFC-0855p-b)
octo-coordinator = { path = "../octo-coordinator" }
# HandoverRequest Envelope substrate (Layer B; RFC-0855p-e; landed with RFC-0855p-e Accept)
octo-handover = { path = "../octo-handover" }
# Sub-group nesting substrate (Layer B; RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3); landed with RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) Accept)
octo-subgroup = { path = "../octo-subgroup" }
```

### Test Vectors (per RFC-0011-d §Test Vectors — Phase 2)

3 additional test vectors per RFC-0011-d §Test Vectors Phase 2 + carry-forward partial-prereq guard (TV-RP-1):

| Vector ID | Description                                                                                                                                                                                                       | Group                                             |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| TV-RC-1   | `role select coordinator --confirm --confirm-acknowledge` — success post-0855p-e Accept; HSM-signed; handover ceremony executed; `handover_envelope_hash` populated                                               | (Phase 2) Coordinator select                      |
| TV-RDC-1  | `role select domain-coordinator --confirm --confirm-acknowledge` — success post-0855p-d + 0855p-e Accept; HSM-signed; sub-group dispatch + handover ceremony; `DomainCoordinatorRecord.platform_admin_id` updated | (Phase 2) Domain-coordinator select               |
| TV-RDC-2  | `role select domain-coordinator` with 0855p-d Accepted but 0855p-e still Draft — exit 33; error names prereq (RFC-0855p-e)                                                                                        | (Phase 2) Domain-coordinator select               |
| TV-RP-1   | `role select coordinator` (Phase 2 blocked) — exit 33; error names prereq (RFC-0855p-e)                                                                                                                           | Partial-prereq guard (carry-forward from Phase 1) |

### Layer direction (per [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — Phase 2 dispatch extension (no new envelopes; reuses `RoleSelectOutput`); HSM gate + redaction pass reused from Phase 1
- `octo-role` (Layer B) — `RoleBinding` extended with `platform_admin_id` + `handover_envelope_hash`; `octo_role::select` extended with `coordinator` + `domain-coordinator` slugs
- `octo-coordinator` (Layer B; RFC-0855p-b) — `CoordinatorLifecycle` state machine integration
- `octo-handover` (Layer B; RFC-0855p-e) — HandoverRequest Envelope ceremony substrate
- `octo-subgroup` (Layer B; RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3)) — sub-group nesting dispatch
- `octo-adapter-{whatsapp,matrix,telegram}` (Layer B; RFC-0855p-c) — `DomainCoordinatorRecord.platform_admin_id` update on `domain-coordinator` select

The CLI depends on Layer-B substrate crates; substrate crates do NOT depend on the CLI. No reverse deps.

## Validation

```bash
cargo fmt --all -- --check   # clean
cargo clippy -p octo-cli -p octo-role -p octo-coordinator -p octo-handover -p octo-subgroup --all-targets -- -D warnings  # clean
cargo test -p octo-cli -p octo-role -p octo-coordinator -p octo-handover -p octo-subgroup --lib --tests  # green
```

## Backward compat

- Additive only: Phase 2 extends Phase 1 substrate types (`RoleBinding` gains `platform_admin_id` + `handover_envelope_hash`); no breaking changes to existing API per RFC migration etiquette.
- CLI exit codes: 31-34 (Phase 1) reused for Phase 2 partial-prereq block; exit 33 (`RoleNotSelectable`) carries the prereq reason payload.
- Output schema: `RoleSelectOutput` extended with `platform_admin_id` + `handover_envelope_hash` as `Option<...>` fields with `#[serde(skip_serializing_if = "Option::is_none")]`; no `schema_version` bump for additive fields.
- Partial-prereq guard: until release_gate unblocks, `coordinator` + `domain-coordinator` continue to return exit 33 with prereq RFC names (RFC-0011-d §Implementation Phases Phase 2 critical callout).

## Cross-references

- RFC-0011-d §7.2 Subcommand Taxonomy `octo role select <role>` table (Phase 2 row)
- RFC-0011-d §7.4 Substrate `[ADD]` signatures (`octo_role::select` Phase 2 extension)
- RFC-0011-d §7.5 Role Summary canonical role → role-token mapping (Phase 2 rows: `coordinator`, `domain-coordinator` use OCTO-O)
- RFC-0011-d §Implementation Phases Phase 2 (gated on RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) + RFC-0855p-e Accepted)
- RFC-0011-d §Compatibility partial-prereq caveat (Draft RFCs flagged)
- RFC-0011-d §Test Vectors Phase 2 (+3 vectors: coordinator success, domain-coordinator success, domain-coordinator blocked on RFC-0855p-e only)
- RFC-0011-d §Error Handling exit-code table (exit 33 reused for partial-prereq block)
- RFC-0011-d Appendix E Partial-prereq flow (sequence diagram)
- RFC-0011-d §Security Considerations (1-6) + §Adversarial Review (carry-forward + Phase 2 additions)
- RFC-0011 — `octo` CLI substrate (parent amendment chain host)
- RFC-0900 — AI Quota Marketplace (slash ledger substrate; first-offense + escalation slashing model)
- RFC-0855 — Mission Overlay Networks (role namespace; dual-stake model; participant flag bits)
- RFC-0855p-b — Mission Coordinator Lifecycle (slash tally; lifecycle states inherited by DomainCoordinator specialization)
- RFC-0855p-c — DomainCoordinator Role (physical-platform binding authority; platform-mediated handover pattern)
- RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) — Sub-Domain / Sub-Group Nesting (Draft; Phase 2 prereq)
- RFC-0855p-e — HandoverRequest Envelope & Coordinator Term Handover (Draft; Phase 2 prereq)
- RFC-0009 — Identity Management (DID derivation for the role-binding signature)
- RFC-0008 — Deterministic AI Execution Boundary (execution class mapping; role commands are class C)
- RFC-0010 — Canonical DID Codec (ChainId typing for slash ledger partition)
- RFC-0957 — Macaroon Substrate (signer trait cross-reference)
- [[cipherocto-design-principles]] — Layer B stability contract; no central enum; typed discriminator

## Why 1 release cycle gate

Per the `release_gate` block in YAML frontmatter, this mission is release-gated on BOTH RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3) AND RFC-0855p-e reaching Accepted status. The release cycle gate exists because:

1. **Substrate dependency** — `coordinator` + `domain-coordinator` role bindings require the HandoverRequest Envelope substrate (RFC-0855p-e) for the handover ceremony. The envelope canonical form + signature ceremony are NOT defined elsewhere; the role binding cannot compile without the substrate.
2. **Sub-group nesting dependency** — `domain-coordinator` role binding requires sub-group nesting authority (RFC-0855p-d (INDEX; chain: RFC-0855p-d1 + RFC-0855p-d2 + RFC-0855p-d3)). Flat-domain coordinator binding only covers flat domains; the DomainCoordinator specialization requires the sub-group substrate to participate in mission-level handover.
3. **Amendment chain rationale (RFC-0011-d §Why Phase 1 / Phase 2 split)** — folding Phase 2 into RFC-0011-d would force the amendment to wait for both Draft RFCs to reach Accepted — which is precisely the dependency chain the amendment chain pattern is designed to break. Phase 1 (RFC-0011-d) lands unblocked; Phase 2 (this mission) lands when the substrate RFCs mature.
4. **Verification protocol per [[feedback_no_guess_hard_check]]** — before claiming this mission, verify BOTH prereq RFCs have reached Accepted status (per `git log` of `rfcs/accepted/` directory + VH table). Substrate status ≠ RFC status; never assume. The mission frontmatter's `release_gate.require` field is the canonical gate reference.
5. **Partial-prereq guard until release_gate unblocks** — until both prereq RFCs reach Accepted, `coordinator` + `domain-coordinator` subcommands continue to return exit 33 (`RoleNotSelectable`) with the prereq RFC numbers named verbatim in the error message (RFC-0011-d §Implementation Phases Phase 2 + §Compatibility partial-prereq caveat + Appendix E Partial-prereq flow). The partial-prereq guard is the operator UX for the gate state.

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-01)
