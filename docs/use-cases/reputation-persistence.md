# Use Case: Persisted Reputation

**Date:** 2026-07-24
**Status:** Draft → RFC Draft Created
**Author:** @cipherocto + @mmacedoeu

---

## Problem

CipherOcto depends on reputation signals to gate, rank, and slash participants across the protocol. The reputation of a provider, coordinator, or agent is the primary defense against Sybil clusters, low-quality output, and bad actors.

Today, every reputation store is in-memory:

- `SlashReputationStore` (octo-network mon) — coordinator slash aggregation.
- `DcRootedSlashReputationStore` (octo-network dc) — DC slash aggregation.
- `ProviderReputationRegistry` (quota-router-core marketplace) — ask provider EWMA.

A daemon restart loses all reputation. A new node has no reputation. Cross-layer reputation (e.g., a coordinator with high slash count asking for marketplace work) is invisible. Operators have no on-disk audit trail. There is no canonical DID encoding — both `did:octo:` strings and raw 32-byte keys have been accepted, enabling reputation laundering across noncanonical forms. Recorder write authority is implicit (any in-process code path can mutate), so trust between daemons sharing state cannot be enforced cryptographically.

## Stakeholders

- **Primary:** providers (LLM, retrieval, proof), coordinators, agents, DCs.
- **Secondary:** marketplace buyers, task market askers, slash auditors.
- **Affected:** protocol operators, security reviewers, on-chain reputation consumers.

## Motivation

Reputation is the protocol's soft permissioning layer. Without persistence:

- New daemons are amnesiacs — must re-accept bad actors before slashing them.
- Long-tail reputation (months of good conduct) is lost on restart.
- Cross-layer federation (slash + outcome + latency) is impossible.
- On-chain anchoring (RFC-0955) requires persisted source-of-truth.
- Reputation can be laundered across noncanonical DID encodings.
- Recorder write authority is implicit; trust cannot be cryptographically verified.

Reputation persistence is the prerequisite for: honest ranking, meaningful slashing, cross-mission gossip, on-chain proof-of-reputation, and write-authoritative reputation state.

## Success Metrics

| Metric                                      | Target                                               | Measurement                             |
| ------------------------------------------- | ---------------------------------------------------- | --------------------------------------- |
| Reputation durability across daemon restart | 100% preserved                                       | Restart + check `score_ewma` matches    |
| Cross-layer query latency (p99)             | < 50ms                                               | stoolap SELECT benchmark                |
| Adapter migration cost (existing in-memory) | < 200 LOC per adapter                                | Diff size                               |
| New signal type schema additions            | 0 (data) + 1 (Rust enum)                             | DDL changes                             |
| On-chain anchoring readiness                | single source of truth per DID                       | RFC-0955 follow-up (mission 0968a)      |
| Recorder write authority                    | cryptographic via ed25519 + stake                    | Signature verify on `record_signal`     |
| Severity enforcement                        | suspend atomically at aggregate severity ≥ 5         | Transaction rollback + concurrency test |
| Attestation persistence                     | 100% restart-durable with event FK                   | v006 round-trip + restart test          |
| Canonical DID enforcement                   | 100% parse via `Did::parse` rejects raw 32-byte keys | Fuzz test                               |

## Constraints

