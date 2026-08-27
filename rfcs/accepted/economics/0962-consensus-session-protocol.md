# RFC-0962 (Economics): ExecutionEnvelope — Database Transaction Object Protocol

## Status

Accepted v2.1 — Status header synced with §Version History v2.1-R8-F5-Clarify row per M37 corpus-wide sync check (prior header read v2.0; VH row latest added 2026-07-24 with §7.1 MultiEnvelope recursive nesting depth clarification).

> **Note:** Companion RFC to RFC-0960 §10 (Execution Envelopes) and §1 (Deterministic SQL Engine). Defines the wire-protocol shape, lifecycle states, signature aggregation, ZK commitment, and reconciliation semantics of an `ExecutionEnvelope`. Builds on RFC-0959 (SettlementReceipt envelope), RFC-0957 (Capability), RFC-0862 (sync as propagation), RFC-0961 (Deterministic SQL dialect), and RFC-0967 (Policy Object reference).

> **Note (v2.0 rename):** v2.0 (2026-07-23) renames the object from `ConsensusSession` to `ExecutionEnvelope` per strategic reframe (RFC-0960 §1.2): an ExecutionEnvelope is a database-level unit of work that produces a deterministically-replayable WAL segment. Consensus is an implementation detail of WAL certification, not the primary abstraction. Field `mode = CONSENSUS_SAFE` renamed to `mode = DETERMINISTIC`. Field `version_tag` bumped from 1 to 2 to signal breaking change.

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-22 | @cipherocto + @mmacedoeu | Initial draft (as `ExecutionEnvelope`). |
| v2.0 | 2026-07-23 | @cipherocto + @mmacedoeu | Strategic reframe (R17+): renamed to `ExecutionEnvelope`. WAL-as-primary inversion. `mode = DETERMINISTIC`. `version_tag = 2`. Outer namespace tag `0x04` retained for `ExecutionEnvelope` (was `ConsensusSession`); cross-RFC namespace-tag table extended in RFC-0964 §0 to include `0x07 = PolicyObject` (RFC-0967). |
| v2.0-Accepted | 2026-07-23 | @cipherocto + @mmacedoeu | **Promoted Draft → Accepted.** R1-R28 multi-round adversarial review closed with R28 clean round (zero actionable defects). Companion RFCs (RFC-0960, RFC-0961, RFC-0963, RFC-0964, RFC-0965, RFC-0967) promoted in lockstep on 2026-07-23. |
| v2.1-Resolved | 2026-07-23 | @cipherocto + @mmacedoeu | **Risk-closure round.** All 6 Open Questions resolved with concrete answers (max session size caps, WAL two-phase hash binding, MultiEnvelope audit window + offline recovery semantics, `RequireProof` constraint integration). §6.4 added for WAL hash two-phase binding. Additive (non-breaking) bump. |
| v2.1-R8-F5 | 2026-07-24 | @cipherocto + @mmacedoeu | **MultiEnvelope recursive nesting (R8-F5).** §7.1 added to define the `nested: Option<Box<MultiEnvelope>>` field on `MultiEnvelope` and the `check_nesting_depth` recursion contract (`MAX_NESTING_DEPTH = 4`, accepting depths `0..=3`). Additive (non-breaking): `#[serde(default)]` on the new field keeps v2.0 wire payloads deserializable with `nested = None`. |
| v2.1-R8-F5-Clarify | 2026-07-24 | @cipherocto + @mmacedoeu | **§7.1 clarification (post-review).** Depth enumeration now spelled out by level (depth 0 = root, depth 3 = max accepted, depth 4 = rejected) instead of edge-count wording. Open conformance gap for `multi_envelope_id` derivation explicitly noted as out-of-scope for R8-F5. |

## Authors

- Author: @cipherocto (grand-design Execution Envelope work)
- Contributor: @mmacedoeu (RFC-0962 protocol extraction + v2.0 rename)

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @cipherocto

## Summary

An `ExecutionEnvelope` is a database-level unit of work. One envelope bundles N SQL operations (or Deterministic SQL procedure invocations) into a single signed, hash-committed, deterministically-replayable block that produces a **WAL segment**. Externally the application sees a JDBC transaction; internally the consensus layer sees one signed envelope that certifies one WAL segment. The envelope is a *projection* of the WAL (RFC-0960 §1.1, §1.2); consensus is one possible certifier of that projection.

Three artifacts:

1. **`ExecutionEnvelope`** — content-addressable signed envelope binding capability holder (or `policy_id` reference, RFC-0967), WAL segment hash, SQL statement list, and timestamp. Hash = `BLAKE3(version_tag=2 || canonical_ser(envelope_unsigned))`.
2. **`EnvelopeCommitment`** — consensus-layer commitment that mirrors the RFC-0959 `SettlementReceipt` *envelope shape* (canonical_ser + BLAKE3 hash + Ed25519 signature) but binds an `envelope_id` instead of an `ask_id`. **Not** a `SettlementReceipt`; the two objects are disjoint. Replay defense via `ConsumedEnvelopeIndex` (§6.3).
3. **`EnvelopeProof`** — optional ZK proof that the envelope's SQL operations were executed under the capability's policy without revealing the operation bodies (per RFC-0958).

Coexists with RFC-0959. RFC-0959 governs per-node Ask pricing; RFC-0962 governs multi-statement database transactions under capabilities. Both use the same BLAKE3 envelope shape but bind different objects.

**v2.0 framing:** The envelope is one *projection* of the Deterministic WAL (RFC-0960 §1.1). Other projections (Replication, Time Travel, Materialized Views, Event Stream, Git-branches) are defined elsewhere. The envelope is the SQL-facing surface; the WAL is the protocol.

