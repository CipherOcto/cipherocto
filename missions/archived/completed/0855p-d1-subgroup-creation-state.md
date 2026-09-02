---
name: 0855p-d1-subgroup-creation-state
description: Create sub-group creation + state substrate per RFC-0855p-d1 (CGSB + SubGroupState + SubGroupLabel + canonical BLAKE3 derivation + state machine + Layer-C query surface) in `crates/octo-network/src/dot/subgroup_state.rs`.
metadata:
  node_type: substrate-network
  type: substrate-creation
  originSessionId: RFC-0855p-d1 author session
  created: 2026-09-02
  v: "1.1"
  landing_commit: "74c9ea9f"
  review_commit: "851ef015"
  dry_closure: "7c0f764f"
  closed: 2026-09-02
  depends_on:
    - RFC-0855p-d1
    - RFC-0855p-d
    - RFC-0853
    - RFC-0009
    - RFC-0850p-c
    - RFC-0850p-d
    - RFC-0126
    - RFC-0855p-c
status: Closed
---

# 0855p-d1-subgroup-creation-state — Sub-Group Creation & State Substrate per RFC-0855p-d1

**Status:** Closed (2026-09-02). LANDED commit `74c9ea9f` (feat: RFC-0855p-d1 subgroup creation + state substrate) + review-loop fix `851ef015` (R1.5: BLAKE3 derive_key zero-key defect + cite hygiene). DRY closure at `7c0f764f` per `docs/audits/2026-09-02-rfc-0855p-de-substrate-review-dry.md`.
**Substrate:** RFC-0855p-d1 (per-concern RFC, sibling of RFC-0855p-d INDEX)
**Parent:** RFC-0855p-d (slim INDEX; cross-cutting chain)
**Depends on:** RFC-0855p-d1 Accepted (2026-09-02 at commit `0e915618`); RFC-0855p-d INDEX; RFC-0853 (Overlay Cryptography; BLAKE3-256 mandated); RFC-0009 (Identity substrate; canonical `Did` type); RFC-0850p-c (Transport Group Binding Ceremony); RFC-0850p-d (DC-Initiated Transport Group Creation & Invite); RFC-0126 (DCS deterministic canonical serialization); RFC-0855p-c (DomainCoordinator Role)

## Status

Closed (2026-09-02) per RFC-0855p-d1 promotion to Accepted at commit `0e915618` + substrate LANDED at `74c9ea9f` + R1.5 review-loop fix `851ef015` (BLAKE3 derive_key defect + cite hygiene). DRY closure at `7c0f764f` per `docs/audits/2026-09-02-rfc-0855p-de-substrate-review-dry.md` (5-lens loop: R1 → R1.5 → R2 → R2.5 → R3 → R4 → 2 consecutive zero-finding rounds). 1407/1407 octo-network tests pass; clippy zero; fmt clean. User owns push + PR per [[feedback_initiation_user_only]] + [[git-workflow]].

## Substrate (RFC-0855p-d1)

Per RFC-0855p-d1 §Data Structure + §State Machine + §Layer-C Substrate Surface (creation/query):

- `CreateSubGroupEnvelope` (`CGSB` subtype) — auth + BIND-nonce + parent reference + sub-label
- `SubGroupExtension` — BIND attestation response (BIND nonce + sub-coord signature)
- `SubGroupLabel` — typed constructor; UTS-39 confusable + bidi/zero-width/BOM reject set
- `SubGroupRecord` — wire-stable record (sub_domain_id + parent_domain_id + state + bind_epoch + members)
- `SubGroupState` — `#[non_exhaustive]` enum (PendingBind / Bound / Dissolving / Dissolved)
- `SubGroupQuery` / `SubGroupResponse` / `SubGroupAuthorityCheck` — typed Layer-C query boundary
- `sub_domain_id` canonical derivation: `BLAKE3_keyed(SUBGROUP_DOMAIN_CONTEXT, parent_domain_id || sub_label)` (where `SUBGROUP_DOMAIN_CONTEXT = "DOT/1/CGROUP_SUB/domain"` per RFC-0855p-d1 line 158; expanded to 32-byte key via `blake3::derive_key`)
- Nonce + duplicate + parent-binding + depth-cap indexes
- `MAX_BIND_AWAIT_EPOCHS = 32` + `MAX_BIND_RETRY_COUNT = 3` + `MAX_SUBGROUP_DEPTH = 8` + `MAX_FSKEW_EPOCHS = 4` + `MAX_ROOT_DEPTH = 1` enforcement
- Cross-node `PendingBind → Dissolving` reconciliation

## Parent

