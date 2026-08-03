# Mission: Role Binding Implementation + Cross-RFC §Roles Updates (RFC-0971)

## Status

Open

## RFC

RFC-0971 (Networking): Destination-Node Role Consolidation — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0971-destination-node-role-consolidation.md` (top-level decomposition mission)

## Summary

Implement RFC-0971 §Phase 1 (Role Binding Declaration) and §Phase 2 (Cross-Role Data Flow Documentation). Author `RoleTag` typed enum (5 variants: `Router`, `TokenIssuer`, `Asker`, `PureForwarder`, `ReputationAnchor`), `RoleBindingDeclaration` struct, `RoleBindingLifecycle` state machine (Active, Draining, Suspended, Retired), cross-role data flow documentation for deal settlement + forwarded request. Implement role-binding audit trail (append-only log of transitions). Update RFC-0870, RFC-0957, RFC-0959, RFC-0955-R1 §Roles sections with cross-references to RFC-0971. Author inline §Developer Guide section (inline in this mission).

Predicate-based definition `DestinationNode = Router ∧ TokenIssuer ∧ Asker` is canonical (R23-N9 fix). `ReputationAnchor` is OPTIONAL (R13-N8 fix). Pure forwarder exception is explicit (Finding A18 defense). `seller_signature ≡ Asker signature`.

## Acceptance Criteria

### Type definitions

- [ ] `crates/quota-router-core/src/node/role_binding.rs` (NEW) — `RoleTag` typed enum: `Router`, `TokenIssuer`, `Asker`, `PureForwarder`, `ReputationAnchor`. NO string literals — typed enum enforced at compile time.
- [ ] `RoleBindingDeclaration` struct: `node_did: Did`, `required_roles: BTreeSet<RoleTag>` (must contain `{Router, TokenIssuer, Asker}` per predicate), `optional_roles: BTreeSet<RoleTag>` (may contain `ReputationAnchor`), `lifecycle: RoleBindingLifecycle`, `minted_at_millis_unix: i64`.
- [ ] `RoleBindingLifecycle` state machine: `Active`, `Draining`, `Suspended`, `Retired`. Transitions per RFC-0971 §Lifecycle Requirements §Role-Binding State Machine.

### Cross-role data flow

- [ ] Documentation + tests for cross-role data flow: `DealSettled` (RFC-0959-A1) flows through `Asker` → `TokenIssuer` (mints CapabilityToken via `CapabilityToken::mint` per RFC-0957-A1) → `Router` (forwards via `ForwardRequestPayload` per RFC-0970). End-to-end integration test.
- [ ] Cross-role data flow audit trail entry emitted at each transition.

### Pure forwarder exception

- [ ] Configuration: `RoleBindingDeclaration` with `required_roles = {}` (empty) + `optional_roles = {PureForwarder}` declares a pure forwarder node. No `Router` / `TokenIssuer` / `Asker` binding.
- [ ] Pure forwarder does NOT emit `DealSettled` events (no `Asker` binding) and does NOT mint tokens (no `TokenIssuer` binding).

### ReputationAnchor optional

- [ ] Configuration: `RoleBindingDeclaration` with `required_roles = {Router, TokenIssuer, Asker}` + `optional_roles = {ReputationAnchor}` declares a destination node that MAY anchor reputation. NOT REQUIRED to.
- [ ] ReputationAnchor binding is configured at runtime; absence does not block deal settlement or forwarding.

### Role-binding audit trail

- [ ] `crates/quota-router-core/src/node/role_binding_audit.rs` (NEW) — append-only log of role-binding transitions. Per entry: `node_did`, `role_tag: RoleTag` (typed enum), `from_state: RoleBindingLifecycle`, `to_state: RoleBindingLifecycle`, `node_epoch: u64`, `at_millis_unix: i64`.
- [ ] Manual redacting Debug on `RoleBindingAuditEntry` (redact `node_did` per audit log convention; preserve `role_tag` for forensics).

### Cross-RFC §Roles updates

- [ ] RFC-0870 §Roles documentation updated: add `RFC-0971` cross-reference.
- [ ] RFC-0957 §Roles documentation updated: add `RFC-0971` cross-reference.
- [ ] RFC-0959 §Roles documentation updated: add `RFC-0971` cross-reference.
- [ ] RFC-0955-R1 §Roles documentation updated: add `RFC-0971` cross-reference.

### Developer guide (inline §Developer Guide section in this mission)

- [ ] §Developer Guide section authored inline in this mission (inline in this mission). Sections: role-binding declaration, pure forwarder exception, ReputationAnchor opt-in, cross-role data flow, audit trail, troubleshooting.

### Test vectors (RFC-0971 §Test Vectors, all 8 live in this sub-mission)

- [ ] TV1: Role Binding Assertion (Required Roles Present) — `RoleBindingDeclaration { required_roles: {Router, TokenIssuer, Asker} }` validates; missing any one of the three returns `RoleBindingError::MissingRequiredRole`.
- [ ] TV2: Cross-Role Data Flow — Deal Settlement — end-to-end: Asker creates Ask → TokenIssuer mints capability → Seller signs `DealSettled` → all audit entries emitted with correct `role_tag`.
- [ ] TV3: Cross-Role Data Flow — Forwarded Request — end-to-end: Router forwards `ForwardRequestPayload` with `hop_envelope` per RFC-0970 → destination unwraps → audit entries emitted.
- [ ] TV4: Role Binding Lifecycle — transitions Active → Draining → Suspended → Retired. Invalid transitions (e.g., Active → Retired directly) return `RoleBindingError::InvalidTransition`.
- [ ] TV5: Role Binding Exit (R23-N1 fix: Router Resigned only deactivates Router) — Router lifecycle resignation deactivates Router role; TokenIssuer + Asker remain Active.
- [ ] TV6: Pure Forwarder Exception (NEW) — pure forwarder config (`required_roles = {}`, `optional_roles = {PureForwarder}`) accepts forwarded requests but rejects deal settlement attempts.
- [ ] TV7: ReputationAnchor Optional (NEW) — destination node without ReputationAnchor binding performs deal settlement; reputation-anchoring attempts return `RoleBindingError::RoleNotBound`.
- [ ] TV8: Cross-Role Audit Trail (NEW) — every role-binding transition emits an audit entry with typed `role_tag` (no string literals); grep test confirms zero string literals in audit entries.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] `cargo doc --workspace --no-deps` builds without broken-doc-link warnings

## Dependencies

**Requires (RFC gates):**

- RFC-0009 — node identity primitive
- RFC-0853 — per-hop channel binding
- RFC-0862 — HolderRegistry gossip
- RFC-0870 — Router role substrate
- RFC-0957 — Token Issuer role substrate
- RFC-0957-A1 — unified HolderRegistry
- RFC-0959 — Asker role substrate
- RFC-0959-A1 — `DealSettled` event signing
- RFC-0955-R1 — ReputationAnchor role (OPTIONAL)
- RFC-0969 — routing role substrate
- RFC-0970 — forwarding role substrate

**Requires (mission gates):**

- `missions/open/0971-destination-node-role-consolidation.md` (top-level)
- At least one sub-mission from each of the 4 in-batch RFCs MUST be merged (0957-c, 0959-b, 0969-a, 0970-a) so that the role-binding declaration references real types.

```yaml
depends_on:
  - 0957-c-holder-registry-impl # HolderRecord + HolderKind (consumed by Router audit trail)
  - 0959-b-market-delivery-impl # DealSettled (consumed by Asker audit trail)
  - 0969-a-dual-pipeline-gateway # GatewayAuthenticator (consumed by Router audit trail)
  - 0970-a-hop-envelope # HopEnvelope (consumed by Router forwarding audit trail)
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `RoleTag` typed enum
- `RoleBindingDeclaration` struct
- `RoleBindingLifecycle` state machine
- Cross-role data flow documentation + tests
- Cross-RFC §Roles updates
- Pure forwarder exception
- ReputationAnchor optional
- Role-binding audit trail
- Inline §Developer Guide section (per docs/07-developers/ rule; no external developer-guide file)

