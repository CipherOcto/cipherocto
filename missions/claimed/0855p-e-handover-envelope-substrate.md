---
name: 0855p-e-handover-envelope-substrate
description: Complete + reconcile handover envelope substrate per RFC-0855p-e (HORQ/HOAK/HODN/HORC + state machine + race resolution + second-witness quorum gate + slash tally carry-over) in `crates/octo-network/src/dot/handover.rs`.
metadata:
  node_type: substrate-network
  type: substrate-completion
  originSessionId: RFC-0855p-e author session
  created: 2026-09-02
  v: "1.0"
  depends_on:
    - RFC-0855p-e
    - RFC-0855p-b
    - RFC-0855p-c
    - RFC-0853
    - RFC-0009
    - RFC-0008
    - RFC-0850p-c
status: Claimed
---

# 0855p-e-handover-envelope-substrate — Handover Envelope Substrate Completion per RFC-0855p-e

**Status:** Open
**Substrate:** RFC-0855p-e (HandoverRequest Envelope & Mission Coordinator Term Handover)
**Parent:** RFC-0855p-e (Accepted 2026-09-02 at commit `0e915618`)
**Depends on:** RFC-0855p-e Accepted; RFC-0855p-b (Mission Coordinator Lifecycle; slash tally observability + Slash Offense Codes §B + CoordinatorLifecycle 8-state machine); RFC-0855p-c §4 (DomainCoordinator platform-mediated handover; EXCLUDED scope per RFC-0855p-e Summary)

## Status

Open (2026-09-02) per RFC-0855p-e promotion to Accepted at commit `0e915618`. Substrate PRE-EXISTS at `crates/octo-network/src/dot/handover.rs` (1045 lines; pre-v1.3 baseline). This mission is COMPLETION + RECONCILIATION to v1.3 spec, not from-scratch creation. Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] + [[implementation-workflow-hook]].

## Substrate (RFC-0855p-e)

Per RFC-0855p-e §Data Structure + §Specification + §Security + §Layer-C Substrate Types:

**`crates/octo-network/src/dot/handover.rs` (exists; reconcile to v1.3):**

- `HandoverEnvelope` + `HandoverPayload` + `HandoverRequestEnvelope` (HORQ subtype) + `HandoverAckEnvelope` (HOAK subtype) + `HandoverDoneEnvelope` (HODN subtype) + `HandoverCancelEnvelope` (HORC subtype)
- `HandoverReason` + `CoordinatorRole` + `SenderStateSnapshotOrdinal` (private + new() ctor + InvalidOrdinal type; NOT `pub u8` per plateau closure `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`)
- `SlashTally` + `SlashEvent` + `SlashTallyUpdate` + `SlashReasonCode` (local Layer-C types; scheduled to lift into shared `octo-coordinator-types` crate per RFC-0855p-e §Layer-C Substrate Types follow-on)
- `HORC` (HandoverCancel) envelope (`pub const HANDOVER_REQUEST_CANCEL: [u8; 4] = *b"HORC"` per RFC-0855p-e line 424; `pub struct HandoverCancelEnvelope` per RFC-0855p-e line 434) — incumbent lockout rule
- `HandoverReasonTypeId` typed-discriminator
- BLAKE3-keyed lex tiebreak for race resolution
- State machine integration (`Handover` state added; `HandoverComplete` removed)
- Quorum + race resolution + `sender_state_snapshot` verification
- Incumbent-HORQ-in-flight lockout (per §Security Considerations)
- `slash_tally_hash` reference at `current_epoch` (no lookback) + witness quorum validator (Phase 3)

**Module-level split (follow-on refactor; not this mission):**

- `handover_state.rs` — state machine
- `coordinator_handover.rs` — envelope dispatch + quorum
- `slash_tally_carryover.rs` — slash tally carry-over

**Constants (RFC-0855p-e §Layer placement):**

