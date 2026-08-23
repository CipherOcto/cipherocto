---
rfc: 0105-v3.0
title: Private Asset ID Namespace (Sovereign/Private Boundary Clarification)
status: Accepted
version: 3.0
date: 2026-08-22
amends: RFC-0105 v2.3
builds_on:
  - rfcs/accepted/economics/0105-asset-id-derivation.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0105 v3.0 — Private Asset ID Namespace

## 0. Status

**Accepted (v3.0, 2026-08-22).** SEMVER-MAJOR version bump from v2.3. Resolves R2 finding (sovereign/private boundary needs explicit clarification for corporate chain asset namespace).

**Promotion trail:** R2 finding → research doc §3 D1 + §9 amendment table → RFC-0105 v3.0 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0105 v3.0 first in Tier 1 order per research §20 decision #9). BLAKE3 seed string reconciled to 3-component form `BLAKE3("cipherocto/asset/v1/" || role_token)` per RFC-0206 §2.3 canonical asset_id derivation (R13 fix F-R12-LENS-CROSS-CONSISTENCY-002 + F-R12-LENS-CROSS-CONSISTENCY-003).

## 1. Motivation

RFC-0105 v2.3 defines role-token enum (8 tokens, OCTO-W/A/O/B/M/T/S/G) and the canonical asset_id derivation. The vault monetary representation redesign introduces **two asset namespaces**:

1. **Sovereign assets** — `OCTO-*` role tokens + their derivatives; managed by octo treasury; chain-independent.
2. **Private assets** — corporate-chain-specific assets; managed by corporate chain operators; chain-bound via `chain_id` prefix.

v2.3 (2026-08-19) silently excludes OCTO as sovereign. v3.0 makes the boundary explicit.

## 2. Asset Namespace Specification

### 2.1 Sovereign namespace (`namespace = 0x01`)

| Asset                        | Derivation                    |
| ---------------------------- | ----------------------------- |
| `OCTO-W`                     | BLAKE3("cipherocto/asset/v1/" |     | "OCTO-W")[:16]                     |
| `OCTO-A`                     | BLAKE3("cipherocto/asset/v1/" |     | "OCTO-A")[:16]                     |
| (other OCTO-* tokens)        | analogous                     |
| `OCTO-PrivateProvider-Stake` | BLAKE3("cipherocto/asset/v1/" |     | "OCTO-PrivateProvider-Stake")[:16] |

**Namespace-byte overwrite semantics** (R13 fix F-R12-LENS-CROSS-CONSISTENCY-004): For sovereign-namespace assets (`namespace = 0x01`), `asset_id[0]` is the **asset-namespace byte** per `AssetNamespace` enum. Per RFC-0010 v1.7 §3 `derive_chain_id` parallel, the substrate runs `BLAKE3("cipherocto/asset/v1/" || role_token)[..16]` to obtain a 16-byte hash, then **overwrites** `out[0] = AssetNamespace as u8` post-hash. So `asset_id[0] = 0x01` for sovereign assets is a NAMESPACE-BYTE OVERWRITE, not a hash byte. The 15 bytes `[1..16]` are the BLAKE3 output. The overwrite applies to BOTH sovereign (`0x01`) and private (`0x02`) namespaces; the §2.2 `PRIVATE-XYZ` derivation inherits the same overwrite pattern (R13 fix F-R12-LENS-CROSS-CONSISTENCY-004 closure).

Authority: octo treasury only. RFC-0105 §Asset ID Derivation (canonical asset_id derivation in parent RFC).

**Cross-RFC reconciliation note** (R13 fix F-R12-LENS-CROSS-CONSISTENCY-002 + F-R12-LENS-CROSS-CONSISTENCY-003): Prior v3.0 draft L34-37 used 5-component BLAKE3 seed strings (e.g., `"cipherocto/asset/v1/octo/octo-w/v1"`). Per R13 lens, RFC-0206 §2.3 specifies the canonical asset_id derivation as `BLAKE3("cipherocto/asset/v1/" || role_token)[0..16]` (3-component: prefix + role_token only). v3.0 §2.1 + §2.2 reconciled to the 3-component form to match RFC-0206 §2.3 canonical derivation.

### 2.2 Private namespace (`namespace = 0x02`)

| Asset                                 | Derivation                    |
| ------------------------------------- | ----------------------------- |
| `PRIVATE-{chain_id_32B}-{asset_name}` | BLAKE3("cipherocto/asset/v1/" |     | "PRIVATE-" |     | chain_id |     | "-" |     | asset_name)[:16] |

**Namespace-byte overwrite semantics** (R13 fix F-R12-LENS-CROSS-CONSISTENCY-004 closure): For private-namespace assets (`namespace = 0x02`), `asset_id[0]` is overwritten with `AssetNamespace::Private as u8 = 0x02` post-BLAKE3 (parallel to §2.1 sovereign overwrite). The remaining 15 bytes `[1..16]` are the BLAKE3 output.

Authority: corporate chain operator (per RFC-0010 v1.7 §4 Authority Registration Flow). Multiple private assets per chain supported; one chain's `PRIVATE-XYZ` is not portable to another chain's namespace.

### 2.3 Reserved namespaces (`namespace = 0x03 - 0xFF`)

Future allocation. Per RFC-0010 v1.7 §3 Chain-id Derivation reserved-namespace range.

## 3. Authority-to-Issue Table

| Namespace        | Authority DID                | Registration path                                                              |
| ---------------- | ---------------------------- | ------------------------------------------------------------------------------ |
| 0x01 (sovereign) | octo treasury DID            | `policy_kind_authority` row registered by octo treasury                        |
| 0x02 (private)   | corporate chain operator DID | `policy_kind_authority` row registered by chain operator at chain registration |

## 4. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                         | Class | Justification                    |
| ------------------------------- | ----- | -------------------------------- |
| `asset_id_for(PRIVATE-*)`       | A     | Deterministic BLAKE3 derivation  |
| Sovereign asset registration    | A     | octo treasury signing            |
| Private asset registration      | A     | Corporate chain operator signing |
| Asset namespace collision check | A     | BLAKE3 first-16-byte uniqueness  |

## 5. Cross-References

- RFC-0010 v1.7 §3 Chain-id Derivation (chain_id BLAKE3 derivation)
- RFC-0010 v1.7 §4 Authority Registration Flow (corporate operator authority)
- RFC-0105 v2.3 (current role-token enum)
- RFC-0967-A1 §Kind UUID Registry (namespace string allocation for private asset types)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0 §3 D1 + §9 amendment table

## 6. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3.0     | 2026-08-22 | Initial draft. Sovereign/private boundary explicit. Two namespaces (0x01 sovereign / 0x02 private / 0x03-0xFF reserved). Authority-to-Issue table added. SEMVER-MAJOR per R2 finding.                                                                 |
| 3.0     | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0105 v3.0 first in Tier 1). Status bumper + citation trail. Two-namespace model + Authority-to-Issue table + Execution Class Mapping preserved. |
