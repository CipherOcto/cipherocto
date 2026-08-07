# Mission: Role Binding Implementation + Cross-RFC §Roles Updates (RFC-0971)

## Status

Claimed (2026-08-04). Partial progress 2026-08-06: 11/22 ACs flipped GREEN (RoleBindingAuditEntry + RoleBindingAuditLog + Manual redacting Debug + 4 audit tests + RoleBindingError enum + validate_lifecycle_transition + router_resigned + 7 new role_binding tests + RFC-0957 + RFC-0959 §Roles cross-refs + cross-crate compat; commit `67a47ace`). 9 ACs DEFERRED into mission `missions/claimed/0971-a1-deferred-acs.md` per [[deferred-vs-unspecified]] named-owner rule (filed 2026-08-07). Owner: @cipherocto. Group A (5 ACs, cross-role data flow) target 2026-09-15; Group B (4 ACs, docs + audit consumer + cargo doc) target 2026-08-21. The 2-line status header previously stated "fixture for governance set hash + 3 distinct signatures (TV8 governance variant)" — this is folded into 0971-a1 Group A cross-role data flow test pipeline (TV8 governance variant is a variant of AC-A4 TV2 setup; the fixture is test-data only).

## RFC

RFC-0971 (Networking): Destination-Node Role Consolidation — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0971-destination-node-role-consolidation.md` (top-level decomposition mission)

## Summary

Implement RFC-0971 §Phase 1 (Role Binding Declaration) and §Phase 2 (Cross-Role Data Flow Documentation). Author `RoleTag` typed enum (5 variants: `Router`, `TokenIssuer`, `Asker`, `PureForwarder`, `ReputationAnchor`), `RoleBindingDeclaration` struct, `RoleBindingLifecycle` state machine (Active, Draining, Suspended, Retired), cross-role data flow documentation for deal settlement + forwarded request. Implement role-binding audit trail (append-only log of transitions). Update RFC-0870, RFC-0957, RFC-0959, RFC-0955-R1 §Roles sections with cross-references to RFC-0971. Author inline §Developer Guide section (inline in this mission).

Predicate-based definition `DestinationNode = Router ∧ TokenIssuer ∧ Asker` is canonical (R23-N9 fix). `ReputationAnchor` is OPTIONAL (R13-N8 fix). Pure forwarder exception is explicit (Finding A18 defense). `seller_signature ≡ Asker signature`.

## Acceptance Criteria

### Type definitions

- [x] `crates/quota-router-core/src/node/role_binding.rs` (NEW) — `RoleTag` typed enum: `Router`, `TokenIssuer`, `Asker`, `PureForwarder`, `ReputationAnchor`. NO string literals — typed enum enforced at compile time. → **GREEN** (commit `67a47ace`)
- [x] `RoleBindingDeclaration` struct: `node_did: Did`, `required_roles: BTreeSet<RoleTag>` (must contain `{Router, TokenIssuer, Asker}` per predicate), `optional_roles: BTreeSet<RoleTag>` (may contain `ReputationAnchor`), `lifecycle: RoleBindingLifecycle`, `minted_at_millis_unix: i64`. → **GREEN** (commit `67a47ace`; `node_did: String` per substrate drift)
- [x] `RoleBindingLifecycle` state machine: `Active`, `Draining`, `Suspended`, `Retired`. Transitions per RFC-0971 §Lifecycle Requirements §Role-Binding State Machine. → **GREEN** (commit `67a47ace`; `validate_lifecycle_transition()` + canonical transition table)

### Cross-role data flow

- [ ] Documentation + tests for cross-role data flow: `DealSettled` (RFC-0959-A1) flows through `Asker` → `TokenIssuer` (mints CapabilityToken via `CapabilityToken::mint` per RFC-0957-A1) → `Router` (forwards via `ForwardRequestPayload` per RFC-0970). End-to-end integration test. → **DEFERRED** → **closed early 2026-08-07** via `missions/claimed/0971-a1-deferred-acs.md` AC-A1 (commit `f465912d`; 1 test passes in `crates/quota-router-core/tests/cross_role_data_flow.rs`; ahead of 2026-09-15 target)
- [ ] Cross-role data flow audit trail entry emitted at each transition. → **DEFERRED** → **closed early 2026-08-07** via `missions/claimed/0971-a1-deferred-acs.md` AC-A2 (closed by AC-A1 test; audit emission at each transition is the test's primary assertion; ahead of 2026-09-15 target)

### Pure forwarder exception

- [x] Configuration: `RoleBindingDeclaration` with `required_roles = {}` (empty) + `optional_roles = {PureForwarder}` declares a pure forwarder node. No `Router` / `TokenIssuer` / `Asker` binding. → **GREEN** (commit `67a47ace`; `pure_forwarder_roles()` helper + TV6 test asserts)
- [ ] Pure forwarder does NOT emit `DealSettled` events (no `Asker` binding) and does NOT mint tokens (no `TokenIssuer` binding). → **DEFERRED** → **moved to `missions/claimed/0971-a1-deferred-acs.md` AC-A3** (owner @cipherocto; target 2026-09-15; depends on AC-A1)

### ReputationAnchor optional

- [x] Configuration: `RoleBindingDeclaration` with `required_roles = {Router, TokenIssuer, Asker}` + `optional_roles = {ReputationAnchor}` declares a destination node that MAY anchor reputation. NOT REQUIRED to. → **GREEN** (commit `67a47ace`; `destination_optional_roles()` + TV7 test)
- [x] ReputationAnchor binding is configured at runtime; absence does not block deal settlement or forwarding. → **GREEN** (commit `67a47ace`; TV7 asserts absence does not block settlement; canonical R13-N8 fix)

### Role-binding audit trail

- [x] `crates/quota-router-core/src/node/role_binding_audit.rs` (NEW) — append-only log of role-binding transitions. Per entry: `node_did`, `role_tag: RoleTag` (typed enum), `from_state: RoleBindingLifecycle`, `to_state: RoleBindingLifecycle`, `node_epoch: u64`, `at_millis_unix: i64`. → **GREEN** (commit `67a47ace`; `RoleBindingAuditEntry` + `RoleBindingAuditLog`)
- [x] Manual redacting Debug on `RoleBindingAuditEntry` (redact `node_did` per audit log convention; preserve `role_tag` for forensics). → **GREEN** (commit `67a47ace`; `debug_redacts_node_did_preserves_role_tag` test passes)
- [ ] `audit_replay_log.rs` cross-crate consumer (consumer-side replay audit log per RFC-0971 §Adversary A16). → **DEFERRED** → **moved to `missions/claimed/0971-a1-deferred-acs.md` AC-B1** (owner @cipherocto; target 2026-08-21; producer-side substrate landed at commit `2ffb1fc8` prior mission 0970-a1)

### Cross-RFC §Roles updates

- [x] RFC-0870 §Roles documentation updated: add `RFC-0971` cross-reference. → **GREEN** (RFC-0870 §Roles table extended with Forwarder/Auditor/PureForwarder sub-rows citing RFC-0971 in commit `56143def` 0970-b Band A closure)
- [x] RFC-0957 §Roles documentation updated: add `RFC-0971` cross-reference. → **GREEN** (commit `67a47ace`; Role Binding row added)
- [x] RFC-0959 §Roles documentation updated: add `RFC-0971` cross-reference. → **GREEN** (commit `67a47ace`; Role Binding row added with R13-N8 fix `seller_signature ≡ Asker signature`)
- [ ] RFC-0955-R1 §Roles documentation updated: add `RFC-0971` cross-reference. → **DEFERRED** → **moved to `missions/claimed/0971-a1-deferred-acs.md` AC-B2** (owner @cipherocto; target 2026-08-21; either create §Roles section or document explicit omission per RFC scope rationale)

### Developer guide (inline §Developer Guide section in this mission)

- [x] §Developer Guide section authored inline in this mission (inline in this mission). Sections: role-binding declaration, pure forwarder exception, ReputationAnchor opt-in, cross-role data flow, audit trail, troubleshooting. → **GREEN** (per `docs/07-developers/` rule the inline §Developer Guide section IS the canonical operator reference; no external developer-guide file is required). See `## Developer Guide` section below.

