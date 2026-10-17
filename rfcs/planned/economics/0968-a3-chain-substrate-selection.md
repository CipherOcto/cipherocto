---
rfc: 0968-A3
title: Chain-Substrate Selection Protocol
status: Draft
version: 0.2.0
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

**Draft v0.2.0** — R2 review fixed codepoint collision with substrate-truth `ChainNamespace {Rfc, User}`. Initial stub per claim-and-implement plan v1.0 Session 5 deferred-work unblocking. Closes P0 BLOCKER for `chain-substrate-selection` per research doc §16 §External Blockers + §22 B0 atomic-blocker (storage restructure plan S6).

Mission `0968-a3-chain-substrate-selection` (Session 5 deferred per F-P5.2-3 RETAIN → implemented per claim-and-implement scope inversion).

## Summary

RFC-0968-A3 defines the protocol by which a participant selects the substrate (consensus engine, ledger, validator set) for a new chain registered via RFC-0010 §2 `ledger_chain_registry`. The selection binds the chain to a `ChainNamespace` codepoint matching substrate-truth per `octo-policy::policy_kinds::ChainNamespace { Rfc(0x01), User(0x02) }`. There are exactly two substrate-acceptable codepoints; corporate sidechains are expressed via `validator_set_kind` (separate field, TBD per §6 follow-on RFC), NOT a third namespace byte.

| Class                                              | Substrate                            | Codepoint                     | Validator Set                                              |
| -------------------------------------------------- | ------------------------------------ | ----------------------------- | ---------------------------------------------------------- |
| **RFC_NAMESPACE** (canonical: CIPHEROCTO_MAINNET)  | Stoolap fork (`feat/blockchain-sql`) | 0x01 (`ChainNamespace::Rfc`)  | RFC-0008 §Roles and Authorities (24 signers)               |
| **USER_NAMESPACE** (canonical: CIPHEROCTO_TESTNET) | Stoolap fork (`feat/blockchain-sql`) | 0x02 (`ChainNamespace::User`) | RFC-0008 §Roles and Authorities (24 signers, permissioned) |

The 1-byte discriminator written into `ledger_chain_registry.chain_namespace` accepts exactly `{0x01, 0x02}` per migration v017 §1 CHECK constraint (`chain_namespace IN ('01', '02')`). Substrate rejects 0x00, 0x03, and 0x04-0xFF with a CHECK violation.

## §1 Registrant-Selected Substrate

Unlike RFC-0968-A3 v0.1.0 (which proposed deterministic hashing with `n % 1000` weighted 90/10 split), the substrate-acceptable design lets the **registrant choose the `chain_namespace` codepoint explicitly** in the registration body. Three justifications:

1. **Substrate truth**: there are exactly two codepoints (`Rfc` / `User`); a 90/10 split is meaningless against two values.
2. **No phantom randomness**: deterministic hashing would produce apparent uniformity but no participant can predict the outcome without running the hash; explicit selection is more auditable.
3. **Validator-set binding is separate**: the chain's validator set is determined by `validator_set_kind` (a separate registration-body field per §3), NOT by the namespace byte.

The selection is therefore **explicit + non-forkable** — the registrant writes the codepoint directly into the registration body, signed by `operator_signature`.

## §2 Substrate Codepoints

Codepoints are the 1-byte discriminator written into `ledger_chain_registry.chain_namespace` (per RFC-0010 §2):

| Codepoint | Class (canonical)                   | Substrate truth        | Notes                                                               |
| --------- | ----------------------------------- | ---------------------- | ------------------------------------------------------------------- |
| 0x01      | RFC_NAMESPACE (CIPHEROCTO_MAINNET)  | `ChainNamespace::Rfc`  | RFC-allocated namespace (RFC-0968-A3 canonical: production traffic) |
| 0x02      | USER_NAMESPACE (CIPHEROCTO_TESTNET) | `ChainNamespace::User` | Corporate/private chain (RFC-0968-A3 canonical: testnet traffic)    |

The Stoolap fork substrate enforces 0x01 / 0x02 only (rejects 0x00, 0x03, 0x04-0xFF) per migration v017 §1 CHECK constraint (`CHECK (CAST(chain_namespace AS TEXT) IN ('01', '02'))`).