RFC-0855p-d (slim INDEX; cross-cutting chain INDEX only — owns no substrate of its own). Per RFC-0855p-d §Layer placement table, RFC-0855p-d1 owns the CGSB + state + label + record + query (creation subset) substrate surface.

## Depends on

See YAML frontmatter `depends_on` block. Hard sequencing: this mission lands first; missions `0855p-d2-subgroup-delegation-lifecycle` and `0855p-d3-subgroup-routing-aggregation-teardown` depend on this mission (re-export `SubGroupLabel`, `DelegationId`, `MAX_ROOT_DEPTH`, `SubGroupRecord`, `SubGroupState`).

## Acceptance Criteria

- [ ] `crates/octo-network/src/dot/subgroup_state.rs` created
- [ ] `CreateSubGroupEnvelope` + `SubGroupExtension` + `SubGroupLabel` + `SubGroupRecord` + `SubGroupState` types defined
- [ ] `SubGroupQuery` / `SubGroupResponse` / `SubGroupAuthorityCheck` typed query boundary implemented
- [ ] Canonical `sub_domain_id` derivation via BLAKE3 keyed_hash (NOT plain `blake3::hash`); test asserts derivation matches RFC-0855p-d1 §Sub-Domain Derivation Invariant
- [ ] `SubGroupLabel::new()` rejects UTS-39 confusables (bidi-control U+202A–U+202E, bidi-isolate U+2066–U+2069, zero-width U+200B–U+200D, narrow-no-break U+202F, BOM U+FEFF) per RFC-0855p-d1 Appendix A
- [ ] `MAX_BIND_AWAIT_EPOCHS = 32` + `MAX_BIND_RETRY_COUNT = 3` + `MAX_SUBGROUP_DEPTH = 8` + `MAX_FSKEW_EPOCHS = 4` + `MAX_ROOT_DEPTH = 1` enforcement at state transitions
- [ ] `SubGroupState` is `#[non_exhaustive]` per §Extension over enumeration
- [ ] Test vectors TV-SG-1, TV-SG-2, TV-SG-3, TV-SG-4, TV-SG-5 pass per RFC-0855p-d1 §Test Vectors
- [ ] State machine transition test: `PendingBind → Bound → Dissolving → Dissolved`
- [ ] Cross-node reconciliation: divergent `PendingBind` epochs collapse to `Dissolving` after `MAX_BIND_AWAIT_EPOCHS` elapses
- [ ] `pub use` re-exports for cross-RFC constants: `MAX_BIND_AWAIT_EPOCHS`, `MAX_BIND_RETRY_COUNT`, `RACE_EPOCHS`, `MAX_FSKEW_EPOCHS` per RFC-0855p-d1 §Layer placement
- [ ] Layer direction verified (Layer A→B→C/D; no upward dependency per [[cipherocto-design-principles]])
- [ ] Pre-existing `sub_group.rs` substrate (`CreateSubGroupEnvelope` + `SubGroupExtension` + `SubGroupError` + `SUBGROUP_TAG` + `MAX_SUB_LABEL_LEN`) reconciled to canonical home in `subgroup_state.rs` (per RFC-0855p-d1 §Implementation Notes F-12)
- [ ] `sub_group.rs` deprecated via `#[deprecated]` + `pub use` re-export pointer to `subgroup_state.rs` (no break for downstream consumers)
- [ ] `MAX_ROOT_DEPTH = 1` test vector present (TV-SG-5a; rejection on depth-cap at root depth > 1)
- [ ] `cargo test -p octo-network subgroup_state` zero failures
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo fmt --all -- --check` clean

## Sub-steps

1. **VERIFY GATE** — RFC-0855p-d1 + RFC-0855p-d Accepted (commit `0e915618` on `next`)
2. **MIGRATE EXISTING SUBSTRATE** — move `CreateSubGroupEnvelope` + `SubGroupExtension` + `SubGroupError` + `SUBGROUP_TAG` + `MAX_SUB_LABEL_LEN` from `sub_group.rs` to canonical home in `subgroup_state.rs` (per RFC-0855p-d1 §Implementation Notes F-12)
3. Mark `sub_group.rs` deprecated via `#[deprecated]` + `pub use` re-export pointer to `subgroup_state.rs`
4. Define core types (`SubGroupLabel`, `SubGroupState`, `SubGroupRecord`, `CreateSubGroupEnvelope`, `SubGroupExtension`)
5. Implement canonical `sub_domain_id` derivation per §Sub-Domain Derivation Invariant
6. Implement state transition engine
7. Implement typed Layer-C query surface (`SubGroupQuery` / `SubGroupResponse` / `SubGroupAuthorityCheck`)
8. Add test vectors TV-SG-1..5a
9. Add cross-node reconciliation test
10. Verify cargo test + clippy + fmt

