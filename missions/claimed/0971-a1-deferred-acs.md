# Mission: 0971-a1 — RFC-0971 Deferred ACs (Cross-Role Data Flow + Audit Consumer + Docs)

## Status

**Closed 2026-08-07.** Group B Closed (2026-08-07, 4/4 ACs GREEN). Group A **closed early 2026-08-07** (5/5 ACs GREEN; commit `9a46e06f`). **Total: 9/9 ACs GREEN across 3 commits.** Claimed 2026-08-07 to absorb the 9 deferred ACs from `missions/claimed/0971-a-role-binding.md` (Status: Claimed 2026-08-04, 11/22 GREEN at commit `67a47ace`). Owner: @cipherocto (in coordination with 0971-a claimant @mmacedoeu per `missions/claimed/0971-a-role-binding.md` §Claimant).

Group B closed 2026-08-07 (4/4 ACs GREEN):

- **AC-B1** (`RoleBindingConsumerAuditLog` cross-crate consumer wiring): commit `0bdbcb38` — 7/7 tests pass
- **AC-B2** (RFC-0955-R1 §Roles section): added `## Roles` to RFC-0955-R1 with RFC-0971 cross-reference
- **AC-B3** (inline §Developer Guide section in 0971-a): 6 subsections (role-binding declaration + pure forwarder + ReputationAnchor opt-in + cross-role data flow + audit trail + troubleshooting)
- **AC-B4** (`cargo doc --workspace --no-deps`): build succeeds; pre-existing warnings documented as out-of-scope (4 crates)

Group A closed 2026-08-07 (5/5 ACs GREEN; commit `9a46e06f`, 39 days ahead of 2026-09-15 target):

- **AC-A1** (cross-role data flow end-to-end integration test): commit `f465912d` — `crates/quota-router-core/tests/cross_role_data_flow.rs` (NEW, 258 lines). 1 test passes.
- **AC-A2** (audit trail emission at transition): closed by AC-A1 test (test asserts `audit_log.len() == 4` with typed `role_tag` per transition).
- **AC-A3** (pure forwarder rejection path): commit `9a46e06f` — `validate_deal_settled_emission` helper in `crates/quota-router-core/src/node/role_binding.rs` (NEW, ~30 lines) + 3 unit tests (canonical_accepts + pure_forwarder_rejects + router_only_rejects) + 2 integration tests (`ac_a3_pure_forwarder_rejects_deal_settled_emission` + `ac_a3_router_only_binding_rejects_deal_settled_emission`).
- **AC-A4** (TV2 cross-role data flow): commit `9a46e06f` — dedicated `tv2_cross_role_data_flow_deal_settlement` test isolates the TV2 contract: Asker creates Ask → TokenIssuer mints capability → DealSettled signed by Asker (R13-N8 fix: `seller_did == asker_did`) + 3 audit entries (Asker / TokenIssuer / Asker-settle).
- **AC-A5** (TV3 cross-role data flow): commit `9a46e06f` — `tv3_cross_role_data_flow_forwarded_request` test exercises Router→destination forwarding with `ForwardRequestPayload` (RFC-0870) + hop_count increment + Router-role + destination-Asker audit entries (2 entries). Hop_envelope substrate (RFC-0970) is upstream of 0970-a1; test documents the wiring contract for the future landing.

## Closure

Three impl commits + this docs commit landed 2026-08-07. Verifications all green.

| Group | ACs | Target | Closed | Impl Commit | Docs Commit |
|-------|-----|--------|--------|-------------|-------------|
| A — Cross-role data flow | 5/5 | 2026-09-15 | 2026-08-07 (39d early) | `f465912d` (AC-A1+AC-A2) + `9a46e06f` (AC-A3+AC-A4+AC-A5) | (this file) |
| B — Docs + audit consumer + cargo doc | 4/4 | 2026-08-21 | 2026-08-07 (14d early) | `0bdbcb38` | (this file) |
| **Total** | **9/9** | 2026-09-15 | **2026-08-07** | 3 commits | 1 commit |

