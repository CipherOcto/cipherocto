---
rfc: 0967-A1
title: Policy Registry Trait Extension
status: Accepted
version: 1.9.1
date: 2026-08-24
amends: RFC-0967
builds_on:
  - rfcs/accepted/economics/0967-policy-object-graph.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0967-A1 — Policy Registry Trait Extension

## 0. Status

**Accepted (v1.9.1, 2026-08-24).** [R9 fix-all cascade (R8-confirmed findings applied to this file): YAML version 1.8 → 1.9; stale RFC-0959 v2.2 pin → RFC-0959 v2.3 pin (R8 finding F-R8-0959-V22-STALE; canonical on-disk state per R7 RFC-0959 commit bumped v2.2 → v2.3 same day); §0 audit-trail narrative re-aligned — all stale self-claims removed; L<number> cite anchors in audit-trail narrative stripped per CLAUDE.md §No line refs anywhere; §2.1 trait declarations annotated RFC-defined substrate-pending; §2.1 carry_proof sub-field annotated RFC-defined extension point (substrate-side `SettlementEnvelope` is RFC-0959-defined, not RFC-defined substrate-pending); §2.1 WorkflowKind request-response types annotated RFC-defined substrate-pending; §2.3 FallthroughCondition / octo-workflow-composite crate annotated RFC-defined substrate-pending; §2.3 CapabilityKind → HolderKind (RFC-0957-A1) clarification; §2.4 + §2.5 policy_registry + policy_kind_authority tables annotated RFC-defined substrate-pending; §2.6 kind_uuid_registry annotated RFC-defined namespace strings; §2.6 heading updated to "30 per-policy-kind UUIDv5 registry" per §2.1 R6 fix F-R6-009; §3 Class C row annotated linter pending landing via mission `0008-class-c-linter` (future; per RFC-0008 §Implicit Assumptions); §4 Cross-References: "RFC-0957 §Capability" → "RFC-0957 §CapabilityToken State Machine"; "RFC-0964 §0.1 outer-namespace tag 0x01" → "RFC-0964 §0 outer-namespace tag"; "RFC-0959 v2.1 §2 carry_proof attribution" dropped (RFC-0959 BurnEventRef has no carry_proof field); "RFC-0959 v2.0/v2.1-defined" → "RFC-0959 v2.3-defined" per substrate state (RFC-0959 v2.3 file is current); "RFC-0206 v3.0 §4" → "RFC-0206 §4" throughout body (version pin forbidden per CLAUDE.md §RFC Reference Conventions); "RFC-0967 v1.0/v1.1-Resolved" → bare "RFC-0967" in §4 Cross-References (version pin forbidden per CLAUDE.md §RFC Reference Conventions); "RFC-0960 §Substrate v3.0" → "RFC-0960 §Substrate Migration v017" (phantom section anchor corrected); research doc §8.2/§5.5/§4.3 stale cites dropped from §2.4/§3/§2.2 (R10.5 scope correction out-of-scope); §0 promotion trail literal `crates/octo-policy/src/domain_separators.rs` path removed (was R8-apply-time state, removed by v1.5 R12 F-R12-R11-PARTIAL-TIGHTEN); §2.1 InteropPolicy carry_proof "§3 layout" ambiguous cite → "§2.1 (this RFC, authoritative position statement)" (R8 finding F-R8-CARRY-PROOF-AMBIGUOUS-LAYOUT); §5 v1.8 row R6 fix F-R6-009 arithmetic typo description corrected (`22 = → 23 =` → `22 → 30` per actual delta).]