## Dependencies

### Required RFCs

| RFC | Status | Reason |
|-----|--------|--------|
| RFC-0960 | **Accepted v2.0 (2026-07-23; promoted in lockstep with this RFC)** | Defines §10 Execution Envelopes architecture (renamed from §12 Consensus Sessions in v2.0) |
| RFC-0961 | **Accepted v2.0 (2026-07-23; promoted in lockstep with this RFC)** | CIPHERO_SQL deterministic procedure language |
| RFC-0959 | Accepted (v1.0, 2026-07-20) | SettlementReceipt envelope shape; same canonical_ser pattern |
| RFC-0957 | Accepted | Capability binding (capability_holder field) |
| RFC-0958 | Accepted | ZK capability subclass for `EnvelopeProof` |
| RFC-0862 | Accepted (v1.2.0) | Sync as propagation; sessions ship as event batches |
| RFC-0126 | Accepted (v2.5.1) | Canonical serialization for session envelope |
| RFC-0102 | Accepted | Wallet cryptography (Ed25519 substrate for session signature) |
| RFC-0009 | Draft | Node identity for signature verification |
| RFC-0853 | Draft | BLAKE3 primitive source |

### Companion RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0963 | Builds on | Resource shard routing; cross-shard sessions use `MultiEnvelope` — Accepted v2.0 (2026-07-23; promoted in lockstep) |
| RFC-0964 | Builds on | Constraint encoding for capability constraint evaluation — Accepted v1.1 (2026-07-23; promoted in lockstep) |
| RFC-0965 | Builds on | Capability extension format (caveat types referenced by `capability_holder`) — Accepted v1.1 (2026-07-23; promoted in lockstep) |
| RFC-0967 | Refers to | Policy Object Graph (one-shot reference, RFC-0962 §3 imports `policy_id` per RFC-0967) — Accepted v1.0 (2026-07-23, NEW; promoted in lockstep) |

### Dependency Validation

Standalone, top-level section to satisfy BLUEPRINT v1.3 mandatory section set.

| Dependency | Type | Current Status (2026-07-23) | Assumed Before Accept? | Hard-block on RFC-0962 acceptance? |
|------------|------|------------------------------|------------------------|-------------------------------------|
| RFC-0960 | Requires | **Accepted v2.0 (promoted in lockstep)** | Yes | **YES → resolved** |
| RFC-0961 | Requires | **Accepted v2.0 (promoted in lockstep)** | Yes | **YES → resolved** |
| RFC-0959 | Requires | Accepted | Already | No |
| RFC-0957 | Requires | Accepted | Already | No |
| RFC-0958 | Requires | Accepted | Already | No |
| RFC-0862 | Requires | Accepted | Already | No |
| RFC-0126 | Requires | Accepted | Already | No |
| RFC-0102 | Requires | Accepted | Already | No |
| RFC-0009 | Requires | Draft | Yes | YES |
| RFC-0853 | Requires | Draft | Yes | YES |

**DAG check:** `0962 ← {0960, 0961, 0959, 0957, 0958, 0862, 0126, 0102, 0009, 0853}` — acyclic. No back-edges to RFC-0962.

**Implicit Assumptions Audit:**
- IA-1: RFC-0957 reaches Accepted with caveat DSL stable enough for `capability_holder` binding. → resolved (Accepted).
- IA-2: RFC-0958 ZK circuit accepts session-style commitments (not just ask-style). → resolved (Accepted).
- IA-3: RFC-0009 node identity provides DID format compatible with `signed_by` field.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Session hash deterministic across implementations | Two nodes replaying same `(capability_id, sql_statements, timestamp)` produce identical 32-byte hash |
| G2 | One signature per session | N SQL operations → 1 Ed25519 signature → 1 ZK proof (optional) |
| G3 | Replay defense | Same `(envelope_unsigned, signed_by)` from same signer yields distinct `envelope_id` via monotonic counter + nonce |
| G4 | DETERMINISTIC enforcement | Sessions marked `mode = DETERMINISTIC` reject any non-deterministic statement at parse time |
| G5 | Cross-shard atomicity | `MultiEnvelope` aggregates N sub-envelopes; all-or-nothing commit |
| G6 | Sync-friendly | Sessions serialize as event-log entries; no UPDATE conflicts on replay |
| G7 | ZK-friendly | Session envelope is canonical_ser → compatible with R1CS / PLONK / STWO circuits |

## Motivation

### 1. Why a new object?

Enterprise applications assume the session model:

```
Login → Session → Many operations → Logout
```

Existing blockchain primitives force a one-transaction-per-signature model that breaks the session abstraction. ORMs (Hibernate, SQLAlchemy, Diesel) batch N writes per session; frameworks expect to commit once per session; auditors expect a single signature per logical unit of work.

The `ExecutionEnvelope` is the architectural answer: **one signed object for N SQL operations**. Hibernate's `session.commit()` becomes `ExecutionEnvelope.commit()`. The application keeps session semantics. Consensus sees one signed envelope.

### 2. Why not just use SettlementReceipt (RFC-0959)?

RFC-0959 binds an `ask_id` (per-node pricing quote). It is shaped for marketplace consumption events. RFC-0962 binds a `capability_id` (delegated authorization token) and a `wal_segment_hash` (database-level commitment). They are different surfaces:

- RFC-0959: "Provider X spent Y resources per Ask Z."
- RFC-0962: "Capability holder C executed SQL operations S against database state D."