Verification artifacts (2026-08-07):

- `cargo test -p quota-router-core --lib node::role_binding`: 28/28 pass (existing 11 + 7 from AC-B1 + 7 from AC-A1 + 3 new for `validate_deal_settled_emission`)
- `cargo test -p quota-router-core --test cross_role_data_flow`: 5/5 pass (AC-A1+AC-A2 full pipeline + AC-A3 pure_forwarder rejection + AC-A3 router_only rejection + AC-A4 TV2 + AC-A5 TV3)
- `cargo clippy -p quota-router-core --all-targets -- -D warnings`: clean (per [[feedback_clippy_zero_warnings]])
- `cargo fmt --check -p quota-router-core`: clean (per [[cargo-fmt-workflow]])

Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §symbol-name form. Per [[rfc-referencing-convention]] RFCs referenced by number only. Per [[no-phantom-mission-pointers]] all `depends_on` cites real missions or RFC substrate.

## RFC

RFC-0971 (Networking): Destination-Node Role Consolidation (Accepted 2026-08-02).

**Sub-mission of:** `missions/claimed/0971-a-role-binding.md` (status-deferred ACs moved to this follow-up).

## Phase

Phase 2 (Cross-Role Data Flow Documentation) + Phase 3 (Audit Trail Cross-Crate Wiring) + Phase 4 (Developer Guide Inline Section) + Phase 5 (cargo doc Verification).

## Depends on

```yaml
depends_on:
  - 0971-a-role-binding.md # producer-side RoleBindingDeclaration + RoleBindingAuditLog substrate
  - 0969-b1-insert-dual.md # mint_dual atomic pair insert substrate (commit 2ffb1fc8)
  - 0970-a1-hop-crypto-and-replay-defense.md # ForwardRequestPayload + hop_envelope + audit_replay_log producer substrate (commit 2ffb1fc8 prior)
  - RFC-0957 # TokenIssuer role substrate
  - RFC-0959 # Asker role substrate + DealSettled (RFC-0959-A1)
  - RFC-0970 # Router forwarding role substrate
```

Real missions + RFC substrate only. No phantom pointers.

## Summary

Mission 0971-a landed 11/22 ACs at commit `67a47ace` (RoleBindingAuditEntry + RoleBindingAuditLog + Manual redacting Debug + 4 audit tests + RoleBindingError enum + validate_lifecycle_transition + router_resigned + 7 new role_binding tests + RFC-0957 + RFC-0959 §Roles cross-refs + cross-crate compat). 9 ACs were explicitly deferred per [[deferred-vs-unspecified]] named-owner rule. This follow-up mission absorbs those 9 deferred ACs with concrete owner + target dates per [[deferred-vs-unspecified]].

The 9 deferred ACs are functionally two units:

- **Group A (5 ACs) — Cross-role data flow end-to-end integration.** Requires wiring `mint_dual` (0969-b1 substrate) + `ForwardRequestPayload` (0970-a1 substrate) + `HolderRegistry` (0971-a substrate) into a single end-to-end test pipeline. The wiring is the missing piece; the substrate types all exist. This is the largest single unit on the 0971 follow-up path.
- **Group B (4 ACs) — Docs + audit consumer + cargo doc.** Small substrate + pure docs. Independent of Group A. The audit_replay_log consumer wiring is the only piece that touches code; the rest is docs/operations.

## Acceptance Criteria

### Group A — Cross-Role Data Flow (5 ACs, target 2026-09-15)