**Removed in v0.2.0**: `0x03 CORPORATE_SIDECHAIN` codepoint. Corporate sidechains are now expressed via `validator_set_kind = CorporateOperatorSet` on the `User` (0x02) namespace, NOT a third reserved codepoint. The substrate CHECK forbids 0x03; the previous v0.1.0 codepoint assignment would have failed INSERT at the application layer.

## §3 Validator-Set-Kind Override — Corporate Sidechain

Operators may deploy a corporate sidechain by registering a chain with `chain_namespace = USER_NAMESPACE (0x02)` plus `validator_set_kind = CorporateOperatorSet` (TBD per §6 follow-on RFC). The override:

1. Requires `operator_did ∈ CORPORATE_OPERATOR_SET` (RFC-0008 §Roles and Authorities maintains a 24-signer allowlist; CORPORATE_OPERATOR_SET is a separate allowlist managed by the chain governance committee).
2. Records the override in `ledger_chain_registry.registration_body` (CBOR-encoded `{ "validator_set_kind": "CorporateOperatorSet", "operator_attestation": <signature> }`).
3. The chain is substrate-valid (`chain_namespace = 0x02` passes the CHECK constraint) but governance-distinct from a plain User-namespace chain.

**Removed in v0.2.0**: §3 v0.1.0 claimed `chain_namespace = 0x03` is substrate-acceptable. Per migration v017 §1 CHECK constraint, `0x03` is rejected at INSERT time. The validator-set override has been migrated to `validator_set_kind` (a separate registration-body field) so corporate sidechains remain expressible within the substrate-acceptable codepoint space.

## §4 Cross-References

- RFC-0010 §2 (ledger_chain_registry schema — namespace codepoint)
- RFC-0968 §13 (chain namespace discriminant table — parent amendment source)
- RFC-0968-A2 §3 (parent §13 vs substrate drift — sibling codepoint correction)
- RFC-0008 §Roles and Authorities (24-signer validator set)
- Research doc §16 + §22 B0 (selection algorithm source)
- `crates/octo-policy/src/policy_kinds.rs` (substrate-truth `ChainNamespace { Rfc, User }`)
- `crates/quota-router-storage/migrations/v017__add_chain_metadata_and_policy_registry.sql` (CHECK constraint)

## §5 Lifecycle Requirements

- On Accept: migrate `crates/octo-chain/src/substrate_selection.rs` from `TODO()` to the registrant-selected codepoint path (priority: blocking for S6 §22 B0 atomic-blocker per storage restructure plan).
- On Accept: add 12 TV to `crates/octo-chain/tests/substrate_selection.rs` covering: explicit-codepoint round-trip, 0x03 rejection (substrate CHECK), 0x00 / 0x04-0xFF rejection, `validator_set_kind` round-trip.
- On Accept: amend RFC-0010 v1.9.2 §2 to reference this RFC for the chain_namespace binding.
- On Accept: open follow-on RFC for `validator_set_kind` field shape (TBD per §6).

## §6 Follow-on RFCs

- `validator_set_kind` field shape (TBD): separate registration-body field for distinguishing `Rfc24SignerSet` / `UserPermissionedSet` / `CorporateOperatorSet`. Schema not yet defined; out of scope for RFC-0968-A3.

## §7 Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.1.0   | 2026-08-24 | Initial stub. Selection algorithm (§1) + codepoint table (§2) + operator override (§3) + cross-refs (§4). 12 TV target deferred to accept-cycle per §5.                                                                                                                                                                                                                                                             |
| 0.2.0   | 2026-08-24 | R2 review fixed codepoint collision; substrate-truth ChainNamespace {Rfc, User} only; removed 0x03 CORPORATE_SIDECHAIN per substrate CHECK constraint per RFC-0206 §Substrate Validation Table. Replaced v0.1.0 §1 deterministic selection algorithm with explicit registrant selection (no hashing). Migrated v0.1.0 §3 corporate-sidechain codepoint override to `validator_set_kind` field per §6 follow-on RFC. |