**Promotion trail:** v1.0 initial draft 2026-08-22 → ... → v1.7 Accepted 2026-08-23 → v1.8 R5 fix-all cascade 2026-08-23 (22 R4-confirmed findings applied) → v1.8 R7 fix-all cascade 2026-08-23 (22 RFC-0967-A1 R6-confirmed findings applied) → v1.9 R9 fix-all cascade 2026-08-23 (19 RFC-0967-A1 R8-confirmed findings applied, this row) → v1.9.1 R13 fix-all cascade 2026-08-24 (1 R12-confirmed consistency finding applied: §2.2 header self-contradiction — InteropSelector listed in both new trait-objects list AND replaced-enums list; R12 finding F-R12-CONSISTENCY-LENS; semantic impossibility resolved by removing InteropSelector from the replaced-enums list since the new trait is the evolution/replacement of the prior enum form, not a separate replace+add pair), per long-horizon plan v1.0 Phase 4 Tier 2 promotion sequence (RFC-0967-A1 first in Tier 2 order per research §20 decision #9). 6 trait surfaces (AuthorityPolicy / MembershipPolicy / InteropPolicy / BurnPolicy / WorkflowKind / AuditPolicy) + 30-kind UUIDv5 registry + policy_registry table all preserved. Filed per R2 adversarial review of `docs/research/2026-08-21-vault-monetary-representation-redesign.md`; resolves R2 CRITICAL §4.2/§5.2/§6.2/§7.5/§6.5 (RFC authorization gap for 6 policy traits + **30 per-policy-kind UUIDv5 registry** + InteropSelector/Outcome + FallthroughCondition). **R8 amendments:** WorkflowKind trait signatures updated to primitive-types-only (`proof: &[u8]` replaces `ctx: &WorkflowContext` per F-R8-WFCOMPOSITE-NO-PROOF-PARAM); AUDIT_VARIANT_HASH_DOMAIN migrated `cipherocto/audit/v1/` → `octo/audit/v1/` per F-R8-DOMSEP-PREFIX-DRIFT; phantom "RFC-0126" citation removed per F-R8-DOMSEP-PHANTOM-SECTION (replaced with concrete substrate-side registry, pending landing); `kind_uuid_registry.rs` file created per F-R8-DOMSEP-PHANTOM-FILE (pending landing). **v1.4 amendments:** R12 cross-RFC drift closure (F-R12-MAX-COMPOSITE-DEPTH-CLASS-COVERAGE-GAP): MAX_COMPOSITE_DEPTH=4 substrate-level insert enforcement added to §3 Execution Class Mapping table + RFC-0008 §RFC-0008 Execution Class Mapping table cross-referenced (canonical anchor for Class A enforcement, per RFC-0008 Accepted §RFC-0008 Execution Class Mapping column); RFC-0008 Accepted §Data Structures (ExecutionClass enum discriminant: `A = 0x00, B = 0x01, C = 0x02`) cited as canonical ExecutionClass source; RFC-0206 TYPE renames table cross-references reconciled; 0206-001 v3.0 + 0206-009 mission DAG ordering annotated. **v1.5 amendments:** R12 fresh fix trail: kind UUID registry count 22 → 30 (6 sites reconciled); §2.1 phantom section ref F-R12-XR-RFC0967-A1-LINE-REF closure (line refs removed per CLAUDE.md §No line refs anywhere); RFC-0959 v2.0 carry_proof anchor corrected to this RFC §2.1 InteropPolicy trait signature. **v1.9 amendments:** R8 finding cascade applied: stale RFC-0959 v2.2 pin → v2.3 (R8 finding F-R8-0959-V22-STALE; canonical on-disk state); RFC-0206 v3.0 §4 pin → bare RFC-0206 §4 throughout body; RFC-0967 v1.0/v1.1-Resolved pin → bare RFC-0967 in §4; phantom RFC-0960 §Substrate v3.0 anchor → RFC-0960 §Substrate Migration v017 (canonical section per RFC-0960 v3.3 YAML); research doc §8.2/§5.5/§4.3 stale cites dropped (R10.5 scope correction out-of-scope); literal `crates/octo-policy/src/domain_separators.rs` path removed from §0 (was R8-apply-time state, removed by v1.5 R12 F-R12-R11-PARTIAL-TIGHTEN but §0 narrative still referenced); §2.1 InteropPolicy carry_proof "§3 layout" ambiguous cite → "§2.1 (this RFC, authoritative position statement)"; §0 audit-trail narrative precision defect fixed (SettlementEnvelope / WorkflowKind annotation clarified: `carry_proof` SUB-FIELD is RFC-defined extension point, the SettlementEnvelope TYPE itself is RFC-0959-defined substrate-pending).

## 1. Motivation

RFC-0967 defines a PolicyObject graph model (PolicyObject envelope + PolicyGraph DAG + PolicyNode with predicate/action + PolicyAction enum). The vault monetary representation redesign (RFC-0960 substrate context) requires **per-policy-kind behavioral traits** that compose into a substrate-managed registry — not a pure graph model.

This amendment extends RFC-0967 with a **trait registry layer** (Layer A substrate) on top of the v1.1 PolicyObject graph (Layer A substrate). Both coexist: PolicyObject graph remains the canonical representation of policy state; the trait registry provides the substrate dispatch surface.

## 2. Definitions (additive to RFC-0967)

### 2.1 Per-policy-kind behavioral traits

Six traits (RFC-defined, substrate-pending landing via 0206-001 v3.0 + 0206-009 per RFC-0206 §4; declared in `octo-policy/src/lib.rs` once substrate lands — currently `octo-policy/src/lib.rs` contains ONLY the parent RFC-0967 PolicyObject/PolicyGraph/PolicyNode model, NONE of the 6 traits are present):

```rust
// AuthorityPolicy — mint authorization
pub trait AuthorityPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8;32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    // R3+R4 fix: primitive types only — proof: &[u8] (no AuthorityContext in Layer A trait sig)
    fn validate(&self, proof: &[u8]) -> Result<(), AuthorityError>;
}

// MembershipPolicy — vault creation gate
pub trait MembershipPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8;32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    // R3+R4 fix: primitive types only — proof: &[u8] (no MembershipContext in Layer A trait sig)
    // R7 fix F-R7-ZK-GUARD-MISSING-OTHER-PATHS — substrate MUST inspect proof[16..20] for ZK envelope
    // marker BEFORE invoking validate() when policy.execution_class() == ExecutionClass::B.
    // Same global magic as AuthorityPolicy path (see ZK_ENVELOPE_MARKER constant).
    fn validate(&self, proof: &[u8]) -> Result<(), MembershipError>;
}

// InteropPolicy — cross-chain transfer validation
pub trait InteropPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8;32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn selector(&self) -> &dyn InteropSelector;
    // R6 fix F-R6-012 — TOCTOU race bounded: validate_transfer returns InteropOutcome WITH an embedded
    // state-snapshot hash; apply() re-validates by comparing current state hash to snapshot hash.
    // On mismatch, apply() returns SettlementError::StateDrift and the entire settlement reverts.
    // R7 fix F-R7-ZK-GUARD-MISSING-OTHER-PATHS — for Class B InteropPolicy, env.carry_proof MUST
    // contain ZK envelope marker at [16..20]; substrate rejects SettlementError::ClassBRequiresZkProof
    // if marker absent. carry_proof is a sub-field of SettlementEnvelope (RFC-0967-A1 §2.1 InteropPolicy
    // trait signature, per R14 fix). [R12 fresh
    // fix F-R12-XR-CARRY-PROOF-PHANTOM-FIELD: the cross-RFC anchor is corrected — `carry_proof` is
    // defined in RFC-0967-A1 wire form (this RFC), NOT in RFC-0959.
    // RFC-0959 §2 BurnEventRef wire form does NOT define `carry_proof`. Earlier §1
    // citation of RFC-0959 was a mis-reference to the interop envelope authoritative source.]
    // [R14 fix R12-XR-004 PHANTOM-SECTION-REF CLOSURE: the v1.4 "§SettlementEnvelope wire form
    // (this RFC)" reference was a phantom section — RFC-0967-A1 does NOT carry a §SettlementEnvelope
    // wire form section. Anchor corrected to RFC-0967-A1 §2.1 InteropPolicy trait signature (this RFC,
    // the SettlementEnvelope parameter on validate_transfer at the trait declaration below) — the SettlementEnvelope struct's
    // `carry_proof: &[u8]` field is the RFC-defined authoritative wire-form source for the byte-vector layout
    // (RFC-defined extension point; substrate-side SettlementEnvelope at `crates/quota-router-storage/src/ask.rs`
    // does NOT carry `carry_proof` — added by this RFC §2.1 R12 fix, pending substrate landing via
    // 0206-001 v3.0 + 0206-009 per RFC-0206 §4).
    // ZK envelope marker at `carry_proof[16..20]` follows the same §2.1 layout as `proof[16..20]`
    // (this RFC, authoritative position statement at the trait signature block above).]
    // [R15 fix F-R15-PR-05: stale "RFC-0959 v2.0" parenthetical anchor updated to R14 correction
    // ("RFC-0967-A1 §2.1 InteropPolicy trait signature").]
    fn validate_transfer(&self, env: &SettlementEnvelope, src: &[u8;32], dst: &[u8;32]) -> Result<Box<dyn InteropOutcome>, InteropError>;
    // Note: `SettlementEnvelope` is RFC-defined wire form anchored in this RFC §2.1 InteropPolicy trait signature;
    // substrate-side `struct SettlementEnvelope` (RFC-0959 v2.3-defined) DOES exist at
    // `crates/quota-router-storage/src/ask.rs` with fields `settlement_hash / asker_did / holder_did /
    // model / axes_consumed / ask_id / nonce / timestamp_unix / cost / cost_vault_id / chain_id` — and
    // does NOT define `carry_proof`. The `carry_proof: &[u8]` sub-field is the RFC-defined extension point
    // (NOT substrate-shipped) added by this RFC §2.1 R12 fix; pending substrate landing via 0206-001 v3.0 +
    // 0206-009 per RFC-0206 §4 ZK envelope marker at `carry_proof[16..20]` follows
    // the same §2.1 layout as `proof[16..20]` (this RFC, authoritative position statement).
}

