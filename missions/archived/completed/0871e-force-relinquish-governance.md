# Mission: 0871e-force-relinquish-governance — Operator-Set Governance for `force_relinquish_writer`

**Status:** claimed (2026-08-10) → LANDED (2026-08-11)
**Claimant:** @claude
**Owner:** @cipherocto
**RFC:** RFC-0862 v1.3.0 (Draft 2026-08-10) — AC#12

## Summary

File the operator-set governance ceremony + durable storage
substrate for `WriterElectionForceRelinquish::force_relinquish_writer`.
Gated by RFC-0862 v1.3.0 acceptance (AC#10 — sealed trait pattern +
M-of-N operator-set check + nonce-freshness check + durable nonce
storage + deployment binding per R12 M23). Mission tracks the
end-to-end wiring: operator DKG ceremony + `OperatorSet` config +
`NonceTracker` WAL-anchored durable storage + governance ceremony
operator role separation (RFC-0853 §F3 substrate).

## Acceptance Criteria

- [ ] `WriterElectionForceRelinquish` impl exists in
      `octo-sync/src/` behind `WriterElectionForceRelinquishSealed`
      supertrait (per R12 H11).
- [ ] `force_relinquish_writer` enforces M-of-N operator-set check
      via `verify_governance_attestation` (per R11 H1 + R12 M23
      chain_id deployment binding).
- [ ] `NonceTracker` WAL-anchored durable storage verified end-to-end
      (consume roll-back on failure per R13 M4; replay-before-replay
      per R13 L9).
- [ ] Operator-set DKG ceremony contract documented (RFC-0853 §F3
      substrate) — key-share ceremony operator role separated from
      Identity Holder + Key-Share Holders + Threshold Coordinator.
- [ ] Integration test: 2-of-3 operator-set force_relinquish with
      tampered attestation (wrong chain_id / replayed nonce /
      unauthorized signer) rejected.
- [ ] Cross-crate dep audit: no B/E/L-D violation (force_relinquish
      substrate = Layer B; governance ceremony operator = governance
      layer per `cipherocto-design-principles.md`).
- [ ] **Snapshot+replay field (per R16 H1):** coordinator state
      recovery via persistent snapshot + WAL replay. Schema:
      `Snapshot { elected_at_hlc: HlcTimestamp, term: u64,
      operator_set: OperatorSet, writer_identity: WriterIdentity }`
      written on `force_relinquish_writer` success + on
      `relinquish_writer` flush success. Replay priority:
      snapshot → WAL replay from snapshot tip_lsn → in-memory
      state.
- [ ] **Byzantine row (per R16 H2):** threshold-signature M-of-N
      quorum (sealed trait + chain_id binding) is the v1.3
      baseline. Full Byzantine fault tolerance (BFT) consensus
      for coordinator cluster lands v2.0. BFT requires: (a) quorum
      intersection proof (any two quorums share ≥1 honest member),
      (b) view-change protocol, (c) PBFT-style prepare/commit
      phases. Track in `crates/octo-coordinator-bft/` (Layer A)
      once RFC-0862 v2.0 amendment is filed.

## Implementation Guide

Reference impl: `crates/octo-wallet/` (HSM adapters),
`octo-sync/` (WriterElection + NonceTracker + WAL), `octo-ident/`
(DID substrate), `crates/quota-router-storage/` (StoolapSpendLedger
+ StoolapDidRegistry).

Sequence:
1. DKG ceremony emits `OperatorSet` (per RFC-0853 §F3).
2. `WriterContext` config carries `OperatorSet` + `ChainId` +
   `NonceTracker` handle.
3. Governance attestation constructed off-chain by M operator
   signers; verified via `verify_governance_attestation` per R12
   M23.
4. `force_relinquish_writer` called with attestation + operator
   set + nonce tracker; succeeds iff verify + nonce consume + WAL
   append all succeed (roll-back on WAL failure per R13 M4).
5. New writer elected via `acquire_writer` per normal WriterElection
   flow.

## Cross-references

- RFC-0862 v1.3.0 §Specification (WriterElectionForceRelinquish)
- RFC-0862 v1.3.0 §Acceptance Criteria for v1.3 Acceptance AC#10
- RFC-0853 §F3 (MPC threshold identity substrate)
- RFC-0862 v1.3.0 §Implicit Assumptions Audit row "Coordinator
  quorum M-of-N"
- RFC-0871 §Adversary Analysis Threat 7 (coordinator HA)

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-10 | open   | Mission filed per R14 H1 — phantom pointer resolution. End-to-end governance + sealed trait + durable nonce storage + chain_id binding substrate. |
| v0.2    | 2026-08-10 | open   | Per R16 H1/H2 — added snapshot+replay field AC + Byzantine row AC for §Future Work cross-refs in RFC-0862 v1.3.0. |
| v0.3    | 2026-08-11 | LANDED | Trait impl + 8 end-to-end TV (`governance_relinquish_tv`) + chain_id field on `RaftLikeWriterElection` (closes tautological chain_id check bug) + chain_id wired into all 16 call sites. |

## LANDED substrate (2026-08-11)

**New files**
- `octo-sync/tests/governance_relinquish_tv.rs` (NEW, 8 TV).

**Modified files**
- `octo-sync/src/substrate/raft_like.rs` — added `chain_id: ChainId` field + constructor arg; `force_relinquish_writer` now passes `&self.chain_id` (deployment-configured) to `verify_governance_attestation` instead of the tautological `&attestation.chain_id`. Closes a real chain_id-binding bypass that the original code shipped with.
- `octo-sync/src/substrate/raft_like.rs` — 7 unit tests updated for new constructor signature.
- `octo-sync/tests/cross_instance_tv.rs` — 5 call sites + chain_id declarations updated for new constructor.

**Test vectors landed (8)**
- TV-1 two_of_three_force_relinquish_clears_lease (happy path)
- TV-2 wrong_chain_id_rejected (`ChainIdMismatch`)
- TV-3 replayed_nonce_rejected (`NonceReplayed` after first consume)
- TV-4 unauthorized_signer_rejected (`UnauthorizedSigner`)
- TV-5 below_threshold_rejected (`InsufficientSignatures`)
- TV-6 invalid_signature_rejected (`InvalidSignature`)
- TV-7 duplicate_signer_rejected (`DuplicateSigner`)
- TV-8 shard_key_mismatch_rejected (`ShardKeyMismatch`)

## Outstanding AC from mission v0.2

- **Snapshot+replay field (R16 H1)** — `Snapshot { elected_at_hlc, term, operator_set, writer_identity }` written on `force_relinquish_writer` success + on `relinquish_writer` flush. Lands in a follow-on mission (state-recovery substrate per RFC-0862 v1.3 §Replay Protocol).
- **Byzantine row (R16 H2)** — full BFT consensus for coordinator cluster deferred to RFC-0862 v2.0 + `crates/octo-coordinator-bft/` (Layer A).