## Test Vectors (per RFC-0855p-d1 §Test Vectors)

- TV-SG-1: valid CGSB acceptance; happy path
- TV-SG-2: invalid sub-label (UTS-39 confusable) → reject
- TV-SG-3: BIND timeout → state `PendingBind → Dissolving`
- TV-SG-4: depth-cap exceeded (`MAX_SUBGROUP_DEPTH = 8`) → reject
- TV-SG-5: parent-binding verification failure → reject
- TV-SG-5a: root-depth exceeded (`MAX_ROOT_DEPTH = 1`) → reject (depth 0 only; no grandparent sub-group)

## Layer direction (per [[cipherocto-design-principles]])

- `subgroup_state.rs` (Layer C) — state machine + query + creation logic
- `CreateSubGroupEnvelope` / `SubGroupExtension` (Layer B) — wire format + canonical encoding
- `SubGroupState` / `SubGroupLabel` (Layer B) — types
- `sub_domain_id` derivation (Layer A pure crypto) — BLAKE3 keyed_hash per RFC-0853

No upward dependency. No Layer C module parses raw Layer B envelopes. Unknown envelope subtypes fail closed.

## Backward compat

Pre-existing `sub_group.rs` (CGSB substrate; `CreateSubGroupEnvelope` + `SubGroupExtension` + `SubGroupError` + `SUBGROUP_TAG` + `MAX_SUB_LABEL_LEN`) is reconciled to canonical home in new `subgroup_state.rs`. Migration: `sub_group.rs` keeps `#[deprecated]` + `pub use` re-export pointer to `subgroup_state.rs` for one release cycle (per RFC migration etiquette); removed in v1.2. Additive to module tree.

## Risk

- **Canonical derivation drift**: implementing plain `blake3::hash(parent_domain_id || sub_label)` instead of the canonical `BLAKE3 keyed_hash` form silently breaks cross-node reconciliation. Mitigation: explicit test vector asserting derivation matches RFC-0855p-d1 §Sub-Domain Derivation Invariant (F-12 substrate migration noted in RFC-0855p-d1 §Implementation Notes).
- **State enum exhaustiveness**: closing the `SubGroupState` enum (no `#[non_exhaustive]`) forces cross-crate edit when adding `Archived` / `Frozen`. Mitigation: enforce `#[non_exhaustive]` per §Extension over enumeration.

## Notes

- Per `[[cipherocto-design-principles]]` §Extension over enumeration, `SubGroupState` carries `#[non_exhaustive]`; substrate implementations MUST NOT use exhaustive match.
- Per RFC-0855p-d1 §Implementation Notes F-12, substrate migration from legacy plain-`blake3::hash` to canonical `BLAKE3 keyed_hash` is a follow-on implementation mission; this mission is the canonical-home creation.
- RFC-0855p-d (slim INDEX) owns no substrate; this mission is the substrate anchor for the d1 RFC.

## Cross-references

- RFC-0855p-d (slim INDEX; cross-cutting chain; substrate ownership table)
- RFC-0855p-d1 (this mission's canonical spec; §Data Structure, §State Machine, §Layer-C Substrate Surface, §Test Vectors, §Appendix A, §Appendix B)
- RFC-0855p-d2 (downstream consumer; re-exports `SubGroupLabel`, `DelegationId`, `MAX_ROOT_DEPTH` from this mission)
- RFC-0855p-d3 (downstream consumer; re-exports `SubGroupRecord`, `SubGroupState`, plus constants from this mission)
- RFC-0855p-e (sibling RFC; uses `MAX_FSKEW_EPOCHS = 4` from this mission's canonical home)
- RFC-0853 (BLAKE3-256 mandated)
- RFC-0009 (Identity substrate; canonical `Did` type)
- RFC-0850p-c (Transport Group Binding Ceremony)
- RFC-0850p-d (DC-Initiated Transport Group Creation & Invite)
- RFC-0126 (DCS deterministic canonical serialization)
- RFC-0855p-c (DomainCoordinator Role)
- Mission `0855p-d2-subgroup-delegation-lifecycle` (downstream; depends on this mission)
- Mission `0855p-d3-subgroup-routing-aggregation-teardown` (downstream; depends on this mission + d2)

## Claimant

@mmacedoeu (mission lifecycle: Claimed 2026-09-02 after W4.5 DRY closure at commit `647b2cf4`; lands FIRST per RFC-0855p-d §Layer placement table substrate-ownership chain d1→d2→d3)