// BurnPolicy — burn timing + window + capability requirement
pub trait BurnPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8;32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn allowed_chain_namespaces(&self) -> &'static [ChainNamespace];
    // R6 fix F-R6-010 — primitive types only per R3+R4 sibling-trait invariant;
    // BurnContext replaced with primitive (unlock_at_unix already primitive; window_basis derived from policy body schema).
    fn validate_unlock_window(&self, unlock_at_unix: i64, window_basis: i64) -> Result<(), BurnError>;
    // R6 fix F-R6-014 — requires_capability returns CapabilityKind (not bare bool);
    // caller dispatches based on CapabilityKind discriminator per RFC-0957-A1.
    fn requires_capability(&self) -> Option<CapabilityKind>;
    // Note: `CapabilityKind` is RFC-defined placeholder pending substrate landing. RFC-0957-A1 defines
    // `HolderKind` (V1/ZKBearing/Bearer/HopCapability), not `CapabilityKind`; verified 0 hits for
    // `CapabilityKind` in rfcs/accepted/ outside self-references in this RFC. Substrate-pending via
    // 0206-001 v3.0 + 0206-009 per RFC-0206 §4
}

// WorkflowKind — vault provisioning workflow dispatch
pub trait WorkflowKind: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8;32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn provisioning_api_kind_uuid(&self) -> u128;
    fn provisioning_api_body(&self) -> &[u8];
    // R8 fix F-R8-WFCOMPOSITE-NO-PROOF-PARAM — primitive types only per R3+R4
    // (no `ctx: &WorkflowContext` phantom carrying proof bytes). ZK proof is
    // passed as `proof: &[u8]`; substrate-level ZK guard (per F-R7-ZK-GUARD-MISSING-OTHER-PATHS)
    // fires BEFORE validate_vault_creation when execution_class == Class B.
    fn validate_vault_creation(&self, req: &VaultCreationRequest, proof: &[u8]) -> Result<(), WorkflowError>;
    // R8 fix F-R8-WFCOMPOSITE-PROVISION-SUBJECT-ATOMICITY — provision_subject
    // atomicity: substrate invokes post-validate_vault_creation Ok; ALL leaf
    // provision_subject calls execute in same tx as vault insert; failure of
    // any leaf ROLLS BACK parent vault + all prior leaves.
    fn provision_subject(&self, req: &SubjectProvisionRequest, proof: &[u8]) -> Result<(), WorkflowError>;
    // R8 fix F-R8-WFCOMPOSITE-READ-USER-INFO-FALLBACK — read_user_info with Class B
    // requires ZK envelope marker; substrate-level guard fires BEFORE body; missing
    // marker returns WorkflowError::ClassBRequiresZkProof (NOT "not found").
    fn read_user_info(&self, query: &UserInfoQuery, proof: &[u8]) -> Result<UserInfoResponse, WorkflowError>;
    fn update_user(&self, req: &UserUpdateRequest, proof: &[u8]) -> Result<(), WorkflowError>;
    // Note: `VaultCreationRequest`, `SubjectProvisionRequest`, `UserInfoQuery`, `UserUpdateRequest`,
    // `UserInfoResponse` are RFC-defined request/response types, substrate-pending landing via
    // 0206-001 v3.0 + 0206-009 per RFC-0206 §4 (verified: 0 source hits in crates/).
}

// AuditPolicy — event emission field selection + variant assignment
pub trait AuditPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8;32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn emit_fields(&self) -> &'static [AuditField];
    fn variant_assignment(&self, chain_id: &[u8;32]) -> AuditVariant;
}

