# RFC-0955-R1: Reputation Anchoring Amendment

## Status

Accepted 2026-07-27 (sibling amendment promoted to Accepted in concert with RFC-0955 promotion; RFC-0968 is Accepted so the dependency gate is satisfied per RFC-0955 Status).

## Authors

- Author: @cipherocto
- Author: @mmacedoeu

## Maintainers

- Maintainer: @cipherocto
- Maintainer: @mmacedoeu

## Version History

| Version   | Date       | Changes                                                                                                                 |
| --------- | ---------- | ----------------------------------------------------------------------------------------------------------------------- |
| 1.0-draft | 2026-07-27 | Initial draft. Promoted from in-file amendment at RFC-0955 lines 912-1023 to a sibling Draft RFC per BLUEPRINT process. |

## Summary

This RFC defines the **on-chain reputation anchoring binding** for the Model
Liquidity Layer. It promotes the in-draft amendment that previously lived at
the tail of RFC-0955 (`0955-model-liquidity-layer.md` §"Reputation Anchoring
Amendment", pre-promotion revision) into a sibling Draft RFC, per BLUEPRINT
RFC-lifecycle stage `Draft` (not the ad-hoc "in-draft amendment" state). The
binding carries a `ReputationAnchorBatch` to chain-side infrastructure, with
the per-controller Merkle-root batching model from RFC-0968-A1 amendment 48 (beyond A2 scope — future amendment round TBD), the
governance-set hash binding from RFC-0968 amendment 24, and a 32-byte
domain-separated BLAKE3 anchor digest over the canonical 24-byte Dfp BLOB
(RFC-0104 / RFC-0968 §10).

## Dependencies

**Requires:**

- RFC-0955 — Model Liquidity Layer (parent; the binding target is `ComputeOffer.reputation: ReputationDigest` defined in §"Compute Assets")
- RFC-0968 — Reputation Registry (canonical source of `ReputationError::AnchorTupleFanoutExceeded (0x2A, reserved band 0x2A..=0xFF per RFC-0968 §13)`, `SignalKind` and `ReputationLayer` enums, `governance_set_hash`, `GovernanceProof` per §28.1 amendment 24)
- RFC-0104 — Deterministic Floating-Point (24-byte Dfp encoding)

**Optional:**

- RFC-0927 — RouterConfig Extension (per-deployment `ANCHOR_INTERVAL_SECS` override)

## Relationship to RFC-0955

This RFC is the **canonical authority** for the on-chain anchoring binding. The
parent RFC-0955 cross-references this RFC from its §"Compute Assets" and
§"Implementation Phases" sections; the parent does not duplicate the wire
contract. The promotion of this sibling to Accepted is independent of
RFC-0955's promotion; the full anchoring binding requires both Accepted.

The amendment R1 was previously an in-file block at RFC-0955 lines 912-1023.
It is promoted to a sibling RFC so the binding contract is reviewable in
isolation, with a separate diff history, separate reviewers, and a clean
promotion path (Draft → Accepted) independent of RFC-0955 acceptance
sequencing.

## Constants

All constants are `u64` for chain-compatibility with `chain_block_height`. The
canonical home is `crates/octo-reputation/src/constants.rs`; this RFC declares
them via re-export reference. Any value change requires a paired amendment in
both this RFC and `crates/octo-reputation/src/constants.rs`.

## Wire Contract

A reputation anchoring transaction binds a tuple
`(did, signal_kind, layer, last_event_id)` to the canonical 24-byte Dfp
encoding of the post-EWMA score plus provenance counters. The digest is:

```
anchor_digest = BLAKE3(
    BLAKE3_REPUTATION_ANCHOR_DOMAIN ||
    did                       ||  // 32 bytes canonical did:octo:b<52> hash_part
    u8(signal_kind)           ||  // see SignalKind range below
    u8(layer)                 ||  // see ReputationLayer range below
    last_event_id             ||  // 32 bytes
    DfpEncoding::from_dfp(&score_ewma).to_bytes() ||  // 24 bytes, BE
    u64::to_be_bytes(last_event_unix) ||
    u64::to_be_bytes(samples) ||
    u64::to_be_bytes(severity_total)
)
```

### Range constraints

- `signal_kind` MUST be one of `SignalKind::{Slash, Outcome, Latency, Capacity,
Discovery, Rotation}` (RFC-0968 §10); values `6..255` are reserved and
  rejected at the chain-side contract.
- `layer` MUST be one of `ReputationLayer::{Mon, Dc, Marketplace, TaskMarket,
Retrieval, ProofMarket}` (RFC-0968 §10); values `6..255` are reserved and
  rejected at the chain-side contract.

The range check is enforced at the chain-side contract, NOT at the recorder
side, so a misbehaving recorder cannot bypass the range gate.

### Dfp encoding

`score_ewma_raw` is the byte-exact 24-byte
`DfpEncoding::from_dfp(&score_ewma).to_bytes()` output (RFC-0968 §10; the
`octo_determin::Dfp` type per RFC-0104). The encoding is the bit-deterministic
24-byte BLOB pinned by RFC-0968 §23. The 24-byte length is canonical; no
other length is accepted. Serialized as `serde_bytes` over `[u8; 24]` (the
`DfpBytes` newtype in RFC-0968 §10) at any RPC / CLI boundary.

`ReputationAnchorBatch.score_ewma_raw: [u8; 24]` carries the 24-byte Dfp
encoding verbatim. The on-chain anchoring job verifies
`anchor_digest == BLAKE3(...)` over the exact byte sequence shown above; the
field is a byte slice, not a digest, so the chain does not double-hash.

Two unrelated DIDs with identical `score_ewma = Dfp::from_f64(1.0)` produce
**different** digests because the tuple identity is mixed in. Two histories
that converge to the same per-tuple score produce the same digest (this is
the byte-deterministic property).

## ReputationDigest

```rust
/// The 32-byte length is the BLAKE3-256 output. Future BLAKE3 variants
/// (BLAKE3x for 64-byte output) MUST be introduced as a newtype, e.g.
/// `ReputationDigestX64([u8; 64])`, with a paired RFC-0968 §10 + this
/// RFC amendment.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReputationDigest([u8; 32]);

impl ReputationDigest {
    pub fn from_bytes(bytes: [u8; 32]) -> Self { Self(bytes) }
    pub fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}
```

`ReputationDigest` is the `ComputeOffer.reputation` field type. The previous
8-byte `u64` design was insufficient to carry the 24-byte Dfp encoding
(RFC-0104) or the full tuple identity required for chain-side idempotency;
the field is introduced as a new type in this RFC, not renamed from a live
on-chain field (RFC-0955 was Draft at amendment time).

**Rust source migration.** Any `ComputeOffer { reputation: 0u64 }` literal in
existing code must be replaced with
`ComputeOffer { reputation: ReputationDigest::from_bytes([0u8; 32]) }`. The
compiler rejects any unmigrated literal.

## ReputationAnchorBatch

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationAnchorBatch {
    // Field order matches §"Wire Contract" envelope order to reduce
    // reordering bugs at the implementation layer.
    pub did: [u8; 32],
    pub signal_kind: u8,
    pub layer: u8,
    pub last_event_id: [u8; 32],
    /// Raw 24-byte Dfp encoding of the post-EWMA score, NOT a BLAKE3
    /// digest of that encoding. The anchor envelope hashes these 24 bytes
    /// verbatim (see §"Wire Contract").
    pub score_ewma_raw: [u8; 24],
    pub last_event_unix: u64,
    pub samples: u64,
    pub severity_total: u64,
    pub rotation_receipt_id: Option<[u8; 32]>,
    pub governance_snapshot: GovernanceSnapshot,
    pub governance_proof: GovernanceProof,
    pub governance_set_hash: [u8; 32],
    pub chain_block_height: Option<u64>, // None at submission; set when
                                          // anchor reaches
                                          // MIN_FINALITY_BLOCKS depth
    pub batch_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceSnapshot {
    pub block_height: u64,
    pub epoch: u64,
    pub finalized_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceSigner {
    /// Ed25519 pubkey (32 bytes). Required so the chain-side contract can
    /// recover which key signed each signature (a sorted-key-set 3-of-3
    /// quorum is fragile under committee rotation).
    pub pubkey: [u8; 32],
    pub signature: [u8; 64],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceProof {
    /// Per-signer signatures. Length `GOVERNANCE_QUORUM = 3`. Each entry
    /// carries the signer's pubkey for active-set-membership recovery.
    /// `governance_set_hash` is the BLAKE3 digest of the active
    /// governance-set pubkeys at snapshot time, recomputed at every
    /// snapshot per RFC-0968 §28.1 amendment 24.
    pub signers: Vec<GovernanceSigner>,
}
```

## Chain-Level Idempotency

`(did, signal_kind, layer, last_event_id)` is the chain-level primary key for
an anchoring batch. A duplicate anchoring submission at the same tuple key
returns the existing `anchor_tx_hash` without producing a new transaction. This
permits local-persistence-only failures after a successful chain acceptance
to safely retry without double-charging.

**Local upsert.** On chain-returned `anchor_tx_hash` (either fresh acceptance
or duplicate-key response), the local anchoring job MUST upsert the
`reputation_anchors` row keyed by `event_id` (NOT by `anchor_tx_hash`, since
multiple events may share an anchor). The local upsert is idempotent: a row
that already exists with the same `event_id` is left unchanged; a missing row
is populated with `(event_id, anchor_tx_hash, anchored_at_unix, controller_id,
anchor_root, leaf_count)`. The upsert uses
`INSERT ... ON CONFLICT (event_id) DO NOTHING` in stoolap.

## Finality

Anchoring transactions are stored with the chain block height at which they
achieve `MIN_FINALITY_BLOCKS = 12` confirmation depth; consumers MUST NOT
treat an anchor as final before this depth. Reorgs deeper than
`MIN_FINALITY_BLOCKS` invalidate the local anchor row.

**Reorg re-submission.** On reorg deeper than `MIN_FINALITY_BLOCKS`, the
chain-side anchoring job MUST:

1. Detect the reorg via the chain node's `chain_reorg_depth` event.
2. Re-submit the anchor as a fresh transaction (NOT a duplicate-submission
   retry, because the prior tx is now orphaned and the chain-level idempotency
   rule would return the orphan's hash).
3. Re-emit a fresh `anchor_tx_hash` and update the local `reputation_anchors`
   row.

The fresh submission produces a new `anchor_root` only if the leaf set has
changed; if the leaf set is identical, the new tx returns the new (post-reorg)
`anchor_tx_hash` with the same root.

**DID rotation.** If a `consume_rotation_receipt` for the anchor's `did` is
finalized in the chain BEFORE the anchor's `MIN_FINALITY_BLOCKS` is reached,
the anchor submission is invalidated (treated like a reorg that drops the
anchor). The anchoring job re-submits the anchor for `new_did` with the
post-decay `score_ewma` (the `0.9` decay factor per RFC-0968 §2.1 step 3).
If the rotation consumes AFTER the anchor's finality depth, the anchor
remains authoritative for the pre-rotation aggregate; the post-rotation
aggregate is anchored separately.

## Governance Snapshot Binding

Every anchoring transaction carries a
`GovernanceSnapshot { block_height, epoch, finalized_at_unix }`. The
implementation validates
`snapshot.finalized_at_unix + MAX_GOVERNANCE_SNAPSHOT_AGE_SECS >= now_unix`
before the snapshot-bound registry lookup, per RFC-0968 §3. There are no
snapshot exceptions.

Additionally, every anchoring transaction carries a `GovernanceProof` with
`governance_set_hash: [u8; 32]` + `signers: Vec<GovernanceSigner>` of length
`GOVERNANCE_QUORUM = 3`, per RFC-0968 §28.1 amendment 24 and §21 Review
Round 7 cross-mission-governance #1. The chain-side contract verifies the
3-of-3 signatures against the recomputed `active_set_digest(snapshot)`
before accepting the anchor; failure returns
`ReputationError::GovernanceSetHashMismatch (0x13)` or
`ReputationError::GovernanceQuorumNotMet (0x12)`.

## Merkle-Root Construction

The Merkle root is computed as follows:

1. Leaves are the per-tuple `anchor_digest` (BLAKE3 output, 32 bytes) sorted
   lexicographically by `did || signal_kind || layer || last_event_id` (the
   natural tuple key).
2. The tree is a binary BLAKE3-256 Merkle tree (default algorithm pinned at
   this RFC's promotion to Accepted; alternative SHA-256 requires an
   amendment).
3. Odd leaves at each level are promoted (no duplicate-last-leaf hack).

The 32-byte `anchor_root` is the Merkle root of this tree. The verification
path is a sibling-hash list from the leaf to the root.

## Cost Model

Each `(controller_id, ANCHOR_INTERVAL_SECS)` window produces exactly one
anchor root containing up to `MAX_TUPLES_PER_ROOT = 100` leaves per the
`MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL = 1` cap. At 288
intervals/day, the per-controller throughput is
`288 anchor roots/controller/day`.

**Fee schedule.**

- `ANCHOR_FEE_PER_ROOT = 5_000 OCTO` (non-refundable, paid up-front).
- `MIN_FEE_PER_LEAF = 50 OCTO`. The fee is proportional to leaf count.
  1-leaf = 50 OCTO; 100-leaf = 5_000 OCTO (upper bound).
- The fee is paid by the attested `controller_id`'s role-token balance at
  submission time. A pre-funded controller-side escrow
  (`controller_anchor_escrow: u64`) is debited at submission.
- Under-funded controllers cannot submit anchors until topped up via
  `top_up_controller_anchor_escrow(controller_id, amount)` (a
  governance-issued transaction). Insufficient balance rejects the anchoring
  job with `ReputationError::ControllerFeeBalance = 0x3D` (RFC-0955-R1
  round 4 addition to the joint RFC-0968 §13 / RFC-0955-R1 table; canonical
  home RFC-0968). Reserved range is `0x3E..=0xFF`.

**Spam amplification bound.** A controller saturating the channel with 1-leaf
roots at every `ANCHOR_INTERVAL_SECS` window pays
`MIN_FEE_PER_LEAF × 288 intervals = 14_400 OCTO/day`. The same controller
at 100-leaf roots pays `5_000 × 288 = 1_440_000 OCTO/day`. The 100× gap
closes the 1-leaf spam amplification (Round 10 R10-N03).

A per-chain cost estimate with three recorder-count scenarios
(low: ≤100 recorders, medium: 100-10_000, high: >10_000) is published as
part of `missions/claimed/0968a-reputation-anchoring.md` (gated on this
RFC acceptance). The estimate uses the upper bound
`288 anchor roots/controller/day` (per `DEFAULT_ANCHOR_INTERVAL_SECS = 300`)
× `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL` × recorder-count, NOT the
legacy `events / ANCHOR_BATCH_SIZE` heuristic. The estimate is a pre-condition
for this RFC's promotion-to-Accepted review.

## Tuple-Fanout Defense

A recorder emitting events across many distinct `(did, kind, layer)` triples
forces one anchor submission per active tuple per interval, multiplying the
per-recorder chain-fee footprint by the tuple cardinality. To bound this,
RFC-0968-A1 amendment 48 (beyond A2 scope — future amendment round TBD) introduces **Merkle-root batching at the controller
level** (per-controller, NOT per-recorder):

- `MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL = 1`. Each controller
  (attestor-attested `controller_id` per RFC-0968 amendment 44 (deferred to RFC-0968-A2 — controller_id = blake3(governance_pubkey) derivation)) submits
  exactly one anchor root per `ANCHOR_INTERVAL_SECS` window.
- `MAX_TUPLES_PER_ROOT = 100`. The leaf set of each root is the union of
  all recorder-tuple digests under that controller; the chain-side contract
  stores `anchor_root`, not individual tuples.
- `MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY = 100` is the per-controller
  24h-rolling cap. A controller saturating the channel with new
  `(did, signal_kind, layer)` tuples per 24h window beyond 100 trips
  `ReputationError::AnchorTupleFanoutExceeded` (`0x2A`, joint RFC-0968 /
  this RFC table entry).

The limits make the worst-case chain-fee footprint per controller
`MAX_ANCHOR_ROOTS_PER_CONTROLLER_PER_INTERVAL × 288 intervals/day = 288
anchor transactions/controller/day`, independent of tuple fanout below the
per-leaf cap. The per-controller aggregation supersedes the prior
per-recorder model; the `ANCHOR_FEE_ESCROW_PER_TUPLE_PER_DAY` field is
REMOVED.

## Roles

This RFC introduces the **`ReputationAnchor`** role binding for destination
nodes (cross-reference to RFC-0971). The role binding is **OPTIONAL** —
a destination node may perform deal settlement and forwarding without the
`ReputationAnchor` role. The role binding is the destination-node-side
counterpart of the on-chain anchoring binding defined in this RFC.

### Relationship to RFC-0971

RFC-0971 §Phase 1 defines `RoleBindingDeclaration` with `required_roles` +
`optional_roles` BTreeSets over the typed `RoleTag` enum. The `ReputationAnchor`
variant is the OPTIONAL role; absence from both `required_roles` and
`optional_roles` does NOT block the destination node from performing deal
settlement or forwarding. The canonical predicate for a destination node is
`Router ∧ TokenIssuer ∧ Asker` with `ReputationAnchor` as an OPTIONAL
augmentation (RFC-0971 §Phase 1 R13-N8 fix).

### Mechanism vs role

The anchoring itself is a **mechanism** (the on-chain binding described in
this RFC) — every eligible destination node MAY anchor reputation to the
chain-side ledger regardless of role binding. The `ReputationAnchor` role
binding is the **destination-node declared intent** to perform anchoring
operations, surfaced via `RoleBindingDeclaration.optional_roles`. The
mechanism is owned by RFC-0955-R1; the role binding is owned by RFC-0971.

### Cross-crate wiring

The canonical `RoleBindingDeclaration` substrate lives at
`crates/quota-router-core/src/node/role_binding.rs` (mission 0971-a
Band A closure, commit `67a47ace`). The `RoleTag::ReputationAnchor` variant
is the typed enum entry. The `destination_optional_roles()` helper
constructs the canonical `optional_roles = {ReputationAnchor}` set for
the destination node pattern. The `validate_destination_binding()` call
asserts the predicate without requiring `ReputationAnchor`, so anchoring
absence is non-blocking per RFC-0971 §Roles cross-reference.

### Audit trail

Role-binding transitions (including `ReputationAnchor` opt-in / opt-out
transitions) are recorded in the append-only `RoleBindingAuditLog` (mission
0971-a commit `67a47ace`, file `crates/quota-router-core/src/node/role_binding_audit.rs`).
The audit trail uses typed `RoleTag` enum (NO string literals); TV8 grep
test enforces. Separate from the chain-side `ReputationAnchorBatch` defined
in this RFC.

## Wire Compatibility

The previous `reputation: u64` field on `ComputeOffer` (RFC-0955 §3.2) is
replaced by `ReputationDigest` (`[u8; 32]`). No on-chain migration is needed
(RFC-0955 was Draft at amendment time). Rust source migration: any
`ComputeOffer { reputation: 0u64 }` literal must be replaced with
`ComputeOffer { reputation: ReputationDigest::from_bytes([0u8; 32]) }`. The
compiler rejects any unmigrated literal; a `#[deprecated]` attribute on a
`From<u64>` impl (REMOVED at acceptance) can surface migration sites.

## Wire-Format Versioning

The on-chain wire format is versioned via the first byte of the
`governance_set_hash` field. The current version is `0x01`. Subsequent
amendments that change the wire format MUST bump the version byte and
update the encoding tables below. A pre-image rejection rule: chain-side
contract MUST reject any anchoring transaction whose `governance_set_hash[0]`
does not match the active version byte. Version-history table:

| Version | Date       | Changes                                                                                                                                                                                                                                   |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `0x01`  | 2026-07-27 | Initial format: 32-byte BLAKE3 anchor digest; `GovernanceProof.signers: Vec<GovernanceSigner>` (per-sig pubkey required); `ReputationAnchorBatch` carries `governance_proof` + `governance_set_hash` + `chain_block_height: Option<u64>`. |

Signer ordering: `GovernanceProof.signers` MUST be sorted lexicographically
by `signer.pubkey` before transmission. Two replicas observing the same
attestor set MUST produce byte-identical `GovernanceProof` serialization.

Legacy rejection: anchoring transactions that carry the pre-R1 wire format
(`governance_set_hash` byte `0x00` or absent; `signatures: Vec<[u8; 64]>`
without per-signer pubkey) MUST be rejected at the chain-side contract with
`ReputationError::GossipEnvelopeInvalid (0x3A)` (RFC-0968 §28.4 amendment 22). No
on-chain migration tooling is provided (RFC-0955-R1 was Draft at the time
of the breaking change; no live pre-R1 anchor transactions exist).

## Performance Targets

| Metric                                                  | Target    | Notes                                                 |
| ------------------------------------------------------- | --------- | ----------------------------------------------------- |
| Anchor submission (single batch, per controller)        | <2s p99   | Mempool admission + chain-side idempotency lookup     |
| Anchor finality (depth confirmation)                    | <5min p99 | `MIN_FINALITY_BLOCKS = 12` at ~25s/block; cap at 5min |
| Anchor Merkle root computation                          | <50ms p99 | 100 leaves per root, in-memory BLAKE3                 |
| Anchor storage (per anchor row in `reputation_anchors`) | <10ms p99 | Indexed PK lookup on `event_id`                       |

## Error Handling

| Hex code | Variant                     | Source                                   | Recovery                                                                       |
| -------- | --------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------ |
| `0x2A`   | `AnchorTupleFanoutExceeded` | RFC-0968 §13 (reserved band 0x2A..=0xFF) | Reject; controller exceeds `MAX_ANCHORED_TUPLES_PER_CONTROLLER_PER_DAY = 100`. |
| `0x12`   | `GovernanceQuorumNotMet`    | RFC-0968 §13                             | Reject; re-submit with full `GOVERNANCE_QUORUM = 3` signatures.                |
| `0x13`   | `GovernanceSetHashMismatch` | RFC-0968 §13                             | Reject; re-fetch the active set hash and re-sign.                              |

The 0955-R1 wire layer re-exports `ReputationError::AnchorTupleFanoutExceeded`
from RFC-0968; the joint-table entry is the single source of truth.

## Test Vectors

The wire-contract envelope MUST have at minimum the following test vectors,
reproducible byte-exact by an independent implementation:

1. **Canonical anchor envelope.** With `did = [0u8; 32]`, `signal_kind = 0`,
   `layer = 0`, `last_event_id = [0u8; 32]`, `score_ewma_raw = [0u8; 24]`,
   `last_event_unix = 0`, `samples = 0`, `severity_total = 0`:
   `anchor_digest = BLAKE3(BLAKE3_REPUTATION_ANCHOR_DOMAIN || [0u8;32] ||
0x00 || 0x00 || [0u8;32] || [0u8;24] || [0u8;8] || [0u8;8] || [0u8;8])`
   yields a deterministic 32-byte digest. Pin the expected bytes in
   `crates/octo-reputation/tests/anchoring/canonical_blobs.rs::CANONICAL_ANCHOR_BLOB`.
2. **Cross-DID distinctness.** Two unrelated DIDs with identical score produce
   different digests (two expected 32-byte outputs).
3. **Dfp score-byte-equality.** Replicate RFC-0968 §23 TV1 with the anchor
   envelope wrapping the canonical `Dfp::from_f64(0.961)` 24-byte BLOB.

**Byte-deterministic property test.** Two independent implementations running
the same `(did, signal_kind, layer, last_event_id, score_ewma_raw,
last_event_unix, samples, severity_total)` tuple MUST produce the same
32-byte `anchor_digest`. An independent Python implementation using the
`hashlib.blake3` library MUST reproduce the same expected bytes.

## Cross-References

- `crates/octo-reputation/src/constants.rs` — canonical home of all six
  constants declared above.
- RFC-0955 — parent RFC; §"Compute Assets" + §"Implementation Phases" +
  §"Performance Targets" reference this RFC.
- RFC-0968 — §3 (recorder authorization), §10 (SignalKind + ReputationLayer
  enum ranges + `election_priority`), §13 (error table; canonical home of
  `AnchorTupleFanoutExceeded (0x2A, reserved band 0x2A..=0xFF per §13)`), §16 (class table for reputation
  reads), §21 (economic analysis + finality coupling), §28.1 amendment 24
  (`governance_set_hash` + `GOVERNANCE_QUORUM = 3`), §28.1 (RFC-0968-A1 amendments 40 + 44, deferred to RFC-0968-A2) (controller-level aggregation), §28.1 amendment 48 (per-controller
  Merkle-root batching), §28.1 amendment 51 (proportional fee per leaf).
- RFC-0104 — Dfp bit-determinism; canonical 24-byte encoding.
- `missions/claimed/0968a-reputation-anchoring.md` — the implementation
  mission; gated on this RFC acceptance (live chain-side binding patch:
  `missions/claimed/0968a2-reputation-anchoring-binding.md`).
- `missions/claimed/0968-reputation-persistence.md` — Phase 1-4 only;
  Phase 5 (anchoring) is owned by mission 0968a.

---

**Version:** 1.0-draft
**Submission Date:** 2026-07-27
**Last Updated:** 2026-07-27
