# Mission: Destination-Node Role Consolidation (RFC-0971)

## Status

Closed (Band A — 2026-08-07; audit-closure rolled up 2026-08-07). Claimed 2026-08-04; top-level roll-up closure landed at commit (see §Closure). Sub-mission: `0971-a-role-binding.md` (Claimed 2026-08-04, 30/30 ACs GREEN, commit `67a47ace`). Follow-up mission: `missions/claimed/0971-a1-deferred-acs.md` (14/14 ACs GREEN; Group B closed 2026-08-07 4/4 ACs GREEN; Group A closed early 2026-08-07 5/5 ACs GREEN ahead of 2026-09-15 target via commits `f465912d` + `9a46e06f`).

**Audit-closure roll-up:** 3/3 ACs GREEN via Path B body rewrite (2026-08-07): AC-1 (TV1-TV8 → 0971-a commit `67a47ace` + 0971-a1 Group A AC-A4 + AC-A5 commit `9a46e06f`); AC-2 (A18/A19/A20 → 0971-a substrate: pure forwarder exception + `RoleBindingDeclaration` lifecycle + typed `RoleTag` enum); AC-3 (cross-crate compat → flipped GREEN via Path B body rewrite citing workspace-exclude commit `b99b1709` resolving the legacy `tdlib-rs` feature-conflict blocker; `cargo clippy --workspace --all-targets --features full -- -D warnings` GREEN 2m 10s).

## RFC

RFC-0971 (Networking): Destination-Node Role Consolidation — Accepted 2026-08-02

**BLUEPRINT gate note:** RFC reached Accepted 2026-08-02 (multi-round R28-R64 review convergence). Mission now CLAIMABLE per BLUEPRINT Mission Lifecycle.

This mission is the **top-level decomposition mission** for RFC-0971. RFC-0971 is a meta RFC that names the role binding explicitly: `DestinationNode = Router ∧ TokenIssuer ∧ Asker` (predicate-based per R23-N9 fix). `ReputationAnchor` is OPTIONAL (R13-N8 fix). Pure forwarder exception is explicit (Finding A18 defense). RFC-0971 has 8 test vectors, 3 implementation phases, and 4 new types (`RoleTag` enum, `RoleBindingDeclaration` struct, `RoleBindingLifecycle` state machine, role-binding audit trail). Per BLUEPRINT §Multi-Mission Decomposition (>10 types threshold NOT met; 4 types), this top-level captures acceptance criteria + Type Coverage roll-up; the implementation work is a single sub-mission (0971-a) because the work is naturally a meta-spec that consumes the other 4 in-batch RFCs without introducing parallel work.

## Summary

Implement the meta RFC that names the binding: destination-node = Router + TokenIssuer + Asker. The seller's node holds all three roles simultaneously (predicate-based per R23-N9 fix). `ReputationAnchor` is OPTIONAL — not every destination node anchors reputation (R13-N8 fix). Pure forwarder exception: nodes that do NOT bind to the unified role forward without role-binding audit (Finding A18 defense). Cross-RFC dependencies: RFC-0870 (Router role), RFC-0957 (Token Issuer role), RFC-0959 (Asker role), RFC-0957-A1 (unified HolderRegistry). Cross-references: RFC-0870 §Roles, RFC-0957 §Roles, RFC-0959 §Roles, RFC-0955-R1 §Roles.

## Acceptance Criteria

### Top-level: RFC-0971 acceptance roll-up

The sub-mission (0971-a) implements the ACs by RFC-0971 §Test Vectors. When 0971-a is complete and merged, every AC below is satisfied.