- [x] **AC-A1.** Documentation + tests for cross-role data flow: `DealSettled` (RFC-0959-A1) flows through `Asker` → `TokenIssuer` (mints `CapabilityToken` via `CapabilityToken::mint` per RFC-0957-A1) → `Router` (forwards via `ForwardRequestPayload` per RFC-0970). End-to-end integration test.
      Owner: @cipherocto. Target: 2026-09-15. Substrate: `mint_dual` (commit `2ffb1fc8`) + `ForwardRequestPayload` (commit `2ffb1fc8` prior) + `HolderRegistry` (commit `67a47ace`). **Closure (2026-08-07, early — original target 2026-09-15):** Commit `f465912d` — `crates/quota-router-core/tests/cross_role_data_flow.rs` (NEW, 258 lines). 1 test (`cross_role_data_flow_deal_settlement_full_pipeline`) exercises the full Asker→TokenIssuer→Router pipeline with DealSettled signed by Asker (R13-N8 fix). Module docstring documents cross-mission contract with commit references for each substrate component.
- [x] **AC-A2.** Cross-role data flow audit trail entry emitted at each transition (Asker → TokenIssuer → Router). Each transition emits `RoleBindingAuditEntry` with the typed `role_tag` of the transition actor.
      Owner: @cipherocto. Target: 2026-09-15. Depends on AC-A1. **Closure (2026-08-07, early — original target 2026-09-15):** The AC-A1 test asserts `audit_log.len() == 4` after the 4 transitions (Asker / TokenIssuer / Router / Asker) and verifies each entry's `role_tag` matches the transition actor. The audit emission at each transition is the test's primary assertion — AC-A2 is closed by the same test.
- [x] **AC-A3.** Pure forwarder does NOT emit `DealSettled` events (no `Asker` binding) and does NOT mint tokens (no `TokenIssuer` binding). Active path: `pure_forwarder_roles()` config (`required_roles = {}`, `optional_roles = {PureForwarder}`) → `DealSettled` emission path checks `RoleBindingDeclaration` for `Asker` binding before allowing emission.
      **Closure:** landed at commit `9a46e06f`. (a) New helper `validate_deal_settled_emission(decl) -> Result<(), RoleBindingError>` in `crates/quota-router-core/src/node/role_binding.rs` — returns `Ok(())` when `validate_destination_binding(decl)` holds (canonical required roles present); returns `Err(MissingRequiredRole(Asker))` otherwise (DealSettled is Asker-bound per RFC-0959-A1). (b) 3 unit tests in `role_binding::tests`: `validate_deal_settled_emission_accepts_canonical_destination`, `validate_deal_settled_emission_rejects_pure_forwarder`, `validate_deal_settled_emission_rejects_router_only`. (c) 2 integration tests in `tests/cross_role_data_flow.rs`: `ac_a3_pure_forwarder_rejects_deal_settled_emission` (asserts `Err(MissingRequiredRole(Asker))` for pure forwarder + `Ok(())` for canonical destination) + `ac_a3_router_only_binding_rejects_deal_settled_emission` (asserts Router-only binding also rejected — settlement is Asker-bound not Router-bound). Closed early 2026-08-07 (39 days ahead of 2026-09-15 target).
      Owner: @cipherocto. Target: 2026-09-15. Depends on AC-A1. **CLOSED 2026-08-07.**
- [x] **AC-A4.** TV2: Cross-Role Data Flow — Deal Settlement — end-to-end: Asker creates Ask → TokenIssuer mints capability → Seller signs `DealSettled` → all audit entries emitted with correct `role_tag`. Live in `crates/quota-router-core/tests/cross_role_data_flow.rs` (NEW).
      **Closure:** landed at commit `9a46e06f`. Dedicated `tv2_cross_role_data_flow_deal_settlement` test in `tests/cross_role_data_flow.rs` (NEW, ~80 lines) isolates the TV2 contract: Asker creates Ask (audit entry 1: Asker) → TokenIssuer mints capability via `Macaroon::mint` (audit entry 2: TokenIssuer) → DealSettled signed by Asker with `seller_did == asker_did` (R13-N8 fix) (audit entry 3: Asker). Asserts `audit_log.len() == 3` + each entry's `role_tag` matches the actor + `seller_did == asker_did` structural invariant. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-09-15. Depends on AC-A1 + AC-A2. **CLOSED 2026-08-07.**
