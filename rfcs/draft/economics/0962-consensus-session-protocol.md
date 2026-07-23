# RFC-0962 (Economics): ConsensusSession — Object Protocol

## Status

Draft

> **Note:** Companion RFC to RFC-0960 §12.9 (Consensus Session object). Defines the wire-protocol shape, lifecycle states, signature aggregation, ZK commitment, and reconciliation semantics of a `ConsensusSession`. Builds on RFC-0959 (SettlementReceipt envelope), RFC-0957 (Capability), RFC-0862 (sync as propagation), and RFC-0961 (CIPHERO_SQL deterministic procedure language).

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-22 | @cipherocto + @mmacedoeu | Initial draft. |

## Authors

- Author: @cipherocto (grand-design Consensus Session work)
- Contributor: @mmacedoeu (RFC-0962 protocol extraction)

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @cipherocto

## Summary

A `ConsensusSession` is the unit of consensus-level mutation in CipherOcto. One session bundles N SQL operations (or CIPHERO_SQL procedure invocations) into a single signed, hash-committed, deterministically-replayable block. Externally the application sees a JDBC transaction; internally the consensus layer sees one signed object.

Three artifacts:

1. **`ConsensusSession`** — content-addressable signed envelope binding capability holder, WAL segment hash, SQL statement list, and timestamp. Hash = `BLAKE3(version_tag || canonical_ser(session_unsigned))`.
2. **`SessionCommitment`** — consensus-layer commitment that mirrors the RFC-0959 `SettlementReceipt` *envelope shape* (canonical_ser + BLAKE3 hash + Ed25519 signature) but binds a `session_id` instead of an `ask_id`. **Not** a `SettlementReceipt`; the two objects are disjoint. Replay defense via `ConsumedSessionIndex` (§6.3).
3. **`SessionProof`** — optional ZK proof that the session's SQL operations were executed under the capability's constraints without revealing the operation bodies (per RFC-0958).

Coexists with RFC-0959. RFC-0959 governs per-node Ask pricing; RFC-0962 governs multi-statement transactions under capabilities. Both use the same BLAKE3 envelope shape but bind different objects.

## Dependencies

### Required RFCs

| RFC | Status | Reason |
|-----|--------|--------|
| RFC-0960 | Draft (companion) | Defines §12 Consensus Sessions architecture |
| RFC-0961 | Draft (companion) | CIPHERO_SQL deterministic procedure language |
| RFC-0959 | Accepted (v1.0, 2026-07-20) | SettlementReceipt envelope shape; same canonical_ser pattern |
| RFC-0957 | Draft | Capability binding (capability_holder field) |
| RFC-0958 | Draft | ZK capability subclass for `SessionProof` |
| RFC-0862 | Accepted (v1.2.0) | Sync as propagation; sessions ship as event batches |
| RFC-0126 | Accepted (v2.5.1) | Canonical serialization for session envelope |
| RFC-0102 | Accepted | Wallet cryptography (Ed25519 substrate for session signature) |
| RFC-0009 | Draft | Node identity for signature verification |
| RFC-0853 | Draft | BLAKE3 primitive source |

### Companion RFCs (Planned)

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0963 | Builds on | Resource shard routing; cross-shard sessions use `MultiSession` |
| RFC-0964 | Builds on | Constraint encoding for capability constraint evaluation |
| RFC-0965 | Builds on | Capability extension format (caveat types referenced by `capability_holder`) |

### Dependency Validation

Standalone, top-level section to satisfy BLUEPRINT v1.3 mandatory section set.

| Dependency | Type | Current Status (2026-07-22) | Assumed Before Accept? | Hard-block on RFC-0962 acceptance? |
|------------|------|------------------------------|------------------------|-------------------------------------|
| RFC-0960 | Requires | Draft (companion) | Yes | YES |
| RFC-0961 | Requires | Draft (companion) | Yes | YES |
| RFC-0959 | Requires | Accepted | Already | No |
| RFC-0957 | Requires | Draft | Yes | YES |
| RFC-0958 | Requires | Draft | Yes | YES |
| RFC-0862 | Requires | Accepted | Already | No |
| RFC-0126 | Requires | Accepted | Already | No |
| RFC-0102 | Requires | Accepted | Already | No |
| RFC-0009 | Requires | Draft | Yes | YES |
| RFC-0853 | Requires | Draft | Yes | YES |