Both use the same envelope shape (RFC-0126 canonical_ser + BLAKE3 hash + Ed25519 signature). The fields differ.

### 3. Why explicit `mode` field?

Three session modes serve three trust levels:

| Mode | Determinism | Use case |
|---|---|---|
| `DETERMINISTIC` | Enforced (RFC-0961) | Production mutations entering consensus |
| `OFF_CHAIN` | Optional | Local-only execution; no consensus impact |
| `AUDIT_ONLY` | Enforced | Read-only sessions that produce audit trail without mutation |

The mode is a runtime gate, not a runtime check. A DETERMINISTIC session's statements are pre-validated at parse time; an OFF_CHAIN session accepts anything JDBC accepts.

## Roles and Authorities

> "Nothing should be implied" rule: every actor affecting correctness, security, accountability, or consensus MUST be named.

### Role/Authority Coverage Table

| Role | Identifier | Authority Scope | Lifecycle | Source/Ref |
|------|------------|-----------------|-----------|------------|
| Capability Holder | `DID` | Owns the session; signs the envelope | One session | RFC-0957 |
| Capability Issuer | `DID` | Minted the capability; co-signs at attenuation | Capability lifetime | RFC-0957 |
| Session Validator | Node role | Validates envelope + replay against log | Per session | RFC-0009 |
| Session Verifier (ZK) | Circuit | Verifies `EnvelopeProof` | Per session | RFC-0958 |
| Replay Defense Index | `ConsumedEnvelopeIndex` | Tracks seen `envelope_id`s per signer. Disjoint from RFC-0959's `ConsumedReceiptIndex` (which tracks `ReceiptId`s per asker). Two indexes, two different replay surfaces. | Persistent | §6.3 |
| Block Producer | Node role | Bundles sessions into block | Per block | RFC-0862 |
| Shard Router | Node role | Routes session to correct shard | Per session | RFC-0963 |

## Specification

### 4. The `ExecutionEnvelope` object

```text
ExecutionEnvelope {
    version_tag:           u8,                   // protocol version (currently 2; v2 = ExecutionEnvelope rename + mode = DETERMINISTIC)
    envelope_id:            EnvelopeID,            // BLAKE3(canonical_ser(envelope_unsigned))
    capability_id:         CapabilityID,         // RFC-0957 macaroon identifier
    capability_holder:     DID,                  // RFC-0009 DID of signer
    sql_statements:        Vec<CanonicalSQL>,    // ordered list of SQL ops
    stored_procs:          Vec<ProcInvocation>,  // CIPHERO_SQL procedure calls (RFC-0961)
    ddl_changes:           Vec<DDLOperation>,    // schema mutations (rare)
    wal_segment_hash:      Hash,                 // RFC-0862 segment commitment (BLAKE3)
    block_height:          u64,                  // block in which session commits
    timestamp_unix_ms:     u64,                  // wall-clock at session creation
    mode:                  EnvelopeMode,          // DETERMINISTIC | OFF_CHAIN | AUDIT_ONLY
    nonce:                 [u8; 32],             // replay defense (RFC-0959 SettlementEnvelope uses [u8; 16]; sessions use [u8; 32] for BLAKE3-derived uniqueness)
    zk_proof:              Option<EnvelopeProof>, // RFC-0958 circuit output
    parent_envelopes:       Vec<EnvelopeID>,       // for MultiEnvelope (cross-shard)
    metadata:              Metadata,             // optional application tags
    signature:             Ed25519Signature,     // over canonical_ser(envelope_unsigned)
}

envelope_unsigned := all fields above except `signature` and `envelope_id`
```

### 5. Canonical serialization

Per RFC-0126 Part 2 (JSON structured data):

```json
{
    "version_tag": 2,
    "capability_id": "blake3:...",
    "capability_holder": "did:cipherocto:...",
    "sql_statements": [
        {"op": "INSERT", "table": "orders", "values": [...]},
        {"op": "UPDATE", "table": "inventory", "where": "...", "set": {...}}
    ],
    "stored_procs": [
        {"proc_id": "blake3:...", "params": [...]}
    ],
    "ddl_changes": [],
    "wal_segment_hash": "blake3:...",
    "block_height": 12345,
    "timestamp_unix_ms": 1753182134000,
    "mode": "DETERMINISTIC",
    "nonce": "base64:...",
    "zk_proof": null,
    "parent_envelopes": [],
    "metadata": {}
}
```

JSON key order is alphabetical (RFC-0126 Part 2). Each value is RFC-0126 canonical encoding.

### 6. Lifecycle

#### 6.1 State machine

```text
       ┌──────────┐
       │ Created  │  (in-memory; not yet signed)
       └────┬─────┘
            │ holder signs
            ▼
       ┌──────────┐
       │  Signed  │  (signed envelope ready; not yet broadcast)
       └────┬─────┘
            │ broadcast to network
            ▼
       ┌──────────┐
       │ Pending  │  (awaiting block inclusion)
       └────┬─────┘
            │ block producer accepts
            ▼
       ┌──────────┐
       │ Included │  (in block; awaiting execution)
       └────┬─────┘
            │ all nodes replay
            ▼
       ┌──────────┐
       │ Replayed │  (executed on every node; WAL committed)
       └────┬─────┘
            │ audit window expires (if DETERMINISTIC)
            ▼
       ┌──────────┐
       │ Finalized│  (terminal; settled to ledger)
       └──────────┘

       Any state → Rejected (parse failure, replay mismatch, signature invalid)
```

#### 6.2 Replay rules

A node receiving a session for replay:

1. **Parse the JSON envelope.** Verify all fields present and canonical.
2. **Verify signature.** `verify(capability_holder_pubkey, canonical_ser(envelope_unsigned), signature)`. Reject on mismatch.
3. **Verify capability.** Look up `capability_id` in local capability store. Reject if revoked, expired, or exhausted. Reject if `capability_holder` ≠ signature signer.
   - **Revocation propagation (in-flight sessions):** revocation is checked at session creation time AND at session replay time. An in-flight session that started before revocation is allowed to complete IF the envelope's `block_height` ≤ the block containing the `CapabilityRevoked` event for that capability; otherwise the session is rejected with `E_CAPABILITY_REVOKED_POST_HOC`. This prevents a revoked capability from continuing to consume resources via pre-signed but un-replayed sessions.
4. **Verify WAL segment hash.** Recompute `BLAKE3` over local WAL segment. Reject if mismatch (node is out of sync).
   - **Block height consistency:** the `block_height` in the session envelope is the block the block producer assigned. A node replaying the session uses the envelope's `block_height` verbatim — it does **not** re-derive from local chain state. If the local chain has not yet reached that block height, the session is queued in a per-node `pending_envelopes` table and replayed once sync catches up. A session whose `block_height` is **higher than the node's current head** is never rejected for "future" content; it is just deferred.
   - **Fork detection:** if the local chain height is more than **`1000 blocks` behind the envelope's `block_height`**, the session is rejected with `E_LOCAL_CHAIN_FORKED` rather than queued indefinitely. The node's `pending_envelopes` table is drained; the operator must resolve the fork (manual sync, or re-join the network) before processing further sessions. Default `1000` blocks is configurable per deployment; smaller values catch forks faster but increase false positives during long sync windows.
5. **Verify SQL determinism (DETERMINISTIC only).** Per RFC-0961 §3.1. Reject if any statement is non-deterministic.
6. **Verify nonce uniqueness.** Check `ConsumedEnvelopeIndex[(signer, nonce)]`. Reject if seen.
7. **Apply statements.** Execute in order. Split into:
   - **Writes (INSERT/UPDATE/DELETE/MERGE):** apply each write and verify the post-statement row count + affected-row set matches the block producer's recorded `expected_post_state_hash` for that statement. Mismatch = `E_REPLAY_MISMATCH`.
   - **Reads (SELECT):** in DETERMINISTIC mode, reads are not part of the session (they cannot be deterministically replayed across nodes if they reference mutable state). In OFF_CHAIN / AUDIT_ONLY modes, read results are recorded as session metadata for later inspection but not verified during replay.
8. **Commit WAL segment.** Append the session's effect to local WAL.
9. **Update ConsumedEnvelopeIndex.** Record `(signer, nonce) → envelope_id`.

If steps 1-7 succeed on every node, the session transitions to `Replayed`. If any node fails, the session transitions to `Rejected` and a `EnvelopeRejectionEvent` is emitted (visible to the capability holder and the block producer).

#### 6.3 ConsumedEnvelopeIndex

```sql
CREATE TABLE consumed_envelopes (
    signer_did     BYTES NOT NULL,
    nonce          BYTES NOT NULL,
    envelope_id     BYTES NOT NULL,
    seen_at_unix_ms BIGINT NOT NULL,
    PRIMARY KEY (signer_did, nonce)
) WITHOUT ROWID;
```

Lookup is O(1) per replay. Index is per-node; doesn't sync across nodes (every node rebuilds its own during sync via WAL replay).

Index GC: entries older than `2 * audit_window_max` are eligible for compaction. Default audit_window_max = 30 days; default GC retention = 60 days.

### 7. MultiEnvelope — cross-shard atomicity

For sessions that touch multiple resource shards (per RFC-0963), a `MultiEnvelope` aggregates N sub-envelopes:

```text
MultiEnvelope {
    multi_envelope_id:    MultiEnvelopeID,        // BLAKE3(sorted(sub_envelope_ids))
    sub_envelopes:        Vec<ExecutionEnvelope>, // one per shard
    completion:          CompletionRule,        // AllRequired | Quorum(n) | AnyOne
    timeout_unix_ms:     u64,                   // hard deadline
    fallback_action:     FallbackAction,        // RollbackAll | CommitPartial | Abort
}
```

All-or-nothing semantics require every sub-envelope to reach `Replayed` within `timeout_unix_ms`. If timeout expires, `fallback_action` is executed (default: `Abort`).

**Reversibility requirement (R8-F3):** Sub-sessions must be designed to be safely reversible at any sub-step. The capability holder's runtime is responsible for ensuring writes are idempotent or wrapped in a transaction that can be rolled back at any intermediate state. The MultiEnvelope coordinator MAY issue an explicit "abort sub-envelope" signal that triggers a `TransferCorrected` event (per RFC-0960 §2.5) for any committed writes. Sub-sessions that do not support reversibility are rejected at MultiEnvelope construction time with `E_SUB_SESSION_NOT_REVERSIBLE`.

This is the database analog of `MultiSettlement` (RFC-0960 §7) but for SQL mutations, not value transfers.

### 7.1. MultiEnvelope recursive nesting (R8-F5)

A `MultiEnvelope` MAY carry an optional recursive child `MultiEnvelope`. The nesting chain is bounded by `MAX_NESTING_DEPTH = 4` to prevent unbounded recursion and the associated verification + storage cost.

**Field shape:**