- **Must not** break existing API surfaces. Existing `SlashReputationStore`, `ProviderReputationRegistry` keep their public methods; persistence is internal.
- **Must use** stoolap (CipherOcto fork) — per `feedback_stoolap-persistence.md` memory. The MVCC guarantee (snapshot-isolation transactions underpinning `record_signal`'s atomic event + aggregate + recorder activity update) is the contract that makes all `record_signal` writes deterministic across replicas. See `feedback_stoolap-persistence.md` for the fork proof and the rationale for the fork-vs-upstream decision.
- **Must** preserve RFC-0008 Class B determinism (EWMA updates deterministic given same inputs).
- **Must** enforce canonical `did:octo:b<52>` encoding (Round 2 C1 + M15: total 62 chars; multibase `b` = base32 standard); reject raw 32-byte keys and legacy `did:octo:z...` strings.
- **Must** require recorder signature + stake for write authority; `stake_proof` signs `BLAKE3(BLAKE3_REPUTATION_STAKE_DOMAIN || recorder_id || stake_amount || requested_at_unix)`. `register_recorder(req, governance_registry, now_unix)` first rejects a stale `req.stake_proof.snapshot` with `GovernanceSnapshotStale`, then accepts the proof signer only when `GovernanceRegistry::lookup_at_snapshot(pubkey, &req.stake_proof.snapshot)` is true; inactive keys return `GovernanceKeyInactive`, invalid signatures return `StakeProofInvalid`, and registry failures propagate as `GovernanceRegistryError(_)`.
- **Must** reject an existing registration row with `RecorderAlreadyRegistered`; registration is INSERT-only. Re-registration after revocation is exactly `resume_recorder` (governance-authorized lifecycle clear/removal) followed by `register_recorder` (fresh proof and INSERT).
- **Must** pass `now_unix` explicitly to `Did::rotate`, `consume_rotation_receipt`, `record_signal`, suspension, and prune mutations; these paths must not read a process wall clock.
- **Must** reject rotation when any `(new_did, kind, layer)` aggregate exists with `RotationDestinationNotEmpty`; otherwise destination INSERTs, source DELETEs, rotation event, and receipt consumption are one atomic transaction.
- **Must** persist attestations in Phase 1 migration `v006__reputation_attestations.sql` and verify the canonical `attestation_id` before insert. `AttestationId` and `EventId` are distinct namespaces. `record_attestation(att, now_unix)` (Round 10 OQ) validates drift between `att.received_at_unix` and `now_unix` against `MAX_ATTESTATION_DRIFT_SECS = 60` seconds (the same tolerance as `record_signal`) and rejects with `TimestampDrift` otherwise.
- **Must** authorize `suspend_recorder(recorder_id, reason, auth, governance_registry, now_unix)` with `SuspensionAuth::Governance { proof }` for external governance/manual transitions or the internal-only `SuspensionAuth::Severity { internal: () }` for threshold enforcement. The governance-signed proof is verified via `verify_governance_suspension` over `BLAKE3(BLAKE3_REPUTATION_SUSPENSION_DOMAIN || recorder_id || reason_hash || governance_pubkey || now_unix)` where `BLAKE3_REPUTATION_SUSPENSION_DOMAIN = b"cipherocto/reputation/suspension/v1"` (Round 10 H1 + Round 13 H1).
- **Must** atomically update the event, nine-field aggregate, recorder `last_signal_at_unix`, severity self-check, and suspension in one store-level stoolap MVCC transaction. A per-`recorder_id` admission lock blocks concurrent `record_signal` calls until commit/rollback.
- **Must** write `aggregate_checkpoint` in `v007__aggregate_checkpoints.sql` at each pruned-prefix boundary in the same transaction as `prune_event`; audit replay is checkpoint + retained events.
- **Round 2 M14 wire format:** `canonical_ser` MUST be **CipherOctoCanonical** (per `crates/cipherocto-encoding`), not CBOR. BLAKE3 domain-separator prefix; integers big-endian fixed-width; `f64` NOT supported natively (use `octo_determin::Dfp` per RFC-0104 — serialized as the canonical 24-byte `DfpEncoding::to_bytes()` form: 16-byte mantissa + 4-byte exponent + 4-byte class_sign, all big-endian; v3.0-r15 Gap 9 replaces the previous `i64` micro-units workaround); enum 1-byte tag + payload; strings 4-byte length prefix + UTF-8; bytes 4-byte length prefix + raw bytes; Option 1-byte tag (0=None, 1=Some) + payload; sorted maps by lexicographic key.
- **Round 2 M14 recorder key binding:** `Did::from_pubkey(pk)` MUST produce a DID whose `hash_part` equals `data_encoding::BASE32_NOPAD_LOWER.encode(blake3::hash(pk).as_bytes())`. `register_recorder` verifies `blake3(req.pubkey) == req.recorder_did.hash_part` before persisting.
- **Round 5 M2 branded type:** `RecorderId::new` is private; callers use `RecorderId::registered(did, &RecorderRegistration)`. Compile-time safety is best-effort; the runtime registration/state check in `record_signal` is the authoritative gate.
- **Round 6 L4 `register_recorder` exception:** storage code in `StoolapReputationStore::register_recorder` mints a fresh `RecorderId` via the module-private `RecorderId::new` because no row exists yet; external callers cannot mint directly. The same pattern applies to `ReaderId::authenticated(auth, verifier, now_unix)`, `AuditorId::authenticated(auth, verifier, now_unix)`, and `AttestorId::registered(store, did)`.
- **Round 2 M14 normalizer zero-denominator guards:** `LatencyNormalizer` rejects `target_ms == 0`; `CapacityNormalizer` rejects `max_capacity == 0`; `DiscoveryNormalizer` rejects `max_lookups == 0`. All return `ReputationError::NormalizerDivByZero`.
- **Round 2 M14 backfill barrier:** Phase 2.5 backfill replays in-memory events into `ReputationStore` with `received_at_unix = canonicalized_now` (storage-time) and `payload` marker `b"BACKFILL_V1"`. The cutover is gated on `parity_score > 0.999` sustained for 24h with `total >= 100` triples (min-traffic guard).
- **Round 2 M14 DFP upgrade path (REMOVED at v3.0-r15, Gap 9):** v1.0 uses `octo_determin::Dfp` per RFC-0104. Cross-replica determinism is achieved at the type level — there is no `f64` migration. `SignalEvent.score_delta`, `ReputationAggregate.score_ewma`, `update_ewma` parameters/return, and the five normalizers all carry `Dfp` values; the SQL columns are `BLOB NOT NULL CHECK (length(...) = 24)` (canonical 24-byte `DfpEncoding::to_bytes()` form).
- **Round 6 v008 migration:** `recorder_registration` table holds the lifecycle row + `roles: u64` bitfield. `RETENTION_ROLE = 1 << 0` is required by `retention_prune` / `prune_event`; `READER_ROLE = 1 << 1` and `AUDITOR_ROLE = 1 << 2` are reserved for future role-based authorization.
- **Round 6 M6 + M1 + M2:** `register_recorder`, `resume_recorder`, and `retention_prune` take `now_unix` for caller-supplied drift validation (5-minute tolerance). `MAX_REGISTRATION_DRIFT_SECS = MAX_RESUME_DRIFT_SECS = 300`.
- **Round 6 M4:** `consume_rotation_receipt` holds a per-DID admission lock for both `old_did` and `new_did` for the duration of the transaction.
- **Round 6 H5:** the rotation event's `did` field is `new_did`, not `old_did`. Audit replay locates the row by the new DID's canonical key.
- **Round 6 H1 + H2 + Round 9 L2:** branded identifier fields are private; the five minting paths are exhaustive. **External** (callers may use): `RecorderId::registered(did, &RecorderRegistration)`, `ReaderId::authenticated(auth, verifier, now_unix)`, `AuditorId::authenticated(auth, verifier, now_unix)`, `AttestorId::registered(store, did)`. **Module-private** (`pub(crate)`, called only from inside the reputation module): `RecorderId::new(did)` (sole caller `StoolapReputationStore::register_recorder`), `AttestorId::new(did)` (sole caller `register_attestor` after a successful INSERT). Runtime registration/state checks in `record_signal` and `AttestorId::registered` are the authoritative gates; compile-time safety is best-effort.
- **Round 6 H3:** `EventId` and `AttestationId` are newtypes (not aliases), preserving the two distinct namespaces.
- **Round 6 L1:** `query_attestations` requires `ReaderId`; ReaderAuth is verified before the result is returned.
- **Round 6 L2 + Round 7 H3 + Round 8 closure:** `GovernanceRegistry::lookup_at_snapshot` returns `Result<bool, GovernanceError>`. Registry-lookup failures propagate as `ReputationError::GovernanceRegistryError(_)`; they do NOT collapse to `GovernanceKeyInactive`. Carried-snapshot age is checked locally first and returns `ReputationError::GovernanceSnapshotStale`.
- **Round 7 H4 + Round 8 H1/H2:** Every authoritative signature or registration carries a `GovernanceSnapshot`, including `GovernanceProof`, `ResumeProof`, and `AttestorAuth`, with no exceptions. Each receiving API validates `snapshot.finalized_at_unix + MAX_GOVERNANCE_SNAPSHOT_AGE_SECS >= now_unix` before any registry lookup and returns `ReputationError::GovernanceSnapshotStale` when the snapshot is too old. Authority is resolved only through `GovernanceRegistry::lookup_at_snapshot(pubkey, &snapshot)`. Cross-replica determinism requires snapshot equality.
- **Round 7 C1 + Round 8 H2:** `register_attestor(governance_registry, attestor_auth, registration, now_unix)` validates `attestor_auth.snapshot` freshness, resolves the governance signer with `lookup_at_snapshot`, verifies the governance signature, validates `blake3(reg.pubkey) == reg.attestor_did.hash_part`, enforces `MAX_REGISTRATION_DRIFT_SECS`, and rejects duplicate DIDs with `AttestorAlreadyRegistered`. Server-stamps `registered_at_unix` at INSERT.
- **Round 7 C2 + M1:** `AttestorId::registered(store, did)` is store-gated. It performs a runtime `attestor_lookup_did(did)` lookup and validates the binding. The module-private `AttestorId::new(did)` is used exclusively by `register_attestor`.
- **Round 7 H1:** `PublicKeyLookup` trait declared canonically in RFC-0968 §10. `record_signal`, `ReaderId::authenticated`, and `AuditorId::authenticated` route through this trait to resolve stored ed25519 pubkeys.
- **Round 7 H2:** `EventId` and `AttestationId` have private fields. `from_bytes([u8;32])` is the validated constructor; `AsRef<[u8;32]>` and `Deref<Target=[u8;32]>` provide transparent access.
- **Round 7 M3 + Round 8 H3:** `RetentionAuth.signature` is `[u8; 64]` (fixed-size ed25519); the signed message is `BLAKE3(BLAKE3_REPUTATION_RETENTION_DOMAIN || recorder.0 || now_unix || older_than_unix)`. Both prune entry points verify the signature, including `older_than_unix`, against the recorder's stored pubkey via `PublicKeyLookup` and enforce the `RETENTION_ROLE` bit before any storage work.
- **Round 7 L1:** `ROTATION_DECAY_Q32_32 = 0xE6666666`. Decimal value is 0.89999998, NOT exactly 0.9.
- **Round 10 H1 + Round 13 H1:** `verify_governance_suspension` is the canonical authorization gate for `SuspensionAuth::Governance`. The signed digest is `BLAKE3(BLAKE3_REPUTATION_SUSPENSION_DOMAIN || recorder_id || reason_hash || governance_pubkey || now_unix)` where `BLAKE3_REPUTATION_SUSPENSION_DOMAIN = b"cipherocto/reputation/suspension/v1"` and `reason_hash = blake3(canonical_ser(reason))`. Verification order matches the Round 13 impl: snapshot freshness → digest + ed25519 signature verification (against `proof.governance_pubkey`) → `lookup_at_snapshot` (Round 7 H4 defense-in-depth). `suspend_recorder` takes `governance_registry: &dyn GovernanceRegistry` so the verify path can drive the registry lookup.
- **Round 10 H2:** `ReputationError` carries `#[repr(u8)]` with explicit discriminants matching the §13 error table 1:1 (0x01..=0x28). The wire-level error code is stable across replicas; source order in the enum declaration is decoupled from the §13 declaration. `0x29..=0xFF` are reserved for future variants (v3.3-r18 C11: `0x28` is now `ScoreEncodingInvalid` per Round 16 L4, so the reserved range starts at `0x29`).
- **Round 10 M1:** §13 error table is monotonic `0x01..=0x28` with `Storage(_)` at 0x21 and `ScoreEncodingInvalid` at 0x28.
- **Round 10 L1:** `AuditorAuth` doc comment references `BLAKE3_REPUTATION_AUDITOR_DOMAIN` (the canonical `b"cipherocto/reputation/auditor/v1"` constant declared in §10) instead of the misleading `"auditor/replay/v1"` literal.
- **Round 10 OQ:** `record_attestation(att, now_unix)` validates drift between `att.received_at_unix` and `now_unix` against `MAX_ATTESTATION_DRIFT_SECS = 60` seconds. The drift check runs immediately after `verify_attestation_id` and before signature verification.
- **Round 6 M7:** `SignalEvent` storage lifecycle is `Recorded → Replayed (gossip)`. The previous "Pending" row has been removed; events are produced only via `record_signal` and immediately `Recorded` on transaction commit.
- **Round 6 M8 + L6:** "RotationReceipt lifecycle" is the §15 row (preferred term); `reputation_rotations` is the table name. `consumed_at_unix IS NULL` is the derived pending marker.
- **Limited to** persistence + query layer in this use case. On-chain anchoring is separate (RFC-0955 follow-up mission 0968a).
- **No reputation token** — reputation is derived signal, not balance.

## Non-Goals

- Tokenizing reputation (no `OCTO-R` token).
- On-chain anchoring (mission 0968a scope, deferred to RFC-0955 follow-up).
- Reputation weighting algorithms beyond per-kind normalizers + weights table.
- Cross-mission gossip protocol (mission 0855p-b scope).
- Reputation-driven pricing (separate RFC).

## Impact

If implemented:

- Daemon restart preserves reputation. Operators ship upgrades without trust resets.
- Cross-layer queries become possible (e.g., "providers with >0.8 Outcome score AND no slash events").
- Recorder write authority is cryptographic, not trusted.
- Noncanonical DID encodings are rejected at parse time.
- Rotation is observable and deterministic through caller-timestamped decay receipts; non-empty destinations are rejected, and `replay_rotation_history` exposes successful receipts.
- Signed attestations survive restart in the Phase 1 v006 table.
- Severity threshold and governance/manual suspension share one auditable, authorized state transition; severity enforcement is atomic with signal persistence.
- Pruned history remains reconstructable from v007 aggregate checkpoints plus retained events.
- On-chain anchoring has a single source-of-truth.
- Mission 0855p-b (cross-mission slash gossip) has a place to persist.
- Future signal types (capacity, discovery) follow the same schema without DDL.

If not implemented:

- Reputation remains rest-lossy.
- Cross-layer fraud (good marketplace score + bad coordinator record) undetected.
- RFC-0955 on-chain anchoring has no backing store.
- Reputation can be laundered across dual DID encodings.
- Recorder trust cannot be cryptographically enforced.

## Related RFCs

- RFC-0008: Deterministic AI Execution Boundary (Class B for EWMA)
- RFC-0104: Deterministic Floating-Point (required for cross-replica agreement; v1.0 uses `octo_determin::Dfp`)
- RFC-0900: AI Quota Marketplace (provider reputation signal)
- RFC-0918: Inference Task Market (provider outcome signal)
- RFC-0927: RouterConfig Extension (per-layer alpha overrides)
- RFC-0955: Model Liquidity Layer (reputation field; on-chain anchoring follows mission 0968a — stub at `missions/deferred/0968a-reputation-anchoring.md`)
- RFC-0967: Policy Object Graph (consumes reputation signal)
- Mission 0855p-b: Cross-mission coordinator reputation (federation target)

## Related Research

- `docs/research/2026-07-24-reputation-persistence-research.md` (feasibility)
- `docs/research/2026-07-22-event-sourced-ledger-precedents.md`

## Status

Draft → RFC Draft Created.

**v3.0-r15 (2026-07-25, Gap 9):** switched the reputation data model from `f64` to `octo_determin::Dfp` per RFC-0104. Cross-replica determinism is achieved at the type level — no v1.1 migration needed. See RFC-0968 v3.0-r15 entry.

**v3.5-r20 (2026-07-25):** research no-on-chain-anchoring wording aligned with the Round 19 M2 RFC-0955 §`reputation` follow-up scope and amendment-required caveat; v3.4-r19 version-history line references corrected.

## Next

- ✓ RFC-0968 created (Draft) — `rfcs/draft/economics/0968-reputation-registry.md`.
- ✓ Mission file created (BLOCKED pending RFC-0968 acceptance) — `missions/open/0968-reputation-persistence-blocked.md`.
- ✓ On-chain anchoring stub — `missions/deferred/0968a-reputation-anchoring.md` (deferred; depends on RFC-0955).
- → RFC-0968 promotion to Accepted per BLUEPRINT Mission Lifecycle.
- → Mission unblock + claim + implement.
