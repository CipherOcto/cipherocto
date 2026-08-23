---
rfc: 0105-v3.1
title: Private Asset ID Namespace (Sovereign/Private Boundary Clarification)
status: Accepted
version: 3.1
date: 2026-08-22
amends: RFC-0105 v2.3
builds_on:
  - rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0105 v3.1 — Private Asset ID Namespace

## 0. Status

**Accepted (v3.1, 2026-08-23).** SEMVER-MAJOR version bump from v2.3 (numeric parent). Resolves R2 finding (sovereign/private boundary needs explicit clarification for corporate chain asset namespace).

**Promotion trail:** R2 finding → research doc §3 D1 + §9 amendment table → RFC-0105 v3.0 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0105 v3.0 first in Tier 1 order per research §20 decision #9) → v3.1 R3 fix-all (substrate-grounded re-write of §1/§2.1/§2.2; cross-RFC cite corrections; VH row consolidation).

**Substrate anchor (v3.1, post-R3 fix-all):** canonical asset_id derivation is `asset_id_for(role_token: &str) -> [u8; 32]` per `determin/src/asset_id.rs:85-93` (2-component BLAKE3: `b"cipherocto/asset/v1/"` || `role_token.as_bytes()`); 9-role-token enumeration per `ROLE_TOKENS` at `determin/src/asset_id.rs:54-64` (OCTO-A/B/D/M/N/O/S/H/W); substrate `AssetId(pub [u8; 32])` at `octo-vault/src/lib.rs:136`.

## 1. Motivation

The numeric parent RFC (`rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md`) §Asset ID Derivation defines the 9-role-token `ROLE_TOKENS` enumeration (OCTO-A/B/D/M/N/O/S/H/W, sovereign `OCTO` excluded per cross-layer capability-attestation path) and the canonical `asset_id_for` BLAKE3-256 derivation. The vault monetary representation redesign introduces **two asset namespaces**:

1. **Sovereign assets** — `OCTO-*` role tokens + their derivatives; managed by octo treasury; chain-independent.
2. **Private assets** — corporate-chain-specific assets; managed by corporate chain operators; chain-bound via `chain_id` segment in the role-token string.

Prior numeric parent (`v2.3`, 2026-08-19) covered only the 9 sovereign `OCTO-*` tokens. v3.0/v3.1 makes the sovereign/private boundary explicit and pins the private-asset derivation path on top of the existing substrate.

## 2. Asset Namespace Specification

### 2.1 Sovereign namespace

| Asset                                                 | Derivation                         |
| ----------------------------------------------------- | ---------------------------------- |
| `OCTO-A`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-A")[:32]`                     |
| `OCTO-B`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-B")[:32]`                     |
| `OCTO-D`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-D")[:32]`                     |
| `OCTO-M`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-M")[:32]`                     |
| `OCTO-N`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-N")[:32]`                     |
| `OCTO-O`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-O")[:32]`                     |
| `OCTO-S`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-S")[:32]`                     |
| `OCTO-H`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-H")[:32]`                     |
| `OCTO-W`                                              | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-W")[:32]`                     |
| `OCTO-PrivateProvider-Stake` (illustrative extension) | `BLAKE3-256("cipherocto/asset/v1/" |     | "OCTO-PrivateProvider-Stake")[:32]` |

**Substrate form:** substrate `asset_id_for(role_token: &str) -> [u8; 32]` (per `determin/src/asset_id.rs:85-93`) accepts the 9 sovereign `ROLE_TOKENS` strings verbatim. BLAKE3 input is the **2-component** concatenation `b"cipherocto/asset/v1/"` (constant `ASSET_ID_DOMAIN_V1`) || `role_token.as_bytes()` — no separator is added at the call site beyond the trailing `/` baked into the prefix. The output is the **full 32-byte BLAKE3-256 digest** (no truncation; no byte-0 namespace overwrite).

**Authority:** octo treasury only. Canonical asset_id derivation home is RFC-0105 §Asset ID Derivation (numeric parent).

### 2.2 Private namespace

| Asset                                     | Derivation                         |
| ----------------------------------------- | ---------------------------------- |
| `PRIVATE-{chain_id_32B-hex}-{asset_name}` | `BLAKE3-256("cipherocto/asset/v1/" |     | "PRIVATE-{chain_id_32B-hex}-{asset_name}")[:32]` |

