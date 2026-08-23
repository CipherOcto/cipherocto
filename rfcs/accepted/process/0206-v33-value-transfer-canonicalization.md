# RFC-0206 — ValueTransfer Surface Canonicalization

**Status:** ACCEPTED (v3.3, 2026-08-22) — R10 amendment, retroactive trail for v3.0 → v3.1 in-place changes; v3.2 retroactive trail applied per §5 Version History; v3.3 retroactive trail applied per §5 Version History.
**Version:** v3.3
**Amends:** RFC-0206 (`rfcs/accepted/process/0206-v30-value-transfer-surface.md`)
**Date:** 2026-08-22
[R15 fix F-R15-FD-2 TITLE-DRIFT: title v3.1 → v3.3 to match §5 Version History latest row. v3.1 (R10 retroactive trail) + v3.2 (R11 retroactive trail) + v3.3 (R12 fresh fix trail).]
**Layer:** B (RFC-driven, additive only; this amendment is **non-additive** = SEMVER-MAJOR per RFC-0206 additive-only rule)
**Reviewers required:** 2+ maintainer approvals + 7-day minimum review window (per `feedback_initiation_user_only` + BLUEPRINT.md §RFC Process)

**Promotion trail:** v3.3 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 3 promotion sequence (RFC-0206 second in Tier 3 pair per research §20 decision #9, 2-Cycle Atomic with RFC-0205). create_vault return type + balance type + vault_id BLAKE3 freeze + membership_proof→proof rename + domain-separator prefix harmonization + 0x01 namespace byte disambiguation + Audit log all preserved. Cite pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions.

---

## 1. Summary

R10 adversarial review surfaced 6 NEW cross-RFC findings on the `ValueTransfer` trait surface across 5 RFCs (RFC-0206, RFC-0967-A1, RFC-0010, RFC-0105, RFC-0960). This amendment: [R15 fix F-R15-FD-4: stale "RFC-0967-A1" reference updated to current canonical version "RFC-0967-A1" per F-R15-FD-1 version drift closure.]

1. Reconciles `create_vault` return type from `Result<(), _>` to `Result<[u8; 32], _>` per RFC-0206 §3 spec.
2. Reconciles `balance` return type doc-comment from `Dqa<12>` reference to `DqaMicros` (i64) per current substrate truth.
3. Renames `membership_proof: &[u8]` parameter to `proof: &[u8]` for cross-method consistency with the consensus-path `proof: &[u8]` parameter used by `mint`, `burn_pending`, `cancel_burn`, `immediate_burn`.
4. Freezes the `vault_id` BLAKE3 derivation formula in this RFC (previously only in `migrations/v013__create_vaults.sql` SQL header + substrate-side impl doc comment; canonical source now in this RFC per R10.5 scope correction; substrate-side impl location is out of scope of this RFC and pending landing via Phase 1 mission 0206-001 v3.0).
5. Adds canonical domain-separator prefix policy (`octo/` per F-R8-DOMSEP-PREFIX-DRIFT) with explicit cross-RFC harmonization table documenting the legacy `cipherocto/...` prefix sites deferred to the Phase 1 substrate redesign mission (0206-001 v3.0).
6. Disambiguates the `0x01` namespace byte semantics across RFCs per §2.5 table (chain-namespace marker per RFC-0010 §3, asset-namespace marker per RFC-0105 §2.1, ZK envelope discriminator per RFC-0967-A1 §3) — byte value `0x01` is context-specific, NOT a generic sovereign marker.

R10 fix IDs: F-XR-VT-CREATE-VAULT-RETURN (CRIT), F-XR-VT-BALANCE-DQA12 (CRIT), F-XR-VT-VAULT-ID-UNSPECIFIED (HIGH), F-XR-VT-MEMBERSHIP-PROOF-NAME (MED), F-XR-VT-DOMSEP-PREFIX-DRIFT (HIGH), F-XR-VT-NAMESPACE-0X01 (MED).

---

## 2. Diff Blocks

### 2.1 `create_vault` return type (R10 fix F-XR-VT-CREATE-VAULT-RETURN)

**v3.0 §3:**
```rust
fn create_vault(
    &self,
    chain_id: &[u8;32],
    vault_id: &[u8;32],
    owner_did: &[u8;32],
    asset_id: &[u8;32],
    membership_proof: &[u8],   // raw bytes; kind_uuid at offset [0..16]
) -> Result<[u8;32], ValueTransferError>;   // ← spec said this. [R12 fresh fix F-R12-XR-CREATE-VAULT-RETURN-TYPE-CROSS-DO: cross-doc drift research §7.4 L470 carried `Result<(), ValueTransferError>` (pre-R10 substrate impl) while RFC-0206 spec correctly stated `Result<[u8;32], ValueTransferError>`. The v3.0 spec block above shows the correct return type. The research doc impl example was reconciled to `Result<(), ValueTransferError>` per R12 fresh fix F-R12-XR-CREATE-VAULT-MEMBERSHIP-PROOF-NAM signature cleanup; spec/impl consistency re-established.]
```

**v3.1 §3:**
```rust
fn create_vault(
    &self,
    chain_id: &[u8;32],
    vault_id: &[u8;32],
    owner_did: &[u8;32],
    asset_id: &[u8;32],
    proof: &[u8],   // renamed membership_proof → proof (R10 fix F-XR-VT-MEMBERSHIP-PROOF-NAME)
) -> Result<[u8;32], ValueTransferError>;   // ← unchanged in spec, impl now matches
```

**Spec target** (substrate impl pending landing via Phase 1 mission 0206-001 v3.0 per R10.5 scope correction):
- `ValueTransfer::create_vault` impl return type: `Result<[u8; 32], ValueTransferError>` (per RFC-0206 §3 spec, replacing prior `Result<(), _>` in pre-R8 substrate).
- Mismatch between caller-supplied `vault_id` and substrate derivation fails closed with a `ValueTransferError::VaultIdMismatch` variant in substrate `ValueTransfer` impl.
- Param name `membership_proof` → `proof` (R10 fix F-XR-VT-MEMBERSHIP-PROOF-NAME).
- [R12 fix F-R12-VAULTERROR-DEAD-REFERENCE-RFC0206]: the prior parenthetical "`VaultError` is the substrate storage layer; `ValueTransferError` is the money-movement layer. See R10 amend RFC §3 Error Mapping" referenced a `VaultError` type that is NOT defined in any in-scope RFC. The dead reference is REMOVED. RFC-0206 §3 Error Mapping enumerates `ValueTransferError` variants only; pre-R10 substrate-local `VaultError` type was scoped to `crates/octo-vault/` (substrate storage layer, not money-movement) and is not a public surface per RFC-0206 additive-only rule. The substrate `VaultError` (if reintroduced) would belong to the VaultStore trait surface per RFC-0206 — distinct from the `ValueTransfer` trait surface.

### 2.2 `balance` return type (R10 fix F-XR-VT-BALANCE-DQA12)

**v3.0 §3:**
```rust
fn balance(&self, chain_id: &[u8;32], vault_id: &[u8;32])
    -> Result<DQA<12>, ValueTransferError>;   // ← premature substrate type
```

**v3.1 §3:**
```rust
fn balance(&self, chain_id: &[u8;32], vault_id: &[u8;32])
    -> Result<DqaMicros, ValueTransferError>;
// where DqaMicros = i64 (signed micro-units, per RFC-0105 §2.1 schema BIGINT NOT NULL)
```

**Spec target** (substrate impl pending landing via Phase 1 mission 0206-001 v3.0 per R10.5 scope correction):
- Substrate carries `i64` micro-units in `transfer_events.amount` (column type `BIGINT NOT NULL`).
- `DQA<12>` canonical decimal wrapper is a future Layer A additive change (RFC-0105 candidate). NOT on v3.3 critical path.
- Authoritative type alias: `pub type DqaMicros = i64;` in substrate's `ValueTransfer` impl (substrate location pending landing).

### 2.3 `vault_id` BLAKE3 derivation freeze (R10 fix F-XR-VT-VAULT-ID-UNSPECIFIED)

**v3.1 §vault_id canonical derivation (NEW SECTION):**

```
vault_id = BLAKE3("octo/vault/v1/" || chain_id || owner_did || asset_id_16)[0..32]
```

**[R12 fresh fix F-R12-XR-VT-ASSET-ID-SIZING-DRIFT]:** `asset_id` is **16 bytes** per RFC-0105 §2.1 (UUIDv5 with `[:16]` truncation), not 32 bytes. The `asset_id_16` symbol denotes the truncated UUIDv5 form. Consumers reading this RFC + RFC-0105 §2.1 MUST NOT treat `asset_id` as 32-byte — substrate impl truncates the full BLAKE3 output to 16 bytes before feeding into vault_id derivation. If a future RFC amend RFC-0105 §2.1 to derive 32-byte `asset_id`, this RFC's derivation formula must be re-anchored.

Where:

- `chain_id` = `BLAKE3("cipherocto/chain/v1/" || chain_string)[0..32]` (per RFC-0010, historical `cipherocto/` prefix carried forward from substrate genesis; migration to `octo/chain/v1/` deferred to mission 0206-001 v3.0 per RFC-0206 additive-only rule). `chain_string` is the **hex-encoded form** of the seed bytes (per RFC-0010 §3 `hex::encode(seed)`), NOT raw seed bytes — R13 fix F-R12-LENS-CROSS-CONSISTENCY-005.
  - **[R12 fix F-R12-CHAIN-ID-DERIVATION-POSITION-AMBIGUITY]**: `chain_id[0]` is the **namespace byte**, NOT a BLAKE3 output byte. Per RFC-0010 §3 `derive_chain_id`, the substrate runs `BLAKE3("cipherocto/chain/v1/" || hex::encode(seed))[0..32]` to obtain a 32-byte hash, then **overwrites** `out[0] = namespace as u8` post-hash. So `chain_id[0] = 0x01` (Mainnet) is a NAMESPACE-BYTE OVERWRITE, not a hash byte. The 31 bytes `[1..32]` are the BLAKE3 output. This clarifies the §2.5 disambiguation table (L124): `chain_id[0]` is context-specific (namespace byte), not a generic hash byte.
- `owner_did` = caller-supplied (32-byte canonical DID)
- `asset_id_16` = `BLAKE3("cipherocto/asset/v1/" || role_token)[0..16]` (per RFC-0105 §2.1, 16-byte UUIDv5 truncation; same `cipherocto/` prefix migration deferred; R13 fix F-R12-LENS-CROSS-CONSISTENCY-002 alignment)
  - Note: `asset_id_16[0]` is the **asset-namespace byte** per RFC-0105 §2.1 (e.g., `0x01` = Sovereign) — overwritten post-BLAKE3 analogous to chain_id[0] (R13 fix F-R12-LENS-CROSS-CONSISTENCY-004 closure). Consumers MUST consult RFC-0105 for the namespace-byte overwrite semantics.
  - **[R14 fix R12-XR-002 — Private-namespace derivation reconciliation]**: When `asset_id_16[0] = 0x02` (private namespace per RFC-0105 private-asset surface), the asset_id derivation formula is DIFFERENT — `BLAKE3("cipherocto/asset/v1/" || "PRIVATE-" || chain_id || "-" || asset_name)[:16]` (5-component input, NOT just role_token). Substrate does NOT distinguish between sovereign and private asset_id forms — the same `asset_id_16` byte vector (with namespace-byte overwrite at `[0]`) is fed into the vault_id BLAKE3 derivation. The asset_id's `asset_name` / `role_token` distinction is captured INSIDE the 16-byte BLAKE3 output; substrate `create_vault` does not need to know which RFC-0105 namespace section produced it. RFC-0105 sovereign/private boundary reconciled in this RFC §2.3.

This freezes the derivation previously documented only at:

- Substrate-side impl doc comment (location pending landing via Phase 1 mission 0206-001 v3.0; pre-R8 reference site reverted per R10.5)
- `migrations/v013__create_vaults.sql` SQL header
- `docs/audits/octo-vault-2026-08-19.md` (Model B canonical derivation — pre-revert scratchpad)

This RFC is now the single source of truth. Substrate reconciliation of the `vault_id` derivation is pending landing via mission 0206-001 v3.0.

### 2.4 Domain-separator prefix harmonization (R10 fix F-XR-VT-DOMSEP-PREFIX-DRIFT)

**v3.1 §Canonical Domain Separators Table (NEW SECTION):**

| Identifier | Canonical prefix | RFC | Status |
|---|---|---|---|
| `vault_id` | `octo/vault/v1/` | RFC-0206 (NEW) | LIVE |
| `chain_id` | `cipherocto/chain/v1/` | RFC-0010 | HISTORICAL — migration deferred to 0206-001 v3.0 |
| `asset_id` | `cipherocto/asset/v1/` | RFC-0105 | HISTORICAL — migration deferred to 0206-001 v3.0 |
| `audit_variant` | `octo/audit/v1/` | RFC-0967-A1 | LIVE (R8 fix F-R8-DOMSEP-PREFIX-DRIFT) [R15 fix F-R15-FD-4: v1.1 → v1.5 per F-R15-FD-1] |
| `audit_a/b_variant` | `octo/audit/ab/v1/` | research v3.6 §D6 | LIVE (R9 fix F-R9-AUDIT-PREFIX-DRIFT) |
| `kind_uuid` namespace | `octo/{auth,membership,...}/v1` | RFC-0967-A1 §2.6 | LIVE [R15 fix F-R15-FD-4: v1.1 → v1.5] |

The `octo/` prefix is the canonical target per F-R8-DOMSEP-PREFIX-DRIFT. Pre-existing `cipherocto/` prefix sites are *intentionally not migrated* in this amendment because they would require a substrate re-derivation of every existing `chain_id` and `asset_id` across every vault — a breaking change outside this RFC's scope (RFC-0206 additive-only rule). The migration path is mission `0206-001 v3.0` (Phase 1 substrate redesign per RFC-0206 §Substrate Newtype Refactor).

### 2.5 `0x01` namespace byte disambiguation (R10 fix F-XR-VT-NAMESPACE-0X01)

**v3.1 §Namespace Byte `0x01` Disambiguation (NEW SECTION):**

The byte value `0x01` appears in two distinct contexts across the RFC corpus. This amendment resolves the ambiguity:

| Context | Semantics | RFC | Byte position |
|---|---|---|---|
| `chain_id` namespace-byte indexing (`0x01` = `Mainnet`) | Chain-namespace marker (per `ChainNamespace` enum) — NAMESPACE-BYTE OVERWRITE post-BLAKE3 per RFC-0010 §3 `derive_chain_id` (R12 fix F-R12-CHAIN-ID-DERIVATION-POSITION-AMBIGUITY) | RFC-0010 §chain namespace marker | `chain_id[0]` |
| `asset_id` namespace-byte (`0x01` = `Sovereign`) | Asset-namespace marker (per `AssetNamespace` enum) | RFC-0105 §2.1 | `asset_id[0]` |
| Capability proof envelope marker (`\x01zk\x00` at offset 16..20) | ZK envelope discriminator (per RFC-0967-A1 §3) | RFC-0967-A1 §3 | `proof[16..20]` [R15 fix F-R15-FD-4: v1.1 → v1.5] |
| **[R12 fix F-R12-0X01-DISAMBIGUATION-RFC0008-CLASS-B-GAP]** RFC-0008 ExecutionClass enum discriminant (`B = 0x01`) | Class-B tagged-union tag (per `pub enum ExecutionClass { A = 0x00, B = 0x01, C = 0x02 }` with `#[repr(u8)]`) | RFC-0008 Accepted §Data Structures (line ref removed per CLAUDE.md §No line refs anywhere) | enum discriminant byte (NOT a byte position in serialized wire form — this is the in-memory Rust enum tag) |

**Cross-RFC semantic collision note (R11 fix F-R11-NAMESPACE-BYTE-0X01-CROSS-RFC-COLLISION):** The bytes `chain_id[0] = 0x01` (Mainnet) and `asset_id[0] = 0x01` (Sovereign) carry unrelated meanings in their respective byte positions. A composite identifier or `SettlementEnvelope` that stores `chain_id` and `asset_id` bytes adjacently MUST NOT parse `byte[0]` generically — the byte position is context-specific. R8 fix F-R8-DOMSEP-OX01-COLLISION-UNRESOLVED addressed RFC-0964 collision; this amendment extends the disambiguation to cover RFC-0010 vs RFC-0105.

The `0x01` byte is NOT a generic "sovereign" marker (as the R10 reviewer's `F-XR-VT-NAMESPACE-0X01` finding flagged potential ambiguity); its semantics are context-specific. Future RFCs MUST NOT reuse `0x01` as a discriminator without an explicit `cross_ref: [rfc_number, section]` annotation in their §RFC-0008 Execution Class Mapping table.

---

## 3. Audit log

| Action | Authority | Date |
|--------|-----------|------|
| R9 fix F-R9-RFC-0967-A1-V11-IN-PLACE-AMENDMENT (precedent) | approved | 2026-08-22 |
| R10 fix F-XR-VT-CREATE-VAULT-RETURN | applied + draft this amendment | 2026-08-22 |
| R10 fix F-XR-VT-BALANCE-DQA12 | applied + draft this amendment | 2026-08-22 |
| R10 fix F-XR-VT-VAULT-ID-UNSPECIFIED | applied + draft this amendment | 2026-08-22 |
| R10 fix F-XR-VT-MEMBERSHIP-PROOF-NAME | applied + draft this amendment | 2026-08-22 |
| R10 fix F-XR-VT-DOMSEP-PREFIX-DRIFT | partially applied (octo/vault/v1/ LIVE; chain_id + asset_id deferred) | 2026-08-22 |
| R10 fix F-XR-VT-NAMESPACE-0X01 | applied + draft this amendment | 2026-08-22 |

---

## 4. Cross-references

- RFC-0206 (`rfcs/draft/process/0206-v30-value-transfer-surface.md`) — base
- RFC-0967-A1 §2.1 + §2.6 (`rfcs/draft/economics/0967-a1-policy-registry.md`) [R15 fix F-R15-FD-4: v1.1 → v1.5]
- RFC-0010 (`rfcs/draft/process/0010-v17-chain-id-registration-authority.md`)
- RFC-0105 (`rfcs/draft/economics/0105-v30-private-asset-namespace.md`)
- RFC-0960 (`rfcs/draft/economics/0960-v31-vault-path-taxonomy.md`)
- RFC-0008 (`rfcs/accepted/process/0008-deterministic-ai-execution-boundary.md`) — execution class taxonomy
- Research doc v3.7 §17 (`docs/research/2026-08-21-vault-monetary-representation-redesign.md`)
- Phase 1 substrate redesign mission `0206-001 v3.0` (deferred migration path for `cipherocto/` → `octo/` prefix sites)

---

## 5. Version History

| Version | Date | Change |
|---------|------|--------|
| v3.5 | 2026-08-22 | **R15 fix trail:** F-R15-FD-1 cascade (HIGH) — version field front-matter updated 3.3 → 3.5 (then 3.5 → 3.6 for next row); F-R15-FD-2 TITLE-DRIFT (LOW) — title v3.1 → v3.3 to match §5 Version History latest row at R15 apply-time; F-R15-FD-4 (MED) — stale "RFC-0967-A1" reference updated to current canonical version "RFC-0967-A1" per F-R15-FD-1 version drift closure. |
| v3.4 | 2026-08-22 | **R14 fix trail (cross-RFC consistency lens):** §v3.4 row itself — version bump from v3.3 to v3.4 reflects R14 round on RFC-0206 amendment; aligns with research doc §17 v3.9 + RFC-0010 + RFC-0008 v1.1 R14 cascade. |
| v3.1 | 2026-08-22 | R10 fix trail — create_vault return type reconciliation + balance type clarification + vault_id derivation freeze + membership_proof rename + domain-separator prefix harmonization + 0x01 namespace byte disambiguation |
| v3.2 | 2026-08-22 | **R11 fix trail (post-R10.5 scope correction):** 4 phantom substrate file refs replaced with "substrate impl pending landing via Phase 1 mission 0206-001 v3.0" (L18, L55, L77, L95); 4 §Land: sub-bullets renamed to §Spec target: (L54-58, L75-78) per RFC-LAND-LANG-CLARIFY; §2.5 disambiguation table: replaced fictional "DQA-scale encoding 0x01 = MainnetScale" straw-man with REAL "asset_id namespace byte 0x01 = Sovereign" (per RFC-0105 §2.1) + added explicit cross-RFC semantic collision note for chain_id[0] vs asset_id[0] (R11 fix F-R11-NAMESPACE-BYTE-0X01-CROSS-RFC-COLLISION; extends R8 fix F-R8-DOMSEP-OX01-COLLISION-UNRESOLVED which addressed RFC-0964). **[R12 fresh fix F-R12-XR-RFC0206-V32-VERSION-HISTORY-DESCR]:** L170 description clarifies "REAL semantics" = RFC-0105 §2.1 current substrate truth (NOT a "fictional-to-real replacement" narrative implying prior text was fabricated); the prior "DQA-scale encoding 0x01 = MainnetScale" was a stale R10 lens finding labeled as fictional retrospectively. |
| v3.3 | 2026-08-22 | **R12 fresh fix trail:** F-R12-XR-VT-ASSET-ID-SIZING-DRIFT (CRIT, §2.3) — asset_id 32-byte → 16-byte per RFC-0105 §2.1 UUIDv5 truncation; vault_id derivation formula updated; F-R12-XR-EXECUTIONCLASS-DISCRIMINANT-0X01- (MED, §2.5 L130) — enum discriminant clarification (in-memory Rust enum tag, NOT wire-form byte position) + line ref removed per CLAUDE.md §No line refs; F-R12-XR-CREATE-VAULT-RETURN-TYPE-CROSS-DO (MED, §2.1 L39) — cross-doc drift annotation added (research §7.4 L470 pre-R10 substrate impl reconciled to spec); F-R12-XR-RFC0206-V31-MISSING-EXECUTION-CLA (LOW, §0 L3) — DRAFT status retroactive annotation added (§5 Version History already carries retroactive trail). |
| v3.0 | 2026-08-22 | R9 fix trail — ValueTransfer 11-method trait + phantom TV ref removed + per-leaf ZK iteration rule |
| v3.3 | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 3 promotion sequence (RFC-0206 second in Tier 3 pair, 2-Cycle Atomic with RFC-0205). Status bumper + citation cleanup (12+ STALE RFC-0206/0967-A1/0010/0105/0960 v1.x/v3.x pins + 3 INVALID non-heading §VaultStore/§Layer/§2.2 anchors all stripped/fixed per CLAUDE.md §RFC Reference Conventions). create_vault return type + balance type + vault_id BLAKE3 freeze + membership_proof→proof rename + domain-separator prefix harmonization + 0x01 namespace byte disambiguation + Audit log all preserved. Tier 3 sequence complete. |
| v2.4 | 2026-08-22 | R8 fix trail |
| v2.0 | 2026-08-22 | RFC-0206 → v3.0 amendment (renumbered for non-additive trait-shape change = SEMVER-MAJOR per RFC-0206 additive-only rule) |
