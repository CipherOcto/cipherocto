# Mission: Destination-Node Role Consolidation (RFC-0971)

## Status

Open

## RFC

RFC-0971 (Networking): Destination-Node Role Consolidation — Accepted 2026-08-02

**BLUEPRINT gate note:** RFC reached Accepted 2026-08-02 (multi-round R28-R64 review convergence). Mission now CLAIMABLE per BLUEPRINT Mission Lifecycle.

This mission is the **top-level decomposition mission** for RFC-0971. RFC-0971 is a meta RFC that names the role binding explicitly: `DestinationNode = Router ∧ TokenIssuer ∧ Asker` (predicate-based per R23-N9 fix). `ReputationAnchor` is OPTIONAL (R13-N8 fix). Pure forwarder exception is explicit (Finding A18 defense). RFC-0971 has 8 test vectors, 3 implementation phases, and 4-6 new types (`RoleTag` enum, `RoleBindingDeclaration` struct, `RoleBindingLifecycle` state machine, role-binding audit trail). Per BLUEPRINT §Multi-Mission Decomposition (>10 types threshold NOT met; 4-6 types), this top-level captures acceptance criteria + Type Coverage roll-up; the implementation work is a single sub-mission (0971-a) because the work is naturally a meta-spec that consumes the other 4 in-batch RFCs without introducing parallel work.

## Summary

Implement the meta RFC that names the binding: destination-node = Router + TokenIssuer + Asker. The seller's node holds all three roles simultaneously (predicate-based per R23-N9 fix). `ReputationAnchor` is OPTIONAL — not every destination node anchors reputation (R13-N8 fix). Pure forwarder exception: nodes that do NOT bind to the unified role forward without role-binding audit (Finding A18 defense). Cross-RFC dependencies: RFC-0870 (Router role), RFC-0957 (Token Issuer role), RFC-0959 (Asker role), RFC-0957-A1 (unified HolderRegistry). Cross-references: RFC-0870 §Roles, RFC-0957 §Roles, RFC-0959 §Roles, RFC-0968 §Roles.

## Acceptance Criteria

### Top-level: RFC-0971 acceptance roll-up

The sub-mission (0971-a) implements the ACs by RFC-0971 §Test Vectors. When 0971-a is complete and merged, every AC below is satisfied.

- [ ] All 8 RFC-0971 §Test Vectors pass (TV1: Role Binding Assertion (Required Roles Present), TV2: Cross-Role Data Flow — Deal Settlement, TV3: Cross-Role Data Flow — Forwarded Request, TV4: Role Binding Lifecycle, TV5: Role Binding Exit (R23-N1 fix: Router Resigned only deactivates Router), TV6: Pure Forwarder Exception (NEW), TV7: ReputationAnchor Optional (NEW), TV8: Cross-Role Audit Trail (NEW))
- [ ] All 3 RFC-0971 §Adversary Analysis findings covered (A18: Role confusion attack, A19: Single point of failure for deal settlement, A20: Cross-role audit trail ambiguity)
- [ ] Predicate-based definition `DestinationNode = Router ∧ TokenIssuer ∧ Asker` is canonical (R23-N9 fix; prior 'super-role' wording superseded)
- [ ] `ReputationAnchor` is OPTIONAL (R13-N8 fix; not every destination anchors reputation)
- [ ] Pure forwarder exception is explicit (Finding A18 defense)
- [ ] `seller_signature ≡ Asker signature` (R13-N8 fix: explicitly equivalent, not separate)
- [ ] `role_tag = RoleTag::Asker` typed enum (NO string literals)
- [ ] Sub-mission 0971-a merged and ACs flipped
- [ ] Cross-crate compat: `cargo build --workspace` green; `cargo test --workspace` green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --check` clean

### Type Coverage

| RFC-0971 Type | Implemented By |
|---------------|----------------|
| `RoleTag` typed enum (`Router`, `TokenIssuer`, `Asker`, `PureForwarder`, `ReputationAnchor`) | Sub-mission 0971-a |
| `RoleBindingDeclaration` struct | Sub-mission 0971-a |
| `RoleBindingLifecycle` state machine (Active, Draining, Suspended, Retired) | Sub-mission 0971-a |
| Cross-role data flow (deal settlement + forwarded request) | Sub-mission 0971-a |
| Cross-RFC §Roles updates (RFC-0870, RFC-0957, RFC-0959, RFC-0968) | Sub-mission 0971-a |
| Pure forwarder exception (configuration) | Sub-mission 0971-a |
| ReputationAnchor optional (configuration) | Sub-mission 0971-a |
| Role-binding audit trail (append-only log of transitions) | Sub-mission 0971-a |
| `destination-node-architecture.md` developer guide | Sub-mission 0971-a |

### Mission Dependency Model

```yaml
depends_on:
  - 0957-a1-holder-registry # unified HolderRegistry + capability registry
  - 0870-router-network-layer # Router role substrate
  - 0957-capability-token-format # Token Issuer role substrate
  - 0959-ask-settlement-chain # Asker role substrate
  - 0968-reputation-anchoring # ReputationAnchor role (optional)
  - 0969-dual-pipeline-authorization # routing role
  - 0970-forwarding-hop-auth-envelope # forwarding role
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
- RFC-0968 — ReputationAnchor role (OPTIONAL; not every destination anchors reputation per R13-N8 fix)
- RFC-0969 — routing role
- RFC-0970 — forwarding role