**Substrate form:** substrate `asset_id_for(role_token: &str) -> [u8; 32]` is reused with the private-asset role-token string `PRIVATE-{chain_id_32B-hex}-{asset_name}` passed as the `role_token` argument. The substrate has no separate private-asset code path; the namespace distinction lives in the role-token string prefix. BLAKE3 input is the same **2-component** form as §2.1; output is the full 32-byte BLAKE3-256 digest.

**Cross-RFC note:** RFC-0206 §2.3 references `asset_id_16` as `BLAKE3("cipherocto/asset/v1/" || role_token)[0..16]` (16-byte UUIDv5 truncation form). That is a separate size convention in the `vault_id` derivation freeze; this RFC (and the substrate) use the full 32-byte form. Cross-RFC drift on asset_id sizing is tracked separately; substrate is the source of truth for this RFC.

**Authority:** corporate chain operator (per RFC-0010 §4 Authority Registration Flow). Multiple private assets per chain supported; one chain's `PRIVATE-XYZ` is not portable to another chain's namespace (the `chain_id_32B-hex` segment binds the asset to its origin chain).

### 2.3 Reserved namespaces

`0x03 - 0xFF` are reserved for future RFC allocation. Per RFC-0010 §2 reserved-namespace CHECK comment (the `0x03-0xFF Reserved` range is annotated in the `chain_namespace` CHECK constraint, not in the §3 BLAKE3 derivation).

## 3. Authority-to-Issue Table

| Namespace             | Authority DID                | Registration path                                                                                                                                                                 |
| --------------------- | ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Sovereign (`OCTO-*`)  | octo treasury DID            | `policy_kind_authority` row registered by octo treasury (RFC-defined substrate-pending per RFC-0967-A1 §2.4 + RFC-0206 v017 migration; pending landing via mission 0206-001 v3.0) |
| Private (`PRIVATE-*`) | corporate chain operator DID | `policy_kind_authority` row registered by chain operator at chain registration (RFC-defined substrate-pending; same migration pending)                                            |

## 4. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                         | Class | Justification                       |
| ------------------------------- | ----- | ----------------------------------- |
| `asset_id_for(OCTO-*)`          | A     | Deterministic BLAKE3-256 derivation |
| `asset_id_for(PRIVATE-*)`       | A     | Deterministic BLAKE3-256 derivation |
| Sovereign asset registration    | A     | octo treasury signing               |
| Private asset registration      | A     | Corporate chain operator signing    |
| Asset namespace collision check | A     | BLAKE3-256 full-32-byte uniqueness  |

## 5. Cross-References

- RFC-0010 §2 reserved-namespace CHECK comment + §4 Authority Registration Flow
- RFC-0105 §Asset ID Derivation (canonical asset_id derivation in numeric parent RFC)
- RFC-0967-A1 §2.4 + §2.5 (policy_kind_authority registration; RFC-defined substrate-pending)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §3 D1 + §9 amendment table

## 6. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 3.1     | 2026-08-23 | **R3 fix-all (post-R16 promotion cascade):** substrate-grounded re-write of §1 (9-token enum per `ROLE_TOKENS`, sovereign/private narrative) + §2.1 (32-byte BLAKE3-256, 2-component input, 9-token table, removed phantom `AssetNamespace` enum + namespace-byte overwrite semantics) + §2.2 (private-namespace derivation re-anchored on substrate `asset_id_for`, no separate enum, cross-RFC size-drift note for RFC-0206 §2.3 `asset_id_16` 16-byte form); frontmatter `builds_on` redirected to `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md` (was non-existent economics/ path); research doc v2.0 pin stripped to bare path per CLAUDE.md §RFC Reference Conventions; phantom fix IDs `F-R12-LENS-CROSS-CONSISTENCY-003` stripped (no on-disk source); phantom "RFC-0105 v2.3" body cites replaced with bare RFC-0105 / numeric parent references; "RFC-0206 §2.3 canonical asset_id derivation" cite redirected (RFC-0206 §2.3 is `vault_id` BLAKE3 freeze, not asset_id derivation); "RFC-0010 §3 reserved-namespace range" cite redirected to RFC-0010 §2 (CHECK comment); "RFC-0967-A1 §Kind UUID Registry (private asset types)" mis-cite removed (§2.6 is policy-kind UUIDs, not private asset types); VH table rows consolidated to single v3.1 row. Two-namespace model + Authority-to-Issue table + Execution Class Mapping preserved. |
