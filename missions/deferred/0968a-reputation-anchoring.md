# Mission 0968a: Reputation On-Chain Anchoring (DEFERRED)

## Status

**Deferred.** Depends on RFC-0955 acceptance (and the RFC-0955 `reputation:blake3_digest` follow-up amendment). RFC-0968 acceptance (now satisfied 2026-07-25) was a transitive prerequisite but was not the binding blocker; future 0968a work remains pending RFC-0955 acceptance + amendment.

**v3.0-r15 (2026-07-25, Gap 9):** RFC-0968's reputation data model now uses `octo_determin::Dfp` per RFC-0104. The `SignalEvent.score_delta` BLOB and the `ReputationAggregate.score_ewma` BLOB are bit-deterministic across compilers and platforms. Anchoring these BLOBs is straightforward: anchor `BLAKE3(DfpEncoding::from_dfp(&reputation).to_bytes())` (a 32-byte BLAKE3 digest of the canonical 24-byte Dfp encoding) into the RFC-0955 `reputation:blake3_digest` field. v3.3-r18 (C12) corrects the previous "anchor 24-byte DFP directly into RFC-0955 `reputation:u64`" wording — `reputation:u64` is 8 bytes and cannot carry a 24-byte BLOB. The BLAKE3 digest form is the canonical binding contract: 32 bytes fit cleanly into `blake3_digest`, the digest commits to the exact 24-byte Dfp encoding bit-for-bit, and anchoring the digest (rather than the encoding) is what makes the binding consistent across on-chain size constraints. No `f64` migration path exists in the parent mission.

**v3.5-r20 (2026-07-25):** research no-on-chain-anchoring wording aligned with the Round 19 M2 RFC-0955 §`reputation` follow-up scope and amendment-required caveat; v3.4-r19 version-history line references corrected.

## RFC

- RFC-0955: Model Liquidity Layer (the on-chain binding target)
- RFC-0968: Reputation Registry (the persisted source-of-truth whose events will be anchored)

## Summary

Anchor `SignalEvent` records from RFC-0968's `reputation_events` table to the
CipherOcto on-chain ledger (RFC-0955's `reputation:blake3_digest` field — 32-byte `BLAKE3(DfpEncoding::from_dfp(&reputation).to_bytes())` digest). The persisted
source aggregate has exactly nine canonical fields (`did`, `kind`, `layer`,
`score_ewma`, `samples`, `severity_total`, `last_event_id`, `last_event_unix`,
`updated_at_unix`). This mission extends `SignalEvent` with
`anchor_tx_hash: Option<[u8; 32]>` and adds a background job that submits a
batching transaction per `(did, kind, layer)` tuple whose `last_event_id` is
unanchored.

This mission is **extracted** from RFC-0968's Phase 5 per Round 1 finding H11
(the original RFC conflated gossip federation + on-chain anchoring under one
mission; RFC-0968 now owns gossip federation (mission 0855p-b substrate) and
this mission owns on-chain anchoring).

## Why deferred?

- RFC-0955 is still Draft. On-chain anchoring requires the binding contract
  to be live; the RFC-0955 `reputation:blake3_digest` follow-up amendment
  must also be accepted before the 32-byte BLAKE3 digest field becomes the
  canonical binding target.
- Anchoring is a separate cost model (gas, batch frequency) from gossip
  federation (storage, durability).
- Mission 0968a unblocks independently of RFC-0968 acceptance (RFC-0968
  was promoted to Accepted 2026-07-25; the blocker is RFC-0955).

## Scope (when unblocked)

- [ ] Extend `SignalEvent` with `anchor_tx_hash: Option<[u8; 32]>`.
- [ ] Add `ReputationStore::anchor_pending(batch_size: u32)` API; returns
      `Vec<anchor_tx_hash>` for the events it successfully anchored.
- [ ] Add `reputation_anchors` table:
      `event_id BLOB PRIMARY KEY, anchor_tx_hash BLOB NOT NULL, anchored_at_unix INTEGER NOT NULL`.
- [ ] Background job: scan `reputation_events` where `anchor_tx_hash IS NULL`,
      submit batch transaction per RFC-0955 binding contract, persist
      `anchor_tx_hash` after inclusion.
- [ ] Define `ANCHOR_INTERVAL_SECS` (default 300) and `ANCHOR_BATCH_SIZE`
      (default 1000) as config.
- [ ] Cross-reference: mission 0855p-b gossip uses `anchor_tx_hash` to
      verify gossiped events have on-chain provenance.

## Out of scope (this mission)

- Persisted reputation storage (RFC-0968 / mission 0968).
- Gossip federation (mission 0855p-b).
- Reputation tokenization (debated, no RFC).

## Acceptance Criteria

- [ ] Anchoring job is idempotent (re-running on anchored events is no-op).
- [ ] Anchoring failure does not corrupt `reputation_events` (events remain
      valid; only anchor persistence is rolled back).
- [ ] `reputation_anchors` is queryable by `did` (joined via `reputation_events`).
- [ ] `reputation_anchors` stores only `EventId` values. `AttestationId` is a
      distinct attestation namespace and is never accepted as an anchor key.
      Round 7 H2: `EventId` and `AttestationId` fields are private with
      `from_bytes(...)` validated constructors, so the anchoring path constructs
      the anchor key only through the typed `EventId` API.
- [ ] Anchor batch size + interval are configurable per deployment.
- [ ] Round 8 snapshot rule: anchoring transactions carry a `GovernanceSnapshot` and validate it before the snapshot-bound registry lookup. As with every authoritative signature or registration—including `GovernanceProof`, `ResumeProof`, and `AttestorAuth`—there are no snapshot exceptions. If `snapshot.finalized_at_unix + MAX_GOVERNANCE_SNAPSHOT_AGE_SECS < now_unix`, the operation returns `ReputationError::GovernanceSnapshotStale`.

## Location

**TBD upon unblock; no migration slot is reserved.** Phase 1 reputation persistence uses v006 for attestations and v007 for aggregate checkpoints. The anchoring crate paths, migration filename/version, and modify-list will be allocated only after RFC-0955 is Accepted and its binding contract shape is stable.

## Complexity

Medium. RFC-0955 binding contract interaction + batch job + storage extension.

## Claimant

(unassigned)

## Pull Request

# (TBD — pending RFC-0955 acceptance)