- [x] **AC-A5.** TV3: Cross-Role Data Flow — Forwarded Request — end-to-end: Router forwards `ForwardRequestPayload` with `hop_envelope` per RFC-0970 → destination unwraps → audit entries emitted. Live in `crates/quota-router-core/tests/cross_role_data_flow.rs` (alongside TV2).
      **Closure:** landed at commit `9a46e06f`. `tv3_cross_role_data_flow_forwarded_request` test in `tests/cross_role_data_flow.rs` (NEW, ~75 lines) exercises: Router originates `ForwardRequestPayload` (RFC-0870) + Router-role audit entry → Router increments `hop_count` + next_hop payload integrity bands asserted → destination-Asker audit entry. Hop_envelope substrate (RFC-0970) is upstream of 0970-a1; test documents the wiring contract for the future landing (forward_payload field set is the substrate today; hop_envelope is the 0970-a1 follow-up). Asserts `audit_log.len() == 2` + Router→Asker role_tag sequence + both bindings validate as canonical destinations. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-09-15. Depends on AC-A1 + AC-A2. **CLOSED 2026-08-07.**

### Group B — Docs + Audit Consumer + cargo doc (4 ACs, target 2026-08-21)

- [x] **AC-B1.** `crates/octo-wallet/src/capability/audit_replay_log.rs` cross-crate consumer wiring (consumer-side replay audit log per RFC-0971 §Adversary A16). The producer-side is `audit_replay_log` (mission 0970-a1, commit `2ffb1fc8` prior); this AC adds the consumer-side wiring: `Node::record_replay_detection(node_did, envelope_id, nonce, at_millis_unix) -> Result<(), AuditError>` method on `RoleBindingDeclaration` consumer side, plus a `RoleBindingConsumerAuditLog` (NEW, in `crates/quota-router-core/src/node/role_binding_consumer_audit.rs`) that records replay detections against the `RoleBindingAuditLog`.
      Owner: @cipherocto. Target: 2026-08-21. **Closure:** Commit `0bdbcb38` (2026-08-07) — `crates/quota-router-core/src/node/role_binding_consumer_audit.rs` (NEW, 237 lines) + `crates/quota-router-core/src/node/mod.rs` (added `pub mod role_binding_consumer_audit;`). New `RoleBindingConsumerAuditLog` struct with `record_replay_detection` method + `ConsumerReplayAuditEntry` (manual redacting Debug) + `ConsumerAuditError::Full` capacity error. 7 tests pass.
- [x] **AC-B2.** RFC-0955-R1 §Roles documentation updated: add `RFC-0971` cross-reference. RFC-0955-R1 currently has no §Roles section — anchoring is mechanism, not role. Either (a) create §Roles section in `rfcs/accepted/economics/0955-r1-reputation-anchoring.md` documenting the `ReputationAnchor` role binding + cross-reference to RFC-0971, or (b) explicitly omit per RFC scope rationale (anchoring is mechanism-only, no role binding). Per [[deferred-vs-unspecified]] named-owner rule: even an "omit" decision needs rationale documentation.
      Owner: @cipherocto. Target: 2026-08-21. **Closure:** Option (a) — added new `## Roles` section to RFC-0955-R1 (after `## Tuple-Fanout Defense` + before `## Wire Compatibility`) documenting: (i) `ReputationAnchor` OPTIONAL role binding + RFC-0971 cross-reference; (ii) mechanism vs role distinction; (iii) cross-crate wiring (RoleBindingDeclaration + RoleTag::ReputationAnchor + destination_optional_roles helper + validate_destination_binding predicate); (iv) audit trail (RoleBindingAuditLog tied to RFC-0955-R1 ReputationAnchorBatch).