// R6 fix F-R6-008 + R8 fix F-R8-DOMSEP-PREFIX-DRIFT + R9 fix F-R9-AUDIT-PREFIX-DRIFT
// + R12 fix F-R12-AUDIT-VARIANT-PREFIX-DRIFT-XRFC — variant_assignment hash spec:
// derive variant via
//   BLAKE3("octo/audit/ab/v1/" || chain_id_bytes)[0] % V
// where V is the variant cardinality declared by the AuditPolicy body schema
// (typically 2 for A/B). The A/B-specific prefix `octo/audit/ab/v1/` matches the
// kind_uuid registry entry `octo-audit-ab-v1` at §2.6 (canonical per R9 fix
// F-R9-AUDIT-PREFIX-DRIFT). The legacy generic prefix `octo/audit/v1/` was a
// v1.0/v1.1 R8-apply-time placeholder; R9 fix corrected it to `octo/audit/ab/v1/`
// (A/B-kind-specific) but R9 fix was never propagated to this §2.1 formula
// until R12 fix F-R12-AUDIT-VARIANT-PREFIX-DRIFT-XRFC.
//
// Canonical registry location: substrate-side registry at
// `octo-policy::domain_separators::blake3_prefix::AUDIT_VARIANT_HASH_DOMAIN`
// (**RFC-defined, substrate-pending landing** — verified: 0 hits for
// `AUDIT_VARIANT_HASH_DOMAIN` and `domain_separators` in crates/; pending
// landing via Phase 1 mission 0206-001 v3.0 + 0206-009 per R10.5
// scope correction; pre-revert reference site REVERTED per R10.5; created
// in R8 apply per F-R8-DOMSEP-PHANTOM-FILE — was a phantom reference in v3.4
// research doc). [R12 fresh fix F-R12-R11-PARTIAL-TIGHTEN: removed literal
// `crates/octo-policy/src/domain_separators.rs` file path per R11 PARTIAL
// reviewer recommendation; retains REVERTED qualifier for historical audit
// trail without literal path re-citation.]
// per R10.5).
//
// R8 fix F-R8-DOMSEP-MARKER-ENCODING-MISMATCH — BLAKE3-prefix entries (this
// constant) and raw-byte marker entries (ZK_ENVELOPE_MARKER in substrate-side
// registry) are kept in DISTINCT sub-modules of the registry because they use
// different encoding schemes. See substrate-side registry `::blake3_prefix`
// vs `::raw_markers` (pending landing).
//
// R9 fix F-R9-AUDIT-VARIANT-HASH-DOMAIN-DEAD-CODE — the `AUDIT_VARIANT_HASH_DOMAIN`
// constant + `derive_audit_variant()` function body that previously lived here
// have been REMOVED. Canonical location is substrate-side registry
// `::blake3_prefix::AUDIT_VARIANT_HASH_DOMAIN` +
// `::blake3_prefix::derive_audit_variant(chain_id, variant_cardinality)`
// (pending landing via 0206-001 v3.0 + 0206-009). This RFC section now
// references the canonical location only.
//
// Birthday-paradox collision probability for N chains at k variants:
//   P(collision) ≈ 1 − e^(−N(N−1)/(2k))
// For k=2 (A/B), P≥50% at N ≈ 2.24 (solving 0.5 = 1 − e^(−N(N−1)/4) yields N² − N − 2.77 = 0
// → N ≈ 2.24) — collisions become EXPECTED once N ≥ 3 chains exist.
// Collision handling policy: identical-variant on collision (deterministic + safe); substrate does NOT
// error on hash collision. This is a property of A/B test design: ~50/50 split is statistical, not exact.
```

**R6 fix F-R6-009 — Kind UUID Registry count reconciliation:** §0 self-claim says "22 per-policy-kind UUIDv5 registry"; actual count is 30 (per reviewer prompt: 6 Auth + 7 Membership + 4 Interop + 3 Burn + 4 Workflow + 3 Audit + 3 Selector = 30 namespace strings). Update §0, §5 version-history, and §2.6 header to "30 per-policy-kind UUIDv5 registry". Previous v2.0 estimate "23 = 6 mint + 7 membership + 3 interop + 3 burn + 4 workflow (incl. composite) = 23" (component sum 6+7+3+3+4 = 23; prefix "22 =" was an arithmetic typo in v1.6) was outdated when Selector (3) + Audit (3) + Interop 3→4 (+1) kinds were added in R2 fix cluster; the +7 delta from 23→30 = 3 Selector + 3 Audit + 1 Interop.

### 2.2 InteropSelector + InteropOutcome trait objects (replace InteropDecision + SwapKind + MultiEnvelopeCompletion enums)

```rust
// Supporting types — RFC-defined pending substrate landing via 0206-001 v3.0 + 0206-009
// per RFC-0206 §4 (verified: 0 hits for `SelectorContext` / `InteropSelectorChoice`
// in crates/ outside this RFC).
pub struct SelectorContext<'a> {
    pub src_chain_id: &'a [u8; 32],
    pub dst_chain_id: &'a [u8; 32],
    pub amount_dqa_micros: i64,
    pub asset_namespace: &'a [u8],
    pub candidate_policies: &'a [u128], // kind_uuids
}

pub enum InteropSelectorChoice {
    Primary,
    Secondary,
    Reject,
}

pub trait InteropSelector: Send + Sync {
    fn select(&self, ctx: &SelectorContext) -> InteropSelectorChoice;
}