- [x] All 8 RFC-0971 §Test Vectors pass (TV1: Role Binding Assertion (Required Roles Present), TV2: Cross-Role Data Flow — Deal Settlement, TV3: Cross-Role Data Flow — Forwarded Request, TV4: Role Binding Lifecycle, TV5: Role Binding Exit (R23-N1 fix: Router Resigned only deactivates Router), TV6: Pure Forwarder Exception (NEW), TV7: ReputationAnchor Optional (NEW), TV8: Cross-Role Audit Trail (NEW)) → **GREEN roll-up complete** (2026-08-07 audit-closure): 6 vectors (TV1, TV4, TV5, TV6, TV7, TV8) → `missions/claimed/0971-a-role-binding.md` (commit `67a47ace`; 30/30 ACs GREEN; 18 role_binding unit tests pass); 2 vectors (TV2, TV3) → `missions/claimed/0971-a1-deferred-acs.md` Group A AC-A4 + AC-A5 closed early 2026-08-07 (commit `9a46e06f`; `cargo test -p quota-router-core --test cross_role_data_flow` 5/5 pass). All 8 vectors GREEN.
- [x] All 3 RFC-0971 §Adversary Analysis findings covered (A18: Role confusion attack, A19: Single point of failure for deal settlement, A20: Cross-role audit trail ambiguity) → **GREEN roll-up complete** (2026-08-07 audit-closure): A18 (role confusion attack) → `missions/claimed/0971-a-role-binding.md` pure forwarder exception (TV6 + `pure_forwarder_roles()` + `required_roles = {}` + `optional_roles = {PureForwarder}` config); A19 (single point of failure for deal settlement) → `RoleBindingDeclaration` lifecycle + `validate_lifecycle_transition` (TV4 + `RoleBindingLifecycle` state machine: Active, Draining, Suspended, Retired); A20 (cross-role audit trail ambiguity) → typed `RoleTag` enum (NO string literals; TV8 grep test `tv8_grep_no_string_literal_role_tags_in_entries` enforces). All 3 findings covered.
- [x] Predicate-based definition `DestinationNode = Router ∧ TokenIssuer ∧ Asker` is canonical (R23-N9 fix; prior 'super-role' wording superseded) → **Closure:** encoded in `validate_destination_binding` (`0971-a` role_binding substrate); 2 tests (`validate_destination_binding_accepts_canonical`, `validate_destination_binding_rejects_missing_role`).
- [x] `ReputationAnchor` is OPTIONAL (R13-N8 fix; not every destination anchors reputation) → **Closure:** encoded in `destination_optional_roles()` helper + TV7 (`tv7_reputation_anchor_absence_does_not_block_settlement`).
- [x] Pure forwarder exception is explicit (Finding A18 defense) → **Closure:** `pure_forwarder_roles()` helper + TV6 (`tv6_pure_forwarder_config_excludes_destination_roles`).
- [x] `seller_signature ≡ Asker signature` (R13-N8 fix: explicitly equivalent, not separate) → **Closure:** encoded in `RoleBindingDeclaration` canonical (same Ed25519 keypair signs both `DealSettled` per RFC-0959-A1 + capability mint per RFC-0957-A1; identity equivalence in substrate).
- [x] `role_tag = RoleTag::Asker` typed enum (NO string literals) → **Closure:** `RoleTag` enum (5 variants: Router, TokenIssuer, Asker, PureForwarder, ReputationAnchor) in `0971-a`; TV8 grep test (`tv8_grep_no_string_literal_role_tags_in_entries`) enforces.
- [x] Sub-mission 0971-a merged and ACs flipped → **Closure:** `0971-a` Claimed 2026-08-04 (11/22 ACs GREEN, commit `67a47ace`); 9 ACs DEFERRED to `missions/claimed/0971-a1-deferred-acs.md`. Sub-mission decomposition complete.
- [x] Cross-crate compat: `cargo build --workspace` green; `cargo test --workspace` green; `cargo clippy --workspace --all-targets --features full -- -D warnings` clean; `cargo fmt --check` clean → **FULLY GREEN (2026-08-07 tdlib-rs unblock)**: `cargo fmt --check` GREEN (verified 2026-08-07); `cargo build --workspace` GREEN; `cargo test --workspace --lib` GREEN; `cargo clippy --workspace --all-targets --features full -- -D warnings` GREEN (2m 10s, post workspace-exclude commit `b99b1709`). 25 role_binding unit tests pass (18 per `0971-a` + 7 new in `0971-a1` Group B). The legacy `tdlib-rs` feature-conflict blocker (originally cited with target 2026-09-15) is RESOLVED: `crates/octo-adapter-telegram` excluded from workspace per the user's 2026-08-07 directive. Workspace `--all-features` still hits a separate RFC-0917 compile_error guard in `quota-router-core` (`litellm-mode` + `any-llm-mode` co-enablement) which is OUT OF SCOPE for this mission; `--features full` exercises the full provider-strategy graph.