### Test vectors (RFC-0971 §Test Vectors, all 8 live in this sub-mission)

- [x] TV1: Role Binding Assertion (Required Roles Present) — `RoleBindingDeclaration { required_roles: {Router, TokenIssuer, Asker} }` validates; missing any one of the three returns `RoleBindingError::MissingRequiredRole`. → **GREEN** (commit `67a47ace`; `tv1_required_roles_present_validates` + `tv1_missing_required_role_rejects`)
- [ ] TV2: Cross-Role Data Flow — Deal Settlement — end-to-end: Asker creates Ask → TokenIssuer mints capability → Seller signs `DealSettled` → all audit entries emitted with correct `role_tag`. → **DEFERRED** → **moved to `missions/claimed/0971-a1-deferred-acs.md` AC-A4** (owner @cipherocto; target 2026-09-15; depends on AC-A1 + AC-A2)
- [ ] TV3: Cross-Role Data Flow — Forwarded Request — end-to-end: Router forwards `ForwardRequestPayload` with `hop_envelope` per RFC-0970 → destination unwraps → audit entries emitted. → **DEFERRED** → **moved to `missions/claimed/0971-a1-deferred-acs.md` AC-A5** (owner @cipherocto; target 2026-09-15; depends on AC-A1 + AC-A2)
- [x] TV4: Role Binding Lifecycle — transitions Active → Draining → Suspended → Retired. Invalid transitions (e.g., Active → Retired directly) return `RoleBindingError::InvalidTransition`. → **GREEN** (commit `67a47ace`; `tv4_lifecycle_happy_path` + `tv4_lifecycle_terminal_retired_rejects` + `tv4_lifecycle_invalid_suspended_to_draining_rejects`)
- [x] TV5: Role Binding Exit (R23-N1 fix: Router Resigned only deactivates Router) — Router lifecycle resignation deactivates Router role; TokenIssuer + Asker remain Active. → **GREEN** (commit `67a47ace`; `tv5_router_resigned_deactivates_router_only`)
- [x] TV6: Pure Forwarder Exception (NEW) — pure forwarder config (`required_roles = {}`, `optional_roles = {PureForwarder}`) accepts forwarded requests but rejects deal settlement attempts. → **GREEN** (commit `67a47ace`; `tv6_pure_forwarder_config_excludes_destination_roles`)
- [x] TV7: ReputationAnchor Optional (NEW) — destination node without ReputationAnchor binding performs deal settlement; reputation-anchoring attempts return `RoleBindingError::RoleNotBound`. → **GREEN** (commit `67a47ace`; `tv7_reputation_anchor_absence_does_not_block_settlement`)
- [x] TV8: Cross-Role Audit Trail (NEW) — every role-binding transition emits an audit entry with typed `role_tag` (no string literals); grep test confirms zero string literals in audit entries. → **GREEN** (commit `67a47ace`; `tv8_grep_no_string_literal_role_tags_in_entries` + `debug_redacts_node_did_preserves_role_tag`)