```text
MultiEnvelope {
    multi_envelope_id:    MultiEnvelopeID,        // BLAKE3(sorted(sub_envelope_ids))
    sub_envelopes:        Vec<ExecutionEnvelope>, // one per shard (leaves)
    completion:           CompletionRule,         // AllRequired | Quorum(n) | AnyOne
    timeout_unix_ms:      u64,                    // hard deadline
    fallback_action:      FallbackAction,         // RollbackAll | CommitPartial | Abort
    nested:               Option<Box<MultiEnvelope>>, // optional recursive child (R8-F5)
}
```

`nested` is optional and `None` by default. Builders (`build_multi_envelope`, `build_multi_envelope_with`) construct with `nested = None`; callers attach a child explicitly when the use case calls for chaining across sub-shards of sub-shards.

**Recursive depth check (`check_nesting_depth`):**

The function `check_nesting_depth(multi, current_depth) -> Result<(), EnvelopeError>` walks the `nested` chain and rejects the envelope when the depth reaches `MAX_NESTING_DEPTH`. The semantics are:

- `current_depth` is the depth of the caller-supplied `multi` itself. The top-level envelope is called with `current_depth = 0`.
- Depth enumeration is by *levels*:
  - `depth = 0` — root envelope (the top-level `MultiEnvelope` passed in by the verifier).
  - `depth = 1` — `root.nested`.
  - `depth = 3` — max accepted depth (a root plus 3 nested levels).
  - `depth = 4` — rejected (would be the 4th nested level beyond the root).
- The recursion walks `multi.nested` only. `sub_envelopes` are `ExecutionEnvelope` leaves and are NOT recursed into; leaves are out of scope for the depth check.
- The depth boundary is the constant `MAX_NESTING_DEPTH = 4`. The comparison `current_depth >= MAX_NESTING_DEPTH` triggers `Err(NestingDepthExceeded(current_depth))`. With this rule, depths `0..=3` are accepted and `current_depth = 4` is rejected.
- Recursion is depth-first. On `Err`, the call chain unwinds without further descent.

**Error payload:**

```text
EnvelopeError::NestingDepthExceeded(current_depth: u8)
```

The `current_depth` field carries the depth at which the cap was reached, allowing the caller to log / surface the precise boundary violation (e.g., `current_depth = 4` indicates the 4th nested level beyond the root was reached).

**Wire format back-compat:**

`nested` carries `#[serde(default)]` on the impl, so v2.0 envelopes serialized before the field was introduced deserialize cleanly with `nested = None`. The change is additive and non-breaking.

**Why bounded:** Unbounded recursion in the `nested` chain would let an attacker construct a chain whose verification cost dominates validator throughput but whose `sub_envelopes` count is small (well under the 1000-stmt cap), bypassing the caps in §4. The fixed cap of 4 keeps worst-case verification work bounded to O(4 × per-envelope cost).

**Open conformance gap (separate gap, not R8-F5 scope):** `multi_envelope_id` derivation per RFC §7 / wire-format identifier is not yet implemented in the Rust `MultiEnvelope` struct. The current struct carries `sub_envelopes` / `completion_rule` / `completion_quorum_n` / `parent_sessions` / `timeout_unix_ms` / `fallback_action` / `nested` but no `multi_envelope_id` field. The identifier computation (`BLAKE3(sorted(sub_envelope_ids))`) and a `MultiEnvelope::multi_envelope_id()` derivation method are tracked as a separate gap. R8-F5 covers nesting depth only.

### 8. Signature aggregation

Two signature layers:

1. **Capability holder signature.** Ed25519 over `canonical_ser(envelope_unsigned)`. Mandatory. This is the **session signature**, distinct from the **capability signature** that bound the capability itself (RFC-0957 + RFC-0965 §6 `holder_signature` field). The two signatures cover different payloads: capability signature proves the holder owns the capability; session signature proves the holder authorized this specific set of SQL operations. A capability signature alone is **not sufficient** to authorize a session; the session signature is always required.
2. **Co-signer signatures (optional).** For sessions requiring multi-sig (e.g., treasury vault access), each co-signer adds an Ed25519 signature over the same `canonical_ser(envelope_unsigned)`. Threshold per capability's `MultiSig` constraint.

For sessions spanning N SQL operations, **one signature covers all N**. The session envelope is the unit of signature, not the individual statement.

### 9. ZK proof integration

For `EnvelopeProof` (RFC-0958):

```text
EnvelopeProof {
    proof_system:        ProofSystem,           // R1CS | PLONK | STWO | Groth16
    circuit_id:          CircuitID,             // e.g., "capability_constraint_satisfaction_v1"
    public_inputs:       Vec<FieldElement>,     // envelope_id, capability_id, wal_segment_hash, sql_statements_hash
    proof_bytes:         Bytes,                 // proof serialization
    verifier_key_id:     VerifierKeyID,         // RFC-0958 verifier key reference
}
```

The circuit proves: "I executed the SQL operations under the capability's constraints without revealing the operation bodies." This enables:

- **Private mutations** — operations hidden from non-participants.
- **Compliance proofs** — "I complied with policy X" without revealing the policy contents.
- **Cross-organization audit** — auditor sees proof of compliance, not data.

Verifier runs alongside signature verification in step 4 of §6.2. Proof verification cost is bounded (RFC-0958 design goal G3).

**Public input commitment:** The `public_inputs` array MUST include `sql_statements_hash = BLAKE3(0xA3 || canonical_ser(sql_statements))` in addition to `envelope_id`, `capability_id`, and `wal_segment_hash`. Without this commitment, a prover could execute a different operation set under the same `envelope_id` and produce a valid proof (soundness defect). The `0xA3` prefix is the SQL-statements-hash domain separator, distinct from the namespace tags (0x00-0x07, RFC-0964 §0 + RFC-0967 §10) and the constraint-hash separator (0xA1, RFC-0964 §5).