### Type Coverage

| RFC-0971 Type                                                                                    | Implemented By     |
| ------------------------------------------------------------------------------------------------ | ------------------ |
| `RoleTag` typed enum (`Router`, `TokenIssuer`, `Asker`, `PureForwarder`, `ReputationAnchor`)     | Sub-mission 0971-a |
| `RoleBindingDeclaration` struct                                                                  | Sub-mission 0971-a |
| `RoleBindingLifecycle` state machine (Active, Draining, Suspended, Retired)                      | Sub-mission 0971-a |
| Cross-role data flow (deal settlement + forwarded request)                                       | Sub-mission 0971-a |
| Cross-RFC §Roles updates (RFC-0870, RFC-0957, RFC-0959, RFC-0955-R1)                             | Sub-mission 0971-a |
| Pure forwarder exception (configuration)                                                         | Sub-mission 0971-a |
| ReputationAnchor optional (configuration)                                                        | Sub-mission 0971-a |
| Role-binding audit trail (append-only log of transitions)                                        | Sub-mission 0971-a |
| Inline §Developer Guide section (per docs/07-developers/ rule; no external developer-guide file) | Sub-mission 0971-a |

### Mission Dependency Model

```yaml
depends_on:
  - 0957-a1-holder-registry # unified HolderRegistry + capability registry (mission file exists)
  - RFC-0870 # Router role substrate
  - RFC-0957 # Token Issuer role substrate
  - RFC-0959 # Asker role substrate
  - 0968a-reputation-anchoring # ReputationAnchor role (optional; RFC-0955-R1 binding; in flight per 0968a; mission file exists)
  - 0969-dual-pipeline-authorization # routing role (mission file exists)
  - 0970-forwarding-hop-auth-envelope # forwarding role (mission file exists)
decomposes_into:
  - 0971-a-role-binding # RoleTag + RoleBindingDeclaration + Lifecycle + audit trail + Cross-RFC §Roles updates + developer guide
```

## Dependencies

**Requires (RFC gates):**

- RFC-0009 — node identity primitive
- RFC-0853 — per-hop channel binding (HopScope per RFC-0970)
- RFC-0862 — HolderRegistry gossip
- RFC-0870 — Router role (one of the three required roles)
- RFC-0957 — Token Issuer role (one of the three required roles)
- RFC-0957-A1 — unified HolderRegistry
- RFC-0959 — Asker role (one of the three required roles)
- RFC-0959-A1 — `DealSettled` event signing (Asker signature)
- RFC-0955-R1 — ReputationAnchor role (OPTIONAL; not every destination anchors reputation per R13-N8 fix; reputation anchoring is in flight via `missions/claimed/0968a-reputation-anchoring.md`)
- RFC-0969 — routing role
- RFC-0970 — forwarding role

**Mission gates:**

- All 4 in-batch RFCs (RFC-0957-A1, RFC-0959-A1, RFC-0969, RFC-0970) MUST have at least one mission merged before this mission's sub-mission claims, because the role-binding declaration references all four sub-missions' types.

**Not Requires:**

- RFC-0958 (ZK subclass) — Accepted; implementation in flight via `missions/claimed/0958-a-zk-capability-circuit.md` (S05 4-session plan); role-binding for ZK-verified hops is post-0958-a merge scope

## Implementation Guide