**DAG check:** `0962 ← {0960, 0961, 0959, 0957, 0958, 0862, 0126, 0102, 0009, 0853}` — acyclic. No back-edges to RFC-0962.

**Implicit Assumptions Audit:**
- IA-1: RFC-0957 reaches Accepted with caveat DSL stable enough for `capability_holder` binding.
- IA-2: RFC-0958 ZK circuit accepts session-style commitments (not just ask-style).
- IA-3: RFC-0009 node identity provides DID format compatible with `signed_by` field.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Session hash deterministic across implementations | Two nodes replaying same `(capability_id, sql_statements, timestamp)` produce identical 32-byte hash |
| G2 | One signature per session | N SQL operations → 1 Ed25519 signature → 1 ZK proof (optional) |
| G3 | Replay defense | Same `(session_unsigned, signed_by)` from same signer yields distinct `session_id` via monotonic counter + nonce |
| G4 | CONSENSUS_SAFE enforcement | Sessions marked `mode = CONSENSUS_SAFE` reject any non-deterministic statement at parse time |
| G5 | Cross-shard atomicity | `MultiSession` aggregates N sub-sessions; all-or-nothing commit |
| G6 | Sync-friendly | Sessions serialize as event-log entries; no UPDATE conflicts on replay |
| G7 | ZK-friendly | Session envelope is canonical_ser → compatible with R1CS / PLONK / STWO circuits |

## Motivation

### 1. Why a new object?

Enterprise applications assume the session model:

```
Login → Session → Many operations → Logout
```

Existing blockchain primitives force a one-transaction-per-signature model that breaks the session abstraction. ORMs (Hibernate, SQLAlchemy, Diesel) batch N writes per session; frameworks expect to commit once per session; auditors expect a single signature per logical unit of work.

The `ConsensusSession` is the architectural answer: **one signed object for N SQL operations**. Hibernate's `session.commit()` becomes `ConsensusSession.commit()`. The application keeps session semantics. Consensus sees one signed envelope.

### 2. Why not just use SettlementReceipt (RFC-0959)?

RFC-0959 binds an `ask_id` (per-node pricing quote). It is shaped for marketplace consumption events. RFC-0962 binds a `capability_id` (delegated authorization token) and a `wal_segment_hash` (database-level commitment). They are different surfaces:

- RFC-0959: "Provider X spent Y resources per Ask Z."
- RFC-0962: "Capability holder C executed SQL operations S against database state D."

Both use the same envelope shape (RFC-0126 canonical_ser + BLAKE3 hash + Ed25519 signature). The fields differ.

### 3. Why explicit `mode` field?

Three session modes serve three trust levels:

| Mode | Determinism | Use case |
|---|---|---|
| `CONSENSUS_SAFE` | Enforced (RFC-0961) | Production mutations entering consensus |
| `OFF_CHAIN_SAFE` | Optional | Local-only execution; no consensus impact |
| `AUDIT_ONLY` | Enforced | Read-only sessions that produce audit trail without mutation |

The mode is a runtime gate, not a runtime check. A CONSENSUS_SAFE session's statements are pre-validated at parse time; an OFF_CHAIN_SAFE session accepts anything JDBC accepts.

## Roles and Authorities

> "Nothing should be implied" rule: every actor affecting correctness, security, accountability, or consensus MUST be named.

### Role/Authority Coverage Table

| Role | Identifier | Authority Scope | Lifecycle | Source/Ref |
|------|------------|-----------------|-----------|------------|
| Capability Holder | `DID` | Owns the session; signs the envelope | One session | RFC-0957 |
| Capability Issuer | `DID` | Minted the capability; co-signs at attenuation | Capability lifetime | RFC-0957 |
| Session Validator | Node role | Validates envelope + replay against log | Per session | RFC-0009 |
| Session Verifier (ZK) | Circuit | Verifies `SessionProof` | Per session | RFC-0958 |
| Replay Defense Index | `ConsumedSessionIndex` | Tracks seen `session_id`s per signer. Disjoint from RFC-0959's `ConsumedReceiptIndex` (which tracks `ReceiptId`s per asker). Two indexes, two different replay surfaces. | Persistent | §6.3 |
| Block Producer | Node role | Bundles sessions into block | Per block | RFC-0862 |
| Shard Router | Node role | Routes session to correct shard | Per session | RFC-0963 |