**Mission gates:**

- All 4 in-batch RFCs (RFC-0957-A1, RFC-0959-A1, RFC-0969, RFC-0970) MUST have at least one mission merged before this mission's sub-mission claims, because the role-binding declaration references all four sub-missions' types.

**Not Requires:**

- RFC-0958 (ZK subclass) — out of scope

## Implementation Guide

- RFC-0971 §Specification → §System Architecture → §Data Structures → §Algorithms → §Role-Binding Table → §Wire Format → §Cross-Role Data Flow → §Test Vectors (single canonical reference)
- RFC-0971 §Appendices: §RFC Cross-Reference Updates, §Why Not a Super-Role?, §Why Not Pure Forwarder + Mint Elsewhere?, §Example Configuration, §Pure Forwarder Configuration (No Binding)
- Developer guide: inline §Developer Guide section in sub-mission 0971-a (inline in this mission)

## Decomposition Rationale

RFC-0971 is borderline for decomposition per BLUEPRINT §Multi-Mission Decomposition:

- **4-6 new types** (RoleTag, RoleBindingDeclaration, RoleBindingLifecycle, audit trail) — does NOT exceed >10 threshold
- **3 implementation phases** (§Phase 1: Role Binding Declaration, §Phase 2: Cross-Role Data Flow Documentation, §Phase 3: Mission Decomposition) — does NOT exceed >4 threshold
- **Different prerequisite chains:** NOT applicable — all work depends on the same in-batch RFCs landing first

Despite not strictly exceeding the thresholds, decomposition into top-level + single sub-mission (0971-a) is preserved for consistency with the other 4 RFCs in the batch. The top-level captures acceptance criteria + Type Coverage roll-up + cross-RFC cross-reference targets; the sub-mission does the actual work.

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- Predicate-based definition `DestinationNode = Router ∧ TokenIssuer ∧ Asker` is canonical per R23-N9 fix (Round 21 R13-N8 superseded). Prior 'super-role' wording in earlier drafts was REJECTED per RFC-0971 §Appendix B "Why Not a Super-Role?".
- `ReputationAnchor` is OPTIONAL (R13-N8 fix). Cross-reference to RFC-0968: destination nodes MAY bind the ReputationAnchor role; they are NOT REQUIRED to.
- Pure forwarder exception: nodes that do NOT bind to the unified role forward without role-binding audit. The `pure_forward` algorithm from RFC-0970 §Algorithms + the `HopScope::PureForwarder` variant from RFC-0970 §Data Structures are the substrate.
- `seller_signature ≡ Asker signature` (R13-N8 fix: explicitly equivalent). This means the same Ed25519 keypair signs both the `Ask` (RFC-0959) and the `DealSettled` event (RFC-0959-A1). NOT two separate keys.
- Cross-RFC §Roles updates: RFC-0870, RFC-0957, RFC-0959, RFC-0968 §Roles sections all gain a cross-reference to RFC-0971. Documentation-only change.
- Role-binding audit trail: append-only log of role-binding transitions (Active → Draining → Suspended → Retired). Per Finding A20 (cross-role audit trail ambiguity), the audit trail MUST include the `role_tag` (typed enum, NOT string literal) and the `node_epoch` for replay protection.
- Developer guide §Developer Guide section (inline in sub-mission 0971-a) documents the role-binding declaration + the pure forwarder exception + the ReputationAnchor opt-in.

### Related

- [Dual-Mode Authorization Batch Accepted 2026-08-02](../rfcs/accepted/networking/0971-destination-node-role-consolidation.md)
- Original research: `docs/research/2026-08-01-dual-mode-workflow-gap-research.md`
- Original use case: `docs/use-cases/dual-mode-authorization-workflow.md`