- `HANDOVER_RACE_WINDOW = 5` + `HORQ_BACKWARD_WINDOW = 5` + `HANDOVER_FORWARD_SKEW_BOOST = 0` (currently UNUSED; reserved per RFC-0855p-e §Layer placement constants block; values from RFC-0855p-e §Layer placement table)
- `MAX_PENDING_ENVELOPES_PER_HODN = 1024` (per RFC-0855p-e line 296, 659; bound on pending envelope count per HODN)
- `MAX_FSKEW_EPOCHS = 4` (`pub use` from RFC-0855p-d1)
- `HANDOVER_REQUEST_TAG = b"HORQ"` + `HANDOVER_ACK_TAG = b"HOAK"` + `HANDOVER_DONE_TAG = b"HODN"` + `HANDOVER_REQUEST_CANCEL = b"HORC"` (RFC-0855p-e line 424)
- `HORQ_CONTEXT = "DOT/1/HANDOVER_REQUEST"`, `HOAK_CONTEXT = "DOT/1/HANDOVER_ACK"`, `HODN_CONTEXT = "DOT/1/HANDOVER_DONE"`, `HORC_CONTEXT = "DOT/1/HANDOVER_CANCEL"` (BLAKE3 domain strings per RFC-0853)
- `MESH_AGGREGATED_SIGNATURE` context

## Parent

RFC-0855p-e (single RFC; not split — Mission Coordinator term handover is one coherent concern; explicitly excludes DomainCoordinator per RFC-0855p-c §4 platform-mediated path).

## Depends on

See YAML frontmatter `depends_on` block. Hard dependencies:

- RFC-0855p-e Accepted (gate met at commit `0e915618`)
- RFC-0855p-b (slash tally + Slash Offense Codes §B + CoordinatorLifecycle 8-state machine) — substrate prereq for `SlashTallyUpdate` + `SlashReasonCode`
- RFC-0855p-c §4 (EXCLUDED scope; DomainCoordinator platform-mediated handover)
- Mission `0855p-e-coordinator-types-shared-crate` (follow-on; extracts local Layer-C types into shared crate)

## Acceptance Criteria

- [ ] `crates/octo-network/src/dot/handover.rs` reconciled to RFC-0855p-e v1.3 spec
- [ ] `SenderStateSnapshotOrdinal` field is private (NOT `pub u8`); has `new()` ctor + `InvalidOrdinal` type (per plateau closure `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`)
- [ ] `HANDOVER_RACE_WINDOW` split into 3 named constants: `HANDOVER_RACE_WINDOW` + `HORQ_BACKWARD_WINDOW = 5` + `HANDOVER_FORWARD_SKEW_BOOST` (no triple-overload per RFC-0855p-e §Layer placement constants block)
- [ ] `HANDOVER_REPLAY_WINDOW` phantom removed (replaced by `HORQ_BACKWARD_WINDOW = 5`)
- [ ] `MAX_FSKEW_EPOCHS = 4` cross-referenced from RFC-0855p-d1 (canonical home via `pub use`); ±1 of local epoch → `±MAX_FSKEW_EPOCHS = 4`
- [ ] HOAK second-witness quorum gate enforced at acceptance site: when accepting a HORQ whose `sender_state_snapshot_ordinal != SenderStateSnapshotOrdinal::Active`, count distinct `attests_to_predecessor_state=true` HOAK signatures per `(coordinator_id, coordinator_term_id, current_epoch)` and reject unless count reaches `horq_quorum(witness_set_size)` (per plateau closure `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md`)
- [ ] HORC (HandoverCancel) envelope implemented with incumbent lockout rule
- [ ] `HandoverCancelEnvelope` struct defined per RFC-0855p-e §Handover Envelope Subtypes (envelope_subtype: b"HORC", payload_hash, term_id; payload_hash domain-prefixed per RFC-0855p-e §Payload Hash)
- [ ] `MeshAggregatedSignature` coverage includes bitmap for HORC + S2PA predecessors
- [ ] BLAKE3 domain separation: `HORQ_CONTEXT = "DOT/1/HANDOVER_REQUEST"`, `HOAK_CONTEXT = "DOT/1/HANDOVER_ACK"`, `HODN_CONTEXT = "DOT/1/HANDOVER_DONE"`, `HORC_CONTEXT = "DOT/1/HANDOVER_CANCEL"`
- [ ] `horq_quorum(witness_set_size: usize)` function distinct from `hodn_quorum` (d3) — DIFFERENT function, kept separate on purpose (HORQ-side mirror)
- [ ] `pub use crate::rfc_0855p_d3::hodn_quorum` re-export present (canonical home for d3)
- [ ] Test vectors TV-HO-1..9 pass per RFC-0855p-e §Test Vectors (including TV-HO-9 for second-witness quorum gate: witness_set_size=3, horq_quorum(3)=2, 1 second-witness HOAK → reject, 2 distinct second-witness HOAKs → gate passes)
- [ ] `cargo test -p octo-network handover` zero failures
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean

## Sub-steps

1. **VERIFY GATE** — RFC-0855p-e Accepted (commit `0e915618`); `crates/octo-network/src/dot/handover.rs` exists
2. **RECONCILE EXISTING SUBSTRATE** to RFC-0855p-e v1.3 spec (1045L baseline; surgical edits, not rewrite)
3. Make `SenderStateSnapshotOrdinal` private; add `new()` ctor + `InvalidOrdinal` type
4. Split `HANDOVER_RACE_WINDOW` into 3 named constants
5. Replace `HANDOVER_REPLAY_WINDOW` phantom with `HORQ_BACKWARD_WINDOW = 5`
6. Update `±1 of local epoch` → `±MAX_FSKEW_EPOCHS = 4` (cross-ref d1 via `pub use`)
7. Add HOAK second-witness quorum gate at HORQ acceptance site
8. Implement HORC envelope + lockout rule
9. Add test vector TV-HO-9 for second-witness quorum gate
10. Verify all TV-HO-1..9 pass
11. Verify cargo test + clippy + fmt

## Test Vectors (per RFC-0855p-e §Test Vectors)

- TV-HO-1: valid HORQ acceptance; sender_state_snapshot_ordinal = Active; happy path
- TV-HO-2: valid HOAK acceptance; witness signature; attests_to_predecessor_state = true
- TV-HO-3: valid HODN acceptance; successor + quorum
- TV-HO-4: race window enforcement — multiple HORQs for same `(coordinator_id, coordinator_term_id)`; lex tiebreak selects one
- TV-HO-5: HANDOVER_RACE_WINDOW bounds — outside window → reject
- TV-HO-6: incumbent-HORQ-in-flight lockout; new HORQ rejected
- TV-HO-7: HORC acceptance during lockout — incumbent cancel envelope (`HandoverCancelEnvelope` per RFC-0855p-e line 434); payload_hash domain check passes; lockout cleared; new HORQ accepted
- TV-HO-8: slash tally carry-over — `slash_tally_hash` reference at `current_epoch`
- TV-HO-9: second-witness quorum gate — sender_state_snapshot_ordinal != Active; witness_set_size=3; horq_quorum(3)=2; 1 second-witness HOAK → reject; 2 distinct second-witness HOAKs → gate passes

## Layer direction (per [[cipherocto-design-principles]])

- `handover.rs` (Layer C) — envelope dispatch + quorum + state machine
- `HandoverEnvelope` / `HandoverPayload` (Layer B) — wire format + canonical encoding
- `SenderStateSnapshotOrdinal::new()` (Layer A) — pure function (validates wire-stable ordinal)
- `horq_quorum` policy (Layer C) — looks up witness-set-size threshold (HORQ-side mirror; distinct from `hodn_quorum`)
- `SlashTallyUpdate` + `SlashReasonCode` + `HandoverReasonTypeId` (Layer C) — local types per RFC-0855p-e §Layer-C Substrate Types (LOCAL NOW; scheduled to lift to Layer B shared in follow-on `0855p-e-coordinator-types-shared-crate` per RFC-0855p-e §Layer-C Substrate Types + §Future Work F-7 follow-on note)

No upward dependency. Substrate recipients reading unknown ordinal variants fail closed (private field + `new()` ctor).

## Backward compat

Reconciliation; existing 1045L substrate preserved. Surgical edits per W11 + W12.5 + W12.6 findings. NO rewrite. Module-level split (`handover_state.rs` / `coordinator_handover.rs` / `slash_tally_carryover.rs`) is a follow-on refactor; not this mission.