### 9.1. ReceiptEnvelope hash commitment (informative)

A `ReceiptEnvelope` (quota-router-core projection over `Receipt` + `CacheClassifyMeta`) is a distinct object from `ExecutionEnvelope` and MUST NOT share the `0xA3` domain separator. The `0xA3` prefix is reserved for `sql_statements_hash`; reusing it for a different object would create a soundness-defect waiting for the first verifier (a 32-byte BLAKE3 output has no type tag, and context-unaware dispatchers cannot distinguish a receipt envelope hash from a SQL-statements commitment).

The dedicated domain separator for `ReceiptEnvelope::envelope_hash` is `0xA7` (first free byte in the RFC-0964 §0.1 reserved range `0xA7-0xAF`):

```text
receipt_envelope_hash = BLAKE3(0xA7 || canonical_ser(receipt || cache_classify))
```

`canonical_ser(receipt || cache_classify)` follows the RFC-0126 deterministic encoding (length-prefixed, fixed field order, no whitespace; `axes_consumed` sorted lexicographically by axis name). The router signs the same preimage (sans the `0xA7` prefix) to bind cache-classify to the settlement; verifiers recompute from `envelope.receipt` + `envelope.cache_classify`, check `envelope_hash`, then verify the Ed25519 signature.

**Cross-reference:** RFC-0964 §0.1 domain-separator registry (allocates `0xA7`); RFC-0960 §10 (clarifies `ReceiptEnvelope` is a projection, not an `ExecutionEnvelope`).

### 10. WAL segment binding

A `ExecutionEnvelope` commits to a specific WAL segment via `wal_segment_hash`. This binds the session to a specific database state.

```text
WALSegment {
    segment_id:      SegmentID,
    block_height:    u64,
    prev_segment:    SegmentID,
    operations:      Vec<SegmentOp>,       // SQL ops applied in this segment
    post_state_hash: Hash,                 // BLAKE3 of state root after segment
    signature:       Signature,            // segment-level signature
}
```

`wal_segment_hash` = `BLAKE3(prev_segment_id || canonical_ser(segment_body))` where `segment_body` includes `block_height`, `post_state_hash`, `entries`. Matches the canonical WAL segment hash formula from RFC-0960 §1.1 — a session can only reference WAL segments that exist on every node. The block producer ensures this by including the segment commit in the same block as the session.

### 11. Error codes

| Code | Meaning | Recovery |
|---|---|---|
| `E_PARSE_FAILED` | JSON envelope not canonical | Resign with canonical form |
| `E_SIGNATURE_INVALID` | Ed25519 verification failed | Resign with correct key |
| `E_CAPABILITY_REVOKED` | Capability not in active set | Acquire new capability |
| `E_CAPABILITY_EXPIRED` | Capability past `expires_at` | Acquire new capability |
| `E_CAPABILITY_EXHAUSTED` | Capability constraint violated (e.g., spend cap) | Acquire new capability |
| `E_WAL_SEGMENT_MISMATCH` | Local WAL segment hash differs | Sync from peer |
| `E_NON_DETERMINISTIC_IN_SAFE_MODE` | Envelope with `mode = DETERMINISTIC` contains a non-deterministic statement (RFC-0961) | Rewrite statement as deterministic |
| `E_REPLAY_DETECTED` | Nonce seen in `ConsumedEnvelopeIndex` (renamed from `ConsumedSessionIndex` in v2.0) | Use new nonce |
| `E_ZK_PROOF_INVALID` | EnvelopeProof failed verification (renamed from `SessionProof` in v2.0) | Regenerate proof |
| `E_MULTI_ENVELOPE_TIMEOUT` | Sub-envelope did not reach Replayed within timeout (renamed from `E_MULTI_SESSION_TIMEOUT` in v2.0) | Fallback action |
| `E_SHARD_UNREACHABLE` | Required shard (per RFC-0963) not reachable | Retry on shard recovery |

### 12. Worked example

#### 12.1 Original Hibernate transaction

```java
Session session = sessionFactory.openSession();
session.beginTransaction();

Order order = new Order(customer, items);
session.save(order);

Inventory inv = session.get(Inventory.class, item.sku);
inv.decrement(item.quantity);
session.update(inv);

session.getTransaction().commit();
session.close();
```

#### 12.2 Translated to ExecutionEnvelope

```json
{
    "version_tag": 2,
    "capability_id": "blake3:abc123...",
    "capability_holder": "did:cipherocto:enterprise_app_42",
    "sql_statements": [
        {"op": "INSERT", "table": "orders",
         "values": {"id": "uuid-...", "customer_id": 1234, "items": [...]}},
        {"op": "UPDATE", "table": "inventory",
         "where": "sku = 'SKU-789'", "set": {"quantity": "quantity - 1"}}
    ],
    "stored_procs": [],
    "ddl_changes": [],
    "wal_segment_hash": "blake3:def456...",
    "block_height": 12345,
    "timestamp_unix_ms": 1753182134000,
    "mode": "DETERMINISTIC",
    "nonce": "base64:cGhpcyBpcyBhIG5vbmNl...",
    "zk_proof": null,
    "parent_envelopes": [],
    "metadata": {"app": "enterprise_app_42", "endpoint": "/api/orders"}
}
```

Capability holder signs the canonical JSON. Block producer includes in block. Every node replays. State changes propagate via RFC-0862.

### 13. Catalog schema