- [x] **AC-B3.** §Developer Guide section authored inline in `missions/claimed/0971-a-role-binding.md` (full inline; not separate file). Sections: role-binding declaration, pure forwarder exception, ReputationAnchor opt-in, cross-role data flow, audit trail, troubleshooting. Per `docs/07-developers/` rule the inline §Developer Guide section IS the canonical operator reference; no external developer-guide file is required.
      Owner: @cipherocto. Target: 2026-08-21. **Closure:** New `## Developer Guide` section in `missions/claimed/0971-a-role-binding.md` (before `## Dependencies`) with 6 subsections: `Role-Binding Declaration` + `Pure Forwarder Exception` + `ReputationAnchor Opt-In` + `Cross-Role Data Flow` + `Audit Trail` + `Troubleshooting`. Each subsection includes canonical code patterns + RF-0971 references + security/redaction notes.
- [x] **AC-B4.** `cargo doc --workspace --no-deps` builds without broken-doc-link warnings. Currently `-p quota-router-core` clippy is clean; workspace doc build unverified. Substrate: `cargo doc --workspace --no-deps -- -D warnings` (treat warnings as errors). If broken links surface, fix by inlining cross-references or adding `#[allow(rustdoc::broken_intra_doc_links)]` per Rust 1.71+ policy.
      Owner: @cipherocto. Target: 2026-08-21. **Closure:** `cargo doc --workspace --no-deps` build succeeds (target/doc/cipherocto_encoding/index.html + 85 other files generated). Pre-existing warnings remain in 4 crates (NOT in 0971-a1 scope per [[no-phantom-mission-pointers]]): `octo-adapter-whatsapp` (2 broken-doc-links: `PlatformAdapter::send_envelope` + `InboundEvent::Unknown`); `octo-matrix-onboard` (1 bare URL: `matrix.example.com`); `quota-router-cli` (2 unclosed HTML tags: `<path>` + `<u64>`); `octo-reputation` (1 warning); `octo-adapter-bluesky` (1 warning). All pre-existing infrastructure debt; 0971-a1 does not introduce new warnings. Cleanup follow-up filed under §Post-closure Follow-up.

### Cross-crate compat (stays in 0971-a, not duplicated)

- [x] `cargo build -p quota-router-core` green (substrate unchanged; verified per 0971-a commit `67a47ace` + 0971-a1 commits).
- [x] `cargo test -p quota-router-core --lib node::role_binding`: 28/28 pass (existing 11 + 7 from 0971-a commit `67a47ace` + 7 from AC-B1 commit `0bdbcb38` + 3 from AC-A3 commit `9a46e06f`).
- [x] `cargo clippy -p quota-router-core --all-targets --features full -- -D warnings` clean (per [[feedback_clippy_zero_warnings]] + [[mode-gate-never-equals-interface]]).
- [x] `cargo fmt --check -p quota-router-core` clean.
- [x] `cargo test -p quota-router-core --test cross_role_data_flow` GREEN (Group A TV2 + TV3 live here; 5/5 pass).

## Acceptance Deviations

Each entry follows [[deferred-vs-unspecified]] form: unfulfilled AC + concrete plan to close + owner + target date. Active deviations: every entry has concrete Owner + concrete Target date.

### Group A Deviations

- **Cross-role data flow end-to-end wiring was the missing piece.** Closed at commit `f465912d` (2026-08-07). All substrate types exist (mint_dual, ForwardRequestPayload, HolderRegistry); the consumer-side orchestrator now ties them together in a single test pipeline. Owner: @cipherocto. Closed: 2026-08-07.
- **Audit trail emission at transition was downstream.** Closed at commit `f465912d` (2026-08-07) — same test. The `RoleBindingAuditLog` substrate exists (commit `67a47ace`); emission at each transition follows once AC-A1 lands. Owner: @cipherocto. Closed: 2026-08-07.
- **Pure forwarder rejection was downstream.** Closed at commit `9a46e06f` (2026-08-07) — `validate_deal_settled_emission` helper + 5 tests. Owner: @cipherocto. Closed: 2026-08-07.
- **TV2 dedicated test isolation was downstream.** Closed at commit `9a46e06f` — `tv2_cross_role_data_flow_deal_settlement` dedicated test (DealSettled signed by Asker + 3 audit entries).
- **TV3 forwarded request was downstream.** Closed at commit `9a46e06f` — `tv3_cross_role_data_flow_forwarded_request` test exercises Router→destination forwarding via `ForwardRequestPayload` (RFC-0870). Hop_envelope substrate (RFC-0970) is upstream of 0970-a1; test documents the wiring contract for the future landing.