pub trait InteropOutcome: Send + Sync {
    // R6 fix F-R6-012 — apply() takes a state-snapshot hash and re-validates internally;
    // returns SettlementError::StateDrift on mismatch (TOCTOU race bounded).
    fn apply(&self, env: &mut SettlementEnvelope, state_snapshot: &[u8;32]) -> Result<(), SettlementError>;
}
```

**R5 fix F-R5-003 — FallthroughCondition::AlwaysBoth nested semantics:** `AlwaysBoth` for a 2-element composite `[primary, secondary]` means BOTH `primary.validate*()` AND `secondary.validate*()` MUST succeed; failure of either rejects the operation. For N>2 chains constructed via nested composites, semantics are **left-associative AND**: composite `[A, B_composite]` with `B_composite.condition = AlwaysBoth` over `[C, D]` is equivalent to `A AND C AND D` (depth-1 left-associative). The MAX_COMPOSITE_DEPTH=4 bound (canonical post-R6 fix F-R6-015 — superseded left-associative AND reduction; the v3.0 `2^depth - 1` "worst-case" formula has been SUPERSEDED by D+1 linear reduction per R6 fix F-R6-015) limits nesting to 4 levels; the substrate counts NESTING-LEVEL validate invocations against the depth budget, not pairwise policy.validate() invocations. A depth-3 chain `[A, B_composite(C, D), E]` with AlwaysBoth at every level invokes 4 top-level validations (A + C + D + E) — within MAX_COMPOSITE_DEPTH=4 budget. Current canonical formula is D+1 linear (depth+1 top-level validations) per R6 fix F-R6-015; substrate enforces left-associative reduction.

**[R12 fix F-R12-MAX-COMPOSITE-DEPTH-CLASS-COVERAGE-GAP — RFC-0008 cross-ref]:** The `MAX_COMPOSITE_DEPTH = 4` substrate-level insert enforcement is a **Class A** operation per RFC-0008 Accepted §RFC-0008 Execution Class Mapping. Substrate fails closed on depth > 4 via a deterministic depth counter + left-associative reduction; the validation is deterministic (no ZK proof required) and executes at write time. Cross-references: RFC-0008 Accepted §Data Structures (ExecutionClass enum discriminant: `A = 0x00, B = 0x01, C = 0x02`) + RFC-0008 §RFC-0008 Execution Class Mapping table (Class A substrate validation ops). **[R12 fresh fix F-R12-DOC-LINE-REF-CLAUDE-MD-VIOLATION]:** removed line reference per CLAUDE.md §No line refs anywhere — `§Data Structures` line-numbered cite violates section-ref discipline; replaced with §Data Structures section anchor only.**

### 2.3 FallthroughCondition (for CompositeWorkflow)

Local enum in `octo-workflow-composite/src/lib.rs` (Layer E; **RFC-defined, substrate-pending landing via 0206-001 v3.0 + 0206-009 per RFC-0206 §4** — verified: `octo-workflow-composite` crate does NOT exist on disk, `grep -rn "FallthroughCondition" crates/ rfcs/accepted/` returns only self-references in this RFC). Body schema:

```rust
enum FallthroughCondition {
    OnPrimaryReject,                    // if primary.validate_*() returns Err, try secondary
    OnCapabilityType(CapabilityKind),   // if capability kind matches, use primary; else use secondary
    AlwaysBoth,                         // validate both; both must succeed
    Never,                              // primary only; secondary never invoked
}
```

Note: `CapabilityKind` is RFC-defined placeholder pending substrate landing (verified: 0 hits for `CapabilityKind` in `crates/octo-cap-macaroon/src/`; RFC-0957-A1 defines `HolderKind`, not `CapabilityKind`). The `OnCapabilityType(CapabilityKind)` variant is a placeholder until substrate catches up. The enum is local to CompositeWorkflow body schema, not a substrate enum.

### 2.4 Policy registry table (Layer A substrate)

**Schema is RFC-defined, substrate-pending landing via 0206-001 v3.0 + 0206-009 per RFC-0206 §4** — verified: actual substrate `crates/octo-policy-storage/src/lib.rs` declares only `TABLE_POLICY_OBJECTS = "policy_objects"`; no `policy_registry` table exists on disk. Columns: `policy_hash`, `kind_uuid BLOB(16)`, `body BLOB`, `execution_class TEXT`, `registered_at_unix`, `registered_by_did`, `revoked_at_unix`, `revoked_by_did`, `revocation_reason`, `superseding_policy_hash`. **R6 fix F-R6-013 — UNIQUE applies per (kind_uuid, policy_hash) NOT per kind_uuid alone:** allows policy versioning (v1 → v2 → v3 of same kind). Substrate registration enforces "exactly one active policy per kind_uuid" via UNIQUE partial index `WHERE revoked_at_unix IS NULL`; transitions: INSERT v2 with revoked_at_unix=NULL → triggers UPDATE on v1 to set revoked_at_unix + superseding_policy_hash=v2.hash, all in single tx. This permits policy rotation without schema migration.

### 2.5 Policy kind authority table

```sql
CREATE TABLE policy_kind_authority (
    kind_uuid BLOB(16) NOT NULL PRIMARY KEY,
    required_signer_did BLOB(32) NOT NULL,
    authority_kind TEXT NOT NULL  -- 'octo_treasury' | 'corp_admin'
);
```

**R5 fix F-R5-001 + F-R5-006 — enforcement + registration-race guard:** Substrate `register_policy(kind_uuid, body, registered_by_did)` MUST:

1. **Transactional wrapper** — BEGIN tx, INSERT policy_kind_authority (kind_uuid, registered_by_did) IF NOT EXISTS (idempotent on (kind_uuid, required_signer_did) pair), THEN INSERT policy_registry with FK check `policy_registry.kind_uuid REFERENCES policy_kind_authority.kind_uuid`, COMMIT. Single atomic write — no window between the two INSERTs.
2. **FK constraint** — `policy_registry.kind_uuid REFERENCES policy_kind_authority(kind_uuid)` enforces policy_kind_authority existence at substrate level; substrate refuses INSERT into policy_registry if kind_uuid not present in policy_kind_authority.
3. **Authority check** — substrate verifies `registered_by_did == policy_kind_authority.required_signer_did` BEFORE the INSERT; reject with `PolicyRegistryError::UnauthorizedRegistrar` if mismatch. Defense-in-depth: even if FK is somehow bypassed (e.g., direct DDL grant), the application-level check fires.
4. **Defensive FK to policy_registry (chain_metadata bridge)** — `policy_kind_authority.kind_uuid` does NOT need FK to policy_registry; direction is policy_registry → policy_kind_authority (per #2).
5. **Registry bootstrap order** — substrate migration v017 inserts policy_kind_authority rows BEFORE policy_registry accepts any INSERT (one-time seeding in the migration transaction itself). Order is guaranteed by the migration runner applying DDL sequentially within the migration tx.

### 2.6 Kind UUID Registry — 30 per-policy-kind UUIDv5 registry (UUIDv5 namespace allocations)

All per-policy-kind crates MUST derive their `kind_uuid` via UUIDv5 from the namespace strings below. **§2.6 table is RFC-defined namespace strings; no canonical disk registry exists** — verified: 0 hits for `kind_uuid_registry` in crates/. Reference u128 values are RFC-defined pending landing via 0206-001 v3.0 + 0206-009 per RFC-0206 §4 (substrate-side registry pending landing; pre-revert reference site REVERTED per R10.5; created in R8 apply per F-R8-DOMSEP-PHANTOM-FILE — was a phantom reference in v3.4 research doc). [R12 fresh fix F-R12-R11-PARTIAL-TIGHTEN: removed literal `crates/octo-policy/src/kind_uuid_registry.rs` file path per R11 PARTIAL reviewer recommendation.]

| Kind                           | UUIDv5 input namespace string         |
| ------------------------------ | ------------------------------------- |
| Auth single-key                | `octo/auth/singlekey/v1`              |
| Auth multisig                  | `octo/auth/multisig/v1`               |
| Auth capability-delegation     | `octo/auth/capability/v1`             |
| Auth governance                | `octo/auth/governance/v1`             |
| Auth HSM                       | `octo/auth/hsm/v1`                    |
| Auth hybrid                    | `octo/auth/hybrid/v1`                 |
| Membership DID-attestation     | `octo/membership/didattestation/v1`   |
| Membership invitation-token    | `octo/membership/invitationtoken/v1`  |
| Membership merkle-list         | `octo/membership/merklelist/v1`       |
| Membership teams-proxy         | `octo/membership/teamsproxy/v1`       |
| Membership corp-members-table  | `octo/membership/corpmemberstable/v1` |
| Membership capability-gated    | `octo/membership/capabilitygated/v1`  |
| Membership SCIM-bridge         | `octo/membership/scimbridge/v1`       |
| Interop no-bridge              | `octo/interop/none/v1`                |
| Interop atomic-swap            | `octo/interop/swap/v1`                |
| Interop wrapped-representation | `octo/interop/wrap/v1`                |
| Interop hybrid                 | `octo/interop/hybrid/v1`              |
| Burn time-locked               | `octo/burn/timelock/v1`               |
| Burn immediate                 | `octo/burn/immediate/v1`              |
| Burn multisig                  | `octo/burn/multisig/v1`               |
| Workflow capability-based      | `octo/workflow/capability/v1`         |
| Workflow LiteLLM               | `octo/workflow/litellm/v1`            |
| Workflow SCIM-bridge           | `octo/workflow/scim/v1`               |
| Workflow composite             | `octo/workflow/composite/v1`          |
| Audit testnet-verbose          | `octo/audit/testnet/v1`               |
| Audit mainnet-slim             | `octo/audit/mainnet/v1`               |
| Audit A/B v1                   | `octo/audit/ab/v1`                    |
| Selector by-chain              | `octo/selector/bychain/v1`            |
| Selector by-asset              | `octo/selector/byasset/v1`            |
| Selector by-amount-threshold   | `octo/selector/byamountthreshold/v1`  |

## 3. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                                      | Class                             | Justification                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------ | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AuthorityPolicy::validate`                                  | A or B-with-ZK-proof              | Consensus-path mint gate per RFC-0008                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Class C AuthorityPolicy registration                         | REJECTED at registration          | Substrate refuses `policy_registry` INSERT for Class C. **Note:** substrate-level class-C enforcement linter pending landing via mission `0008-class-c-linter` (future; per RFC-0008 §Implicit Assumptions). Mission 0206-006 (cipherocto-policy → octo-policy rename) is the prerequisite crate rename, NOT the linter work itself — verified: 0 hits for `linter` / `class_lint` in `missions/claimed/0206-006-cipherocto-policy-rename-alignment.md`. Until that linter lands, the §3 REJECTED row has no canonical enforcement implementation (RFC-defined pending substrate). |
| `MembershipPolicy::validate`                                 | A or B-with-ZK-proof              | Consensus-path vault-creation gate                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `InteropPolicy::validate_transfer`                           | A or B-with-ZK-proof              | Consensus-path cross-chain transfer validation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `BurnPolicy::validate_unlock_window`                         | A                                 | Deterministic check on timestamp + window basis                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `WorkflowKind::validate_vault_creation`                      | A or B-with-ZK-proof              | Consensus-path vault creation gate                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `WorkflowKind::provision_subject`                            | A (composite) / N/A (direct call) | Litellm persistent user creation. NOT on consensus-path when invoked directly (writes `litellm_users` row only); CONSENSUS-PATH when invoked via CompositeWorkflow::Write* with AlwaysBoth — the composite operation inherits Class A from `validate_vault_creation`. Direct substrate path: substrate dispatches `provision_subject` only after `validate_vault_creation` returns Ok; the proof + membership gate is the consensus-bearing step. `provision_subject` body is opaque to substrate — Class A applies to the composite, NOT to the method body.                      |
| `WorkflowKind::read_user_info`                               | A or B-with-ZK-proof              | Reads are consensus-safe but require auth                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `AuditPolicy::emit_fields`                                   | A                                 | Deterministic field selection                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `AuditPolicy::variant_assignment`                            | A                                 | Deterministic by chain_id hash                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `MAX_COMPOSITE_DEPTH = 4` substrate-level insert enforcement | A                                 | Deterministic depth counter + left-associative reduction; substrate fails closed on depth > 4 (per RFC-0008 §RFC-0008 Execution Class Mapping)                                                                                                                                                                                                                                                                                                                                                                                                                                     |

