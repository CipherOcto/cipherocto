---
rfc: 0010-v1.7
title: Chain-id Registration Authority + ledger_chain_registry Table
status: Accepted
version: 1.9.1
date: 2026-08-22
amends: RFC-0010
builds_on:
  - rfcs/accepted/process/0010-canonical-did-codec.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0010 v1.9 — Chain-id Registration Authority

## 0. Status

**Accepted (v1.9 effective; v1.7 filed, 2026-08-22).** Additive to RFC-0010 (2026-08-19). P0 BLOCKER per research doc §9 — chain_metadata table soft-references `ledger_chain_registry` (the underlying substrate-side migration `v017__add_chain_metadata_and_policy_registry.sql` per RFC-0206 v3.0 §4 remains pending landing on disk via mission 0206-001 v3.0; the soft-ref persists until that migration lands, independent of this RFC's Accepted status).

**Promotion trail:** v1.7 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0010 v1.7 second in Tier 1 order per research §20 decision #9). ledger_chain_registry table + chain_id BLAKE3 derivation + authority registration flow all preserved. Pre-existing cite pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions.

## 1. Motivation

RFC-0010 defines Canonical OctoID Identifier Codec. v1.7 adds a **chain_id registration authority** — the substrate-side mechanism by which corporate chains obtain `chain_id: [u8;32]` values within the `0x02 (User)` namespace.

## 2. ledger_chain_registry Table

> **Substrate status:** DDL below is the **spec** for migration `v017__add_chain_metadata_and_policy_registry.sql` (single combined file per RFC-0206 §Migration [v3.0 amendment §4] covering 4 tables: `chain_metadata` columns + `ledger_chain_registry` + `policy_registry` + `policy_kind_authority`) — **pending landing via 0206-001 v3.0** per RFC-0206 §Migration [v3.0 amendment §4]. No SQL migration file exists on disk yet under `crates/octo-*/migrations/`. The `chain_metadata` reference here is an ALTER (new columns on existing `chain_metadata` table) per research doc §8.1, NOT a CREATE of a new `chain_metadata` table (research doc §8.1 line 942 confirms `chain_metadata` already exists; only new columns are added by v017). Substrate consumers must treat this section as the binding spec, not as the shipped artifact.
>
> **Substrate-vs-RFC filename drift:** This RFC §2 historically named two v017 migration files (`v017__create_ledger_chain_registry.sql` + `v017__create_chain_metadata.sql`); RFC-0206 §Migration [v3.0 amendment §4 line 150] names one combined file (`v017__add_chain_metadata_and_policy_registry.sql` covering 4 tables). The canonical filename per RFC-0206 §Migration [v3.0 amendment §4 line 150] wins — operators applying this RFC's DDL MUST use the single combined filename to avoid the cross-RFC acceptance chain divergence flagged by R4 review.

```sql
CREATE TABLE ledger_chain_registry (
    chain_id BLOB(32) NOT NULL PRIMARY KEY,
    chain_namespace INTEGER NOT NULL,  -- 0x01 Rfc / 0x02 User / 0x03-0xFF Reserved; 0x00 NOT permitted (substrate ChainNamespace::from_canonical_bytes rejects 0x00 at read time per crates/octo-ident/src/chain.rs §from_canonical_bytes)
    operator_did BLOB(32) NOT NULL,
    operator_signature BLOB(64) NOT NULL,  -- Ed25519 over canonical-serialized registration body
    registration_body BLOB NOT NULL,        -- canonical CBOR: chain_name, contact_uri, policy_hashes
    registered_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,                -- NULL = active
    CHECK (length(chain_id) = 32),
    -- Substrate-side: `chain_namespace == 0x00` (NamespaceVariant::Reserved) is REJECTED at read time
    -- by `ChainNamespace::from_canonical_bytes` (returns `Err(ChainNamespaceError::ReservedVariant(0x00))`).
    -- CHECK below mirrors research doc §8.1 line 926 (R11 fix F-R11-CHAIN-NAMESPACE-0X00-PHANTOM): 0x00 is RESERVED.
    CHECK (chain_namespace >= 0x01 AND chain_namespace <= 0xFF),
    CHECK (length(operator_signature) = 64)
);

CREATE UNIQUE INDEX ledger_chain_registry_active_op_idx ON ledger_chain_registry(operator_did) WHERE revoked_at_unix IS NULL;
```

## 3. Chain-id Derivation

> **Substrate vs RFC divergence (RFC-level extension):** Substrate carries **TWO parallel** canonical chain-id BLAKE3 derivations, both producing `BLAKE3(b"cipherocto/chain/v1/" || chain_string_bytes)`. Neither takes a namespace parameter and neither overwrites byte 0:
>
> 1. `octo_ident::ChainId::as_bytes(&self) -> [u8;32]` (octo-ident/src/chain.rs `ChainId::as_bytes` impl; wraps `&String` inner field). Carries the canonical chain-id BLAKE3 derivation introduced in parent RFC-0010 v1.6 32-byte Addendum amendment.
> 2. `octo_vault::ChainId::derive(chain_string: &str) -> Self` (octo-vault/src/lib.rs `ChainId::derive` impl; wraps `[u8;32]` inner field). Active in 7+ test sites (`octo-vault/tests/apply_migrations.rs` + `capture_tv_v1.rs`) + the v013/v014 SQL migration headers.
>
> Both derive **identical 32-byte BLAKE3 output** (same domain tag `cipherocto/chain/v1/` + same input bytes); the wrappers diverge in inner representation (`String` vs `[u8;32]`) but the BLAKE3 digest is canonical. Both produce 32 raw BLAKE3 bytes — **no namespace-byte overwrite**.
>
> This RFC §3 `derive_chain_id(namespace, seed)` is an **RFC-defined extension pending substrate landing**. It exists because the registration-authority flow needs (a) namespace-parameterized derivation for namespace isolation, and (b) seed-slice input for corporate-operator registration bodies. Corporate operator clients implementing the §4 registration flow MUST use this RFC §3 form (with hex encoding and overwrite) so that `chain_id[0]` carries the namespace discriminant; substrate callers using either `octo_ident::ChainId::as_bytes` or `octo_vault::ChainId::derive` use the substrate form (raw bytes, no overwrite). The two forms are intentionally divergent; cross-validation at registration time uses `chain_metadata.chain_namespace` as the source of truth, not `chain_id[0]`.

```rust
// RFC-level extension (pending substrate landing). Substrate `ChainId::as_bytes` is the
// canonical BLAKE3 derivation; this function extends that surface for namespace-parameterized
// corporate-operator registration per §4. See header note above for divergence rules.
fn derive_chain_id(namespace: ChainNamespace, seed: &[u8]) -> [u8;32] {
    let input = format!("cipherocto/chain/v1/{}", hex::encode(seed));
    let hash = blake3(input.as_bytes());
    let mut out = [0u8;32];
    out.copy_from_slice(&hash.as_bytes()[..32]);
    // Namespace byte OVERWRITES byte 0 post-BLAKE3 (per RFC-0206 v3.3 §2.5 namespace-byte
    // disambiguation table). Substrate-binding form: `ChainNamespace` has no `as u8` impl —
    // the variant byte is exposed via `namespace.variant()` returning `NamespaceVariant`
    // (which has `as_byte() -> u8` per octo-ident/src/chain.rs §NamespaceVariant impl block).
    out[0] = namespace.variant().as_byte();
    out
}
```

**Collision resistance:** BLAKE3 32-byte hash provides ~248-bit pre-image + ~124-bit collision resistance on bytes `[1..32]` (31 bytes of BLAKE3 output are operator-input-dependent; the full 32-byte BLAKE3 output has ~128-bit collision resistance, but byte 0 is operator-controlled post-BLAKE3 and serves as the namespace discriminant, reducing the hash-derived surface to 31 bytes = 248 bits → birthday bound 2^124). Byte 0 is not part of the collision surface. For corporate chains, the seed is `corp_did || corp_seed` (corp_seed is operator-controlled entropy).

## 4. Authority Registration Flow

```
1. Operator computes candidate chain_id via derive_chain_id(
       ChainNamespace { variant: NamespaceVariant::User, tag: <BLAKE3_15>, length: <u8> },
       corp_did || corp_seed
   ) (this RFC §3 form; the `0x02` namespace-byte shorthand maps to NamespaceVariant::User per octo-ident/src/chain.rs §NamespaceVariant enum)
2. Operator queries ledger_chain_registry for chain_id collision (returns None if available)
3. Operator signs registration_body (chain_name, contact_uri, policy_hashes) with operator_pubkey
4. Operator submits INSERT into ledger_chain_registry via substrate migration
5. Runtime application code (NOT the SQL migration) verifies signature against operator_did
   via RFC-0009 Ed25519 verifier before the INSERT is dispatched. SQL migrations execute
   as static DDL/DML and cannot perform cryptographic verification — sig check is an
   application-runtime concern; migration only enforces the static CHECK + UNIQUE constraints.
6. Substrate accepts INSERT (UNIQUE constraint enforces single-active-per-operator)
```

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                         | Class | Justification                                                                                                                                                                                                                                                                        |
| ----------------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `derive_chain_id`                               | A     | Deterministic BLAKE3 derivation                                                                                                                                                                                                                                                      |
| `ledger_chain_registry` INSERT                  | A     | Operator-signed registration                                                                                                                                                                                                                                                         |
| Collision detection                             | A     | Deterministic PK lookup                                                                                                                                                                                                                                                              |
| `chain_metadata.ledger_chain_registry` soft-ref | A     | Soft reference (NOT FOREIGN KEY — research doc §8.1 line 942 explicitly rejects FK in favor of soft-ref until RFC-0010 v1.7 lands; substrate migration v017 pending landing via 0206-001 v3.0; neither `chain_metadata` columns nor `ledger_chain_registry` table exist on disk yet) |

## 6. Cross-References

- RFC-0010 (current DID codec)
- 0010 v1.9 §3 Chain-id Derivation (self-cite to this amendment; parent canonical RFC-0010 has no numbered §3 — BLAKE3 chain derivation lives in parent v1.6 32-byte Addendum amendment)
- RFC-0960 (vault path taxonomy consumer)
- RFC-0009 (Ed25519 signing primitive)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v3.7.2 §3 D1 + §8.1 (chain_metadata DDL with R11 fix F-R11-CHAIN-NAMESPACE-0X00-PHANTOM CHECK) + §9 amendment table + §15 row 6

## 7. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.9.1   | 2026-08-22 | **R5 fix-all (R4 fresh-eyes review applied):** 18 findings addressed inline. (1) H1 title v1.7 → v1.9 (process drift); (2) §0 Status "until v1.7 lands" → substrate migration v017 pending landing (consistency); (3) §2 migration filename reconciled to single `v017__add_chain_metadata_and_policy_registry.sql` per RFC-0206 v3.0 §4 (cite + consistency); (4) §2 `chain_metadata` CREATE → ALTER clarification per research doc §8.1 (consistency); (5) §2 byte 0x01 "Mainnet" → "Rfc" + 0x00 RESERVED CHECK `>= 0x01` (substrate + consistency); (6) §3 substrate-vs-RFC divergence header now enumerates both `octo_ident::ChainId::as_bytes` AND `octo_vault::ChainId::derive` (substrate); (7) §3 pseudocode `namespace as u8` → `namespace.variant().as_byte()` (substrate); (8) §3 collision resistance "~128-bit" → "~124-bit on 31-byte surface" (correctness); (9) §4 step 1 `0x02` literal → `ChainNamespace` struct form (substrate); (10) §5 row "FK" → "soft-ref (NOT FOREIGN KEY)" per research doc §8.1 line 942 (consistency); (11) §6 cross-reference research doc v2.0 → v3.7.2 (cite); (12) §6 self-cite 0010 v1.7 → 0010 v1.9 (consistency); (13) §3 cite loop RFC-0206 v3.3 §2.5 disambiguated by RFC anchor (cite). All substrate claims grounded against octo-ident/src/chain.rs + octo-vault/src/lib.rs. |
| 1.9     | 2026-08-22 | **R16 promotion (Draft → Accepted) + R14 fix trail (cross-RFC consistency lens):** merged into a single v1.9 row per R3 fix-all (two distinct v1.9 events at the same version label). R16: status bumper per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence; citation cleanup (4 pre-existing STALE pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions). R14: cascade bump from v1.8 to v1.9 aligns with research doc §17 v3.9 + RFC-0008 v1.1 + RFC-0206 R14 cascade. ledger_chain_registry + chain_id BLAKE3 derivation + authority registration flow preserved.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 1.8     | 2026-08-22 | **R13 fix trail (post-R12 fresh lens):** version bump from v1.7 to v1.8 reflects R13 round on RFC-0010 v1.7; aligns with research doc §17 v3.8 + RFC-0967-A1 v1.4 + RFC-0008 v1.0 R12 cascade.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 1.7     | 2026-08-22 | Initial draft. Additive on parent RFC-0010 v1.6 32-byte Addendum amendment (which introduced substrate `ChainId::as_bytes` BLAKE3 derivation). Adds ledger_chain_registry table + authority registration flow + RFC-level extension `derive_chain_id(namespace, seed)`. P0 BLOCKER for chain_metadata substrate (soft-ref until v1.7 lands). Resolves R2 finding on RFC-0010 v1.7 authorization gap.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