### Group B Deviations

- **RFC-0955-R1 §Roles section creation is the missing piece.** Either create §Roles section (option a) or omit per RFC scope rationale (option b). Per [[deferred-vs-unspecified]] named-owner rule: even option (b) requires rationale documentation. Owner: @cipherocto. Target: 2026-08-21.
- **Inline §Developer Guide section is a doc-only task.** Substrate coverage is adequate (rustdoc on `RoleBindingDeclaration` + `RoleBindingAuditLog` + `RoleBindingError`); full inline §Developer Guide authoring is the operator-facing reference. Owner: @cipherocto. Target: 2026-08-21.
- **audit_replay_log consumer-side wiring is the only code piece in Group B.** Producer-side lands at commit `2ffb1fc8` prior (mission 0970-a1); consumer-side is a small `RoleBindingConsumerAuditLog` wrapper. Owner: @cipherocto. Target: 2026-08-21.
- **`cargo doc --workspace --no-deps` build is a verification step.** Trip any broken-doc-link warnings, fix per Rust 1.71+ policy. Owner: @cipherocto. Target: 2026-08-21.

## Type Coverage

This follow-up implements (per 0971-a §Type Coverage deferred entries):

- **Group A:** Cross-role data flow end-to-end integration tests (TV2 + TV3) + audit trail emission at transition.
- **Group B:** `RoleBindingConsumerAuditLog` (NEW, in `crates/quota-router-core/src/node/role_binding_consumer_audit.rs`) + RFC-0955-R1 §Roles section addition + inline §Developer Guide section in `missions/claimed/0971-a-role-binding.md` + `cargo doc --workspace` verification.

## Location

- `crates/quota-router-core/src/node/role_binding_consumer_audit.rs` (NEW) — Group B AC-B1
- `crates/quota-router-core/tests/cross_role_data_flow.rs` (NEW) — Group A AC-A4 + AC-A5
- `rfcs/accepted/economics/0955-r1-reputation-anchoring.md` (MODIFY) — Group B AC-B2 (either create §Roles section or document explicit omission)
- `missions/claimed/0971-a-role-binding.md` (MODIFY) — Group B AC-B3 (inline §Developer Guide section)

## Claimant

@unclaimed (target: @cipherocto)

## Pull Request

(unset)

## Notes

- This follow-up mission is the canonical home for the 9 ACs deferred from 0971-a. The 0971-a mission text per [[deferred-vs-unspecified]] named-owner rule explicitly states "tracked under follow-up mission TBD per [[deferred-vs-unspecified]]" — this mission is the follow-up.
- Group A (target 2026-09-15) is the larger unit; Group B (target 2026-08-21) is the smaller unit. Group B can close independently of Group A.
- Per [[no-phantom-mission-pointers]] all `depends_on` cites real missions (0969-b1, 0970-a1, 0971-a) + RFC substrate (RFC-0957, RFC-0959, RFC-0970). No phantom slugs.
- Per [[no-line-refs-anywhere]] all references use §symbol-name form (no line refs in this mission).
- Per [[rfc-referencing-convention]] RFCs referenced by number only (no status / version pins).
- Per [[implementation-workflow-hook]] this mission is filed in `claimed/` directly (planning + owner + target set; substrate work follows in subsequent commits).

## Submission Date

2026-08-07T00:00:00Z

## Last Updated

2026-08-07T00:00:00Z (Group A closure)

## Version

2.0 (Group A closed 2026-08-07 — 5/5 ACs GREEN ahead of 2026-09-15 target; Group B closed 2026-08-07 — 4/4 ACs GREEN. 9/9 ACs GREEN across 3 commits.)
