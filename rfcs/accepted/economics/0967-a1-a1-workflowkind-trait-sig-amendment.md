# RFC-0967-A1-A1 — `WorkflowKind` Trait Signature Amendment

**Status:** Accepted (v1.2 effective; v1.0 filed, 2026-08-22)
**Date:** 2026-08-22
**Supersedes:** None (amends RFC-0967-A1 in-place changes)
**Layer:** B (RFC-driven, additive only — but the in-place trait-sig changes are SEMVER-MAJOR per RFC-0206 §Layer B additive-only rule)
**Related:** [RFC-0967-A1](../economics/0967-a1-policy-registry.md), [RFC-0206](../process/0206-v30-value-transfer-surface.md), [RFC-0008](../../accepted/process/0008-deterministic-ai-execution-boundary.md)

**Promotion trail:** v1.0 retroactive amendment draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 2 promotion sequence (RFC-0967-A1-A1 second in Tier 2). Tracks retroactive trail for RFC-0967-A1 in-place changes (Draft v1.0 → Draft v1.1 prior to this amendment's filing): WorkflowKind trait sig amendments (F-R8-WFCOMPOSITE-NO-PROOF-PARAM), AuditPolicy domain-separator migration (F-R8-DOMSEP-PREFIX-DRIFT), phantom RFC-0126 anchor + phantom kind_uuid_registry.rs resolution (F-R8-DOMSEP-PHANTOM-SECTION + F-R8-DOMSEP-PHANTOM-FILE), AuditPolicy duplicate constant removal (F-R9-AUDIT-VARIANT-HASH-DOMAIN-DEAD-CODE). 10 phantom substrate file refs wrapped with "pending landing via 0206-001 v3.0 + 0206-009; pre-revert reference site REVERTED per R10.5" (F-R12-XR-PHANTOM-0967-A1-A1-MISSED). Cite pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions.

---

## 1. Summary

RFC-0967-A1 was promoted from v1.0 to v1.1 in-place on 2026-08-22 with three categories of changes:

1. **WorkflowKind trait signature amendments** (R8 fix F-R8-WFCOMPOSITE-NO-PROOF-PARAM): all four methods (`validate_vault_creation`, `provision_subject`, `read_user_info`, `update_user`) amended to take `proof: &[u8]` parameter, replacing phantom `ctx: &WorkflowContext` references that violated the primitive-types-only rule.

2. **AuditPolicy domain-separator prefix migration** (R8 fix F-R8-DOMSEP-PREFIX-DRIFT): `AUDIT_VARIANT_HASH_DOMAIN` constant migrated from `cipherocto/audit/v1/` to canonical `octo/audit/v1/`.

3. **Domain-separator registry consolidation** (R8 fix F-R8-DOMSEP-PHANTOM-SECTION + F-R8-DOMSEP-PHANTOM-FILE): phantom RFC-0126 §Canonical Serialization reference replaced with concrete registry location (substrate-side registry pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009; pre-revert reference site was `crates/octo-policy/src/domain_separators.rs`, REVERTED per R10.5); phantom file reference to `crates/octo-policy/src/kind_uuid_registry.rs` resolved by creating it — file REVERTED per R10.5 scope correction, canonical source is RFC-0967-A1 §2.6.

**R9 fix F-R9-RFC-0967-A1-V11-IN-PLACE-AMENDMENT** — per BLUEPRINT.md §RFC Process, in-place amendments require a separately filed amendment draft. This RFC tracks the retroactive trail for the v1.0 → v1.1 in-place changes.

## 2. Motivation

RFC-0967-A1 had three structural gaps that the v1.1 in-place changes addressed:

- **G1.** WorkflowKind trait signatures carried a phantom `WorkflowContext` type that R5 fix F-R5-005 had already removed from the substrate (no substrate-defined `WorkflowContext`; ZK proof is encoded as a primitive byte vector). R8 fix F-R8-WFCOMPOSITE-NO-PROOF-PARAM brought the trait signatures into alignment with the substrate's primitive-types-only rule.

- **G2.** `AUDIT_VARIANT_HASH_DOMAIN` used `cipherocto/audit/v1/` as the BLAKE3 domain-separator prefix, but the canonical prefix standard (F-R8-DOMSEP-PREFIX-DRIFT) is `octo/audit/v1/`. R8 fix F-R8-DOMSEP-PREFIX-DRIFT migrated the constant to align with the registry.

- **G3.** RFC-0967-A1 §3 footnote referenced `RFC-0126 §Canonical Serialization` (a phantom — RFC-0126 is about NUMERIC ENCODING, not domain separators) and `crates/octo-policy/src/kind_uuid_registry.rs` (a phantom file path that did not exist on disk). R8 fixes F-R8-DOMSEP-PHANTOM-SECTION + F-R8-DOMSEP-PHANTOM-FILE resolved both phantoms: the file was created with 30 UUIDv5 namespace strings + frozen kind_uuid constants + unit tests (file REVERTED per R10.5; canonical source RFC-0967-A1 §2.6); the footnote was amended to point to the concrete registry location (substrate-side registry pending landing via 0206-001 v3.0 + 0206-009).

## 3. Proposed Changes (v1.1 retroactive trail)

### 3.1 WorkflowKind trait signature changes

```diff
 pub trait WorkflowKind: Send + Sync {
-    fn validate_vault_creation(&self, req: &VaultCreationRequest, ctx: &WorkflowContext) -> Result<(), WorkflowError>;
-    fn provision_subject(&self, req: &SubjectProvisionRequest, ctx: &WorkflowContext) -> Result<(), WorkflowError>;
-    fn read_user_info(&self, query: &UserInfoQuery, ctx: &WorkflowContext) -> Result<UserInfoResponse, WorkflowError>;
-    fn update_user(&self, req: &UserUpdateRequest, ctx: &WorkflowContext) -> Result<(), WorkflowError>;
+    fn validate_vault_creation(&self, req: &VaultCreationRequest, proof: &[u8]) -> Result<(), WorkflowError>;
+    fn provision_subject(&self, req: &SubjectProvisionRequest, proof: &[u8]) -> Result<(), WorkflowError>;
+    fn read_user_info(&self, query: &UserInfoQuery, proof: &[u8]) -> Result<UserInfoResponse, WorkflowError>;
+    fn update_user(&self, req: &UserUpdateRequest, proof: &[u8]) -> Result<(), WorkflowError>;
 }
```

### 3.2 AuditPolicy domain-separator migration

```diff
-const AUDIT_VARIANT_HASH_DOMAIN: &[u8] = b"cipherocto/audit/v1/";
+const AUDIT_VARIANT_HASH_DOMAIN: &[u8] = b"octo/audit/v1/";
```

Canonical location: substrate-side registry `octo-policy::domain_separators::blake3_prefix::AUDIT_VARIANT_HASH_DOMAIN` (pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009; pre-revert reference site was `crates/octo-policy/src/domain_separators.rs::blake3_prefix::AUDIT_VARIANT_HASH_DOMAIN`, REVERTED per R10.5).

### 3.3 Phantom reference resolution

§3 footnote replaced:

```diff
-The ZK envelope marker bytes 0x01 0x7a 0x6b 0x00 are defined per RFC-0126 §Canonical Serialization.
+The ZK envelope marker bytes 0x01 0x7a 0x6b 0x00 are SUBSTRATE-DEFINED and registered in the canonical domain-separator registry at substrate-side `octo-policy::domain_separators::raw_markers::ZK_ENVELOPE_MARKER` (pending landing via 0206-001 v3.0 + 0206-009; pre-revert reference site was `crates/octo-policy/src/domain_separators.rs::raw_markers::ZK_ENVELOPE_MARKER`, REVERTED per R10.5) (R8 fix F-R8-DOMSEP-PHANTOM-SECTION).
```

`kind_uuid_registry.rs` reference resolved by creating the file at `crates/octo-policy/src/kind_uuid_registry.rs` with 30 UUIDv5 namespace strings + 30 frozen `kind_uuid` constants + `kind_uuid_for()` runtime function + `total_count_matches_rfc` + `all_uuids_distinct` unit tests. **[R12 fix F-R12-XR-PHANTOM-0967-A1-A1-MISSED]:** file REVERTED per R10.5 scope correction; canonical source is RFC-0967-A1 §2.6.

### 3.4 AuditPolicy duplicate constant removal (R9 fix F-R9-AUDIT-VARIANT-HASH-DOMAIN-DEAD-CODE)

The RFC-0967-A1 §2.1 still carried a duplicate `AUDIT_VARIANT_HASH_DOMAIN` constant + `derive_audit_variant()` function block (lines 142-160) AFTER the canonical migration. This block is dead code (substrate uses substrate-side registry `octo-policy::domain_separators::blake3_prefix::derive_audit_variant` instead — pending landing via 0206-001 v3.0 + 0206-009; pre-revert reference site was `crates/octo-policy/src/domain_separators.rs::blake3_prefix::derive_audit_variant`, REVERTED per R10.5). R9 fix F-R9-AUDIT-VARIANT-HASH-DOMAIN-DEAD-CODE removes the duplicate; RFC-0967-A1 §2.1 now references the canonical location only.

## 4. Downstream Impact

- Per-policy-kind crates implementing WorkflowKind (RFC-0967-A1 §2.1) MUST add the `proof: &[u8]` parameter. Audit-driven sweep of `crates/` for `impl WorkflowKind for` blocks completed in R9 verify; no in-scope crate breaks compile (no impls land yet — WorkflowKind is a trait stub awaiting consumer migration in Phase 3 per research §10).

- All substrate-level ZK envelope marker checks (per RFC-0967-A1 §3 + research §4.3 inline examples) operate on the `proof: &[u8]` byte slice; no substrate code change needed (the byte-vector layout — kind_uuid [0..16] + zk_marker [16..20] + body [20..N] — was already substrate-defined per F-R7-ZK-LAYOUT-UNDEFINED).

- Domain-separator registry location canonical at substrate-side registry `octo-policy::domain_separators` (pending landing via 0206-001 v3.0 + 0206-009; pre-revert reference site was `crates/octo-policy/src/domain_separators.rs`, REVERTED per R10.5); all references in research doc + RFCs migrated in R8 + R9.

## 5. Cross-References

- RFC-0967-A1 (in-place changes tracked by this amendment)
- RFC-0206 §3 (ValueTransfer Trait — 11 money-movement methods)
- RFC-0008 §RFC-0008 Execution Class Mapping (Class A/B/C taxonomy)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v3.6
- Substrate-side registry `octo-policy::domain_separators` (canonical registry — pending landing via 0206-001 v3.0 + 0206-009)
- Substrate-side registry `octo-policy::kind_uuid_registry` (30 UUIDv5 namespace registry — pending landing via 0206-001 v3.0 + 0206-009; pre-revert reference site was `crates/octo-policy/src/kind_uuid_registry.rs`, REVERTED per R10.5)
- Substrate-side trait `octo-vault::value_transfer::ValueTransfer` (ValueTransfer owner trait + MAX_COMPOSITE_DEPTH — pending landing via 0206-001 v3.0; pre-revert reference site was `crates/octo-vault/src/value_transfer.rs`, REVERTED per R10.5)

## 6. Version History

| Version | Date | Change |
|---------|------|--------|
| 1.0 | 2026-08-22 | Initial amendment draft. Retroactive trail for RFC-0967-A1 → v1.1 in-place changes (R8 fixes F-R8-WFCOMPOSITE-NO-PROOF-PARAM + F-R8-DOMSEP-PREFIX-DRIFT + F-R8-DOMSEP-PHANTOM-SECTION + F-R8-DOMSEP-PHANTOM-FILE) + R9 fix F-R9-AUDIT-VARIANT-HASH-DOMAIN-DEAD-CODE (RFC-0967-A1 §2.1 duplicate constant removal). Filed per BLUEPRINT.md §RFC Process in-place-amendment-separation requirement (R9 fix F-R9-RFC-0967-A1-V11-IN-PLACE-AMENDMENT). |
| 1.2 | 2026-08-22 | **R12 fix trail (F-R12-XR-PHANTOM-0967-A1-A1-MISSED):** 10 phantom substrate file refs (L19 + L57 + L65 + L68 + L72 + L80 + L88 + L89 + L90) wrapped with "substrate-side registry pending landing via Phase 1 mission 0206-001 v3.0 + 0206-009; pre-revert reference site REVERTED per R10.5 scope correction". Per BLUEPRINT.md §Adversarial Review Process, R12 lens caught this amendment carried pre-R10.5 narrative drift after R10/R11 fixes were applied to the base RFC-0967-A1 + RFC-0206 but not propagated to this amendment. |
| 1.2 | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 2 promotion sequence (RFC-0967-A1-A1 second in Tier 2). Status bumper + citation cleanup (4 pre-existing STALE RFC-0967-A1 version pins + 2 STALE RFC-0206 version pins + 1 INVALID RFC-0126 phantom anchor all stripped per CLAUDE.md §RFC Reference Conventions). WorkflowKind trait sig amendments + AuditPolicy domain-separator migration + phantom resolution trails preserved. |