```sql
CREATE TABLE execution_envelopes (
    envelope_id           BLOB PRIMARY KEY,         -- BLAKE3 hash
    capability_id        BLOB NOT NULL,
    capability_holder    BLOB NOT NULL,            -- DID
    wal_segment_hash     BLOB NOT NULL,
    block_height         BIGINT NOT NULL,
    timestamp_unix_ms    BIGINT NOT NULL,
    mode                 TEXT NOT NULL,            -- DETERMINISTIC | OFF_CHAIN | AUDIT_ONLY
    state                TEXT NOT NULL,            -- Pending | Replayed | Finalized | Rejected
    sql_statement_count  INT NOT NULL,
    zk_proof             BLOB NULL,                -- RFC-0958 EnvelopeProof bytes
    proof_system         TEXT NULL,                -- R1CS | PLONK | STWO | Groth16
    verifier_key_id      BLOB NULL,                -- RFC-0958 verifier key reference
    signature            BLOB NOT NULL,            -- Ed25519
    metadata             BLOB NULL,                -- canonical_ser JSON
    FOREIGN KEY (capability_id) REFERENCES capabilities(capability_id)
);

CREATE INDEX ix_envelopes_holder ON execution_envelopes (capability_holder, timestamp_unix_ms);
CREATE INDEX ix_envelopes_block ON execution_envelopes (block_height);
CREATE INDEX ix_envelopes_mode ON execution_envelopes (mode, state);
CREATE INDEX ix_envelopes_proof ON execution_envelopes (proof_system) WHERE zk_proof IS NOT NULL;

CREATE TABLE multi_envelopes (
    multi_envelope_id     BLOB PRIMARY KEY,
    completion_rule      TEXT NOT NULL,            -- AllRequired | Quorum | AnyOne
    completion_quorum_n  INT NULL,                 -- threshold for Quorum rule; NULL for AllRequired/AnyOne
    timeout_unix_ms      BIGINT NOT NULL,
    fallback_action      TEXT NOT NULL,            -- RollbackAll | CommitPartial | Abort
    state               TEXT NOT NULL             -- Pending | Committed | Aborted | Partial
);

-- Pending sessions (R8-F2): sessions whose block_height is higher than
-- the local chain head. Drained on fork detection (R7-F5).
CREATE TABLE pending_envelopes (
    envelope_id           BLOB PRIMARY KEY,
    envelope             BLOB NOT NULL,            -- full serialized ExecutionEnvelope
    queued_at_unix_ms    BIGINT NOT NULL,
    target_block_height  BIGINT NOT NULL,
    reason               TEXT NOT NULL             -- 'future_block' | 'partial_sync'
);

CREATE INDEX ix_pending_envelopes_block ON pending_envelopes (target_block_height);
CREATE INDEX ix_pending_envelopes_queued ON pending_envelopes (queued_at_unix_ms);

-- Session statement expectations (R9-F3): one row per write statement in
-- the session, recording the post-state hash the block producer computed.
-- Replay nodes verify their own post-state against this expected value.
CREATE TABLE envelope_statement_expectations (
    envelope_id           BLOB NOT NULL,
    statement_index      INT NOT NULL,             -- 0-based position in sql_statements
    op_type              TEXT NOT NULL,            -- INSERT | UPDATE | DELETE | MERGE
    target_table         TEXT NOT NULL,
    expected_post_hash   BYTES NOT NULL,            -- BLAKE3 of expected post-state rows
    FOREIGN KEY (envelope_id) REFERENCES execution_envelopes(envelope_id),
    PRIMARY KEY (envelope_id, statement_index)
);

CREATE TABLE multi_envelope_members (
    multi_envelope_id     BLOB NOT NULL,
    sub_envelope_id       BLOB NOT NULL,
    shard_id             INT NOT NULL,             -- per RFC-0963
    PRIMARY KEY (multi_envelope_id, sub_envelope_id)
);
```

### 14. Sync interaction (RFC-0862)

Sessions are event-log entries. Sync propagates them as:

```text
envelope_event := {
    event_type: "EnvelopeReplayed",
    envelope_id: ...,
    block_height: ...,
    ...
}
```

RFC-0862 sync ships event batches; replay order is block_height + within-block position. No UPDATE conflicts because sessions are append-only.

Catch-up sync: a node joining mid-blockchain receives the full event log from genesis (per Phase 4 §6.2). Sessions replay deterministically; the new node converges to identical state.

## Open Questions

| # | Question | Resolution Target |
|---|----------|-------------------|
| 1 | What is the maximum session size (SQL statement count, total bytes)? | Operational tuning; default 1000 statements, 1 MB envelope |
| 2 | How is `wal_segment_hash` bound at session creation if the WAL is still being written? | Two-phase: tentative hash at sign time, final hash at commit; rejected if mismatch |
| 3 | Can OFF_CHAIN sessions transition to DETERMINISTIC? | No — mode is fixed at session creation |
| 4 | How does audit window interact with MultiEnvelope? | Each sub-envelope has its own audit window; MultiEnvelope finalizes when all sub-envelopes finalize |
| 5 | What if a node is offline during a MultiEnvelope timeout? | Node catches up via RFC-0862 sync; MultiEnvelope retries until quorum |
| 6 | Can ZK proof be mandatory for some session modes? | Yes — capability may carry `RequireProof` constraint (RFC-0965) |

## Resolved Decisions (v2.1-Resolved)

All Open Questions resolved with concrete answers (R28+ risk-closure round):