## Specification

### 4. The `ConsensusSession` object

```text
ConsensusSession {
    version_tag:           u8,                   // protocol version (currently 1)
    session_id:            SessionID,            // BLAKE3(canonical_ser(session_unsigned))
    capability_id:         CapabilityID,         // RFC-0957 macaroon identifier
    capability_holder:     DID,                  // RFC-0009 DID of signer
    sql_statements:        Vec<CanonicalSQL>,    // ordered list of SQL ops
    stored_procs:          Vec<ProcInvocation>,  // CIPHERO_SQL procedure calls (RFC-0961)
    ddl_changes:           Vec<DDLOperation>,    // schema mutations (rare)
    wal_segment_hash:      Hash,                 // RFC-0862 segment commitment (BLAKE3)
    block_height:          u64,                  // block in which session commits
    timestamp_unix_ms:     u64,                  // wall-clock at session creation
    mode:                  SessionMode,          // CONSENSUS_SAFE | OFF_CHAIN_SAFE | AUDIT_ONLY
    nonce:                 [u8; 32],             // replay defense (RFC-0959 SettlementEnvelope uses [u8; 16]; sessions use [u8; 32] for BLAKE3-derived uniqueness)
    zk_proof:              Option<SessionProof>, // RFC-0958 circuit output
    parent_sessions:       Vec<SessionID>,       // for MultiSession (cross-shard)
    metadata:              Metadata,             // optional application tags
    signature:             Ed25519Signature,     // over canonical_ser(session_unsigned)
}

session_unsigned := all fields above except `signature` and `session_id`
```

### 5. Canonical serialization

Per RFC-0126 Part 2 (JSON structured data):

```json
{
    "version_tag": 1,
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
    "mode": "CONSENSUS_SAFE",
    "nonce": "base64:...",
    "zk_proof": null,
    "parent_sessions": [],
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
            │ audit window expires (if CONSENSUS_SAFE)
            ▼
       ┌──────────┐
       │ Finalized│  (terminal; settled to ledger)
       └──────────┘

       Any state → Rejected (parse failure, replay mismatch, signature invalid)
```

#### 6.2 Replay rules

A node receiving a session for replay:

1. **Parse the JSON envelope.** Verify all fields present and canonical.
2. **Verify signature.** `verify(capability_holder_pubkey, canonical_ser(session_unsigned), signature)`. Reject on mismatch.
3. **Verify capability.** Look up `capability_id` in local capability store. Reject if revoked, expired, or exhausted. Reject if `capability_holder` ≠ signature signer.
4. **Verify WAL segment hash.** Recompute `BLAKE3` over local WAL segment. Reject if mismatch (node is out of sync).
5. **Verify SQL determinism (CONSENSUS_SAFE only).** Per RFC-0961 §3.1. Reject if any statement is non-deterministic.
6. **Verify nonce uniqueness.** Check `ConsumedSessionIndex[(signer, nonce)]`. Reject if seen.
7. **Apply statements.** Execute in order. Split into:
   - **Writes (INSERT/UPDATE/DELETE/MERGE):** apply each write and verify the post-statement row count + affected-row set matches the block producer's recorded `expected_post_state_hash` for that statement. Mismatch = `E_REPLAY_MISMATCH`.
   - **Reads (SELECT):** in CONSENSUS_SAFE mode, reads are not part of the session (they cannot be deterministically replayed across nodes if they reference mutable state). In OFF_CHAIN_SAFE / AUDIT_ONLY modes, read results are recorded as session metadata for later inspection but not verified during replay.
8. **Commit WAL segment.** Append the session's effect to local WAL.
9. **Update ConsumedSessionIndex.** Record `(signer, nonce) → session_id`.

If steps 1-7 succeed on every node, the session transitions to `Replayed`. If any node fails, the session transitions to `Rejected` and a `SessionRejectionEvent` is emitted (visible to the capability holder and the block producer).

#### 6.3 ConsumedSessionIndex

```sql
CREATE TABLE consumed_sessions (
    signer_did     BYTES NOT NULL,
    nonce          BYTES NOT NULL,
    session_id     BYTES NOT NULL,
    seen_at_unix_ms BIGINT NOT NULL,
    PRIMARY KEY (signer_did, nonce)
) WITHOUT ROWID;
```

Lookup is O(1) per replay. Index is per-node; doesn't sync across nodes (every node rebuilds its own during sync via WAL replay).