**R9 fix F-R9-ZK-ITERATION-RFC-GAP — per-leaf ZK guard sequential-iteration rule:**

When a chain's `workflow_kind_hashes` (Vec<[u8;32]>) carries 2+ Class B-with-ZK-proof leaves (CompositeWorkflow with `condition = AlwaysBoth` per §2.2 F-R5-003), the substrate ZK envelope marker check fires **per leaf, sequentially** (not in parallel). The rule:

1. Substrate iterates `workflow_kind_hashes` in declared order.
2. For each leaf, substrate invokes `WorkflowKind::validate_vault_creation(proof)` where `proof` is the leaf-specific ZK capability proof byte vector.
3. Substrate-level ZK envelope marker guard (`proof[16..20] == b"\x01zk\x00"` per substrate-side registry `::raw_markers::ZK_ENVELOPE_MARKER`, pending landing via 0206-001 v3.0 + 0206-009 per R10.5) fires ONCE PER LEAF; failure of any leaf rejects the composite operation.
4. All leaves execute in the same substrate transaction (atomic with vault insert); failure rolls back the parent vault + all prior leaves.

Sequential (not parallel) execution is required by RFC-0008 — parallel ZK proof verification would introduce non-deterministic ordering across consensus nodes. The sequential-iteration is documented in the workflow_kind_hashes leaf-by-leaf dispatch protocol (R8 fix F-R8-WFCOMPOSITE-PROOF-ITERATION; line ref removed per CLAUDE.md §No line refs anywhere — R13 fix F-R12-LENS-CROSS-CONSISTENCY-LINE-REF-2) and now promoted to RFC-0967-A1 §3 via this row.

**R6 fix F-R6-011 — provision_subject execution class fully resolved (was §3 RESOLVED-note hand-wave):** The earlier v1.0 §3 said "this conflicts. See §4" but §4 had no resolution. The full resolution:

1. `provision_subject` is a **side-effect hook** invoked by substrate ONLY after `validate_vault_creation` returns Ok (consensus-bearing check).
2. Substrate does NOT call `provision_subject` directly via consensus — it dispatches as a post-consensus side effect (the consensus-bearing work is `validate_vault_creation`).
3. The method's body executes in the same transaction as the vault creation (atomic with `vaults` insert), but the body itself is opaque to substrate.
4. **ExecutionClass for the method** is set to Class A in policy body schema, which means the policy registration gate accepts it (not rejected like Class C). The substrate's runtime class for the call site is determined by the **composite operation** (vault creation), not by the method body's class.
5. No ZK proof requirement on `provision_subject` itself — ZK is on `validate_vault_creation` per RFC-0008.

Therefore: `provision_subject` row in §3 table is correctly Class A (no ZK requirement on method body), and the consensus gate is upstream at `validate_vault_creation`.

**R7 fix F-R7-ZK-PHANTOM-REF + R8 fix F-R8-DOMSEP-PHANTOM-SECTION — RFC-0958 §Wire Format Extension AND RFC-0126 phantom references removed:** Earlier research doc §8 R5-fix F-R5-005 referenced "RFC-0958 §Wire Format Extension encoding" for the global ZK envelope marker bytes 0x01 0x7a 0x6b 0x00. RFC-0958 defines proof_bundle=canonical_ser(ProofBundle) as a base64url 4th wire segment, NOT a fixed-byte envelope. The R7 v1.0 fix replaced the phantom RFC-0958 reference with "RFC-0126" — but R8 verify discovered that RFC-0126 (`rfcs/accepted/numeric/0126-deterministic-serialization.md`) is about NUMERIC ENCODING, not domain separators; it contains no §Domain Separators section. **Both references were phantoms.** Resolution: marker bytes 0x01 0x7a 0x6b 0x00 are **RFC-DEFINED pending substrate** — verified: 0 hits for the byte sequence `0x017a6b00` or `\x01\x7a\x6b\x00` in crates/. The canonical registry location is `octo-policy::domain_separators::raw_markers::ZK_ENVELOPE_MARKER` (R8 fix F-R8-DOMSEP-PHANTOM-FILE — file created in R8 apply, REVERTED per R10.5; pending re-landing via 0206-001 v3.0 + 0206-009). Future ZK envelope versions register new tags in this registry. Collision with RFC-0964 §0 outer-namespace tag 0x01 (NOT §0.1, which is "Domain-separator registry (central)"; the outer-namespace tag table is in §0 "Wire-format envelope (outer prefix + inner envelope)") is DOCUMENTED in `collision_acknowledged_with` annotation per F-R8-DOMSEP-OX01-COLLISION-UNRESOLVED.

