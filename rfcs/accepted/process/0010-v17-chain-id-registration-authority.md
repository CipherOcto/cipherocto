---
rfc: 0010-v1.7
title: Chain-id Registration Authority + ledger_chain_registry Table
status: Accepted
version: 1.9
date: 2026-08-22
amends: RFC-0010
builds_on:
  - rfcs/accepted/process/0010-canonical-did-codec.md
  - docs/research/2026-08-21-vault-monetary-representation-redesign.md
---

# RFC-0010 v1.7 — Chain-id Registration Authority

## 0. Status

**Accepted (v1.9 effective; v1.7 filed, 2026-08-22).** Additive to RFC-0010 (2026-08-19). P0 BLOCKER per research doc §9 — chain_metadata table soft-references `ledger_chain_registry` until v1.7 lands.

**Promotion trail:** v1.7 initial draft 2026-08-22 → Accepted 2026-08-22 per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence (RFC-0010 v1.7 second in Tier 1 order per research §20 decision #9). ledger_chain_registry table + chain_id BLAKE3 derivation + authority registration flow all preserved. Pre-existing cite pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions.

## 1. Motivation

RFC-0010 defines Canonical OctoID Identifier Codec. v1.7 adds a **chain_id registration authority** — the substrate-side mechanism by which corporate chains obtain `chain_id: [u8;32]` values within the `0x02 (User)` namespace.

## 2. ledger_chain_registry Table

> **Substrate status:** DDL below is the **spec** for migration `v017__create_ledger_chain_registry.sql` (and companion `v017__create_chain_metadata.sql`) — **pending landing via 0206-001 v3.0** per RFC-0206 v3.0 §4 line 150 tagging. No SQL migration file exists on disk yet under `crates/octo-*/migrations/`. Substrate consumers must treat this section as the binding spec, not as the shipped artifact.

```sql
CREATE TABLE ledger_chain_registry (
    chain_id BLOB(32) NOT NULL PRIMARY KEY,
    chain_namespace INTEGER NOT NULL,  -- 0x00 Reserved / 0x01 Mainnet / 0x02 User / 0x03-0xFF Reserved
    operator_did BLOB(32) NOT NULL,
    operator_signature BLOB(64) NOT NULL,  -- Ed25519 over canonical-serialized registration body
    registration_body BLOB NOT NULL,        -- canonical CBOR: chain_name, contact_uri, policy_hashes
    registered_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,                -- NULL = active
    CHECK (length(chain_id) = 32),
    -- Loosened to allow 0x00 (substrate NamespaceVariant::Reserved) — see crates/octo-ident/src/chain.rs:311.
    -- 0x00 row reserved for backfill of legacy / future-amendment namespaces; not assignable to new operators.
    CHECK (chain_namespace >= 0x00 AND chain_namespace <= 0xFF),
    CHECK (length(operator_signature) = 64)
);

CREATE UNIQUE INDEX ledger_chain_registry_active_op_idx ON ledger_chain_registry(operator_did) WHERE revoked_at_unix IS NULL;
```

## 3. Chain-id Derivation

> **Substrate vs RFC divergence (RFC-level extension):** Substrate `ChainId::as_bytes(&self) -> [u8;32]` (octo-ident/src/chain.rs:137-142) is the canonical chain-id BLAKE3 derivation, introduced in parent RFC-0010 v1.6 32-byte Addendum amendment (per `0010-canonical-did-codec.md` v1.6 VH row). It takes no namespace parameter and no seed slice — it derives the 32-byte digest as `BLAKE3(b"cipherocto/chain/v1/" || self.0.as_bytes())` over the receiver's `String` field, with **no namespace-byte overwrite** (all 32 bytes are raw BLAKE3 output). The substrate form is used by the in-process identity substrate for canonical chain_id emission.
>
> This RFC §3 `derive_chain_id(namespace, seed)` is an **RFC-defined extension pending substrate landing**. It exists because the registration-authority flow needs (a) namespace-parameterized derivation for namespace isolation, and (b) seed-slice input for corporate-operator registration bodies. Corporate operator clients implementing the §4 registration flow MUST use this RFC §3 form (with hex encoding and overwrite) so that `chain_id[0]` carries the namespace discriminant; substrate callers using `ChainId::as_bytes` use the substrate form (raw bytes, no overwrite). The two forms are intentionally divergent; cross-validation at registration time uses `chain_metadata.chain_namespace` as the source of truth, not `chain_id[0]`.

```rust
// RFC-level extension (pending substrate landing). Substrate `ChainId::as_bytes` is the
// canonical BLAKE3 derivation; this function extends that surface for namespace-parameterized
// corporate-operator registration per §4. See header note above for divergence rules.
fn derive_chain_id(namespace: ChainNamespace, seed: &[u8]) -> [u8;32] {
    let input = format!("cipherocto/chain/v1/{}", hex::encode(seed));
    let hash = blake3(input.as_bytes());
    let mut out = [0u8;32];
    out.copy_from_slice(&hash.as_bytes()[..32]);
    // Namespace byte OVERWRITES byte 0 post-BLAKE3 (per RFC-0206 v3.3 §2.5)
    out[0] = namespace as u8;
    out
}
```

**Collision resistance:** BLAKE3 32-byte hash provides ~128-bit collision resistance on bytes `[1..32]`; byte 0 is operator-controlled post-BLAKE3 and serves as the namespace discriminant (not part of the collision surface). For corporate chains, the seed is `corp_did || corp_seed` (corp_seed is operator-controlled entropy).

## 4. Authority Registration Flow

```
1. Operator computes candidate chain_id via derive_chain_id(0x02, corp_did || corp_seed) (this RFC §3 form)
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

| Surface                                   | Class | Justification                                                                                                                        |
| ----------------------------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `derive_chain_id`                         | A     | Deterministic BLAKE3 derivation                                                                                                      |
| `ledger_chain_registry` INSERT            | A     | Operator-signed registration                                                                                                         |
| Collision detection                       | A     | Deterministic PK lookup                                                                                                              |
| `chain_metadata.ledger_chain_registry` FK | A     | Soft-ref pending substrate landing via 0206-001 v3.0 (neither `chain_metadata` nor `ledger_chain_registry` tables exist on disk yet) |

## 6. Cross-References

- RFC-0010 (current DID codec)
- 0010 v1.7 §3 Chain-id Derivation (self-cite to this amendment; parent canonical RFC-0010 has no numbered §3 — BLAKE3 chain derivation lives in parent v1.6 32-byte Addendum amendment)
- RFC-0960 (vault path taxonomy consumer)
- RFC-0009 (Ed25519 signing primitive)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0 §3 D1 + §8.1 + §9 amendment table + §15 row 6

## 7. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.9     | 2026-08-22 | **R16 promotion (Draft → Accepted) + R14 fix trail (cross-RFC consistency lens):** merged into a single v1.9 row per R3 fix-all (two distinct v1.9 events at the same version label). R16: status bumper per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence; citation cleanup (4 pre-existing STALE pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions). R14: cascade bump from v1.8 to v1.9 aligns with research doc §17 v3.9 + RFC-0008 v1.1 + RFC-0206 R14 cascade. ledger_chain_registry + chain_id BLAKE3 derivation + authority registration flow preserved. |
| 1.8     | 2026-08-22 | **R13 fix trail (post-R12 fresh lens):** version bump from v1.7 to v1.8 reflects R13 round on RFC-0010 v1.7; aligns with research doc §17 v3.8 + RFC-0967-A1 v1.4 + RFC-0008 v1.0 R12 cascade.                                                                                                                                                                                                                                                                                                                                                                                                       |
| 1.7     | 2026-08-22 | Initial draft. Additive on parent RFC-0010 v1.6 32-byte Addendum amendment (which introduced substrate `ChainId::as_bytes` BLAKE3 derivation). Adds ledger_chain_registry table + authority registration flow + RFC-level extension `derive_chain_id(namespace, seed)`. P0 BLOCKER for chain_metadata substrate (soft-ref until v1.7 lands). Resolves R2 finding on RFC-0010 v1.7 authorization gap.                                                                                                                                                                                                 |