Index GC: entries older than `2 * audit_window_max` are eligible for compaction. Default audit_window_max = 30 days; default GC retention = 60 days.

### 7. MultiSession — cross-shard atomicity

For sessions that touch multiple resource shards (per RFC-0963), a `MultiSession` aggregates N sub-sessions:

```text
MultiSession {
    multi_session_id:    MultiSessionID,        // BLAKE3(sorted(sub_session_ids))
    sub_sessions:        Vec<ConsensusSession>, // one per shard
    completion:          CompletionRule,        // AllRequired | Quorum(n) | AnyOne
    timeout_unix_ms:     u64,                   // hard deadline
    fallback_action:     FallbackAction,        // RollbackAll | CommitPartial | Abort
}
```

All-or-nothing semantics require every sub-session to reach `Replayed` within `timeout_unix_ms`. If timeout expires, `fallback_action` is executed (default: `Abort`).

This is the database analog of `MultiSettlement` (RFC-0960 §7) but for SQL mutations, not value transfers.

### 8. Signature aggregation

Two signature layers:

1. **Capability holder signature.** Ed25519 over `canonical_ser(session_unsigned)`. Mandatory. This is the **session signature**, distinct from the **capability signature** that bound the capability itself (RFC-0957 + RFC-0965 §6 `holder_signature` field). The two signatures cover different payloads: capability signature proves the holder owns the capability; session signature proves the holder authorized this specific set of SQL operations. A capability signature alone is **not sufficient** to authorize a session; the session signature is always required.
2. **Co-signer signatures (optional).** For sessions requiring multi-sig (e.g., treasury vault access), each co-signer adds an Ed25519 signature over the same `canonical_ser(session_unsigned)`. Threshold per capability's `MultiSig` constraint.

For sessions spanning N SQL operations, **one signature covers all N**. The session envelope is the unit of signature, not the individual statement.

### 9. ZK proof integration

For `SessionProof` (RFC-0958):

```text
SessionProof {
    proof_system:        ProofSystem,           // R1CS | PLONK | STWO | Groth16
    circuit_id:          CircuitID,             // e.g., "capability_constraint_satisfaction_v1"
    public_inputs:       Vec<FieldElement>,     // session_id, capability_id, wal_segment_hash
    proof_bytes:         Bytes,                 // proof serialization
    verifier_key_id:     VerifierKeyID,         // RFC-0958 verifier key reference
}
```

The circuit proves: "I executed the SQL operations under the capability's constraints without revealing the operation bodies." This enables:

- **Private mutations** — operations hidden from non-participants.
- **Compliance proofs** — "I complied with policy X" without revealing the policy contents.
- **Cross-organization audit** — auditor sees proof of compliance, not data.

Verifier runs alongside signature verification in step 4 of §6.2. Proof verification cost is bounded (RFC-0958 design goal G3).

### 10. WAL segment binding

A `ConsensusSession` commits to a specific WAL segment via `wal_segment_hash`. This binds the session to a specific database state.

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

`wal_segment_hash` = `BLAKE3(segment_id || block_height || post_state_hash)`. A session can only reference WAL segments that exist on every node. The block producer ensures this by including the segment commit in the same block as the session.

### 11. Error codes

| Code | Meaning | Recovery |
|---|---|---|
| `E_PARSE_FAILED` | JSON envelope not canonical | Resign with canonical form |
| `E_SIGNATURE_INVALID` | Ed25519 verification failed | Resign with correct key |
| `E_CAPABILITY_REVOKED` | Capability not in active set | Acquire new capability |
| `E_CAPABILITY_EXPIRED` | Capability past `expires_at` | Acquire new capability |
| `E_CAPABILITY_EXHAUSTED` | Capability constraint violated (e.g., spend cap) | Acquire new capability |
| `E_WAL_SEGMENT_MISMATCH` | Local WAL segment hash differs | Sync from peer |
| `E_NON_DETERMINISTIC_IN_SAFE_MODE` | CONSENSUS_SAFE session contains non-deterministic op (RFC-0961) | Rewrite as deterministic |
| `E_REPLAY_DETECTED` | Nonce seen in `ConsumedSessionIndex` | Use new nonce |
| `E_ZK_PROOF_INVALID` | SessionProof failed verification | Regenerate proof |
| `E_MULTI_SESSION_TIMEOUT` | Sub-session did not reach Replayed within timeout | Fallback action |
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