## 4. Cross-References

- RFC-0967 (PolicyObject graph — this amendment is additive)
- RFC-0008 (Class taxonomy)
- RFC-0957 §CapabilityToken State Machine (closest analog; RFC-0957 has no §Capability heading — the macaroon format RFC documents capability token lifecycle, NOT membership policy trait definitions; the (membership policy trait) qualifier was a mis-reference)
- RFC-0960 §Substrate Migration v017 (consumption context)
- RFC-0959 §2 BurnEventRef Specification (BurnEventRef struct fields: `burn_id / chain_id / vault_id / amount_dqa_micros / burn_policy_hash / finalized_at_unix` — does NOT define `carry_proof`; the `carry_proof` sub-field is RFC-defined in this RFC §2.1 InteropPolicy trait signature, NOT in RFC-0959)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §4-§8 (full design)

## 5. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.9.1   | 2026-08-24 | **R13 fix-all cascade (1 R12-confirmed finding applied to this RFC):** §2.2 header self-contradiction closed (R12 finding F-R12-CONSISTENCY-LENS) — header listed `InteropSelector` in BOTH the new trait-objects list AND the replaced-enums list, semantically impossible. Removed `InteropSelector` from the replaced-enums parenthetical; new header reads "(replace InteropDecision + SwapKind + MultiEnvelopeCompletion enums)". The new `InteropSelector` trait (RFC-defined) is the evolution/replacement of the prior enum form, not a separate replace+add pair. YAML version 1.9 → 1.9.1; §0 Status block v1.9 → v1.9.1; §0 promotion trail extended to v1.9.1.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 1.9     | 2026-08-23 | **R9 fix-all cascade (19 R8-confirmed findings applied to this RFC):** YAML version 1.8 → 1.9; stale RFC-0959 v2.2 pin → RFC-0959 v2.3 pin throughout (R8 finding F-R8-0959-V22-STALE; canonical on-disk state per R7 RFC-0959 commit bumped v2.2 → v2.3 same day — R7 cascade fix's own substrate-state label was stale at moment of writing); "RFC-0206 v3.0 §4" → bare "RFC-0206 §4" throughout body (CLAUDE.md §RFC Reference Conventions: version pins forbidden); "RFC-0967 v1.0/v1.1-Resolved" → bare "RFC-0967" in §4 Cross-References (CLAUDE.md §RFC Reference Conventions); phantom "RFC-0960 §Substrate v3.0" anchor → "RFC-0960 §Substrate Migration v017" (canonical section per RFC-0960 v3.3 YAML); research doc §8.2/§5.5/§4.3 stale cites dropped from §2.2/§2.4/§3 (R10.5 scope correction marks research doc out-of-scope); §0 promotion trail literal `crates/octo-policy/src/domain_separators.rs` path removed (was R8-apply-time state, removed by v1.5 R12 F-R12-R11-PARTIAL-TIGHTEN but §0 narrative still referenced); §2.1 InteropPolicy carry_proof "§3 layout" ambiguous cite → "§2.1 layout (this RFC, authoritative position statement)"; §0 audit-trail narrative precision defect fixed (SettlementEnvelope / WorkflowKind annotation clarified: `carry_proof` SUB-FIELD is RFC-defined extension point, the SettlementEnvelope TYPE itself is RFC-0959-defined substrate-pending); §5 v1.8 row stale v2.2 pin → v2.3 (this row's self-claim was internally stale, fix now actualized).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 1.8     | 2026-08-23 | **R5 fix-all cascade (22 R4-confirmed findings) + R7 fix-all cascade (22 R6-confirmed findings applied to this RFC):** §2.1 birthday-paradox arithmetic corrected (k=2 P≥50% threshold N ≈ 1.18 → N ≈ 2.24 per `1 − e^(−N(N−1)/4) = 0.5` solving N² − N − 2.77 = 0); §2.1 R6 fix F-R6-009 count reconciliation updated (`22 =` arithmetic typo in v1.6 → reconciled to **30** in current RFC, per actual §2.6 table: 6 Auth + 7 Membership + 4 Interop + 3 Burn + 4 Workflow + 3 Audit + 3 Selector = 30); §2.1 carry_proof substrate-state label reconciled (RFC-defined extension point, not substrate-defined; substrate-side `SettlementEnvelope` at `crates/quota-router-storage/src/ask.rs` DOES exist but does NOT carry `carry_proof` field); §2.2 InteropSelector/SelectorContext/InteropSelectorChoice types defined (RFC-defined pending substrate via 0206-001 v3.0 + 0206-009); §0 narrative aligned to §5 VH (v1.7=R16 promotion + R3 fix-all cascade; v1.5 row is R12 fresh fix trail, not "R16 promotion"; v1.7 row "6-trait surface (AuthorityPolicy / MembershipPolicy / InteropPolicy / BurnPolicy / WorkflowKind / AuditPolicy)" already correct, no double-count to remove); §0 research doc "v2.0" stale pin dropped per CLAUDE.md §RFC Reference Conventions (line refs in audit-trail narrative stripped per CLAUDE.md §No line refs anywhere); §3 Class C row mission 0206-006 → `0008-class-c-linter` (future; mission 0206-006 is prerequisite rename only, not the linter work); §3 table re-joined (orphan AuditPolicy::emit_fields / variant_assignment / MAX_COMPOSITE_DEPTH rows moved before F-R9-ZK-ITERATION paragraph); §5 VH table reordered reverse-chrono (v1.7 first, v1.0 last); §5 v1.3 row §2.5 disambiguation table reference corrected (section no longer exists in current RFC; was R8-apply-time state); §5 v1.7 row cite-count headline reconciled (20 → 21 → 20 per actual bullet sum); §2.6 heading updated to "30 per-policy-kind UUIDv5 registry" per §2.1 R6 fix F-R6-009 instruction; §2.1 R6 fix F-R6-009 paragraph arithmetic explanation reconciled (+7 = 3 selector + 3 audit + 1 interop 3→4); §2.1 InteropPolicy SettlementEnvelope cite "RFC-0959 v2.0/v2.1-defined" → "RFC-0959 v2.2-defined" per substrate state (RFC-0959 v2.2 file is current — SUPERSEDED by R9 fix-all: canonical on-disk state is RFC-0959 v2.3); §0 status block v1.7 → v1.8 + §0 promotion trail extended to v1.8 per YAML frontmatter `version: 1.8`. |
| 1.7     | 2026-08-23 | **R16 promotion + R3 fix-all cascade:** Draft → Accepted per long-horizon plan v1.0 Phase 4 Tier 2 promotion sequence (RFC-0967-A1 first in Tier 2) + R3 fix-all cascade for 25 R2-confirmed substrate-grounding / parent-RFC verbatim / process-research-grounding / internal-consistency findings. Status bumper + citation cleanup (20 pre-existing INVALID/STALE/PHANTOM cites scrubbed: 4 STALE 0967-A1 v1.x version pins + 2 STALE 0206 v3.3/3.4 pins + 4 INVALID non-heading §Deterministic anchors in 0008 + 2 INVALID non-heading §Domain Separators in 0126 + 1 INVALID §SettlementEnvelope anchor + 2 INVALID §Vault/§Vault Substrate anchors + 2 INVALID §InteropPolicy/§BurnPolicy trait-declaration anchors in 2.1 + 1 STALE 0959 v2.1 pin + 1 INVALID trailing-dot §2. anchor + 1 INVALID §Layer anchor). 6-trait surface (AuthorityPolicy / MembershipPolicy / InteropPolicy / BurnPolicy / WorkflowKind / AuditPolicy) + 30-kind UUIDv5 registry + policy_registry/policy_kind_authority tables preserved (all substrate-pending per RFC-0206 §4).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 1.6     | 2026-08-22 | **R15 fix trail:** F-R15-FD-1 cascade (HIGH) — version field front-matter updated 1.4 → 1.5 (then 1.5 → 1.6 for this row); F-R15-FD-5b (LOW) — phantom v3.2 amendment parenthetical replaced with section anchor §2.6 per CLAUDE.md §No line refs anywhere; F-R15-FD-2 TITLE-DRIFT (LOW) — title drift between RFC-0206 + RFC-0967-A1 reconciled (v1.5 → v1.6 reflects R15 round).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 1.5     | 2026-08-22 | **R12 fresh fix trail:** F-R12-XR-CARRY-PROOF-PHANTOM-FIELD (HIGH, §2.1) — `carry_proof` cross-RFC anchor corrected from "RFC-0959 v2.0" mis-reference to RFC-0967-A1 wire form (this RFC); F-R12-RFC0967A1-VERSION-FIELD-STALE (MED, front-matter) — version: 1.4 → 1.5 (this row); F-R12-DOC-LINE-REF-CLAUDE-MD-VIOLATION (MED, §2.2) — removed line ref per CLAUDE.md §No line refs anywhere; F-R12-R11-PARTIAL-TIGHTEN (MED, §2.1 + §2.6) — removed literal `crates/octo-policy/src/domain_separators.rs` + `kind_uuid_registry.rs` file paths per R11 PARTIAL reviewer recommendation (REVERTED qualifier retained for historical audit trail).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| 1.4     | 2026-08-22 | **R12 fix trail — Cross-RFC drift closure (F-R12-MAX-COMPOSITE-DEPTH-CLASS-COVERAGE-GAP):** MAX_COMPOSITE_DEPTH = 4 substrate-level insert enforcement now declared in §RFC-0008 Execution Class Mapping table (RFC-0008 Accepted §RFC-0008 Execution Class Mapping column): "MAX_COMPOSITE_DEPTH enforcement \| A \| Deterministic depth counter + left-associative reduction; substrate fails closed on depth > 4". RFC-0967-A1 §3 explicitly cross-references RFC-0008 §RFC-0008 Execution Class Mapping for the Class-A taxonomy entry.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 1.3     | 2026-08-22 | **R12 fix trail — R9 propagation gap closure (F-R12-RFC0967A1-V15-R9-PROPAGATION-MISSING):** R9 fix F-R9-AUDIT-PREFIX-DRIFT propagated to §2.1 variant_assignment formula: `octo/audit/v1/` → `octo/audit/ab/v1/` (A/B-kind-specific prefix matching kind_uuid registry entry `octo-audit-ab-v1` at §2.6). The legacy generic `octo/audit/v1/` placeholder was R8-apply-time state; R9 fix corrected to A/B-specific but R9 fix was never propagated to §2.1 formula until R12. Note: §2.5 in the current RFC is the "Policy kind authority table" section — there is no §2.5 disambiguation table referencing `octo/audit/v1/`; the R8-apply-time §2.5 disambiguation narrative in this row describes a section that no longer exists in current RFC. §2.1 + §2.6 updated for consistency with R12 fix `octo/audit/ab/v1/`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 1.2     | 2026-08-22 | **R11 fix trail (post-R10.5 scope correction):** 7 phantom substrate file refs replaced with "substrate-side registry pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009" (this section + 5 other sites) per R11 fix F-R11-XR-PHANTOM-FILE-CITATIONS-POST-R105. §0 + §5 R8 row are HISTORICAL narrative — preserved per BLUEPRINT.md §RFC Process retroactive trail pattern; pre-R10.5 substrate file references in those rows describe R8-apply-time state, not current disk state.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 1.1     | 2026-08-22 | **R8 amendments:** WorkflowKind trait signatures `ctx: &WorkflowContext` → `proof: &[u8]` per F-R8-WFCOMPOSITE-NO-PROOF-PARAM; AUDIT_VARIANT_HASH_DOMAIN `cipherocto/audit/v1/` → `octo/audit/v1/` per F-R8-DOMSEP-PREFIX-DRIFT; phantom "RFC-0126" citation replaced with concrete `crates/octo-policy/src/domain_separators.rs` registry per F-R8-DOMSEP-PHANTOM-SECTION; `kind_uuid_registry.rs` file created per F-R8-DOMSEP-PHANTOM-FILE; BLAKE3-prefix vs raw-byte marker encoding classes distinguished per F-R8-DOMSEP-MARKER-ENCODING-MISMATCH.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 1.0     | 2026-08-22 | Initial draft. Resolves R2 CRITICAL: AuthorityPolicy / MembershipPolicy / InteropPolicy / BurnPolicy / WorkflowKind / AuditPolicy traits + InteropSelector/Outcome + FallthroughCondition + **30-kind UUIDv5 registry** + policy_registry/policy_kind_authority tables.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
