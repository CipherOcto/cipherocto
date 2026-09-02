---
name: 0855p-e-coordinator-types-shared-crate
description: Extract shared coordinator types into new `octo-coordinator-types` crate per RFC-0855p-e §Layer-C Substrate Types follow-on note (SlashTallyUpdate + SlashReasonCode + HandoverReasonTypeId lift from octo-network local Layer-C types to shared Layer-B types usable by both RFC-0855p-b + RFC-0855p-e).
metadata:
  node_type: substrate-shared-crate
  type: substrate-extraction
  originSessionId: RFC-0855p-e follow-on extraction session
  created: 2026-09-02
  v: "1.0"
  depends_on:
    - RFC-0855p-e
    - RFC-0855p-b
    - mission 0855p-e-handover-envelope-substrate
status: Open
---

# 0855p-e-coordinator-types-shared-crate — Extract `octo-coordinator-types` shared crate per RFC-0855p-e §Layer-C Substrate Types

**Status:** Open (follow-on post-acceptance mission)
**Substrate:** New crate `octo-coordinator-types` (Layer B; shared between `octo-network` and `octo-coordinator` future consumers)
**Parent:** RFC-0855p-e (Accepted 2026-09-02 at commit `0e915618`; §Layer-C Substrate Types follow-on note)
**Depends on:** RFC-0855p-e Accepted; mission `0855p-e-handover-envelope-substrate` (substrate-truth baseline required before extraction)

## Status

Open (2026-09-02) per RFC-0855p-e §Layer-C Substrate Types follow-on note: "until the shared `octo-coordinator-types` crate lands (post-acceptance mission). When that crate extracts, both `SlashTallyUpdate` and `SlashReasonCode` move to the shared crate and `0855p-e` re-imports them as Layer-B types, mirroring the [RFC-0855p-b] behavior." Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] + [[implementation-workflow-hook]].

## Substrate (new crate `octo-coordinator-types`)

Per RFC-0855p-e §Layer-C Substrate Types (defined locally in 0855p-e) follow-on note:

**Types to extract from `octo-network/src/dot/handover.rs` local Layer-C surface:**

- `SlashTallyUpdate` — slash tally update event (currently local in handover.rs; lift to shared)
- `SlashReasonCode` — `#[non_exhaustive]` enum (currently local; lift to shared; carry 0x0013-0x0016 entries per RFC-0855p-e §Future Work F-7)
- `HandoverReasonTypeId` — typed-discriminator (currently local; lift to shared)

**Re-import pattern (post-extraction):**

- `octo-network` (handover.rs) → `use octo_coordinator_types::{SlashTallyUpdate, SlashReasonCode, HandoverReasonTypeId}` (Layer-B consumer)
- `octo-network` (slash tally substrate per RFC-0855p-b at `crates/octo-network/src/dot/slash.rs`) → re-exports `SlashReasonCode` from `octo-coordinator-types` (RFC-0855p-b slash tally references `HandoverReasonTypeId` for `SubDCVoluntaryResignation` slash reason mapping per RFC-0855p-e §Layer-C Substrate Types follow-on; substrate-truth: confirm import site via `grep -n "HandoverReasonTypeId" crates/octo-network/src/dot/slash.rs`)
- Future `octo-coordinator` crate (if/when it lands) → re-exports from `octo-coordinator-types`

## Parent

RFC-0855p-e (Accepted 2026-09-02 at commit `0e915618`); §Layer-C Substrate Types + §Future Work F-7 explicit follow-on note.

## Depends on

See YAML frontmatter `depends_on` block. Hard sequencing:

1. `mission 0855p-e-handover-envelope-substrate` (substrate-truth baseline required; e substrate reconciled to v1.3 spec)
2. This mission lands SECOND (extraction + re-import)

## Acceptance Criteria

- [ ] New crate `crates/octo-coordinator-types/` created
- [ ] `Cargo.toml` registered in workspace
- [ ] `crates/octo-coordinator-types/src/lib.rs` exports `SlashTallyUpdate` + `SlashReasonCode` + `HandoverReasonTypeId` as Layer-B types
- [ ] `SlashReasonCode` is `#[non_exhaustive]` per §Extension over enumeration
- [ ] `SlashReasonCode` carries 0x0013-0x0016 entries per RFC-0855p-e §Future Work F-7 (`FalseAttestation` / `QuorumTimeout` / `TallyTamper` / `LateDelivery`)
- [ ] `HandoverReasonTypeId` follows typed-discriminator pattern (UUID or 128-bit tag with RFC-allocated namespace) per §Extension over enumeration (typed-discriminator over central enum)
- [ ] `crates/octo-network/src/dot/handover.rs` re-imports the 3 types from `octo_coordinator_types` (no local definitions remain)
- [ ] `crates/octo-network/Cargo.toml` declares `octo-coordinator-types` dependency (Layer B)
- [ ] RFC-0855p-b substrate (`crates/octo-network/src/dot/slash.rs`) re-exports from `octo-coordinator-types` (NOT duplicates)
- [ ] Layer direction verified (Layer B shared crate; Layer C consumers re-import; no upward dependency)
- [ ] `cargo test -p octo-coordinator-types` zero failures
- [ ] `cargo test -p octo-network handover slash` zero failures (after re-import)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean

## Sub-steps

1. **VERIFY GATE** — RFC-0855p-e Accepted; mission `0855p-e-handover-envelope-substrate` substrate landed
2. Create `crates/octo-coordinator-types/` directory + `Cargo.toml` (workspace registration)
3. Create `crates/octo-coordinator-types/src/lib.rs` skeleton
4. Define `SlashTallyUpdate` + `SlashReasonCode` + `HandoverReasonTypeId` as Layer-B shared types
5. Update `crates/octo-network/src/dot/handover.rs` to re-import from `octo_coordinator_types`
6. Remove local definitions of the 3 types from `handover.rs`
7. Update `crates/octo-network/Cargo.toml` to declare `octo-coordinator-types` dep
8. Update `crates/octo-network/src/slash/` (RFC-0855p-b substrate) to re-export from `octo_coordinator_types`
9. Add `cargo test -p octo-coordinator-types` test vectors
10. Verify `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --all -- --check`

## Test Vectors (per RFC-0855p-e + RFC-0855p-b §Test Vectors)

- TV-CT-1: `SlashTallyUpdate` round-trip serialization matches handover.rs baseline
- TV-CT-2: `SlashReasonCode` 0x0013-0x0016 entries present; `#[non_exhaustive]` exhaustiveness preserved
- TV-CT-3: `HandoverReasonTypeId` typed-discriminator pattern preserved across extraction
- TV-CT-4: RFC-0855p-b substrate recipients reading `SlashReasonCode` see 0x0013-0x0016 as valid (NOT unallocated)

## Layer direction (per [[cipherocto-design-principles]])

- `octo-coordinator-types` (Layer B) — shared coordinator types
- `octo-network` (Layer C) — consumer; re-imports from `octo-coordinator-types`
- Future `octo-coordinator` (Layer C) — consumer; re-imports from `octo-coordinator-types`

No upward dependency. No Layer C crate owns the types (canonical home is the shared Layer-B crate).

## Backward compat

Net-additive: new crate + new dep. `octo-network` re-imports the 3 types (no API change for downstream consumers; type path changes from local to `octo_coordinator_types::`). If `octo-network` `pub use` re-exports are updated, downstream consumers see the type at the new path.

## Risk

- **Type-path change breaks downstream**: external crates importing `octo_network::dot::handover::SlashReasonCode` break. Mitigation: `pub use` re-export in `octo-network` at the old path that forwards to `octo_coordinator_types::SlashReasonCode`; document path change in CHANGELOG.
- **Duplicate definitions during migration**: leaving local defs in `handover.rs` while shared crate is created causes type-identity mismatch. Mitigation: atomic migration (single commit; no half-state).
- **Layer direction violation**: defining `SlashTallyUpdate` as Layer C in the shared crate violates §Layer placement. Mitigation: enforce Layer B (no I/O, no business logic; pure types + canonical encoding).

## Notes

- Per RFC-0855p-e §Future Work F-7 BLOCKING ACCEPTANCE GATE: "this RFC MUST NOT be promoted to Accepted until EITHER (a) RFC-0855p-b §B amendment merges with the four `SlashReasonCode` entries above, OR (b) `SlashReasonCode` + `HandoverReasonTypeId` lift into a shared `octo-coordinator-types` crate shipping in the same release." Substrate-truth check: `crates/octo-network/src/dot/slash.rs:86` reserves `0x0013..0xFFFF` (entries NOT allocated); 4 entries have NOT landed in 0855p-b substrate as of commit `0e915618`. RFC-0855p-e promotion proceeded via option (b) deferred path — the §Future Work F-7 gate was scheduled to land via THIS mission. This mission implements option (b): extracts the 3 types AND allocates 0x0013-0x0016 in `SlashReasonCode` per RFC-0855p-e §Layer placement constants block (FalseAttestation / QuorumTimeout / TallyTamper / LateDelivery; 4 entries specified in 0855p-e §Layer placement, NOT yet materialized in 0855p-b substrate slash enum).
- Per `[[cipherocto-design-principles]]` §Stable Abstractions Principle + §No parallel abstractions, the shared crate is the canonical home; both 0855p-b and 0855p-e re-import from it.

## Cross-references

- RFC-0855p-e (§Layer-C Substrate Types + §Future Work F-7; this mission's canonical spec)
- RFC-0855p-b (slash tally substrate; re-exports from shared crate)
- Mission `0855p-e-handover-envelope-substrate` (prerequisite; substrate-truth baseline)
- `docs/audits/2026-09-02-rfc-0855p-de-review-dry.md` (DRY closure audit; cross-RFC invariant consolidation)

## Claimant

(none — Open mission)
