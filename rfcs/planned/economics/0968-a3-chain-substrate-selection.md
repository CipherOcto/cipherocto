---
rfc: 0968-A3
title: Chain-Substrate Selection Protocol
status: Draft
version: 0.1.0
date: 2026-08-24
authors:
  - cipherocto-claim-and-implement-plan
maintainers:
  - cipherocto-core
depends_on:
  - RFC-0010
  - RFC-0968
  - RFC-0968-A2
---

# Chain-Substrate Selection Protocol (RFC-0968-A3)

## Status

**Draft v0.1.0** — initial stub per claim-and-implement plan v1.0 Session 5 deferred-work unblocking. Closes P0 BLOCKER for `chain-substrate-selection` per research doc §16 §External Blockers + §22 B0 atomic-blocker (storage restructure plan S6).

Mission `0968-a3-chain-substrate-selection` (Session 5 deferred per F-P5.2-3 RETAIN → implemented per claim-and-implement scope inversion).

## Summary

RFC-0968-A3 defines the protocol by which a participant selects the substrate (consensus engine, ledger, validator set) for a new chain registered via RFC-0010 §2 `ledger_chain_registry`. The selection is a deterministic function of (chain_id, registered_at_unix, operator_did), binding the chain to one of three canonical substrate classes:

| Class | Substrate | Codepoint | Validator Set |
|---|---|---|---|
| **CIPHEROCTO_MAINNET** | Stoolap fork (`feat/blockchain-sql`) | 0x01 | RFC-0008 §Roles and Authorities (24 signers) |
| **CIPHEROCTO_TESTNET** | Stoolap fork (`feat/blockchain-sql`) | 0x02 | RFC-0008 §Roles and Authorities (24 signers, permissioned) |
| **CORPORATE_SIDECHAIN** | Stoolap fork (corporate deploy) | 0x03 | Operator-controlled (variable threshold) |

The selection is **deterministic + non-forkable** — given the same inputs, any participant computes the same class without out-of-band coordination.

## §1 Selection Algorithm

```
fn select_substrate(chain_id: &[u8; 32], registered_at_unix: i64, operator_did: &[u8; 32]) -> SubstrateClass {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"octo/chain-substrate-selection/v1/");
    hasher.update(chain_id);
    hasher.update(&registered_at_unix.to_be_bytes());
    hasher.update(operator_did);
    let digest = hasher.finalize();
    let n = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    match n % 1000 {
        0..=899 => SubstrateClass::CIPHEROCTO_MAINNET,
        900..=999 => SubstrateClass::CIPHEROCTO_TESTNET,
        _ => unreachable!(),
    }
}
```

**Weighted selection**: 90% mainnet, 10% testnet. Corporate sidechains require explicit operator opt-in (not random).

## §2 Substrate Codepoints

Codepoints are the 1-byte discriminator written into `ledger_chain_registry.chain_namespace` (per RFC-0010 §2):

| Codepoint | Class | Notes |
|---|---|---|
| 0x01 | CIPHEROCTO_MAINNET | Production traffic |
| 0x02 | CIPHEROCTO_TESTNET | Testnet traffic (separate validator keyspace) |
| 0x03 | CORPORATE_SIDECHAIN | Operator-controlled (RFC-0010 §ChainId Namespace Extension) |

The Stoolap fork substrate enforces 0x01 / 0x02 / 0x03 only (rejects 0x00, 0x04-0xFF) per migration v017 §1 CHECK constraint.

## §3 Operator Override — Corporate Sidechain

Operators may explicitly opt into CORPORATE_SIDECHAIN by setting `chain_namespace = 0x03` in the registration body. The override:

1. Requires `operator_did ∈ CORPORATE_OPERATOR_SET` (RFC-0008 §Roles and Authorities maintains a 24-signer allowlist; CORPORATE_OPERATOR_SET is a separate allowlist managed by the chain governance committee).
2. Records the override in `ledger_chain_registry.registration_body` (CBOR-encoded `{ "explicit_substrate": "CORPORATE_SIDECHAIN", "operator_attestation": <signature> }`).
3. Skips the deterministic selection algorithm — corporate sidechains are not randomly distributed.

## §4 Cross-References

- RFC-0010 §2 (ledger_chain_registry schema — namespace codepoint)
- RFC-0968 §13 (chain namespace discriminant table — parent amendment source)
- RFC-0968-A2 §3 (parent §13 vs substrate drift — sibling codepoint correction)
- RFC-0008 §Roles and Authorities (24-signer validator set)
- Research doc §16 + §22 B0 (selection algorithm source)

## §5 Lifecycle Requirements

- On Accept: migrate `crates/octo-chain/src/substrate_selection.rs` from `TODO()` to the selection algorithm per §1 (priority: blocking for S6 §22 B0 atomic-blocker per storage restructure plan).
- On Accept: add 12 TV to `crates/octo-chain/tests/substrate_selection.rs` covering: 90/10 split determinism, operator override, codepoint rejection.
- On Accept: amend RFC-0010 v1.9.2 §2 to reference this RFC for the chain_namespace binding.

## §6 Version History

| Version | Date | Change |
|---|---|---|
| 0.1.0 | 2026-08-24 | Initial stub. Selection algorithm (§1) + codepoint table (§2) + operator override (§3) + cross-refs (§4). 12 TV target deferred to accept-cycle per §5. |