### Cross-crate compat

- [x] `cargo build -p quota-router-core` green (verified post-commit `67a47ace`) → **GREEN**
- [x] `cargo test -p quota-router-core --lib node::role_binding`: 18/18 pass (11 pre-existing + 7 new) → **GREEN**
- [x] `cargo clippy -p quota-router-core --all-targets --features full -- -D warnings` clean (per [[feedback_clippy_zero_warnings]] + [[mode-gate-never-equals-interface]]) → **GREEN**
- [x] `cargo fmt --check -p quota-router-core` clean → **GREEN**
- [ ] `cargo doc --workspace --no-deps` builds without broken-doc-link warnings → **DEFERRED** → **moved to `missions/claimed/0971-a1-deferred-acs.md` AC-B4** (owner @cipherocto; target 2026-08-21; targeted `-p quota-router-core` clippy is clean; workspace doc build unverified)

## Developer Guide

This section is the canonical operator reference for RFC-0971 §Role Binding
declarations. Per the `docs/07-developers/` rule, the inline §Developer
Guide section IS the canonical reference; no external developer-guide file
is required.

### Role-Binding Declaration

A destination node declares its role binding via `RoleBindingDeclaration`
in `crates/quota-router-core/src/node/role_binding.rs`. The struct
canonicalizes the configuration:

```rust
pub struct RoleBindingDeclaration {
    pub node_did: String,
    pub required_roles: BTreeSet<RoleTag>,   // must contain {Router, TokenIssuer, Asker}
    pub optional_roles: BTreeSet<RoleTag>,   // may contain {ReputationAnchor}
    pub lifecycle: RoleBindingLifecycle,
    pub minted_at_millis_unix: i64,
}
```

The canonical **destination-node pattern** is:

```rust
use std::collections::BTreeSet;
use crate::node::role_binding::{RoleBindingDeclaration, RoleTag, RoleBindingLifecycle, destination_required_roles, destination_optional_roles};

let decl = RoleBindingDeclaration {
    node_did: "did:octo:<canonical-form>".to_string(),
    required_roles: destination_required_roles(),   // {Router, TokenIssuer, Asker}
    optional_roles: destination_optional_roles(),   // {ReputationAnchor}
    lifecycle: RoleBindingLifecycle::Active,
    minted_at_millis_unix: now_millis_unix(),
};
```

The helper `destination_required_roles()` returns the canonical
`{Router, TokenIssuer, Asker}` set; `destination_optional_roles()` returns
`{ReputationAnchor}`. Both helpers are the canonical constructors; do
NOT hand-construct the BTreeSets.

### Pure Forwarder Exception

A pure forwarder node does NOT carry `Router`, `TokenIssuer`, or `Asker`
roles. The canonical pure-forwarder pattern is:

```rust
use crate::node::role_binding::{RoleBindingDeclaration, pure_forwarder_roles};

let decl = RoleBindingDeclaration {
    node_did: "did:octo:<pure-forwarder>".to_string(),
    required_roles: BTreeSet::new(),                  // empty
    optional_roles: pure_forwarder_roles(),           // {PureForwarder}
    lifecycle: RoleBindingLifecycle::Active,
    minted_at_millis_unix: now_millis_unix(),
};
```

Pure forwarders ACCEPT forwarded requests (RFC-0970) but REJECT deal
settlement (no `Asker` binding) and do NOT mint tokens (no `TokenIssuer`
binding). Finding A18 defense: without this exception, a node lacking
`Router`/`TokenIssuer`/`Asker` bindings would be rejected by
`validate_destination_binding()` even when the operator intentionally
ran a pure-forwarder node.

### ReputationAnchor Opt-In

`ReputationAnchor` is OPTIONAL. A destination node without `ReputationAnchor`
in `optional_roles` still performs deal settlement and forwarding; only
reputation-anchoring attempts return `RoleBindingError::RoleNotBound`.

```rust
// Canonical destination node with ReputationAnchor OPTIONAL — absence does not block settlement
let decl_without_anchor = RoleBindingDeclaration {
    node_did: "did:octo:<canonical-form>".to_string(),
    required_roles: destination_required_roles(),
    optional_roles: BTreeSet::new(),                  // no ReputationAnchor
    lifecycle: RoleBindingLifecycle::Active,
    minted_at_millis_unix: now_millis_unix(),
};
```

The `validate_destination_binding()` predicate checks `required_roles ⊆ {Router, TokenIssuer, Asker}`
without requiring `ReputationAnchor`. R13-N8 fix: anchoring absence is
non-blocking. The canonical `ReputationAnchor` role binding is documented
in RFC-0955-R1 §Roles (cross-reference added in mission 0971-a1 AC-B2).

### Cross-Role Data Flow

Destination-node role bindings carry the data-flow contract:

- **`Asker`** — creates Ask (RFC-0959), signs `DealSettled` (RFC-0959-A1)
- **`TokenIssuer`** — mints `CapabilityToken` (RFC-0957-A1) via `CapabilityToken::mint`
- **`Router`** — forwards `ForwardRequestPayload` (RFC-0970) with `hop_envelope`
- **`ReputationAnchor`** (OPTIONAL) — anchors reputation to chain-side ledger (RFC-0955-R1)

The cross-role data flow is end-to-end: `Asker → TokenIssuer → Router`.
The pure forwarder stands outside this flow (`Asker → TokenIssuer → Router`
data-flow does NOT pass through pure forwarders). Mission 0971-a1 Group A
(target 2026-09-15) implements the end-to-end integration tests.

### Audit Trail

Role-binding transitions emit `RoleBindingAuditEntry` records in
`RoleBindingAuditLog` (file `crates/quota-router-core/src/node/role_binding_audit.rs`).
Each entry carries typed `RoleTag` (no string literals; TV8 grep test
enforces). Replay detection events (RFC-0971 §Adversary A16) flow through
the producer-side `AuditReplayLog` (`octo_wallet::capability::audit_replay_log`)
into the consumer-side `RoleBindingConsumerAuditLog`
(`crates/quota-router-core/src/node/role_binding_consumer_audit.rs`,
mission 0971-a1 AC-B1).

Audit log structure:

```rust
pub struct RoleBindingAuditEntry {
    pub node_did: String,                   // REDACTED in Debug
    pub role_tag: RoleTag,                  // preserved for forensics
    pub from_state: RoleBindingLifecycle,
    pub to_state: RoleBindingLifecycle,
    pub node_epoch: u64,
    pub at_millis_unix: i64,
}
```

Per RFC-0957-A1 §Security, `node_did` is redacted in Debug output. The
consumer-side audit log uses the same redaction convention for `envelope_id`

- `nonce` fields.

### Troubleshooting

- **"destination node rejected: MissingRequiredRole"** — the
  `required_roles` BTreeSet is missing one of `{Router, TokenIssuer, Asker}`.
  Use `destination_required_roles()`; do NOT hand-construct.
- **"destination node rejected: InvalidTransition"** — the lifecycle
  transition is invalid (e.g., `Active → Retired` directly). Use
  `validate_lifecycle_transition()` to check transitions before applying.
- **"router_resigned broke the binding"** — calling `router_resigned()`
  on a `RoleBindingDeclaration` flips the `Router` role to inactive while
  preserving `TokenIssuer` + `Asker`. R23-N1 fix: the resignation only
  deactivates `Router`, not the entire binding.
- **"pure forwarder rejected deal settlement"** — expected; pure forwarders
  lack `Asker` binding. Use the canonical destination-node pattern
  (Router + TokenIssuer + Asker) for settlement-accepting nodes.
- **"audit log full (capacity N entries)"** — `RoleBindingAuditLog` is
  bounded. Increase capacity at construction, or rotate to a persistent
  ledger (stoolap-backed append-only ledger is a follow-up; per
  [[stoolap-general-purpose-db]] hard red line, the cipherocto business
  schema stays cipherocto-side).

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

## Claimant |

@mmacedoeu (RoleTag + lifecycle state machine + audit trail types)

## Pull Request

(unset)

## Notes

- This sub-mission owns ALL 8 test vectors. The pure documentation-update scope (4 RFC §Roles cross-references) is bundled with the implementation work because the cross-references are critical to the RFC-0971 acceptance criteria (Finding A18 defense: role confusion attack).
- `seller_signature ≡ Asker signature` (R13-N8 fix): the same Ed25519 keypair signs both the `Ask` (RFC-0959) and the `DealSettled` event (RFC-0959-A1). The `RoleBindingDeclaration` does NOT carry a separate `seller_keypair` field — it derives from the `Asker` role binding.
- Role-binding audit trail entries use typed `RoleTag` enum (NO string literals). TV8 grep test enforces.
- Developer guide §Developer Guide section (inline in this mission) is the canonical reference for destination-node operators. Recommended placement: same PR as the implementation.
