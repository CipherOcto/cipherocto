# Research: Reputation Persistence

**Date:** 2026-07-24
**Status:** Viable → Use Case → RFC Draft Created
**Author:** @cipherocto + @mmacedoeu
**Scope:** Feasibility of persisted, DID-keyed, extensible, cryptographically-signed reputation infrastructure for CipherOcto

---

## Executive Summary

Three independent reputation primitives exist today, all **in-memory**, **DID-keyed**, and **without canonical encoding or write authority**:

1. `SlashReputationStore` (octo-network mon) — `coordinator_pubkey -> Vec<SlashEvent>`, cross-mission aggregation.
2. `DcRootedSlashReputationStore` (octo-network dc) — `dc_pubkey -> Vec<DcSlashEvent>`, cross-domain aggregation.
3. `ProviderReputationRegistry` (quota-router-core marketplace) — `provider_id -> ProviderScore`, EWMA + latency (Gap 7).

Gap 5 review noted: **EWMA scores live only in memory; a daemon restart loses history.** Mission 0855p-b is open for cross-mission gossip but lacks storage.

This research confirms feasibility of a unified, persisted, DID-keyed reputation registry with extensible signal types, canonical `did:octo:b<52>` encoding (62 chars total), and cryptographic recorder write authority. **Recommendation: proceed to Use Case → RFC Draft (already created).**

---

## Problem Statement

CipherOcto mixes three reputation concepts in three places:

- **Slash reputation** (network layer) — used for coordinator/DC priority.
- **Provider reputation** (marketplace layer) — used for ask ranking + circuit-breaker.
- **Outcome reputation** (planned) — for inference-task, retrieval, proof markets.

All share structural similarity (DID → score + history) but use distinct in-memory types. Result:

- Daemon restart loses all reputation.
- No cross-layer reputation (e.g., a coordinator with high slash count can't be deprioritized by marketplace).
- No gossip replication (mission 0855p-b is open).
- No on-chain anchoring (RFC-0955 §`reputation` follow-up scope — extends existing `u64` or adds new `blake3_digest` field — RFC-0955 amendment required).
- Noncanonical DID encodings accepted (raw 32-byte + `did:octo:` string), enabling reputation laundering.

## Research Scope

**Included:**

- Storage backend candidates (stoolap, kvstore, in-memory).
- Canonical DID encoding (`did:octo:b<52>` only — 62 chars total).
- DID-keyed schema design with composite `(did, signal_kind, layer)` PK.
- Extensibility for scored signal types (SlashEvent, Outcome, Latency, Capacity, Discovery); Rotation is identity-migration metadata persisted separately and excluded from weights.
- Cross-layer reputation federation with per-kind normalizers.
- Recorder write authority via ed25519 + stake + state machine.
- Determinism class (per RFC-0008; RFC-0104 DFP upgrade path).
- Adapter mapping rules + equivalence testing strategy.

**Excluded:**

- On-chain anchoring (separate gap, mission 0968a per RFC-0955; stub: `missions/deferred/0968a-reputation-anchoring.md`).
- Reputation tokenization; reputation remains a derived signal, not a token or balance.
- Reputation weighting algorithms beyond per-kind normalizers.

## Findings

### Storage Backend

| Backend                          | Pros                                                                                     | Cons                        | Verdict          |
| -------------------------------- | ---------------------------------------------------------------------------------------- | --------------------------- | ---------------- |
| **stoolap** (existing repo fork) | MVCC, vector SQL, already wired (`crates/quota-router-storage`), CipherOcto fork mandate | Heavier than KV             | **Recommended**  |
| sled                             | Fast embedded KV                                                                         | Not MVCC, no SQL, no vector | Rejected         |
| redb                             | Pure Rust, ACID                                                                          | No SQL extensions           | Rejected for now |
| In-memory + periodic snapshot    | Fast                                                                                     | Loss window on crash        | Insufficient     |

stoolap wins on three counts: (a) MVCC gives snapshot reads for cross-mission aggregation, (b) vector SQL enables future semantic search over reputation-tiered providers, (c) the storage layer already exists with `BUILTIN_MIGRATIONS` runner. The MVCC guarantee is the load-bearing contract for `record_signal`'s atomic event + aggregate + recorder activity update; see `feedback_stoolap-persistence.md` (Round 6 L7) for the fork proof and the rationale for the fork-vs-upstream decision.

### Signal Type Schema (Round 1 finding M1: composite PK + two tables)

Three signal types share structure but differ in event payload. **Recommended design (revised per Round 1): two tables with composite `(did, signal_kind, layer)` PK** — single-table-per-DID alternative rejected because it loses the ability to evolve the aggregate shape independently of the event log.

**Event log table (append-only, audit-friendly):**

```sql
CREATE TABLE reputation_events (
  event_id BLOB PRIMARY KEY,                  -- BLAKE3 of unsigned canonical
  did TEXT NOT NULL,                          -- canonical did:octo:b<52>
  signal_kind INTEGER NOT NULL,
  layer INTEGER NOT NULL,
  -- v3.0-r15 (Gap 9): score_delta is octo_determin::Dfp 24-byte encoding
  -- (RFC-0104). BLOB length enforced via CHECK.
  score_delta BLOB NOT NULL CHECK (length(score_delta) = 24),
  samples_delta INTEGER NOT NULL,
  severity INTEGER NOT NULL DEFAULT 0,
  payload BLOB,
  source_did TEXT NOT NULL,                   -- recorder
  observed_at_unix INTEGER NOT NULL,
  received_at_unix INTEGER NOT NULL,          -- monotonic per source_did
  retention_pruned_at_unix INTEGER,           -- authenticated soft-prune marker
  signature BLOB NOT NULL                     -- ed25519 (64 bytes)
);
```

**Aggregate table (read-optimized, derived):**

```sql
CREATE TABLE reputation_aggregates (
  did TEXT NOT NULL,
  signal_kind INTEGER NOT NULL,
  layer INTEGER NOT NULL,
  -- v3.0-r15 (Gap 9): score_ewma is octo_determin::Dfp 24-byte encoding
  -- (default = Dfp::from_f64(1.0) serialized). RFC-0104.
  score_ewma BLOB NOT NULL CHECK (length(score_ewma) = 24),
  samples INTEGER NOT NULL DEFAULT 0,
  severity_total INTEGER NOT NULL DEFAULT 0,
  last_event_id BLOB NOT NULL,
  last_event_unix INTEGER NOT NULL,
  updated_at_unix INTEGER NOT NULL,
  PRIMARY KEY (did, signal_kind, layer)
);
```

The canonical `ReputationAggregate` has exactly nine fields: `did`, `kind`, `layer`, `score_ewma`, `samples`, `severity_total`, `last_event_id`, `last_event_unix`, and `updated_at_unix`.

**Pruned-prefix checkpoint table (Phase 1 v007):**

```sql
CREATE TABLE aggregate_checkpoint (
  did TEXT NOT NULL,
  signal_kind INTEGER NOT NULL,
  layer INTEGER NOT NULL,
  checkpoint_id BLOB NOT NULL,
  checkpoint_event_id BLOB NOT NULL,
  checkpoint_unix INTEGER NOT NULL,
  -- v3.0-r15 (Gap 9): score_ewma_at_checkpoint is octo_determin::Dfp
  -- 24-byte encoding (RFC-0104).
  score_ewma_at_checkpoint BLOB NOT NULL CHECK (length(score_ewma_at_checkpoint) = 24),
  samples_at_checkpoint INTEGER NOT NULL,
  severity_total_at_checkpoint INTEGER NOT NULL,
  PRIMARY KEY (did, signal_kind, layer, checkpoint_id)
);
```

`prune_event` writes the checkpoint and prune marker atomically. Audit replay reconstructs each aggregate from the latest applicable `aggregate_checkpoint` plus retained events after `checkpoint_event_id`; the mutable current aggregate is not used as a substitute for a missing event prefix.

**Attestation table (Phase 1 v006):**

```sql
CREATE TABLE reputation_attestations (
  attestation_id BLOB PRIMARY KEY,
  attestor_did TEXT NOT NULL,
  event_id BLOB NOT NULL,
  signature BLOB NOT NULL,
  observed_at_unix INTEGER NOT NULL,
  received_at_unix INTEGER NOT NULL,
  FOREIGN KEY (event_id) REFERENCES reputation_events(event_id)
);
CREATE INDEX reputation_attestations_by_event ON reputation_attestations(event_id);
CREATE INDEX reputation_attestations_by_attestor ON reputation_attestations(attestor_did);
```

Attestations land with core persistence rather than deferred federation. `record_attestation` verifies `attestation_id = BLAKE3(BLAKE3_REPUTATION_ATTESTATION_DOMAIN || attestor_did || event_id)` before inserting. `AttestationId` is an attestation identifier and occupies a namespace distinct from `EventId`; the two identifiers are never interchangeable.

The two-table split enables:

- Event replay for audit and federation without aggregate lock contention.
- Aggregate rebuild from events when schema evolves or corruption is detected.
- Independent retention policies (events 90-day, aggregates unbounded).

### Extensibility

Scored signal types (per RFC-0955 references):

- `Outcome` — task success/failure (Gap 6 task market).
- `Latency` — observed response time (Gap 7).
- `Capacity` — served requests/time unit.
- `Discovery` — found in lookups.

`Rotation` is a sixth `SignalKind`, but it is identity-migration metadata written by `consume_rotation_receipt`, not an adapter-scored signal. It has no `KIND_WEIGHTS` row and contributes nothing to composite scores.

The two-table schema supports all via `signal_kind` enum + new event payload decoding. New signal kinds require **zero DDL**; only a Rust enum extension.

### Cross-Layer Federation

Each layer (mon, dc, marketplace, task market, retrieval, proof market) contributes to a single reputation fingerprint per `(did, kind, layer)`. Cross-layer aggregation uses a per-kind normalizer (maps each kind to `[-1, 1]`) and a weights table:

| Kind        | Normalizer                               | Default weight |
| ----------- | ---------------------------------------- | -------------- |
| `Slash`     | `-clamp(severity / max_severity, 0, 1)`  | 1.0            |
| `Outcome`   | identity                                 | 0.8            |
| `Latency`   | `1 - clamp(latency / (10*target), 0, 1)` | 0.4            |
| `Capacity`  | `clamp(served / max_capacity, 0, 1)`     | 0.2            |
| `Discovery` | `clamp(lookups / max_lookups, 0, 1)`     | 0.2            |

Composite: `SUM(score_ewma * kind_weight) / SUM(kind_weight)`. This is one indexed SELECT with a static `kind_weights` join.

### Recorder Write Authority

Every event MUST be signed by a registered recorder. `register_recorder(req, governance_registry, now_unix)` gates registration on stake (≥1000 OCTO role-token per token-design §12), DID/key binding, and a stake proof whose signer is active at `req.stake_proof.snapshot` according to `GovernanceRegistry::lookup_at_snapshot(pubkey, snapshot)`. The receiving API first rejects a snapshot older than `MAX_GOVERNANCE_SNAPSHOT_AGE_SECS` relative to `now_unix` with `ReputationError::GovernanceSnapshotStale`. Every authoritative signature or registration carries a `GovernanceSnapshot`, including `GovernanceProof`, `ResumeProof`, and `AttestorAuth`, with no exceptions. Caller-supplied governance keys are never authoritative without the snapshot-bound lookup. Registration is INSERT-only: an existing `recorder_id` returns `RecorderAlreadyRegistered`, preserving all suspension/revocation state. Re-registration after revocation is the explicit two-step `resume_recorder` (governance-authorized lifecycle clear/removal) → `register_recorder` (fresh proof and INSERT). Invalid cryptographic proofs return `StakeProofInvalid`; inactive keys return `GovernanceKeyInactive`; registry failures propagate as `ReputationError::GovernanceRegistryError(_)` and do NOT collapse to `GovernanceKeyInactive`. Event signatures cover `BLAKE3(BLAKE3_REPUTATION_EVENT_DOMAIN || canonical_ser(event_unsigned))`, and `event_id` is derived from the unsigned canonical form, which excludes `event_id` and `signature`.

Severity suspension is atomic with signal recording: a store-level stoolap MVCC transaction under a per-`recorder_id` admission lock commits or rolls back the event, aggregate, recorder activity clock, severity self-check, and suspension together. External governance/manual suspension uses registry-validated `SuspensionAuth::Governance { proof }`; only the in-transaction self-check may construct `SuspensionAuth::Severity { internal: () }`. `resume_recorder` validates `ResumeProof.snapshot` against `now_unix` and resolves `governance_pubkey` through `GovernanceRegistry::lookup_at_snapshot`; `register_attestor` applies the same freshness and snapshot-bound lookup rules to `AttestorAuth.snapshot`.

`RetentionAuth` binds its bulk-prune boundary into the authorization: both retention entry points verify an ed25519 signature over `BLAKE3(BLAKE3_REPUTATION_RETENTION_DOMAIN || recorder.0 || now_unix || older_than_unix)` before any checkpoint or soft-prune write. Changing `older_than_unix` therefore invalidates the signature.

`RecorderId::new` is module-private (`pub(crate)`). The best-effort `RecorderId::registered(did, &RecorderRegistration)` factory requires a matching registered row, but compile-time safety is not absolute; the runtime registration/state check in `record_signal` is the authoritative gate. **Round 6 L4 exception:** storage code in `StoolapReputationStore::register_recorder` mints a fresh `RecorderId` via the module-private `RecorderId::new` because no row exists yet; external callers cannot mint directly. The corresponding factories are `ReaderId::authenticated(auth, verifier, now_unix)`, `AuditorId::authenticated(auth, verifier, now_unix)`, and the store-gated `AttestorId::registered(store, did)`. The module-private `AttestorId::new(did)` is used exclusively by `register_attestor` after a successful INSERT.

This eliminates the previous "any in-process code path can mutate" trust model and makes cross-daemon state sharing safe: an event from an unauthorized recorder is rejected at parse time, not at consensus.

### Canonical DID Encoding

DIDs are **strings** of the form `did:octo:b<base32-lowercase-no-padding BLAKE3-256 hash of pubkey>` (total 62 chars: 9 + 1 + 52). The `b` multibase prefix is the standard base32 indicator (Round 2 M15). Raw 32-byte keys are **rejected at parse time**. This eliminates reputation laundering via noncanonical encodings (Round 1 finding C3 + Round 2 C1: length was 63, corrected to 62).

DID rotation: `Did::rotate(old, new, proof, old_pubkey, new_pubkey, now_unix)` produces a new DID bound to the old via ed25519 proof over `BLAKE3("cipherocto/reputation/rotation/v1" || old.0 || new.0)`. The caller supplies `now_unix`; no process wall clock is read. The function verifies `blake3(old_pubkey) == old.hash_part` AND `blake3(new_pubkey) == new.hash_part` (Round 2 C3). `consume_rotation_receipt(receipt, now_unix)` first rejects any existing `(new_did, kind, layer)` aggregate with `RotationDestinationNotEmpty`; rotations to non-empty destinations are forbidden to preserve reputation integrity. Otherwise one transaction INSERTs the new decayed aggregates, DELETEs the old aggregates, INSERTs the rotation event, and consumes the receipt. `replay_rotation_history(recorder_id)` exposes the persisted receipts.

### Determinism

Per RFC-0008: reputation is **Class B** (deterministic when configured correctly). EWMA update + read MUST be deterministic given:

- Same input events.
- Same `alpha` (decay factor).
- Same clock.

**Dfp determinism (v3.0-r15, Gap 9):** v1.0 uses `octo_determin::Dfp` per RFC-0104. The 24-byte BLOB encoding is bit-deterministic across compilers and platforms, so two replicas running the same EWMA sequence produce byte-identical `score_ewma` BLOBs. Cross-replica agreement is achieved at the type level — there is no `f64` migration path.

Clock dependency is explicit: callers pass `now_unix` into every mutating time-sensitive API, while persisted `last_event_unix` supports time-bounded queries. EWMA decay itself is clock-free. `record_signal` atomically updates the event, aggregate, recorder `last_signal_at_unix`, and severity-triggered suspension in one store transaction; other writes for the same recorder are admission-blocked until it completes.

## Storage Targets (Round 1 finding M2 reconciled)

- Aggregate row: ~200 bytes (did TEXT ~80 + smallints + REALs + BLOB 32 = ~200).
- Theoretical max per DID: 6 kinds × 6 layers = 36 tuples = ~7.2 KB.
- Event row: ~720 bytes (large TEXT, BLOB payload, BLOB signature).
- At 100k DIDs: ~720 MB aggregate; events bounded by 90-day retention.
- At 1M events/day × 90 days = 90M events × 720 bytes ≈ 65 GB event log.

## Recommendations

1. **Use stoolap** — single backend, MVCC, vector SQL future.
2. **Two-table core** (`reputation_events` + nine-field `reputation_aggregates`) with composite `(did, signal_kind, layer)` PK, plus `reputation_rotations` (v005), `reputation_attestations` (Phase 1 v006), `aggregate_checkpoint` (Phase 1 v007), and `recorder_registration` (Phase 1 v008, Round 6 C1) for coherent pruned-prefix replay, recorder lifecycle state, and the role bitfield (`RETENTION_ROLE`, `READER_ROLE`, `AUDITOR_ROLE` per Round 6 C3).
3. **Canonical `did:octo:b<52>`** encoding only (62 chars total); reject raw 32-byte keys.
4. **Recorder signature + stake + state machine** for write authority; `register_recorder` API with `stake_proof` (Round 2 H6).
5. **Per-kind normalizers + weights table** for cross-layer AVG; `Normalizer` trait + `MAX_SEVERITY` + `KIND_WEIGHTS` constants (Round 2 H9).
6. **Type-safe adapter** in `crates/quota-router-storage` with `ReputationStore` trait.
7. **Defer on-chain anchoring** to mission 0968a (RFC-0955 follow-up; stub: `missions/deferred/0968a-reputation-anchoring.md`).
8. **Defer reputation tokenization** — keep reputation as derived signal, not balance.

## Risks

| Risk                                                   | Mitigation                                                                                                                                                                                            |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Adoption gap (existing in-memory stores won't migrate) | Phase 1: shadow mode (write to both, read from in-memory). Phase 2.5: backfill + reconciliation (parity_score > 0.999 for 24h, with `total >= 100` triples min-traffic guard). Phase 3: switch reads. |
| Schema growth as signal types added                    | `signal_kind` enum + `event_payload` BLOB keeps DDL stable.                                                                                                                                           |
| Clock skew on EWMA                                     | Use `event_received_at_unix` set by recorder at observation time; storage validates `received_at_unix <= now + 60s` (drift tolerance).                                                                |
| Storage growth unbounded                               | 90-day authenticated soft-prune marks `retention_pruned_at_unix`; v007 captures the aggregate state at each pruned-prefix boundary so replay remains checkpoint + retained events.                    |
| Cross-mission gossip (mission 0855p-b scope)           | Shared table records cross-mission events; gossip is transport layer concern (separate).                                                                                                              |
| Reputation laundering via dual DID encodings           | `Did::parse` rejects raw 32-byte keys AND legacy `did:octo:z...` strings; canonical `did:octo:b<52>` (62 chars) enforced.                                                                             |
| Reputation laundering via rotation                     | `Did::rotate` binds both old_pubkey + new_pubkey; `consume_rotation_receipt` is one-time and rejects every non-empty destination aggregate.                                                           |
| Federated event bypasses recorder auth                 | Round 2 C4: Attestor is replication-only; gossip event with no recorder signature is rejected with `RecorderSignatureMissing`.                                                                        |
| Sybil recorder inflation                               | Stake ≥1000 OCTO role-token + `stake_proof` (governance sig) + signature + state machine (suspend / revoke / under-staked / stale / expired).                                                         |
| Dfp cross-replica determinism (v3.0-r15, Gap 9)       | Achieved at the type level — `octo_determin::Dfp` (RFC-0104) is the v1.0 type for `score_delta`, `score_ewma`, normalizers, and `update_ewma`. The 24-byte BLOB encoding is bit-deterministic across compilers and platforms; no `f64` migration path exists. `update_ewma` returns `Result<octo_determin::Dfp, ReputationError>` in all builds (release + debug). |

## Storage Targets Summary

| Tier      | Aggregate size | Event log (90d) | Notes            |
| --------- | -------------- | --------------- | ---------------- |
| 10k DIDs  | ~72 MB         | ~6.5 GB         | typical operator |
| 100k DIDs | ~720 MB        | ~65 GB          | large operator   |
| 1M DIDs   | ~7.2 GB        | ~650 GB         | federation-tier  |

## Next Steps (status: completed)

- ✓ Create Use Case (`docs/use-cases/reputation-persistence.md`).
- ✓ RFC draft (`rfcs/draft/economics/0968-reputation-registry.md`).
- ✓ Mission (`missions/open/0968-reputation-persistence-blocked.md`, BLOCKED pending RFC-0968 acceptance).
- ✓ Round 10 convergence: `verify_governance_suspension` defined; `ReputationError` gains `#[repr(u8)]` with explicit discriminants; §13 error table monotonic 0x01..=0x27; `AuditorAuth` doc comment uses `BLAKE3_REPUTATION_AUDITOR_DOMAIN`; `record_attestation` takes `now_unix` and validates drift.
- → RFC-0968 promotion to Accepted per BLUEPRINT Mission Lifecycle.
- → Mission unblock + claim + implement Phases 1-3.
- → Mission 0968a (on-chain anchoring) — separate, deferred to RFC-0955 follow-up.

**v3.0-r15 (2026-07-25, Gap 9):** the recommendation above is updated to use `octo_determin::Dfp` (RFC-0104) as the v1.0 numeric type for `score_delta`, `score_ewma`, normalizers, and `update_ewma`. The 24-byte BLOB encoding is bit-deterministic across compilers and platforms. SQL columns `score_delta REAL`, `score_ewma REAL`, and `score_ewma_at_checkpoint REAL` become `BLOB NOT NULL CHECK (length(...) = 24)`. Cross-replica determinism is achieved at the type level — no `f64` migration path exists.

**v3.5-r20 (2026-07-25):** research no-on-chain-anchoring wording aligned with the Round 19 M2 RFC-0955 §`reputation` follow-up scope and amendment-required caveat; v3.4-r19 version-history line references corrected.

## Related RFCs

- RFC-0008: Deterministic AI Execution Boundary
- RFC-0104: Deterministic Floating-Point (required for cross-replica EWMA; v1.0 uses `octo_determin::Dfp`)
- RFC-0900: AI Quota Marketplace (provider reputation signal)
- RFC-0918: Inference Task Market (provider outcome signal)
- RFC-0927: RouterConfig Extension (per-layer alpha overrides)
- RFC-0955: Model Liquidity Layer (reputation field; on-chain anchoring follows mission 0968a)
- RFC-0967: Policy Object Graph (graph-capability reputation signal)
- Mission 0855p-b: Cross-mission coordinator reputation (open mission)

## Related Research

- `2026-07-22-event-sourced-ledger-precedents.md`
- `ai-quota-marketplace-research.md`
- `bifrost-litellm-providers.md`