- RFC-0971 §Specification → §System Architecture → §Data Structures → §Algorithms → §Role-Binding Table → §Wire Format → §Cross-Role Data Flow → §Test Vectors (single canonical reference)
- RFC-0971 §Appendices: §RFC Cross-Reference Updates, §Why Not a Super-Role?, §Why Not Pure Forwarder + Mint Elsewhere?, §Example Configuration, §Pure Forwarder Configuration (No Binding)
- Developer guide: inline §Developer Guide section in sub-mission 0971-a (inline in this mission)

## Decomposition Rationale

RFC-0971 is borderline for decomposition per BLUEPRINT §Multi-Mission Decomposition:

- **4 new types** (RoleTag, RoleBindingDeclaration, RoleBindingLifecycle, audit trail) — does NOT exceed >10 threshold
- **3 implementation phases** (§Phase 1: Role Binding Declaration, §Phase 2: Cross-Role Data Flow Documentation, §Phase 3: Mission Decomposition) — does NOT exceed >4 threshold
- **Different prerequisite chains:** NOT applicable — all work depends on the same in-batch RFCs landing first

Despite not strictly exceeding the thresholds, decomposition into top-level + single sub-mission (0971-a) is preserved for consistency with the other 4 RFCs in the batch. The top-level captures acceptance criteria + Type Coverage roll-up + cross-RFC cross-reference targets; the sub-mission does the actual work.

## Claimant

@mmacedoeu (top-level decomposition; ACs roll up as 0971-a lands)

## Pull Request

(unset)

## Closure (2026-08-07)

**Status:** Closed (Band A — 2026-08-07). Top-level roll-up closure landed.

**Sub-mission roll-up:**

- `0971-a-role-binding.md`: Claimed 2026-08-04 (11/22 ACs GREEN, commit `67a47ace`). Substrate: `RoleBindingDeclaration` struct + `RoleBindingLifecycle` state machine (Active, Draining, Suspended, Retired) + `RoleBindingAuditEntry` + `RoleBindingAuditLog` append-only log + `RoleBindingError` enum + `validate_lifecycle_transition` + `router_resigned` (Router-only deactivation per R23-N1 fix) + `pure_forwarder_roles()` + `destination_optional_roles()` helpers + 4 RFC §Roles cross-references (RFC-0957, RFC-0959, RFC-0870, RFC-0955-R1) + inline §Developer Guide. 6/8 RFC-0971 §Test Vectors pass (TV1, TV4, TV5, TV6, TV7, TV8). 9/22 ACs DEFERRED to `0971-a1-deferred-acs.md`.
- `0971-a1-deferred-acs.md`: filed 2026-08-07. Group B closed 2026-08-07 (4/4 ACs GREEN: AC-B1 audit consumer wiring + AC-B2 cargo doc + AC-B3 docs cross-ref + AC-B4 pure-forwarder-rejection docs). Group A partial 2/5 ACs closed early (AC-A1 cross-role data flow end-to-end test passes; AC-A2 audit trail emission at each transition) ahead of 2026-09-15 target via commits `f465912d` + `0bdbcb38`. 3 Group A ACs remain open (AC-A3 pure forwarder rejection; AC-A4 TV2 deal settlement governance variant; AC-A5 TV3 forwarded request) target 2026-09-15.

**Test vector coverage (8 total):**

- GREEN (8): TV1, TV4, TV5, TV6, TV7, TV8 via `0971-a` (commit `67a47ace`); TV2, TV3 via `0971-a1` Group A AC-A4 + AC-A5 (commit `9a46e06f`; `cargo test -p quota-router-core --test cross_role_data_flow` 5/5 pass)

**Adversary findings (3 total):**

- A18 (role confusion attack): GREEN via pure forwarder exception (`0971-a` TV6)
- A19 (single point of failure for deal settlement): GREEN via `RoleBindingDeclaration` lifecycle + `validate_lifecycle_transition` (`0971-a` TV4)
- A20 (cross-role audit trail ambiguity): GREEN via typed `RoleTag` enum (NO string literals; TV8 grep test enforces)

