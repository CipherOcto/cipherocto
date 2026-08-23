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

```sql
CREATE TABLE ledger_chain_registry (
    chain_id BLOB(32) NOT NULL PRIMARY KEY,
    chain_namespace INTEGER NOT NULL,  -- 0x01 Mainnet / 0x02 User / 0x03-0xFF Reserved
    operator_did BLOB(32) NOT NULL,
    operator_signature BLOB(64) NOT NULL,  -- Ed25519 over canonical-serialized registration body
    registration_body BLOB NOT NULL,        -- canonical CBOR: chain_name, contact_uri, policy_hashes
    registered_at_unix INTEGER NOT NULL,
    revoked_at_unix INTEGER,                -- NULL = active
    CHECK (length(chain_id) = 32),
    CHECK (chain_namespace >= 0x01 AND chain_namespace <= 0xFF),
    CHECK (length(operator_signature) = 64)
);

CREATE UNIQUE INDEX ledger_chain_registry_active_op_idx ON ledger_chain_registry(operator_did) WHERE revoked_at_unix IS NULL;
```

## 3. Chain-id Derivation

```rust
fn derive_chain_id(namespace: ChainNamespace, seed: &[u8]) -> [u8;32] {
    let input = format!("cipherocto/chain/v1/{}", hex::encode(seed));
    let hash = blake3(input.as_bytes());
    let mut out = [0u8;32];
    out.copy_from_slice(&hash.as_bytes()[..32]);
    // [R13 fix F-R12-LENS-CROSS-CONSISTENCY-005]: the BLAKE3 input is the hex-encoded form of `seed` per RFC-0010 §3 above.
    // RFC-0206 cites `BLAKE3("cipherocto/chain/v1/" || chain_string)` where `chain_string` is this hex-encoded form —
    // NOT raw seed bytes. Consumers must hex-encode seed bytes before BLAKE3 input.
    // [R14 fix R12-XR-HISTORICAL-PREFIX-ANNOTATION: this §3 hex::encode(seed) form is the v1.7 FINAL canonical
    // surface. v1.6 §3 used `BLAKE3("cipherocto/chain/v1/" || chain_string)` without explicit hex::encode annotation
    // — v1.7 §3 added the explicit annotation, so callers MUST hex-encode seed bytes before
    // BLAKE3 input. v1.6 form is HISTORICAL and superseded.]
    // Namespace byte OVERWRITES byte 0 post-BLAKE3 (per RFC-0206 §2.5 disambiguation)
    out[0] = namespace as u8;
    out
}
```

**Collision resistance:** BLAKE3 32-byte hash provides ~128-bit collision resistance. For corporate chains, the seed is `corp_did || corp_seed` (corp_seed is operator-controlled entropy).

## 4. Authority Registration Flow

```
1. Operator computes candidate chain_id via derive_chain_id(0x02, corp_did || corp_seed)
2. Operator queries ledger_chain_registry for chain_id collision (returns None if available)
3. Operator signs registration_body (chain_name, contact_uri, policy_hashes) with operator_pubkey
4. Operator submits INSERT into ledger_chain_registry via substrate migration
5. Substrate verifies signature against operator_did
6. Substrate accepts INSERT (UNIQUE constraint enforces single-active-per-operator)
```

## 5. Execution Class Mapping (RFC-0008 §RFC-0008 Execution Class Mapping)

| Surface                                   | Class | Justification                   |
| ----------------------------------------- | ----- | ------------------------------- |
| `derive_chain_id`                         | A     | Deterministic BLAKE3 derivation |
| `ledger_chain_registry` INSERT            | A     | Operator-signed registration    |
| Collision detection                       | A     | Deterministic PK lookup         |
| `chain_metadata.ledger_chain_registry` FK | A     | Soft-ref until v1.7 lands       |

## 6. Cross-References

- RFC-0010 (current DID codec)
- RFC-0010 §3 Chain-id Derivation (chain_namespace byte)
- RFC-0960 v3.1 (vault path taxonomy consumer)
- RFC-0009 (Ed25519 signing primitive)
- `docs/research/2026-08-21-vault-monetary-representation-redesign.md` v2.0 §3 D1 + §8.1 + §9 amendment table + §15 row 5

## 7. Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                            |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.9     | 2026-08-22 | **R14 fix trail (cross-RFC consistency lens):** §v1.9 row itself — version bump from v1.8 to v1.9 reflects R14 round on RFC-0010 v1.7; aligns with research doc §17 v3.9 + RFC-0008 v1.1 + RFC-0206 R14 cascade.                                                                                                                  |
| 1.8     | 2026-08-22 | **R13 fix trail (post-R12 fresh lens):** §v1.8 row itself — version bump from v1.7 to v1.8 reflects R13 round on RFC-0010 v1.7; aligns with research doc §17 v3.8 + RFC-0967-A1 v1.4 + RFC-0008 v1.0 R12 cascade.                                                                                                                 |
| 1.7     | 2026-08-22 | Initial draft. Additive to v1.6. ledger_chain_registry table + chain_id BLAKE3 derivation + authority registration flow. P0 BLOCKER for chain_metadata substrate (soft-ref until v1.7 lands). Resolves R2 finding on RFC-0010 v1.7 authorization gap.                                                                             |
| 1.9     | 2026-08-22 | **R16 promotion:** Draft → Accepted per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence. Status bumper + citation cleanup (4 pre-existing STALE pins stripped to bare RFC numbers per CLAUDE.md §RFC Reference Conventions). ledger_chain_registry + chain_id BLAKE3 derivation + authority registration flow preserved. |