| # | Question | Resolution | Status |
|---|----------|------------|--------|
| 1 | Max session size | **1000 statements / 1MB default; 10000 / 10MB hard cap; beyond = MultiEnvelope.** STWO proof gen ~O(n²) on circuit size; 10000 ≈ 4M constraints ≈ 30s proof time on 32-core node. Caps chosen so per-envelope ZK proof stays under interactive response budget. | Resolved |
| 2 | WAL hash binding at sign-time vs commit-time | **Two-phase commit.** Tentative hash at sign: `wal_segment_hash_tentative = BLAKE3(0xA3 \|\| canonical_ser(envelope_unsigned_with_placeholder))`. Final hash at commit: `wal_segment_hash = BLAKE3(prev_segment_id \|\| canonical_ser(segment_body))`. Mismatch → reject with new error code `E_WAL_HASH_MISMATCH`. See §6.4. | Resolved |
| 3 | OFF_CHAIN → DETERMINISTIC transition | **No.** Mode is fixed at session creation (per §4 envelope struct: `mode` is non-mutable field). Re-issue session under new mode if needed. | Resolved |
| 4 | MultiEnvelope audit window interaction | **Per-sub-envelope independence.** Each sub-envelope has its own audit window starting at its respective settlement landing. MultiEnvelope finalizes only when all sub-envelopes finalize. Cross-shard disputes are filed per sub-envelope. Settle latency = max(per-shard audit window). | Resolved |
| 5 | Offline node during MultiEnvelope timeout | **RFC-0862 sync catch-up.** MultiEnvelope retries until quorum reached OR `timeout_unix_ms` expires. After timeout, MultiEnvelope aborts and resources are released. Offline node catches up on reconnect via event log replay (RFC-0862 §event-replay). | Resolved |
| 6 | ZK proof mandatory for some modes | **Yes via `RequireProof` constraint (RFC-0965).** Capability verifier rejects envelope if `mode = DETERMINISTIC` and `RequireProof` is set but `envelope_proof` field is missing. `EnvelopeProof` envelope (RFC-0958) carries the public-input commitment + circuit. | Resolved |

### §6.4 WAL hash two-phase binding

ExecutionEnvelopes commit SQL operations to a Deterministic WAL segment whose hash is bound into the envelope. Because the WAL is being written concurrently with envelope signing, a two-phase protocol binds the segment hash:

**Sign-time (tentative).** The signer computes:

```text
envelope_unsigned_tentative := envelope_unsigned {
    wal_segment_hash: BLAKE3(0xA3 || canonical_ser(envelope_unsigned_placeholder)),
    ...
}
envelope_id := BLAKE3(0xA3 || canonical_ser(envelope_unsigned_tentative))
signature := Ed25519.sign(signer_sk, canonical_ser(envelope_unsigned_tentative))
```

The placeholder WAL hash is `BLAKE3(0x00 || canonical_ser(envelope_body_predicate))` — deterministic given the SQL operations, but does NOT commit to a specific segment position in the WAL chain. This lets the signer create an envelope_id before the WAL segment is committed.

**Commit-time (final).** When the WAL segment containing the envelope's entries is appended to the chain:

```text
wal_segment_hash_final := BLAKE3(prev_segment_id || canonical_ser(segment_body))
envelope_final := envelope_unsigned_tentative { wal_segment_hash: wal_segment_hash_final }
```

The validator recomputes `envelope_final_id = BLAKE3(0xA3 || canonical_ser(envelope_final))` and rejects if it does not match `envelope_id` (signing identity preserved) **OR** if the tentative→final hash transition is not present in the WAL at the cited segment.

**Error codes (added to §11):**

- `E_WAL_HASH_MISMATCH` — `envelope_final_id ≠ envelope_id` after tentative→final transition.
- `E_WAL_SEGMENT_MISSING` — `wal_segment_hash_final` not found in WAL chain at validator's height.
- `E_WAL_OUT_OF_ORDER` — referenced segment is not yet committed (validator height < segment height).

## Cross-reference (RFC-0960 chain-aware bump, 2026-08-17)

RFC-0960 amended §2.1 root-vault example (removed; replaced with §20.3 lattice note) + added §Vault Substrate subsection. ExecutionEnvelope signature aggregation + ZK commitment + reconciliation semantics (§6) are unaffected — chain_id is now part of the vault_id composite PK + transfer_event PK (`(chain_id, event_id)`), but the envelope hashes the underlying operation set rather than the substrate row, so the §6 envelope semantics remain identical. No change to mode, version_tag, or signature aggregation required.

## Out of Scope

- **Session recovery.** If a capability holder's node crashes mid-session, the session is abandoned. Application layer handles retry.
- **Cross-chain session atomicity.** MultiEnvelope is intra-chain. Cross-chain uses RFC-0960 §7 MultiSettlement (separate RFC).
- **Session migration.** Sessions are immutable. Re-running requires a new session.
- **Session-level access control beyond capability constraints.** Capability constraints are exhaustive (per RFC-0965).

## Status

This RFC = ExecutionEnvelope object protocol (v2.0 strategic reframe). Status: **Accepted v2.0** (promoted from Draft on 2026-07-23 in lockstep with RFC-0960, RFC-0961, RFC-0963, RFC-0964, RFC-0965, and RFC-0967).

All companion RFCs (0961 / 0963 / 0964 / 0965 / 0967) reached Accepted in lockstep on 2026-07-23.

The `cipherocto-execution-envelope` crate implements:
- `ExecutionEnvelope::sign()` — capability holder signs canonical envelope
- `ExecutionEnvelope::verify()` — node replay-time verification
- `MultiEnvelope::coordinate()` — cross-shard coordination
- `ConsumedEnvelopeIndex` — replay defense
- JDBC driver integration (`Connection.commit()` → `ExecutionEnvelope::sign()`)
- WAL segment binding (RFC-0960 §1.1)