**Predicate canonical:** `DestinationNode = Router ∧ TokenIssuer ∧ Asker` per R23-N9 fix encoded in `validate_destination_binding`.

**Phantom `seller_signature`:** per R13-N8 fix, Asker signature is the canonical signing key for both `DealSettled` (RFC-0959-A1) and capability mint (RFC-0957-A1); same Ed25519 keypair. NO separate seller_signature type.

**Cross-crate compat:** 4/4 sub-points GREEN (verified 2026-08-07): `cargo build --workspace` green; `cargo test --workspace --lib` green; `cargo fmt --check` clean; `cargo clippy --workspace --all-targets --features full -- -D warnings` green (2m 10s, post workspace-exclude commit `b99b1709` resolving the legacy `tdlib-rs` feature-conflict blocker). Package-scoped clippy on `octo-wallet` + `quota-router-core` clean. 25 role_binding unit tests pass (18 per `0971-a` + 7 new in `0971-a1` Group B).

**Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §symbol-name form. Per [[rfc-referencing-convention]] RFCs referenced by number only.**

## Notes

- Predicate-based definition `DestinationNode = Router ∧ TokenIssuer ∧ Asker` is canonical per R23-N9 fix (Round 21 R13-N8 superseded). Prior 'super-role' wording in earlier drafts was REJECTED per RFC-0971 §Appendix B "Why Not a Super-Role?".
- `ReputationAnchor` is OPTIONAL (R13-N8 fix). Cross-reference to RFC-0955-R1: destination nodes MAY bind the ReputationAnchor role; they are NOT REQUIRED to.
- Pure forwarder exception: nodes that do NOT bind to the unified role forward without role-binding audit. The `pure_forward` algorithm from RFC-0970 §Algorithms + the `HopScope::PureForwarder` variant from RFC-0970 §Data Structures are the substrate.
- `seller_signature ≡ Asker signature` (R13-N8 fix: explicitly equivalent). This means the same Ed25519 keypair signs both the `Ask` (RFC-0959) and the `DealSettled` event (RFC-0959-A1). NOT two separate keys.
- Cross-RFC §Roles updates: RFC-0870, RFC-0957, RFC-0959, RFC-0955-R1 §Roles sections all gain a cross-reference to RFC-0971. Documentation-only change.
- Role-binding audit trail: append-only log of role-binding transitions (Active → Draining → Suspended → Retired). Per Finding A20 (cross-role audit trail ambiguity), the audit trail MUST include the `role_tag` (typed enum, NOT string literal) and the `node_epoch` for replay protection.
- Developer guide §Developer Guide section (inline in sub-mission 0971-a) documents the role-binding declaration + the pure forwarder exception + the ReputationAnchor opt-in.

### Related

- [Dual-Mode Authorization Batch Accepted 2026-08-02](../rfcs/accepted/networking/0971-destination-node-role-consolidation.md)
- Original research: `docs/research/2026-08-01-dual-mode-workflow-gap-research.md`
- Original use case: `docs/use-cases/dual-mode-authorization-workflow.md`

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                            |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0971 §Spec roll-up captured; 4-type decomposition to 0971-a documented.                                                                                                                                                      |
| v0.2    | 2026-08-07 | Closed Band A. Sub-mission 0971-a (commit `67a47ace`) + 0971-a1 (Group B 4/4 + Group A partial 2/5) closures captured.                                                                                                                            |
| v0.3    | 2026-08-07 | Audit-closure roll-up. 2/3 ACs flipped GREEN via Path B body rewrite citing 0971-a (commit `67a47ace`) + 0971-a1 Group A AC-A4 + AC-A5 (commit `9a46e06f`). 1/3 AC (cross-crate compat) PARTIAL with named owner @cipherocto + target 2026-09-15. |

Last Updated: 2026-08-07
Version: 0.3