## Location

- `crates/quota-router-core/src/node/role_binding.rs` (NEW)
- `crates/quota-router-core/src/node/role_binding_audit.rs` (NEW)
- §Developer Guide section in this mission (inline)
- `rfcs/accepted/networking/0870-router-network-layer.md` (MODIFY) — §Roles cross-reference
- `rfcs/accepted/economics/0957-capability-token-format.md` (MODIFY) — §Roles cross-reference
- `rfcs/accepted/economics/0959-ask-settlement-chain.md` (MODIFY) — §Roles cross-reference
- `rfcs/accepted/economics/0955-r1-reputation-anchoring.md` (MODIFY) — §Roles cross-reference

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- This sub-mission owns ALL 8 test vectors. The pure documentation-update scope (4 RFC §Roles cross-references) is bundled with the implementation work because the cross-references are critical to the RFC-0971 acceptance criteria (Finding A18 defense: role confusion attack).
- `seller_signature ≡ Asker signature` (R13-N8 fix): the same Ed25519 keypair signs both the `Ask` (RFC-0959) and the `DealSettled` event (RFC-0959-A1). The `RoleBindingDeclaration` does NOT carry a separate `seller_keypair` field — it derives from the `Asker` role binding.
- Role-binding audit trail entries use typed `RoleTag` enum (NO string literals). TV8 grep test enforces.
- Developer guide §Developer Guide section (inline in this mission) is the canonical reference for destination-node operators. Recommended placement: same PR as the implementation.
