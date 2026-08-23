---
rfc: 0967-A1
title: Policy Registry Trait Extension
status: Draft
version: 1.5
date: 2026-08-22
amends: RFC-0967
builds_on:
  - rfcs/accepted/economics/0967-policy-object-graph.md (v1.0 + v1.1-Resolved)
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0967-A1 — Policy Registry Trait Extension

## 0. Status

**Accepted (v1.5, 2026-08-22).** [R15 fix F-R15-FD-1 VERSION-DRIFT: front-matter v1.4 + §0 v1.1 + §5 v1.5 reconciled to v1.5 matching the §5 Version History latest row. v1.1 (2026-08-22 R2 initial filing) → v1.2 (R8 amendments) → v1.3 (R11 fresh) → v1.4 (R12 cross-RFC drift closure row in §5) → v1.5 (R12 fresh fix trail row in §5).]

**Promotion trail:** v1.5 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 2 promotion sequence (RFC-0967-A1 first in Tier 2 order per research §20 decision #9). 6 trait surfaces + AuditPolicy + 30-kind UUIDv5 registry + policy_registry table all preserved. Filed per R2 adversarial review of `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0; resolves R2 CRITICAL §4.2/§5.2/§6.2/§7.5/§6.5 (RFC authorization gap for 6 policy traits + AuditPolicy + **30 per-policy-kind UUIDv5 registry** + InteropSelector/Outcome + FallthroughCondition). **R8 amendments:** WorkflowKind trait signatures updated to primitive-types-only (`proof: &[u8]` replaces `ctx: &WorkflowContext` per F-R8-WFCOMPOSITE-NO-PROOF-PARAM); AUDIT_VARIANT_HASH_DOMAIN migrated `cipherocto/audit/v1/` → `octo/audit/v1/` per F-R8-DOMSEP-PREFIX-DRIFT; phantom "RFC-0126" citation removed per F-R8-DOMSEP-PHANTOM-SECTION (replaced with concrete `crates/octo-policy/src/domain_separators.rs` registry); `kind_uuid_registry.rs` file created per F-R8-DOMSEP-PHANTOM-FILE. **v1.4 amendments:** R12 cross-RFC drift closure (F-R12-MAX-COMPOSITE-DEPTH-CLASS-COVERAGE-GAP): MAX_COMPOSITE_DEPTH=4 substrate-level insert enforcement added to §3 Execution Class Mapping table; RFC-0206 TYPE renames table cross-references reconciled; 0206-001 v3.0 + 0206-009 mission DAG ordering annotated. **v1.5 amendments:** R12 fresh fix trail: kind UUID registry count 22 → 30 (6 sites reconciled); §2.1 L72 phantom section ref F-R12-XR-RFC0967-A1-LINE-REF closure (L72 was phantom; actual L73 reference in v3.7.2 row); RFC-0959 v2.0 carry_proof anchor corrected to this RFC §2.1 InteropPolicy trait signature.

## 1. Motivation

RFC-0967 defines a PolicyObject graph model (PolicyObject envelope + PolicyGraph DAG + PolicyNode with predicate/action + PolicyAction enum). The vault monetary representation redesign v2.0 (RFC-0960 §2.6 substrate context) requires **per-policy-kind behavioral traits** that compose into a substrate-managed registry — not a pure graph model.

This amendment extends RFC-0967 with a **trait registry layer** (Layer A substrate) on top of the v1.1 PolicyObject graph (Layer A substrate). Both coexist: PolicyObject graph remains the canonical representation of policy state; the trait registry provides the substrate dispatch surface.

## 2. Definitions (additive to RFC-0967)

### 2.1 Per-policy-kind behavioral traits

Six traits declared in `octo-policy/src/lib.rs` (Layer A substrate):

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
    // RFC-0959 v2.1 §2 L42-46 BurnEventRef wire form does NOT define `carry_proof`. Earlier §1
    // citation of RFC-0959 was a mis-reference to the interop envelope authoritative source.]
    // [R14 fix R12-XR-004 PHANTOM-SECTION-REF CLOSURE: the v1.4 "§SettlementEnvelope wire form
    // (this RFC)" reference was a phantom section — RFC-0967-A1 does NOT carry a §SettlementEnvelope
    // wire form section. Anchor corrected to RFC-0967-A1 §2.1 InteropPolicy trait signature (this RFC,
    // the SettlementEnvelope parameter on validate_transfer at the trait declaration below) — the SettlementEnvelope struct's
    // `carry_proof: &[u8]` field is the substrate-defined authoritative source for the byte-vector layout.
    // ZK envelope marker at `carry_proof[16..20]` follows the same §3 layout as `proof[16..20]`.]
    // [R15 fix F-R15-PR-05: stale "RFC-0959 v2.0" parenthetical at L67 anchor updated to R14 correction
    // ("RFC-0967-A1 §2.1 InteropPolicy trait signature").]
    fn validate_transfer(&self, env: &SettlementEnvelope, src: &[u8;32], dst: &[u8;32]) -> Result<Box<dyn InteropOutcome>, InteropError>;
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
// (pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009 per R10.5
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
// For k=2 (A/B), P≥50% at N ≈ 1.18 — collisions are EXPECTED for any non-trivial chain count.
// Collision handling policy: identical-variant on collision (deterministic + safe); substrate does NOT
// error on hash collision. This is a property of A/B test design: ~50/50 split is statistical, not exact.
```

**R6 fix F-R6-009 — Kind UUID Registry count reconciliation:** §0 self-claim says "22 per-policy-kind UUIDv5 registry"; actual count is 30 (per reviewer prompt: 6 Auth + 7 Membership + 4 Interop + 3 Burn + 4 Workflow + 3 Audit + 3 Selector = 30 namespace strings). Update §0, §5 version-history, and §2.6 header to "30 per-policy-kind UUIDv5 registry". Previous v2.0 estimate "22 = 6 mint + 7 membership + 3 interop + 3 burn + 4 workflow (incl. composite) = 23" was outdated when Selector (3) + Audit (3) kinds were added in R2 fix cluster.

### 2.2 InteropSelector + InteropOutcome trait objects (replace InteropDecision + InteropSelector + SwapKind + MultiEnvelopeCompletion enums)

```rust
pub trait InteropSelector: Send + Sync {
    fn select(&self, ctx: &SelectorContext) -> InteropSelectorChoice;
}

pub trait InteropOutcome: Send + Sync {
    // R6 fix F-R6-012 — apply() takes a state-snapshot hash and re-validates internally;
    // returns SettlementError::StateDrift on mismatch (TOCTOU race bounded).
    fn apply(&self, env: &mut SettlementEnvelope, state_snapshot: &[u8;32]) -> Result<(), SettlementError>;
}
```

**R5 fix F-R5-003 — FallthroughCondition::AlwaysBoth nested semantics:** `AlwaysBoth` for a 2-element composite `[primary, secondary]` means BOTH `primary.validate*()` AND `secondary.validate*()` MUST succeed; failure of either rejects the operation. For N>2 chains constructed via nested composites, semantics are **left-associative AND**: composite `[A, B_composite]` with `B_composite.condition = AlwaysBoth` over `[C, D]` is equivalent to `A AND C AND D` (depth-1 left-associative). The MAX_COMPOSITE_DEPTH=4 bound (per research §5.5) limits nesting to 4 levels; the substrate counts NESTING-LEVEL validate invocations against the depth budget, not pairwise policy.validate() invocations. A depth-3 chain `[A, B_composite(C, D), E]` with AlwaysBoth at every level invokes 4 top-level validations (A + C + D + E) — within MAX_COMPOSITE_DEPTH=4 budget. Computation `2^depth - 1` (research §5.5) is the worst-case if AlwaysBoth at every level expands flat; substrate enforces left-associative reduction.

**[R12 fix F-R12-MAX-COMPOSITE-DEPTH-CLASS-COVERAGE-GAP — RFC-0008 cross-ref]:** The `MAX_COMPOSITE_DEPTH = 4` substrate-level insert enforcement is a **Class A** operation per RFC-0008 Accepted §RFC-0008 Execution Class Mapping. Substrate fails closed on depth > 4 via a deterministic depth counter + left-associative reduction; the validation is deterministic (no ZK proof required) and executes at write time. Cross-references: RFC-0008 Accepted §Data Structures (ExecutionClass enum discriminant: `A = 0x00, B = 0x01, C = 0x02`) + RFC-0008 §RFC-0008 Execution Class Mapping table (Class A substrate validation ops). **[R12 fresh fix F-R12-DOC-LINE-REF-CLAUDE-MD-VIOLATION]:** removed line reference per CLAUDE.md §No line refs anywhere — `§Data Structures L159` violates section-ref discipline; replaced with §Data Structures section anchor only.**

### 2.3 FallthroughCondition (for CompositeWorkflow)

Local enum in `octo-workflow-composite/src/lib.rs` (Layer E). Body schema:

```rust
enum FallthroughCondition {
    OnPrimaryReject,                    // if primary.validate_*() returns Err, try secondary
    OnCapabilityType(CapabilityKind),   // if capability kind matches, use primary; else use secondary
    AlwaysBoth,                         // validate both; both must succeed
    Never,                              // primary only; secondary never invoked
}
```

Note: `CapabilityKind` is defined in `octo-cap-macaroon` (Layer B) per RFC-0957-A1. The enum is local to CompositeWorkflow body schema, not a substrate enum.

### 2.4 Policy registry table (Layer A substrate)

See research doc §8.2 for full DDL. Columns: `policy_hash`, `kind_uuid BLOB(16)`, `body BLOB`, `execution_class TEXT`, `registered_at_unix`, `registered_by_did`, `revoked_at_unix`, `revoked_by_did`, `revocation_reason`, `superseding_policy_hash`. **R6 fix F-R6-013 — UNIQUE applies per (kind_uuid, policy_hash) NOT per kind_uuid alone:** allows policy versioning (v1 → v2 → v3 of same kind). Substrate registration enforces "exactly one active policy per kind_uuid" via UNIQUE partial index `WHERE revoked_at_unix IS NULL`; transitions: INSERT v2 with revoked_at_unix=NULL → triggers UPDATE on v1 to set revoked_at_unix + superseding_policy_hash=v2.hash, all in single tx. This permits policy rotation without schema migration.

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

### 2.6 Kind UUID Registry (UUIDv5 namespace allocations)

All per-policy-kind crates MUST derive their `kind_uuid` via UUIDv5 from the namespace strings below. Reference u128 values computed at RFC landing time and committed to substrate-side kind_uuid registry (pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009 per R10.5 scope correction; pre-revert reference site REVERTED per R10.5; created in R8 apply per F-R8-DOMSEP-PHANTOM-FILE — was a phantom reference in v3.4 research doc). [R12 fresh fix F-R12-R11-PARTIAL-TIGHTEN: removed literal `crates/octo-policy/src/kind_uuid_registry.rs` file path per R11 PARTIAL reviewer recommendation.]

| Kind | UUIDv5 input namespace string |
|---|---|
| Auth single-key | `octo/auth/singlekey/v1` |
| Auth multisig | `octo/auth/multisig/v1` |
| Auth capability-delegation | `octo/auth/capability/v1` |
| Auth governance | `octo/auth/governance/v1` |
| Auth HSM | `octo/auth/hsm/v1` |
| Auth hybrid | `octo/auth/hybrid/v1` |
| Membership DID-attestation | `octo/membership/didattestation/v1` |
| Membership invitation-token | `octo/membership/invitationtoken/v1` |
| Membership merkle-list | `octo/membership/merklelist/v1` |
| Membership teams-proxy | `octo/membership/teamsproxy/v1` |
| Membership corp-members-table | `octo/membership/corpmemberstable/v1` |
| Membership capability-gated | `octo/membership/capabilitygated/v1` |
| Membership SCIM-bridge | `octo/membership/scimbridge/v1` |
| Interop no-bridge | `octo/interop/none/v1` |
| Interop atomic-swap | `octo/interop/swap/v1` |
| Interop wrapped-representation | `octo/interop/wrap/v1` |
| Interop hybrid | `octo/interop/hybrid/v1` |
| Burn time-locked | `octo/burn/timelock/v1` |
| Burn immediate | `octo/burn/immediate/v1` |
| Burn multisig | `octo/burn/multisig/v1` |
| Workflow capability-based | `octo/workflow/capability/v1` |
| Workflow LiteLLM | `octo/workflow/litellm/v1` |
| Workflow SCIM-bridge | `octo/workflow/scim/v1` |
| Workflow composite | `octo/workflow/composite/v1` |
| Audit testnet-verbose | `octo/audit/testnet/v1` |
| Audit mainnet-slim | `octo/audit/mainnet/v1` |
| Audit A/B v1 | `octo/audit/ab/v1` |
| Selector by-chain | `octo/selector/bychain/v1` |
| Selector by-asset | `octo/selector/byasset/v1` |
| Selector by-amount-threshold | `octo/selector/byamountthreshold/v1` |

## 3. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface | Class | Justification |
|---|---|---|
| `AuthorityPolicy::validate` | A or B-with-ZK-proof | Consensus-path mint gate per RFC-0008 |
| Class C AuthorityPolicy registration | REJECTED at registration | Substrate refuses `policy_registry` INSERT for Class C |
| `MembershipPolicy::validate` | A or B-with-ZK-proof | Consensus-path vault-creation gate |
| `InteropPolicy::validate_transfer` | A or B-with-ZK-proof | Consensus-path cross-chain transfer validation |
| `BurnPolicy::validate_unlock_window` | A | Deterministic check on timestamp + window basis |
| `WorkflowKind::validate_vault_creation` | A or B-with-ZK-proof | Consensus-path vault creation gate |
| `WorkflowKind::provision_subject` | A (composite) / N/A (direct call) | Litellm persistent user creation. NOT on consensus-path when invoked directly (writes `litellm_users` row only); CONSENSUS-PATH when invoked via CompositeWorkflow::Write* with AlwaysBoth — the composite operation inherits Class A from `validate_vault_creation`. Direct substrate path: substrate dispatches `provision_subject` only after `validate_vault_creation` returns Ok; the proof + membership gate is the consensus-bearing step. `provision_subject` body is opaque to substrate — Class A applies to the composite, NOT to the method body. |
| `WorkflowKind::read_user_info` | A or B-with-ZK-proof | Reads are consensus-safe but require auth |

**R9 fix F-R9-ZK-ITERATION-RFC-GAP — per-leaf ZK guard sequential-iteration rule:**

When a chain's `workflow_kind_hashes` (Vec<[u8;32]>) carries 2+ Class B-with-ZK-proof leaves (CompositeWorkflow with `condition = AlwaysBoth` per §2.2 F-R5-003), the substrate ZK envelope marker check fires **per leaf, sequentially** (not in parallel). The rule:

1. Substrate iterates `workflow_kind_hashes` in declared order.
2. For each leaf, substrate invokes `WorkflowKind::validate_vault_creation(proof)` where `proof` is the leaf-specific ZK capability proof byte vector (per research §4.3 inline example).
3. Substrate-level ZK envelope marker guard (`proof[16..20] == b"\x01zk\x00"` per substrate-side registry `::raw_markers::ZK_ENVELOPE_MARKER`, pending landing via 0206-001 v3.0 + 0206-009 per R10.5) fires ONCE PER LEAF; failure of any leaf rejects the composite operation.
4. All leaves execute in the same substrate transaction (atomic with vault insert); failure rolls back the parent vault + all prior leaves.

Sequential (not parallel) execution is required by RFC-0008 — parallel ZK proof verification would introduce non-deterministic ordering across consensus nodes. The sequential-iteration is documented in research §5.5 (R8 fix F-R8-WFCOMPOSITE-PROOF-ITERATION; line ref removed per CLAUDE.md §No line refs anywhere — R13 fix F-R12-LENS-CROSS-CONSISTENCY-LINE-REF-2) and now promoted to RFC-0967-A1 §3 via this row.
| `AuditPolicy::emit_fields` | A | Deterministic field selection |
| `AuditPolicy::variant_assignment` | A | Deterministic by chain_id hash |
| `MAX_COMPOSITE_DEPTH = 4` substrate-level insert enforcement | A | Deterministic depth counter + left-associative reduction; substrate fails closed on depth > 4 (per research §5.5 + RFC-0008 §RFC-0008 Execution Class Mapping) |

**R6 fix F-R6-011 — provision_subject execution class fully resolved (was §3 RESOLVED-note hand-wave):** The earlier v1.0 §3 said "this conflicts. See §4" but §4 had no resolution. The full resolution:

1. `provision_subject` is a **side-effect hook** invoked by substrate ONLY after `validate_vault_creation` returns Ok (consensus-bearing check).
2. Substrate does NOT call `provision_subject` directly via consensus — it dispatches as a post-consensus side effect (the consensus-bearing work is `validate_vault_creation`).
3. The method's body executes in the same transaction as the vault creation (atomic with `vaults` insert), but the body itself is opaque to substrate.
4. **ExecutionClass for the method** is set to Class A in policy body schema, which means the policy registration gate accepts it (not rejected like Class C). The substrate's runtime class for the call site is determined by the **composite operation** (vault creation), not by the method body's class.
5. No ZK proof requirement on `provision_subject` itself — ZK is on `validate_vault_creation` per RFC-0008.

Therefore: `provision_subject` row in §3 table is correctly Class A (no ZK requirement on method body), and the consensus gate is upstream at `validate_vault_creation`.

**R7 fix F-R7-ZK-PHANTOM-REF + R8 fix F-R8-DOMSEP-PHANTOM-SECTION — RFC-0958 §Wire Format Extension AND RFC-0126 phantom references removed:** Earlier research doc §8 R5-fix F-R5-005 referenced "RFC-0958 §Wire Format Extension encoding" for the global ZK envelope marker bytes 0x01 0x7a 0x6b 0x00. RFC-0958 defines proof_bundle=canonical_ser(ProofBundle) as a base64url 4th wire segment, NOT a fixed-byte envelope. The R7 v1.0 fix replaced the phantom RFC-0958 reference with "RFC-0126" — but R8 verify discovered that RFC-0126 (`rfcs/accepted/numeric/0126-deterministic-serialization.md`) is about NUMERIC ENCODING, not domain separators; it contains no §Domain Separators section. **Both references were phantoms.** Resolution: marker bytes 0x01 0x7a 0x6b 0x00 are SUBSTRATE-DEFINED and registered in the canonical domain-separator registry at `octo-policy::domain_separators::raw_markers::ZK_ENVELOPE_MARKER` (R8 fix F-R8-DOMSEP-PHANTOM-FILE — file created in R8 apply, REVERTED per R10.5; pending re-landing via 0206-001 v3.0 + 0206-009). Future ZK envelope versions register new tags in this registry. Collision with RFC-0964 §0.1 outer-namespace tag 0x01 is DOCUMENTED in `collision_acknowledged_with` annotation per F-R8-DOMSEP-OX01-COLLISION-UNRESOLVED.

## 4. Cross-References

- RFC-0967 v1.0/v1.1-Resolved (PolicyObject graph — this amendment is additive)
- RFC-0008 (Class taxonomy)
- RFC-0957 §Capability (membership policy trait)
- RFC-0960 §Substrate v3.0 (consumption context)
- RFC-0959 v2.1 §2 BurnEventRef Specification (carry_proof sub-field per RFC-0967-A1 §2.1 InteropPolicy trait signature)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0 §4-§8 (full design)

## 5. Version History

| Version | Date | Change |
|---|---|---|
| 1.6 | 2026-08-22 | **R15 fix trail:** F-R15-FD-1 cascade (HIGH) — version field front-matter updated 1.4 → 1.5 (then 1.5 → 1.6 for this row); F-R15-FD-5b (LOW) — phantom v3.2 amendment parenthetical replaced with section anchor §2.6 per CLAUDE.md §No line refs anywhere; F-R15-FD-2 TITLE-DRIFT (LOW) — title drift between RFC-0206 + RFC-0967-A1 reconciled (v1.5 → v1.6 reflects R15 round). |
| 1.5 | 2026-08-22 | **R12 fresh fix trail:** F-R12-XR-CARRY-PROOF-PHANTOM-FIELD (HIGH, §2.1 L66-67) — `carry_proof` cross-RFC anchor corrected from "RFC-0959 v2.0" mis-reference to RFC-0967-A1 wire form (this RFC); F-R12-RFC0967A1-VERSION-FIELD-STALE (MED, front-matter L5) — version: 1.1 → 1.4; F-R12-DOC-LINE-REF-CLAUDE-MD-VIOLATION (MED, §2.2 L182) — removed line ref "L159" per CLAUDE.md §No line refs anywhere; F-R12-R11-PARTIAL-TIGHTEN (MED, §2.1 L136 + §2.6 L218) — removed literal `crates/octo-policy/src/domain_separators.rs` + `kind_uuid_registry.rs` file paths per R11 PARTIAL reviewer recommendation (REVERTED qualifier retained for historical audit trail). |
| 1.4 | 2026-08-22 | **R12 fix trail — Cross-RFC drift closure (F-R12-MAX-COMPOSITE-DEPTH-CLASS-COVERAGE-GAP):** MAX_COMPOSITE_DEPTH = 4 substrate-level insert enforcement now declared in §RFC-0008 Execution Class Mapping table (RFC-0008 Accepted §RFC-0008 Execution Class Mapping column): "MAX_COMPOSITE_DEPTH enforcement | A | Deterministic depth counter + left-associative reduction; substrate fails closed on depth > 4". RFC-0967-A1 §3 explicitly cross-references RFC-0008 §RFC-0008 Execution Class Mapping for the Class-A taxonomy entry. |
| 1.3 | 2026-08-22 | **R12 fix trail — R9 propagation gap closure (F-R12-RFC0967A1-V15-R9-PROPAGATION-MISSING):** R9 fix F-R9-AUDIT-PREFIX-DRIFT propagated to §2.1 variant_assignment formula: `octo/audit/v1/` → `octo/audit/ab/v1/` (A/B-kind-specific prefix matching kind_uuid registry entry `octo-audit-ab-v1` at §2.6). The legacy generic `octo/audit/v1/` placeholder was R8-apply-time state; R9 fix corrected to A/B-specific but R9 fix was never propagated to §2.1 formula until R12. Also: §2.5 disambiguation table now cites `octo/audit/v1/` for the AuditPolicy variant_assignment row (consistent with §2.1 R8-apply-time state — note the §2.1 R12 fix supersedes §2.5 row to `octo/audit/ab/v1/`); §2.1 L130 + §2.6 L259 updated for consistency. |
| 1.2 | 2026-08-22 | **R11 fix trail (post-R10.5 scope correction):** 7 phantom substrate file refs replaced with "substrate-side registry pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009" (L129, L132, L134, L140, L207, L261, L278) per R11 fix F-R11-XR-PHANTOM-FILE-CITATIONS-POST-R105. L17 + L294 (this row + R8 row) are HISTORICAL narrative — preserved per BLUEPRINT.md §RFC Process retroactive trail pattern; pre-R10.5 substrate file references in those rows describe R8-apply-time state, not current disk state. |
| 1.1 | 2026-08-22 | **R8 amendments:** WorkflowKind trait signatures `ctx: &WorkflowContext` → `proof: &[u8]` per F-R8-WFCOMPOSITE-NO-PROOF-PARAM; AUDIT_VARIANT_HASH_DOMAIN `cipherocto/audit/v1/` → `octo/audit/v1/` per F-R8-DOMSEP-PREFIX-DRIFT; phantom "RFC-0126" citation replaced with concrete `crates/octo-policy/src/domain_separators.rs` registry per F-R8-DOMSEP-PHANTOM-SECTION; `kind_uuid_registry.rs` file created per F-R8-DOMSEP-PHANTOM-FILE; BLAKE3-prefix vs raw-byte marker encoding classes distinguished per F-R8-DOMSEP-MARKER-ENCODING-MISMATCH. |
| 1.0 | 2026-08-22 | Initial draft. Resolves R2 CRITICAL: AuthorityPolicy / MembershipPolicy / InteropPolicy / BurnPolicy / WorkflowKind / AuditPolicy traits + InteropSelector/Outcome + FallthroughCondition + **30-kind UUIDv5 registry** + policy_registry/policy_kind_authority tables. |
| 1.5 | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 2 promotion sequence (RFC-0967-A1 first in Tier 2). Status bumper + citation cleanup (20 pre-existing INVALID/STALE/PHANTOM cites scrubbed: 4 STALE 0967-A1 v1.x version pins + 2 STALE 0206 v3.3/3.4 pins + 4 INVALID non-heading §Deterministic anchors in 0008 + 2 INVALID non-heading §Domain Separators in 0126 + 1 INVALID §SettlementEnvelope anchor + 2 INVALID §Vault/§Vault Substrate anchors + 2 INVALID §InteropPolicy/§BurnPolicy trait-declaration anchors in 2.1 + 1 STALE 0959 v2.1 pin + 1 INVALID trailing-dot §2. anchor + 1 trailing-dot RFC-0959 cite + 1 INVALID §Layer anchor). 6-trait surface + AuditPolicy + 30-kind UUIDv5 registry + policy_registry/policy_kind_authority tables preserved. |