## Risk

- **Substrate-truth drift**: pre-v1.3 substrate (1045L) may encode v0.x semantics that conflict with v1.3 spec. Mitigation: per-RFC substrate-truth check before any surgical edit; flag divergence for separate handling.
- **Second-witness quorum gate bypass**: omitting the gate at HORQ acceptance site allows single-HOAK predecessor-state attestation to bypass 2/3 quorum invariant. Mitigation: explicit test vector TV-HO-9; explicit acceptance-site enforcement.
- **`SenderStateSnapshotOrdinal::pub u8` regress**: re-exposing the field defeats `#[non_exhaustive]`. Mitigation: `git blame` + code review before each reconciliation step.
- **HANDOVER_RACE_WINDOW triple-overload**: reusing the same name for backward/concurrent/forward skew creates semantic ambiguity. Mitigation: split into 3 named constants; explicit deprecation note in code.
- **RFC-0855p-b §B amendment gate**: per RFC-0855p-e §Future Work F-7, promotion was gated on EITHER 0855p-b §B amendment OR `octo-coordinator-types` crate. Substrate-truth: `crates/octo-network/src/dot/slash.rs:86` reserves `0x0013..0xFFFF` (entries NOT yet allocated in 0855p-b slash enum); promotion proceeded via option (b) deferred path. Mitigation: verify SlashReasonCode 0x0013-0x0016 are in `octo-coordinator-types` per this mission before substrate-truth check.

## Notes

- Per RFC-0855p-e §Layer-C Substrate Types, `SlashTallyUpdate` + `SlashReasonCode` lift into shared `octo-coordinator-types` crate is a post-acceptance follow-on mission (see `0855p-e-coordinator-types-shared-crate`).
- Per RFC-0855p-e §Substrate, module-level split into `handover_state.rs` / `coordinator_handover.rs` / `slash_tally_carryover.rs` is a follow-on refactor when substrate complexity justifies it; not this mission.
- Per plateau closure: `horq_quorum` is HORQ-side mirror, `hodn_quorum` is HODN-side; DIFFERENT functions in DIFFERENT RFCs, kept separate on purpose (different param naming: `witness_set_size` for both, but different policy domain).
- Substrate anchor for RFC-0855p-e lives at `crates/octo-network/src/dot/handover.rs` per plateau doc Per-RFC Substrate Ownership table.

## Cross-references

- RFC-0855p-e (this mission's canonical spec; §Data Structure, §Specification, §Security, §Test Vectors, §Layer-C Substrate Types, §Implementation Phases, §Appendices A+B)
- RFC-0855p-b (slash tally observability §"Slash tally observability", Slash Offense Codes §B, CoordinatorLifecycle §"Data Structures")
- RFC-0855p-c §4 (EXCLUDED scope; DomainCoordinator platform-mediated handover)
- RFC-0850p-c (Transport Group Binding Ceremony)
- RFC-0008 (Slash Offense Code registry; slash_reason_code space 0x0001-0xFFFF; `crates/octo-network/src/dot/slash.rs:86` reserves `0x0013..0xFFFF`)
- RFC-0009 (Identity substrate; `mission_id` truncation to 16-byte BLAKE3-256(mission_did) per §Identity)
- RFC-0853 (BLAKE3-256)
- RFC-0855p-d1 (canonical home for `MAX_FSKEW_EPOCHS = 4`; `pub use` re-export)
- RFC-0855p-d3 (canonical home for `hodn_quorum`; `pub use` re-export)
- Mission `0855p-e-coordinator-types-shared-crate` (follow-on; extracts local Layer-C types)
- `docs/audits/2026-09-02-rfc-0855p-de-review-dry.md` (DRY closure audit)
- `docs/audits/2026-09-02-rfc-0855p-de-review-plateau.md` (plateau declaration; Per-RFC Substrate Ownership table)

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-02 after W4.5 DRY closure at commit `647b2cf4`; reconciles 1045L pre-existing handover.rs to RFC-0855p-e v1.3 spec; parallel to d-chain; precedes ct extraction)
