# RFC-0011-n: `octo network` Phase 6 — Closure Artifacts (Bootstrap + Status Deferred)

## Status

Draft (2026-09-20) — RFC-0011-n lands RFC-0011-h §Implementation Phases Phase 6. Two DEFERRED subcommands (`bootstrap` + `status`) land via companion G1 substrate (lands in Phase 2). Three additional substrate missions (G18 writer-election + G20 NetworkSender trait + G1 closure cascade) plus drift-closure mission + 2 follow-on companion missions + final closure artifacts. Layer discipline preserved: zero Layer A change.

> **Amendment chain:** Sixth and final amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per `.gitignore` line 46). Phase 1 = RFC-0011-i (DRY CLOSED). Phase 2 = RFC-0011-j (DRY CLOSED). Phase 3 = RFC-0011-k (DRY CLOSED). Phase 4 = RFC-0011-l (DRY CLOSED). Phase 5 = RFC-0011-m (DRY CLOSED). Phase 6 = RFC-0011-n (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-n lands the **closure artifacts** slice of RFC-0011-h §Implementation Phases. Two DEFERRED CLI subcommands + 2-3 substrate missions + drift-closure + 2 follow-on companions + final closure artifacts:

| Subcommand                            | Authority Role        | Substrate                                                       | Companion mission                                       |
| ------------------------------------- | --------------------- | --------------------------------------------------------------- | ------------------------------------------------------- |
| `octo network bootstrap`              | Bootstrap Authority   | `BootstrapOrchestrator::start_bootstrap(BootstrapConfig)`       | `0011-h-s-a-bootstrap-orchestrator` (G1 — Phase 2)      |
| `octo network status`                 | Bootstrap Authority   | `BootstrapOrchestrator::status() → BootstrapState`              | `0011-h-s-a-bootstrap-orchestrator` (G1 — Phase 2)      |

Plus closure artifacts:

| Artifact                                              | Substrate / Mission YAML                                       |
| ----------------------------------------------------- | -------------------------------------------------------------- |
| `0011-h-s-a-writer-election` (G18)                    | `elect_coordinator` + helpers at `octo-coordinator-types/src/election.rs` |
| `0011-h-s-a-network-sender` (G20)                     | new `NetworkSender` trait + `SendContext` per RFC-0863; per-extension crate pattern |
| Drift-closure mission for `0011-h-drift-0851p-a-seed-health-check` | TBD (drift identified during 6-phase rollout)              |
| 2 follow-on companion missions                        | Created at CLAIMED-time per [[no-phantom-mission-pointers]]   |

**Layer discipline preserved:** zero Layer A change (Layer A frozen contracts per [[cipherocto-design-principles]]). CLI dispatch lands Layer C; substrate additions in this RFC = **2-3 companion missions (Layer B)**; 0 of 2 subcommands has substrate present today (both require companion G1 from Phase 2 + G18 + G20).

## Dependencies

- **RFC-0011-h §Implementation Phases Phase 6** — canonical scope
- **RFC-0011-h §Subcommand Taxonomy** rows for `bootstrap`, `status` (DEFERRED per user decision 2026-09-17; landed in Phase 6 closure)
- **RFC-0011-h §Error Handling** row 89 (slot 89 = `NetworkSubstrateUnavailable`, REUSED; no NEW variants in Phase 6 since `bootstrap` + `status` are deferred clap-arm-not-registered pattern with exit 2 pre-companion)
- **RFC-0011-i (Phase 1, DRY CLOSED)** — hard sequencing dependency
- **RFC-0011-j (Phase 2, DRY CLOSED)** — hard sequencing dependency; G1 `BootstrapOrchestrator` companion lands in Phase 2 and is reused by Phase 6
- **RFC-0011-k (Phase 3, DRY CLOSED)** — hard sequencing dependency
- **RFC-0011-l (Phase 4, DRY CLOSED)** — hard sequencing dependency
- **RFC-0011-m (Phase 5, DRY CLOSED)** — hard sequencing dependency
- **RFC-0011-f (mesh peer-table)** — interim substitute cited by RFC-0011-h row 97-98 for operators needing `bootstrap` / `status` BEFORE Phase 6 closure
- **RFC-0851p-a §3 Mode A** — `BootstrapMode` + `BootstrapOrchestrator` substrate anchors
- **RFC-0862p-a Writer Election Bootstrap** — `elect_coordinator` + helpers substrate anchors
- **RFC-0863 Onion Relay** — `NetworkSender` trait + `SendContext` substrate anchors
- **Companion mission `0011-h-s-a-bootstrap-orchestrator` (G1, Phase 2)** — Layer B substrate for `BootstrapOrchestrator::start_bootstrap` + `status()` (already-claimed in Phase 2; Phase 6 consumes it)
- **Companion mission `0011-h-s-a-writer-election` (G18)** — Layer B substrate for `elect_coordinator` + helpers
- **Companion mission `0011-h-s-a-network-sender` (G20)** — Layer B substrate for `NetworkSender` trait + `SendContext`

## Design Goals

1. **Substrate-first ordering** — companion substrate missions (G1 from Phase 2 + G18 + G20) land BEFORE CLI dispatch per [[no-phantom-mission-pointers]] pairing invariant. Pre-companion, CLI dispatch surfaces exit 2 (clap `UnrecognizedSubcommand`) per DEFERRED-clap-arm-not-registered pattern (RFC-0011-h row 91 footnote pattern a); post-companion, dispatch routes to substrate.
2. **Substrate-faithfulness** — no parallel abstractions, no CLI-side substrate shadow. CLI translates substrate return values 1:1 to JSON envelopes per [[cipherocto-design-principles]] §No premature coupling.
3. **Slot arithmetic preserved** — Phase 6 lands 0 NEW OctoCliError variants; reuses slot 89 (substrate-absent) from Phase 2. Final slot arithmetic: 6 of 10 + slot 91 pre-allocated filled across Phases 1-6 (slots 79, 82, 83, 84, 85, 86, 87, 88, 89 = 9 variants; slot 90 = `NetworkCIDenyDefault` deferred per RFC-0011-h row 543 footnote pending G25 + user decision).
4. **Layer discipline preserved** — zero Layer A change; Layer B substrate = 3 companion missions (G1/G18/G20); Layer C CLI dispatch = 2 subcommand arms + final closure artifacts.
5. **Test vector coverage** — 6 test vectors (3 for `bootstrap` + 3 for `status`) per RFC-0011-h §Implementation Phases Phase 6.
6. **Closure artifact completeness** — drift-closure mission + 2 follow-on companion missions + final audit doc + memory card + MEMORY.md index entry all land in this phase per `0011-h-multiphase-rollout-plan` §2.6.

## Motivation

Phase 1-5 land read-only observability + bootstrap lifecycle + slash reputation + coordinator visibility + bind envelope payload builders + discovery visibility. Phase 6 lands the **DEFERRED `bootstrap` + `status` subcommands** that close the gap from `0011-deprecation-stub-removal` 2026-09-17 (which removed `octo init` / `octo join` / `octo status` with user-accepted deferral of replacements).

Without Phase 6, operators calling `octo init` / `octo join` / `octo status` post-v2.0 cut hit clap `unrecognized subcommand` (exit 2). Phase 6 closes that gap by landing the replacements (`octo network bootstrap` + `octo network status`) wired through RFC-0011-h §Subcommand Taxonomy.

## Roles and Authorities

Per RFC-0011-h §Role/Authority Coverage Table:

| Subcommand                  | Authority Role        | Confirmation axes                                            |
| --------------------------- | --------------------- | ------------------------------------------------------------ |
| `bootstrap`                 | Bootstrap Authority   | (no confirmation flags; substrate-managed)                    |
| `status`                    | Bootstrap Authority   | (read-only; substrate-managed)                                |

Both subcommands are managed entirely by `BootstrapOrchestrator` substrate; no CLI-side confirmation flags per RFC-0011-h row 97-98 (DEFERRED rows lack CLI-side confirmation; substrate handles the state machine).

## Specification

### System Architecture

Phase 6 architecture: Layer C CLI dispatch → Layer B substrate (BootstrapOrchestrator from G1 Phase 2 + writer-election G18 + NetworkSender G20) → Layer A frozen contracts.

```mermaid
graph TD
    CLI["octo-cli Layer C<br/>bootstrap<br/>status"]
    DISPATCH["commands::network::dispatch(...)"]
    SUBSTRATE["octo-network Layer B<br/>BootstrapOrchestrator (G1 Phase 2 companion)<br/>BootstrapOrchestrator::start_bootstrap + status (G1)<br/>WriterElection::elect_coordinator (G18 companion)<br/>NetworkSender trait + SendContext (G20 companion)"]
    FROZEN["Layer A frozen no change<br/>blake3 hash + canonical encoding"]
    CLI --> DISPATCH
    DISPATCH --> SUBSTRATE
    SUBSTRATE --> FROZEN
```

Per [[cipherocto-design-principles]] §Stable Abstractions Principle, Layer A is unchanged. Per §No premature coupling, CLI does not reach into substrate internals — only into public substrate functions.

### Binary Surface

Phase 6 adds 2 new sub-actions to the existing `network` arm of `Commands` enum (closing the DEFERRED gap from `0011-deprecation-stub-removal`):

- `bootstrap` (no confirmation flags; substrate-managed)
- `status` (read-only; substrate-managed)

### Subcommand Taxonomy

Per RFC-0011-h §Subcommand Taxonomy Phase 6 rows:

| Subcommand                  | Existing substrate                                              | Companion mission required                |
| --------------------------- | --------------------------------------------------------------- | ----------------------------------------- |
| `bootstrap`                 | (none — BootstrapOrchestrator missing)                           | G1 (Phase 2): `BootstrapOrchestrator::start_bootstrap` |
| `status`                    | (none — BootstrapOrchestrator missing)                           | G1 (Phase 2): `BootstrapOrchestrator::status()` |

### Output Envelope

Phase 6 lands 2 output envelopes, one per subcommand:

| Subcommand                  | Output envelope                                |
| --------------------------- | ---------------------------------------------- |
| `bootstrap`                 | `NetworkBootstrapOutput`                       |
| `status`                    | `NetworkStatusOutput`                          |

`NetworkStatusOutput` aggregates: bootstrap mode + authority + peers count + trust-graph node count + slash reputation aggregate + governance tally summary + bind envelope summary + discovery cache summary + drift flags. Substrate-faithful to `BootstrapOrchestrator::status()` return type.

### Error Handling

Phase 6 lands 0 NEW OctoCliError variants. The DEFERRED-clap-arm-not-registered pattern means:

- Pre-companion (G1 not landed): exit 2 (clap `UnrecognizedSubcommand`)
- Pre-companion (G1 landed but clap arm not registered in CLI binary surface): exit 2
- Post-companion (clap arm registered): exit 89 if substrate absent for `status` accessor (gated on G18) or exit 0 if substrate present

The Phase 6 closure is the final step that registers the clap arms per RFC-0011-h row 141-142.

### Exit Codes

Phase 6 uses slot 89 (REUSED) + exit 2 (clap). No new slots allocated.

## Performance Targets

Per RFC-0011-h §Performance Targets Phase 6 rows:

| Subcommand                  | Target    | Rationale                               |
| --------------------------- | --------- | --------------------------------------- |
| `bootstrap` wall-clock      | <500ms    | Bootstrap lifecycle orchestration       |
| `status` wall-clock         | <200ms    | Aggregate state query                    |

## Implicit Assumptions Audit

Per RFC-0011-h §Implicit Assumptions Audit Phase 6 rows:

| Assumption                                                 | Affected subcommands                | Fallback                                                |
| ---------------------------------------------------------- | ----------------------------------- | ------------------------------------------------------- |
| `BootstrapOrchestrator` lands (G1 Phase 2)                | `bootstrap`, `status`               | exit 2 (clap `UnrecognizedSubcommand`) pre-Phase 6 closure |
| `WriterElection::elect_coordinator` lands (G18)           | `status` (writer-election state)    | exit 89 `NetworkSubstrateUnavailable` (gated on G18)    |
| `NetworkSender` trait + `SendContext` lands (G20)         | `status` (network sender state)     | exit 89 `NetworkSubstrateUnavailable` (gated on G20)    |

## Security Considerations

Per RFC-0011-h §Security Considerations Phase 6 rows:

- **`bootstrap` is high-blast-radius.** Substrate `BootstrapOrchestrator::start_bootstrap` handles the lifecycle; CLI delegates without confirmation flags per RFC-0011-h row 97 (no CLI-side confirmation; substrate-managed).
- **`status` is read-only.** No write paths.
- **Drift-closure mission.** Drift identified during 6-phase rollout is closed in Phase 6 closure.

## Adversarial Review

Per RFC-0011-h §Adversarial Review Phase 6 rows:

| Threat                                                          | Severity | Mitigation                                                |
| --------------------------------------------------------------- | -------- | --------------------------------------------------------- |
| Operator calls `octo init` / `octo join` / `octo status` post-v2.0 cut | LOW | Replaced by `octo network bootstrap` + `octo network status` in Phase 6 |
| Drift in `0011-h-drift-0851p-a-seed-health-check` identified during rollout | LOW | Drift-closure mission lands in Phase 6 closure |
| `bootstrap` lifecycle race                                       | HIGH     | Substrate `BootstrapOrchestrator` state machine handles concurrency |

## Compatibility

Phase 6 lands additively. No existing CLI subcommand changes. No NEW OctoCliError variants (reuses slot 89 + exit 2 clap). Closes the gap from `0011-deprecation-stub-removal` 2026-09-17.

## Test Vectors

6 test vectors total per RFC-0011-h §Test Vectors Phase 6:

| ID | Subcommand                  | Scenario                                                |
| -- | --------------------------- | ------------------------------------------------------- |
| tv_net6_1 | `bootstrap`             | `BootstrapOrchestrator::start_bootstrap` success        |
| tv_net6_2 | `bootstrap`             | pre-G1 → exit 2 (clap `UnrecognizedSubcommand`)         |
| tv_net6_3 | `bootstrap`             | `BootstrapOrchestrator::start_bootstrap` failure (substrate error) |
| tv_net6_4 | `status`                | All substrate layers present → aggregate surfaced       |
| tv_net6_5 | `status`                | Pre-Phase 6 closure → exit 2                            |
| tv_net6_6 | `status`                | Drift-closure flag set → surfaced in `drift_flags` field |

## Alternatives Considered

1. **Defer `bootstrap` + `status` indefinitely.** Rejected; user decision 2026-09-17 captured in `0011-deprecation-stub-removal` §Out of Scope explicitly identifies Phase 6 closure as the resolution. Operators currently hit exit 2 post-v2.0 cut.
2. **Land `bootstrap` + `status` in Phase 1 or Phase 2.** Rejected; RFC-0011-h §Implementation Phases ordering explicitly defers both to Phase 6 closure. Phase 1 (read-only observability) and Phase 2 (bootstrap lifecycle) are upstream substrate landings; Phase 6 wires the CLI surface to the substrate.
3. **Skip drift-closure mission.** Rejected; per `0011-h-multiphase-rollout-plan` §2.6, drift-closure mission lands in Phase 6 as part of closure artifacts.

## Substrate-Additions Companion Missions

Per [[no-phantom-mission-pointers]] pairing invariant, this RFC cites 3 companion substrate missions + 1 drift-closure mission + 2 follow-on companions.

| Companion mission                                       | Substrate addition                                                  | Layer |
| ------------------------------------------------------- | ------------------------------------------------------------------- | ----- |
| `0011-h-s-a-bootstrap-orchestrator` (G1, Phase 2)      | `BootstrapOrchestrator::start_bootstrap` + `status()`               | B     |
| `0011-h-s-a-writer-election` (G18)                      | `WriterElection::elect_coordinator` + helpers at `octo-coordinator-types/src/election.rs` | B     |
| `0011-h-s-a-network-sender` (G20)                       | `NetworkSender` trait + `SendContext` per RFC-0863; per-extension crate pattern | B     |
| `0011-h-drift-0851p-a-seed-health-check` (drift-closure) | TBD (drift identified during 6-phase rollout)                       | B     |
| 2 follow-on companion missions                          | Created at CLAIMED-time per [[no-phantom-mission-pointers]]         | B     |

Phase 6 RFC carries 5-6 substrate additions total (3 named + 1 drift-closure + 2 follow-on); 0 substrate additions in this RFC itself (all substrate additions land via companion missions per substrate-first ordering).

## Implementation Phases

Per `0011-h-multiphase-rollout-plan` §2.6:

1. **Substrate-first slice (5-6 companion missions):** G1 (Phase 2 reuse) → G18 → G20 → drift-closure → 2 follow-on companions land per companion mission YAMLs.
2. **CLI dispatch slice:** 2 subcommand arms + 2 output envelopes + 6 test vectors land AFTER all companion missions close per substrate-first ordering.
3. **Closure artifacts:** final audit doc + memory card + MEMORY.md index entry + mission YAML archive transitions for all 6 phases per [[feedback_initiation_user_only]] workflow.

User-gated decision on slice ordering per [[feedback_initiation_user_only]].

## Key Files to Modify

| File                                                                            | Action                        |
| ------------------------------------------------------------------------------- | ----------------------------- |
| `crates/octo-cli/src/main.rs::Commands::Network`                                | ADD 2 subcommand arms (`bootstrap`, `status`) |
| `crates/octo-cli/src/commands/network.rs`                                       | ADD 2 dispatch fns + envelopes |
| `crates/octo-network/src/mon/bootstrap.rs` (companion G1 Phase 2)               | Layer B substrate addition (REUSED from Phase 2; NOT new in Phase 6) |
| `crates/octo-coordinator-types/src/election.rs` (companion G18)                  | Layer B substrate addition (NOT this RFC; companion mission) |
| `crates/octo-network/src/sender/` (companion G20)                                | Layer B substrate addition (NOT this RFC; companion mission; per-extension crate pattern) |
| `rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md`                    | Status header amendment for full RFC-0011-h closure (post-Phase 6 IMPLEMENTATION) |

## Future Work

- **RFC-0011-i through RFC-0011-m promotions** (Draft → Accepted per phase, sequenced with substrate landings)
- **Phase 1-6 IMPLEMENTATION sequencing** per substrate-first ordering across all 6 phases
- **drift-0851p-a-seed-health-check** drift-closure mission
- **2 follow-on companion missions** (YAMLs created at CLAIMED-time)

## Rationale

Phase 6 lands the **closure artifacts** for the 6-phase rollout. It closes the `0011-deprecation-stub-removal` gap from 2026-09-17 by wiring `octo network bootstrap` + `octo network status` replacements, lands 3 additional substrate missions (G1 from Phase 2 reuse + G18 + G20), and produces the final closure artifacts (drift-closure + 2 follow-on + audit + memory card + MEMORY.md index).

Substrate-first ordering preserves [[cipherocto-design-principles]] §Stable Abstractions Principle: Layer B (substrate) lands BEFORE Layer C (CLI dispatch), so CLI never depends on a phantom substrate path.

## Version History

| Version | Date       | Notes                                                         |
| ------- | ---------- | ------------------------------------------------------------- |
| v0.1    | 2026-09-20 | Initial draft; pending R1 of 5-len DRY CLOSURE cycle          |