#### 12.2 Translated to ConsensusSession

```json
{
    "version_tag": 1,
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
    "mode": "CONSENSUS_SAFE",
    "nonce": "base64:cGhpcyBpcyBhIG5vbmNl...",
    "zk_proof": null,
    "parent_sessions": [],
    "metadata": {"app": "enterprise_app_42", "endpoint": "/api/orders"}
}
```

Capability holder signs the canonical JSON. Block producer includes in block. Every node replays. State changes propagate via RFC-0862.

### 13. Catalog schema

```sql
CREATE TABLE consensus_sessions (
    session_id           BLOB PRIMARY KEY,         -- BLAKE3 hash
    capability_id        BLOB NOT NULL,
    capability_holder    BLOB NOT NULL,            -- DID
    wal_segment_hash     BLOB NOT NULL,
    block_height         BIGINT NOT NULL,
    timestamp_unix_ms    BIGINT NOT NULL,
    mode                 TEXT NOT NULL,            -- CONSENSUS_SAFE | OFF_CHAIN_SAFE | AUDIT_ONLY
    state                TEXT NOT NULL,            -- Pending | Replayed | Finalized | Rejected
    sql_statement_count  INT NOT NULL,
    has_zk_proof         BOOLEAN NOT NULL,
    signature            BLOB NOT NULL,            -- Ed25519
    metadata             BLOB NULL,                -- canonical_ser JSON
    FOREIGN KEY (capability_id) REFERENCES capabilities(capability_id)
);

CREATE INDEX ix_sessions_holder ON consensus_sessions (capability_holder, timestamp_unix_ms);
CREATE INDEX ix_sessions_block ON consensus_sessions (block_height);
CREATE INDEX ix_sessions_mode ON consensus_sessions (mode, state);

CREATE TABLE multi_sessions (
    multi_session_id     BLOB PRIMARY KEY,
    completion_rule      TEXT NOT NULL,            -- AllRequired | Quorum | AnyOne
    timeout_unix_ms      BIGINT NOT NULL,
    fallback_action      TEXT NOT NULL,            -- RollbackAll | CommitPartial | Abort
    state               TEXT NOT NULL             -- Pending | Committed | Aborted | Partial
);

CREATE TABLE multi_session_members (
    multi_session_id     BLOB NOT NULL,
    sub_session_id       BLOB NOT NULL,
    shard_id             INT NOT NULL,             -- per RFC-0963
    PRIMARY KEY (multi_session_id, sub_session_id)
);
```

### 14. Sync interaction (RFC-0862)

Sessions are event-log entries. Sync propagates them as:

```text
session_event := {
    event_type: "SessionReplayed",
    session_id: ...,
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
| 3 | Can OFF_CHAIN_SAFE sessions transition to CONSENSUS_SAFE? | No — mode is fixed at session creation |
| 4 | How does audit window interact with MultiSession? | Each sub-session has its own audit window; MultiSession finalizes when all sub-sessions finalize |
| 5 | What if a node is offline during a MultiSession timeout? | Node catches up via RFC-0862 sync; MultiSession retries until quorum |
| 6 | Can ZK proof be mandatory for some session modes? | Yes — capability may carry `RequireProof` constraint (RFC-0965) |

## Out of Scope

- **Session recovery.** If a capability holder's node crashes mid-session, the session is abandoned. Application layer handles retry.
- **Cross-chain session atomicity.** MultiSession is intra-chain. Cross-chain uses RFC-0960 §7 MultiSettlement (separate RFC).
- **Session migration.** Sessions are immutable. Re-running requires a new session.
- **Session-level access control beyond capability constraints.** Capability constraints are exhaustive (per RFC-0965).

## Status

This RFC = ConsensusSession object protocol. Status: Draft. Companion RFCs 0961, 0963, 0964, 0965 in flight. Awaiting review and promotion to Accepted.

Once Accepted, the `cipherocto-consensus-session` crate implements:
- `ConsensusSession::sign()` — capability holder signs canonical envelope
- `ConsensusSession::verify()` — node replay-time verification
- `MultiSession::coordinate()` — cross-shard coordination
- `ConsumedSessionIndex` — replay defense
- JDBC driver integration (`Connection.commit()` → `ConsensusSession::sign()`)
