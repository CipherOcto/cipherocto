# Use Case: Canonical OctoID Identifier

**Date:** 2026-07-27
**Status:** Draft → RFC Draft (S3 of plan)
**Author:** @cipherocto + @mmacedoeu

## Problem

CipherOcto identity surfaces diverge across crates:

- `crates/octo-reputation/src/types.rs::RecorderDid` stores 52 raw bytes (no string form).
- `crates/octo-wallet/src/identity.rs::AudienceId` accepts any non-empty string (treated as opaque IKM).
- `crates/quota-router-core/src/marketplace/reputation_compat.rs::parse_canonical_did` strict-rejects any non-`did:octo:b<52>`.
- RFC-0009 §Identity Struct mandates `did:octo:z<base58btc of 32 bytes>` (W3C form).
- 347 test/fixture literals across 7+ crates use bare `:name` (e.g. `did:octo:buyer`).

Cross-mission reputation is impossible without a single form: a recorder rotating under one byte form cannot be matched under another. `docs/use-cases/reputation-persistence.md:19` flags "reputation laundering across noncanonical forms" as the critical security exposure. Reputation research `docs/research/2026-07-24-reputation-persistence-research.md` chose `did:octo:b<52>` (62 chars) to lock the storage form. RFC-0009 mandates `did:octo:z<...>` (W3C form, ~43-44 chars).

A consistent identity is missing. Without it, recorder signatures authenticate across trust boundaries but cannot be replayed across them; gossip topics drift; CLI tools display different byte forms.

## Stakeholders

- **Primary:** providers (LLM, retrieval, proof), coordinators, agents, DCs.
- **Secondary:** marketplace buyers, task market askers, gossip attestors, slash auditors.
- **Affected:** protocol operators; wallet authors; reputation maintenance tooling; CLI consumers.

## Motivation

Identity is the protocol's authorization substrate. Without persistence + canonicality:

- Reputation laundering via shape mismatch (already documented).
- Gossip topic `/dot/reputation/{recorder_did}` becomes ambiguous when the suffix drifts across crates.
- Wallet capability derivation key is bound to whatever `AudienceId::from_str` accepts — currently unconstrained.
- CLI display / log lines cannot be grepped without per-crate normalization.

## Success Metrics

| Metric                      | Target                                                                        | Measurement          |
| --------------------------- | ----------------------------------------------------------------------------- | -------------------- |
| Canonical wire form         | 100% of new code uses W3C `did:octo:z<base58btc>` for cross-mission path      | grep audit           |
| Legacy 52-byte storage form | Kept on reputation storage tables; codec translates to wire at every boundary | v001+ migration test |
| Single codec crate          | `crates/octo-ident/` exists with `<52-byte>` ↔ `<W3C wire>` transcoding       | cargo test           |
| Test fixture codemod        | 0 bare `:name` literals across `tests/` and `fixtures/` directories           | grep audit           |
| Wallet audience validation  | `AudienceId::from_str` accepts canonical form only (post deprecation window)  | unit test            |

## Constraints

- MUST NOT change reputation storage schema (v001+ migrations; 52-byte `RecorderDid` survives).
- MUST NOT revert RFC-0009 §Identity Struct (process-level acceptance).
- MUST be backward-compatible over a 6-month dual-parse window (both `z<base58btc>` and bare legacy are accepted during the transition).
- MUST be W3C DID Core 1.0 conformant on the wire (multibase prefix `z`).
- MUST NOT introduce a new error type at the parse boundary (`RecorderDidMalformed` and `InvalidAudienceId` are reused with new diagnostic messages).

## Non-Goals

- **Key rotation mechanism:** separate use case.
- **Key escrow / multi-sig / threshold sig:** future work.
- **Cross-chain DID resolution:** out of MVP scope.
- **W3C method registration (IA-4):** tracked separately.
- **Recorder signature scheme migration:** separate use case.

## Impact

If implemented:

| Area        | Transformation                                                                                  |
| ----------- | ----------------------------------------------------------------------------------------------- |
| Wallet      | New `AudienceId::parse_canonical(s)` accepts W3C form; legacy `from_str` becomes internal-only  |
| Reputation  | `RecorderDid::to_wire() -> String` produces W3C form for gossip / CLI; storage remains 52 bytes |
| Marketplace | Reputation compat accepts both forms during window; final state accepts W3C only                |
| Gossip      | Topic `/dot/reputation/{recorder_did_wire}` deterministic across crates                         |
| CLI         | Output uses W3C form; log lines grep-able                                                       |

If NOT implemented:

- Three crates continue to disagree on parser.
- 347 literals continue to leak bare-name shape.
- Cross-mission reputation mismatch remains a documented injection vector.

## Related RFCs

- RFC-0009 (process): Identity Management (Accepted, mandates W3C form but wallet accepts arbitrary).
- RFC-0968 (economics): Reputation Registry (canonical `did:octo:b<52>` storage).
- RFC-0968-A1 (in-place amendment): §Dual-read retirement gate.

## Implementation Phases (preview)

### Phase 1: codec crate

`crates/octo-ident/` with `<52-byte raw> ↔ <W3C wire>` transcoding.

### Phase 2: codemod

347 literals replaced with canonical form via documented helper.

### Phase 3: deprecation

`AudienceId::from_str` accepts only canonical form post-deprecation window (6 months).

## Acceptance Criteria

Use Case approved ⇒ triggers S3 RFC Draft.
