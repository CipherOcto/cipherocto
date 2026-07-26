# Mission: Reputation Persistence

> **STATUS: Claimed 2026-07-25.** RFC-0968 status change recorded per BLUEPRINT Mission Lifecycle. RFC-0968 has been amended in place under RFC-0968-A1 (2026-07-26) — 25 amendments closed from a 5-session adversarial review (`docs/plans/2026-07-26-rfc0968-review-session-{01..05}-*.md`) plus the C-C3, I-5, I-6 follow-ons. Acceptance Criteria below fold in every applicable amendment.

## Status

Claimed (unblocks recorded 2026-07-25).

**Amendments folded into ACs (2026-07-26):** RFC-0968-A1 (25 amendments) + `determin` IEEE-754 `Dfp` equality + the C-C3 canonical Dfp test vectors + RFC-0955-R1 reputation anchoring amendment.

## RFC

RFC-0968: Reputation Registry (with RFC-0968-A1 in-place amendment, 2026-07-26)

## Dependencies

**Hard upstream blockers (RFC must be Accepted before this mission lands):**

- RFC-0008: Deterministic AI Execution Boundary
- RFC-0900: AI Quota Marketplace
- RFC-0918: Inference Task Market

**Soft prerequisites (helpful but not blocking):**

- RFC-0104: Deterministic Floating-Point (REQUIRED; v1.0 uses `octo_determin::Dfp`)
- RFC-0927: RouterConfig Extension (per-layer alpha overrides)
- Mission 0855p-b: Cross-mission coordinator reputation (federation target, NOT blocking Phase 1-3 — gated on a claimed 0855p-b per §7 dual-read cutover)

**Removed as hard blocker (RFC-0968-A1, 2026-07-26):** RFC-0955 was historically listed as a hard blocker, but Phase 5 (on-chain anchoring) was extracted to mission 0968a in Round 1 (H11) and is now deferred. RFC-0955 is a Phase 5 dependency, not a Phase 1-4 one. RFC-0955-R1 (in-place amendment, 2026-07-26) defines the binding contract; RFC-0955 itself remains Draft.

**Related missions (separate scope):**

- Mission 0968a-reputation-anchoring (NEW, deferred) — on-chain anchoring per RFC-0955-R1 follow-up. Stub path: `missions/deferred/0968a-reputation-anchoring.md`. Status remains "Deferred; depends on RFC-0955 acceptance + RFC-0955-R1 deployment." Mission 0968a's acceptance criterion is now gated on RFC-0955 Accepted + RFC-0955-R1 Accepted.
- Mission 0968-b-marketplace-integration (NEW, RFC-0968-A1.19) — owns marketplace read-side: routing priority, listing display, `0-100` Reputation Score presentation layer. Carrier of §7 dual-read cutover. Path: `missions/claimed/0968-b-marketplace-integration.md` (to be authored when claimed).

## Summary

Implement the unified, DID-keyed, persisted, cryptographically-signed reputation registry per RFC-0968 as amended by RFC-0968-A1. Phase 1 lands the dedicated `crates/oct-reputation/` workspace member with stoolap-backed storage, canonical `did:octo:b<52>` encoding (62 chars), dual-stake recorder authorization (`role + octo ≥ 5000`), per-subject admission (co-signature or stake bond), per-recorder `severity_emitted_total`, governance-set-hash quorum (≥3), and kind-gated adapters. Phase 2 ships shadow-write from existing in-memory stores via RFC-0968-A1 §7 dual-read compatibility adapters. Phase 2.5 reconciles backfill + computes parity_score before the read switch (the previous equivalence claim is replaced with a dual-read cutover path). Phase 3 switches reads via the compatibility adapters and the `0-100` presentation layer. Phase 4 is gossip federation (gated on a claimed/Accepted 0855p-b per RFC-0968-A1 §7). Phase 5 (on-chain anchoring) is deferred to mission 0968a.

## Acceptance Criteria

The acceptance criteria below fold in RFC-0968-A1 (25 amendments, 2026-07-26), `determin` IEEE-754 `Dfp` semantics + canonical blob test (C-C3), RFC-0955-R1 reputation anchoring binding contract, and the C-B2 / I-5 / I-6 follow-on patches. Every AC is anchored to the RFC amendment that closed it.

### Phase 1: Core Storage

#### Type declarations

- [ ] `crates/oct-reputation/src/{lib,core,event,recorder,reader,auditor,attestor,rotation,suspension,retention,error,constants}.rs` plus `src/storage/{mod,stoolap}.rs` define `SignalEvent`, canonical nine-field `ReputationAggregate`, `ReputationLayer`, `SignalKind`, `Did`, `RecorderId`, `ReaderId`, `AuditorId`, `AttestorId`, `AttestorRegistration`, `ReputationStore` trait (with `register_attestor` + `attestor_lookup_did`), `ReputationError` (now 41 variants, `0x01..=0x29` monotonic per RFC-0968-A1 §13; reserved `0x2A..=0xFF` for future variants; new `AuditorNonceReplay = 0x29` per amendment 22), `Attestation`, `ReplayRecord`, `RotationReceipt`, `AggregateCheckpoint`, `RecorderRegistrationRequest`, `ReaderAuth`, `AuditorAuth`, `RetentionAuth`, `AttestorAuth`, `GovernanceProof` (now carries `governance_set_hash: [u8; 32]` per amendment 26 / I-5), `ResumeProof`, `SuspensionAuth`, `SuspensionReason`, `GovernanceRegistry`, `GovernanceError`, `GovernanceSnapshot`, `PublicKey`, `ReputationPayload`, `EventId` + `AttestationId` (private-field newtypes, distinct namespaces), `PublicKeyLookup` trait + `PublicKeyLookupError`, `Normalizer` trait, `NormalizerInput`, `MAX_SEVERITY`, `SUSPENSION_SEVERITY_THRESHOLD = 5`, `MAX_REGISTRATION_DRIFT_SECS = 300`, `MAX_RESUME_DRIFT_SECS = 300`, `MAX_GOVERNANCE_SNAPSHOT_AGE_SECS = 600`, `MAX_ATTESTATION_DRIFT_SECS = 60`, `KIND_WEIGHTS` (mutable seeded migration v009; seeded in Phase 1 acceptance per amendment 12), `RecorderState` (7 variants: Active, Suspended, Revoked, UnderStaked, Stale, Expired, Unknown), `roles: u64` bitfield, `ROTATION_DECAY_Q32_32 = 0xE6666666`, `verify_attestation_id`, `verify_governance_suspension`, the 10 `BLAKE3_REPUTATION_*_DOMAIN` constants (canonical home in §10), and the new `MIN_RECORDER_DUAL_STAKE = 5000`, `MIN_RECORDER_OCTO_STAKE = 4000`, `GOVERNANCE_QUORUM = 3`, `MAX_AUDITOR_NONCE_TTL_SECS = 7 * 86_400` (default 7 days) constants per amendments 1, 26, 22.

#### Recorder registration (RFC-0968-A1 amendments 1, 2, 5, 26)

- [ ] `register_recorder(req, governance_registry, now_unix)` verifies `blake3(req.pubkey) == req.recorder_did.hash_part`, validates `MAX_REGISTRATION_DRIFT_SECS` against `now_unix`, rejects a stale `req.stake_proof.snapshot` with `GovernanceSnapshotStale`, requires `GovernanceRegistry::lookup_at_snapshot(proof.pubkey, &proof.snapshot)` to return `Ok(true)`, rejects a `governance_set_hash` mismatch (amendment 26 / I-5) with `GovernanceKeyInactive`, verifies `stake_proof = ed25519(proof.pubkey, BLAKE3(BLAKE3_REPUTATION_STAKE_DOMAIN || recorder_id || stake_amount || requested_at_unix))`, and rejects `stake_amount` below `MIN_RECORDER_DUAL_STAKE = 5000` with `StakeBelowMinimum { provided }` (amendment 1). `RecorderRegistration` carries `octo_stake_amount: u64`, `role_stake_amount: u64`, `aggregate_stake_amount = octo + role`, `stake_lock_ref: ChainRef` (tx-hash + lock-script reference per amendment 2 / C-Z2). Registration is INSERT-only.
- [ ] `RecorderId::new` is module-private (`pub(crate)`); `RecorderId::registered(did, &RecorderRegistration)` is the only external minting path. `record_signal` performs the authoritative runtime registration/state check.

#### Recorder lifecycle (RFC-0968-A1 amendment 11 / I-2)

- [ ] `recorder_state_at` returns one of `Active | Suspended | Revoked | UnderStaked | Stale | Expired | Unknown`. Acceptance criterion: a `UnderStaked` row recovers only via `top_up_stake(recorder_id, chain_ref, governance_registry, now_unix)` extending `octo_stake_amount` and/or `role_stake_amount` with on-chain `stake_lock_ref` verification. `Stale` recovers only via `mark_active(recorder_id, governance_registry, now_unix)` with fresh `last_signal_at_unix`. `Expired` is terminal: re-registration requires a NEW canonical DID rotation. `Suspended` → `Revoked` after grace applies the same escalation rule as explicit `Revoked` (amendment 5).
- [ ] Re-registration after grace-revocation requires `stake_amount ≥ MIN_RECORDER_DUAL_STAKE × 2` escalating up to `× 10` cap (amendment 5). Implementation MUST persist the escalation counter and refuse re-registration at lower amounts.

#### Per-recorder severity counter (RFC-0968-A1 amendment 4, I-1)

- [ ] `RecorderRegistration.severity_emitted_total: u64` counter increments on every Slash event the recorder emits, regardless of `(subject, kind, layer)` partition. `suspend_recorder_self_check` thresholds against `SUSPENSION_SEVERITY_THRESHOLD = 5` on this cross-aggregate counter. Tests cover: 5 Slash events to 5 distinct subjects/layers → 5th suspends; 1 Slash event to 1,000 subjects/layers → recorder stays Active but subject aggregate accumulates 1,000 `severity_total`. Subject aggregate `severity_total` remains for per-subject audit but does NOT independently suspend.

#### Per-subject admission (RFC-0968-A1 amendment 6 / C-A1)

- [ ] `record_signal` rejects non-Slash signals (Outcome/Latency/Capacity/Discovery) for `subject_did` when neither (a) the event carries a co-signature from `subject_did`'s recorder key over the canonical event envelope, nor (b) the subject is registered in `reputation_subject_bonds` with bond ≥ 100 OCTO role-token default. Slash events are governance-issued and exempt.
- [ ] Tests: unsigned Outcome for fresh subject → rejected; co-signed Outcome for fresh subject → accepted; bonded subject → accepted without co-sig; subject with bond below threshold → rejected.

#### Governance suspension proof (RFC-0968-A1 amendment, C-A5)

- [ ] `verify_governance_suspension(proof, recorder_id, reason, snapshot, now_unix)` MUST (1) recompute `reason_hash` from the actual `reason` argument and compare against `proof.reason_hash`; (2) assert `proof.recorder_id == recorder_id`; (3) verify `proof.signature` over `BLAKE3(BLAKE3_REPUTATION_SUSPENSION_DOMAIN || recorder_id || recomputed_reason_hash || governance_pubkey || now_unix)`; (4) verify `proof.governance_set_hash` against `GovernanceRegistry::lookup_at_snapshot` active-set digest (amendment 26 / I-5). A valid proof issued for recorder A cannot authorize suspension of recorder B.

#### Auditor nonce replay (RFC-0968-A1 amendment 22 / C-P2)

- [ ] Persistent `auditor_nonces(auditor_did, nonce) → observed_at_unix` table enforces replay rejection within `MAX_AUDITOR_NONCE_TTL_SECS = 7 days` default. `replay_for_audit` consumes the nonce atomically; a duplicate returns `ReputationError::AuditorNonceReplay = 0x29`. Tests: same nonce sequentially → second fails with `AuditorNonceReplay`; nonce after TTL expiry → accepted; nonce consumed across daemon restart → still rejected.

#### Trusted-clock API boundary (RFC-0968-A1 amendment 27 / I-6)

- [ ] Public RPC entrypoints do NOT accept caller-supplied `now_unix`. The receiving service supplies its trusted-clock value via a `Clock` trait. Suspension / resume / rotation proofs carry the timestamp inside the signature digest, not as a separate RPC parameter. Internal store APIs take `now_unix` (determinism); RPC boundary translates from `Clock::now_unix()`.

#### Rotation retirement + cooldown (RFC-0968-A1 amendments 7, 10 / C-A3, I-3)

- [ ] `consume_rotation_receipt` writes a tombstone for `old_did` (`old_did` → `new_did` lineage). Post-rotation events with `subject = old_did` are rejected (or canonicalized to `new_did`). Tests: rotation completes; subsequent events for `old_did` reject; aggregate for `old_did` is NOT recreatable.
- [ ] `Did::rotate` enforces `MAX_ROTATIONS_PER_DAY_PER_SUBJECT = 1` and `rotation_fee = 100 OCTO` burned on-chain (amendment 7). Lineage depth `≤ 3` per audit counter. Receipt contract fields are private; `rotation_id` is derived server-side, never caller-supplied (amendment 7 + B1).
- [ ] `Did::parse` rejects raw 32-byte keys AND legacy `did:octo:z...` strings; only `did:octo:b<52>` (62 chars total) accepted.
- [ ] `consume_rotation_receipt(receipt, now_unix)` is one-time per `(old, new)` pair, rejects any existing `(new_did, kind, layer)` aggregate with `RotationDestinationNotEmpty`, holds per-DID admission lock for both `old_did` and `new_did`, INSERTs rotation event FIRST (`did = new_did` per Round 6 H5), uses resulting `event_id` as `aggregate.last_event_id`. `replay_rotation_history(recorder_id)` returns persisted receipts. `AuditReplay`-for-rotation pre/post reconstructs both histories.

#### EWMA + Dfp determinism (RFC-0104)

- [ ] `update_ewma` returns `Result<octo_determin::Dfp, ReputationError>`; validates alpha ∈ (0,1], delta ∈ [-1,1], rejects NaN/Infinity in all builds (release + debug). Dfp encoding is bit-deterministic.
- [ ] **Canonical blob test (C-C3):** New `rfc0968_section23_canonical_dfp_blobs` test in `determin/tests/ewma_vectors.rs` asserts two replicas produce byte-identical 24-byte BLOBs for all four RFC-0968 §23 events. The previous `1e-9`/`1e-7` tolerance is replaced with `assert_eq!(pinned, canonical_bloobs)` byte equality.
- [ ] `Dfp` derives `PartialEq`/`Eq`/`Hash` per IEEE-754 semantics documented in `dfp_compare.rs` (`+0 == -0`; all NaN compare equal; ±Inf sign-aware). Cross-replica agreement MUST use `DfpEncoding::from_dfp(&d).to_bytes()`, not in-memory `==`.

#### Auth + retention

- [ ] Reader / Auditor / Retention auth: `read_aggregate` requires `ReaderAuth`; `replay_for_audit` requires `AuditorAuth` + nonce; `retention_prune(auth, now_unix)` and `prune_event(auth, event_id, now_unix)` require `RetentionAuth` + `RETENTION_ROLE` bit, verify `BLAKE3(BLAKE3_REPUTATION_RETENTION_DOMAIN || recorder.0 || now_unix || older_than_unix)` before any storage work, atomically capture the pruned-prefix checkpoint (writing `last_event_unix_at_checkpoint`), and set `retention_pruned_at_unix`. `query_attestations` requires `ReaderId`.
- [ ] `StoolapReputationStore` implements `ReputationStore` using stoolap (per `feedback_stoolap-persistence.md`).

#### Storage schema + migrations

- [ ] Migration files v003 through v008 follow RFC-0968 §5 + §2.1 with canonical scores as `BLOB NOT NULL CHECK (length(...) = 24)` per RFC-0104. New v009 migration seeds `kind_weights` deterministically for all scored kinds (Slash, Outcome, Latency, Capacity, Discovery); `Rotation` is OMITTED from `kind_weights`. Weights are immutable in v1.0.
- [ ] **Checkpoint ID derivation (RFC-0968-A1 amendment 13 / C-P3):** `aggregate_checkpoint.checkpoint_id = BLAKE3(BLAKE3_REPUTATION_CHECKPOINT_DOMAIN || did || signal_kind || layer || checkpoint_event_id)`. Audit replay picks the checkpoint with the largest `checkpoint_event_id` such that all events with `received_at_unix < checkpoint_event_received_at_unix` are pruned.
- [ ] **Checkpoint authority choice (RFC-0968-A1 amendment 14 / C-P4):** v1.0 uses the pointer-plus-recompute model: checkpoint stores ONLY `checkpoint_event_id`; auditor re-derives `score_ewma` from retained events. Alternative authoritative-snapshot model deferred.
- [ ] `crates/oct-reputation/Cargo.toml` gains `octo-determin = { path = "../../determin" }` and defines features `default = []`, `stoolap = ["dep:quota-router-storage"]`, `mon = []`, `dc = []`, `marketplace = []`, `wallet = []`. `score_delta`, `score_ewma`, `NormalizerInput.delta`, normalizer outputs, and `update_ewma` parameters/return value are all `octo_determin::Dfp`. **No `f64` anywhere in the persisted reputation data model.**
- [ ] `crates/quota-router-storage/src/migrations.rs` retains `BUILTIN_MIGRATIONS` and appends v003 through v009 in order after v002.
- [ ] **Attestor rate limit (RFC-0968-A1 amendment 16 / I-P7):** `MIN_ATTESTOR_QUORUM = 3` constant; `query_attestations` requires ≥ quorum attestors have observed the event. `GossipCatchUp { attestor_did, since_event_id }` wired over federation.
- [ ] **Federated suspension certificate (RFC-0968-A1 amendment 17 / I-X1):** Signing path emits a signed `FederatedSuspensionCertificate { recorder_did, reason_hash, frozen_at_unix, governance_pubkey, snapshot }`. Election consumers require `freshness_max_secs` import or fail-closed.

#### Record signal transaction integrity (RFC-0968-A1 §3)

- [ ] `record_signal` atomically commits: event INSERT, nine-field aggregate UPSERT (`aggregate.last_event_id` = the new event_id), `RecorderRegistration.last_signal_at_unix = now_unix`, severity self-check, severity-triggered suspension. Concurrent calls for the same recorder are admission-blocked until completion.
- [ ] Monotonicity check is performed under the per-recorder admission lock (closing the pre-lock check race); the lock is acquired before `last_signal_at_unix` is read.

#### Cargo verification

- [ ] `cargo test -p oct-reputation --features stoolap --lib` all pass.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` clean.

### Phase 2: Adapter Shadow-Write (Compatibility Adapters, RFC-0968-A1 amendment 18 / C-P5)

The previous "Phase 2 equivalence via replay" claim is replaced with a **dual-read window** with explicit replacement semantics, because the legacy in-memory stores (`mon::reputation::SlashReputationStore`, `dc::reputation::DcRootedSlashReputationStore`, `quota-router-core::marketplace::ProviderReputationRegistry`) are structurally non-equivalent to a persisted Dfp EWMA aggregate (Session 2 C-P5 / Session 3 I-X5). They do not retain raw event history; the "replay" path is impossible.

- [ ] `SlashReputationStoreCompat (mon)` reads from `ReputationStore` and implements the legacy public API.
- [ ] `DcRootedSlashReputationStoreCompat (dc)` reads from `ReputationStore`, `layer=1`.
- [ ] `ProviderReputationRegistryCompat (marketplace)` reads from `ReputationStore`, `kind=Outcome`, `layer=2`.
- [ ] Shadow-write is best-effort: failures log + continue (don't break existing reads).
- [ ] **Dual-read cutover gate:** retirement of legacy stores is gated on a 24-hour dual-read parity ≥ 0.999 across all `(did, kind, layer)` triples with `total ≥ 100`. Below the threshold, the metric is suppressed.
- [ ] **No `stake / (1 + count)` retention.** Mission 0968-b-marketplace-integration owns the election priority adapter (`election_priority` in RFC-0968 §10) and the `0-100` presentation layer (`round(((score_ewma + 1.0) × 50.0).clamp(0.0, 100.0))` at read time per RFC-0968-A1 §22 / §23.
- [ ] Existing in-memory test suites pass with the compatibility adapter enabled.

### Phase 2.5: Backfill + Reconciliation (RFC-0968-A1 amendment 14 + 18)

- [ ] In-memory stores continue to be authoritative for reads during the dual-read window.
- [ ] Background reconciliation job replays historical events (if any) into `ReputationStore` to seed the persisted aggregates. Backfill events are marked with `payload = b"BACKFILL_V1"` and `received_at_unix = canonicalized_now` (storage-time, not in-memory historical time) to avoid breaking monotonicity going forward.
- [ ] **Pure-Dfp comparisons never use tolerance.** Parity is computed only between two Dfp paths; comparisons involving legacy `f64` history use `1e-6` once because the `f64 → Dfp` conversion introduces a one-time rounding at backfill. A future migration off legacy is exactly `0` tolerance.
- [ ] Parity metrics exported via Prometheus: `reputation_parity_match_count` (counter), `reputation_parity_total_count` (counter). Operators compute `parity_score = match / total`.

### Phase 3: Read Migration (RFC-0968-A1 amendments 19, 20, 23)

- [ ] Adapter reads sourced from `ReputationStore` via the compatibility adapter (Phase 2).
- [ ] In-memory store remains as fallback when storage disabled.
- [ ] **`cross_layer_query` is Class B (RFC-0968-A1 amendment 8):** Rust-side aggregation over hydrated Dfp BLOBs from `reputation_aggregates`, with sample-count weighting `weight ∝ min(samples_k, MAX_RELIABILITY_SAMPLES)` (Bayesian shrinkage) and minimum evidence threshold `samples_total ≥ 30` masking below-threshold rows from the composite.
- [ ] **`election_priority` adapter spec in RFC-0968 §10 (RFC-0968-A1 amendment 20):** `election_priority(candidate_did, stake, store, layer, now_unix) -> Result<Option<u128>, ReputationError>`. Tests cover overflow, NaN, ±Inf, score = 0 at samples=0.
- [ ] CLI surface `quota-router reputation show --did <did>` (RFC-0968-A1 amendment 24). Owned by mission 0968-b-marketplace-integration's Phase 3 acceptance.
- [ ] Daemon restart preserves `score_ewma` across all three adapters (verified by integration test).
- [ ] Parity check continues to run in production (drift detection).

### Phase 4: Federation (Gated on claimed/Accepted 0855p-b, RFC-0968-A1 amendments 21, 22)

- [ ] **Gating (RFC-0968-A1 amendment 22):** Phase 4 ships ONLY when `missions/0855p-b-cross-mission-reputation.md` (or its replacement canonical mission) is in `claimed/` or accepted. The Phase 4 PR MUST reference a claimed 0855p-b in its description.
- [ ] **Authority model (RFC-0968-A1 amendment 21):** Gossip envelopes carry `event_id`, `recorder_did`, `recorder_signature` (authoritative), `source_mission`, `source_domain`. Coordinator signature MAY authorize the source mission but cannot replace recorder authorization. Attestor signature is non-authoritative transport metadata.
- [ ] **Pubkey-keyed topics replaced (RFC-0968-A1 amendment 22, C-X5):** Topics key on canonical DID or stable lineage identifier. Replication payload includes `rotation_receipt` + `old_did/new_did` lineage map. Stale pubkey mappings rejected at gossip ingress with `GossipEnvelopeInvalid`.
- [ ] **Gossip safety (RFC-0968-A1 amendment 16 / I-P7):** `MIN_ATTESTOR_QUORUM = 3` per-event attestation threshold. Attestor rate-limit enforcement.
- [ ] Mission 0855p-b (claimed) owns the gossip protocol; this mission provides the storage substrate.

### Phase 5: On-Chain Anchoring — DEFERRED (mission 0968a per Round 1 H11, RFC-0955-R1 follow-up)

- [ ] **NOT IN THIS MISSION.** Phase 5 is `missions/deferred/0968a-reputation-anchoring.md`. Unblock requires:
  - RFC-0955 Accepted (currently Draft; see `rfcs/draft/economics/0955-model-liquidity-layer.md`).
  - RFC-0955-R1 amendment **deployed**: `reputation:blake3_digest` (32-byte) field replaces `reputation: u64` on `ComputeAsset` per `rfcs/draft/economics/0955-model-liquidity-layer.md:263` + `§ Reputation Anchoring Amendment (RFC-0955-R1, 2026-07-26)`.
  - Anchor commitment envelope is `BLAKE3(BLAKE3_REPUTATION_ANCHOR_DOMAIN || did || kind || layer || last_event_id || DfpEncoding::from_dfp(&score_ewma).to_bytes() || last_event_unix || samples || severity_total)` per RFC-0955-R1 §"Wire contract".
- [ ] See `missions/deferred/0968a-reputation-anchoring.md` for the canonical mission scope and acceptance criteria.

### Implementation Guide

`docs/07-developers/reputation-registry-implementation-guide.md` (new, follow-on):

- Module tree:

  ```text
  crates/oct-reputation/
  ├── Cargo.toml
  ├── src/
  │   ├── lib.rs
  │   ├── core.rs
  │   ├── event.rs
  │   ├── recorder.rs
  │   ├── reader.rs
  │   ├── auditor.rs
  │   ├── attestor.rs
  │   ├── rotation.rs
  │   ├── suspension.rs
  │   ├── retention.rs
  │   ├── error.rs
  │   └── constants.rs
  ├── src/storage/
  │   ├── mod.rs
  │   └── stoolap.rs
  ├── src/kinds/
  │   ├── mod.rs
  │   ├── mon.rs
  │   ├── dc.rs
  │   ├── marketplace.rs
  │   └── wallet.rs
  └── migrations/
      ├── v003__reputation_events.sql
      ├── v004__reputation_aggregates.sql
      ├── v005__reputation_rotations.sql
      ├── v006__reputation_attestations.sql
      ├── v007__aggregate_checkpoints.sql
      └── v008__recorder_registration.sql
  ```

- Canonical Rust snippets for `SignalEvent`, `ReputationStore`, `StoolapReputationStore`, `Did`, `RecorderRegistration`.
- Error type definitions with `thiserror`.
- Migration runner hookup: retain `BUILTIN_MIGRATIONS` in quota-router-storage and append `oct_reputation::migrations::v003__reputation_events()` through `v008__recorder_registration()`.
- Adapter mapping rules (RFC-0968 §7) with equivalence test recipes.

## Claimant

@cipherocto + @mmacedoeu

## Pull Request

# (TBD)

## Location

- New: `crates/oct-reputation/Cargo.toml` (workspace member; `stoolap`, `mon`, `dc`, `marketplace`, `wallet` features)
- New: `crates/oct-reputation/src/{lib.rs, core.rs, event.rs, recorder.rs, reader.rs, auditor.rs, attestor.rs, rotation.rs, suspension.rs, retention.rs, error.rs, constants.rs}`
- New: `crates/oct-reputation/src/storage/{mod.rs, stoolap.rs}`
- New: `crates/oct-reputation/src/kinds/{mod.rs, mon.rs, dc.rs, marketplace.rs, wallet.rs}`
- New: `crates/oct-reputation/migrations/v003__reputation_events.sql`
- New: `crates/oct-reputation/migrations/v004__reputation_aggregates.sql`
- New: `crates/oct-reputation/migrations/v005__reputation_rotations.sql`
- New: `crates/oct-reputation/migrations/v006__reputation_attestations.sql`
- New: `crates/oct-reputation/migrations/v007__aggregate_checkpoints.sql`
- New: `crates/oct-reputation/migrations/v008__recorder_registration.sql` (Round 6 C1 + L5)
- Modified: `crates/quota-router-storage/src/migrations.rs` (retain `BUILTIN_MIGRATIONS`; append calls to `oct_reputation::migrations::v003__reputation_events()` through `v008__recorder_registration()`)
- Modified: `crates/octo-network/Cargo.toml` (add `oct-reputation` with `mon`, `dc`)
- Modified: `crates/quota-router-core/Cargo.toml` (add `oct-reputation` with `marketplace`)
- Modified: `crates/octo-wallet/Cargo.toml` (add `oct-reputation` with `wallet`)
- Modified: `crates/octo-network/src/mon/reputation.rs` (shadow-write)
- Modified: `crates/octo-network/src/dc/reputation.rs` (shadow-write)
- Modified: `crates/quota-router-core/src/marketplace/scoring.rs` (shadow-write)
- New (follow-on): `docs/07-developers/reputation-registry-implementation-guide.md`

## Complexity

Medium-large. Two-table schema + canonical DID + signed events + adapter equivalence tests + backfill/reconciliation + recorder state machine. ~800-1000 LOC + tests.

## Prerequisites

- stoolap fork available at `feat/blockchain-sql` (per `feedback_stoolap-persistence.md`).
- Existing `crates/quota-router-storage` MVCC scaffolding in place.
- `BUILTIN_MIGRATIONS` array exists at `crates/quota-router-storage/src/migrations.rs`; v003 through v007 append in order.
- `cipherocto-encoding` crate for canonical_ser (delegated encoding).

## Notes

### Why blocked, now unblocked?

Originally blocked per BLUEPRINT §"Mission Lifecycle" rule "Missions REQUIRE an approved RFC": Missions are the execution layer (HOW?) and require an Approved RFC (the specification layer). RFC-0968 was not yet final as of mission creation; the mission file captured the full implementation plan ahead of RFC acceptance. Unblocked 2026-07-25 when RFC-0968 status change recorded per BLUEPRINT Mission Lifecycle.

### Why canonical `did:octo:b<52>` only?

Per Round 1 finding C3 + Round 2 C1 + M15: the previous design accepted both `did:octo:` strings AND raw 32-byte keys, enabling reputation laundering via dual encoding. The Round 2 fix uses `did:octo:b<52>` (multibase `b` = base32 standard, lowercase no-padding, total 62 chars). `Did::parse` rejects raw 32-byte keys and legacy `did:octo:z...` strings. `Did::rotate` requires both `old_pubkey` and `new_pubkey` (binding `blake3(pubkey) == did.hash_part` for both) and is consumed via `consume_rotation_receipt` (one-time per pair).

### Round 2 H10 — Federated recorder authority

Per Round 2 C4: Attestors are replication-only peers. The previous "fall back to Attestor signature" design has been removed; recoders are the only source of truth. A gossip event with no recorder signature is rejected with `ReputationError::RecorderSignatureMissing`.

### Why signed events + recorder stake?

Per Round 1 finding C2: the previous design had `source: RecorderId` on `SignalEvent` but no signature, no signed-byte encoding, and no verification. Recorder trust was implicit. The new design requires ed25519 signature over `BLAKE3(BLAKE3_REPUTATION_EVENT_DOMAIN || canonical_ser(event_unsigned))`, recorder registration with ≥1000 OCTO role-token stake, and a state machine (`Active → Suspended → Revoked`). `record_signal` rejects events whose recorder is revoked, suspended, or under-staked.

### Why shadow-write first (Phase 1-2)?

Phase 1-2 ship persistence without breaking existing reads. Phase 2.5 reconciles parity before Phase 3 switches reads. This avoids a flag-day migration and lets operators opt-in per-layer.

### Why two tables not one (Round 1 finding M1)?

Per the research update: a single combined table locks the aggregate shape to the event log. Two tables (`reputation_events` append-only, `reputation_aggregates` derived) enable:

- Event replay for audit and federation without aggregate lock contention.
- Aggregate rebuild from v007 pruned-prefix checkpoints plus retained events when schema evolves or corruption is detected.
- Independent retention policies (events 90-day, aggregates unbounded).

### Why stoolap, not raw SQLite?

Per `feedback_stoolap-persistence.md` memory: stoolap is the CipherOcto fork. RAW SQLite is forbidden. Migration files land in `crates/oct-reputation/migrations/` and are referenced by `BUILTIN_MIGRATIONS` in `crates/quota-router-storage/src/migrations.rs`.

### Cross-mission gossip (mission 0855p-b)

This mission provides storage for mission 0855p-b's gossip replication. Mission 0855p-b (Cross-mission coordinator reputation) owns the gossip protocol; this mission owns the persistence. Phase 4 is the federation.

### On-chain anchoring (DEFERRED to mission 0968a)

Per Round 1 finding H11: Phase 4 originally contained BOTH gossip federation AND on-chain anchoring. The combined ownership was contradictory. Phase 4 is now gossip federation only (mission 0855p-b scope); on-chain anchoring is extracted to a NEW deferred mission `0968a-reputation-anchoring` that scopes the RFC-0955 follow-up. The single-table design supports on-chain binding by extending `SignalEvent` with an `anchor_tx_hash: Option<[u8;32]>` field — that extension lives in mission 0968a, not here.

### Why not a reputation token?

Per `docs/04-tokenomics/token-design.md` §12, reputation is a **mechanism** (slash-count + outcome-based), not a tradable balance. No `OCTO-R` introduced. If a token is later warranted, this mission's `ReputationStore` is the natural backing store.

### Determinism class

Per RFC-0008: EWMA is Class B (deterministic given inputs + alpha). Storage reads are Class A. The adapter's `update_ewma` is a pure function over `octo_determin::Dfp`: same inputs ⇒ byte-identical `DfpEncoding::from_dfp(&score_ewma).to_bytes()` output across compilers and platforms (v3.3-r18 C9: the `Dfp::to_bytes()` method does not exist; use `DfpEncoding::from_dfp(&d).to_bytes()`). **v3.0-r15 (Gap 9):** v1.0 uses `octo_determin::Dfp` per RFC-0104. Cross-replica determinism is achieved at the type level — no `f64` migration path exists. The 24-byte BLOB encoding is bit-deterministic across compilers and platforms, so two replicas running the same EWMA sequence produce byte-identical `score_ewma` BLOBs. Authoritative governance writes are Class B iff `GovernanceRegistry::lookup_at_snapshot` is deterministic for the explicit `(pubkey, GovernanceSnapshot)` pair; the store does not assert this property, and deployment configuration is the source of truth.

### Why fixed section numbers in RFC?

Per Round 1 finding H6: the RFC's section numbers are now fixed (`§3 = Recorder Authorization`, `§10 = Core Interfaces`, `§11 = Audit Trail`, `§12 = Federation`). Mission type-coverage cross-references these explicit numbers so reviewers can navigate.

### Migration naming (Round 1 finding H10)

Migration files use contiguous `v003` through `v007` numeric prefixes, not date-based names. v006 is the Phase 1 attestation table and v007 is the aggregate-checkpoint table; the deferred mission 0968a `reserved slot v006` phrase is a non-binding planning label, not a second migration claim. Files land in `crates/oct-reputation/migrations/` (consistent with `v001__create_asks_table.sql` and `v002__create_asks_indexes.sql`). `BUILTIN_MIGRATIONS` is appended, never reordered.

### Unblock Workflow

Resolved 2026-07-25: RFC-0968 status change recorded, file renamed from `0968-reputation-persistence-blocked.md` → `0968-reputation-persistence.md`, BLOCKED banner removed, Status set to Claimed, Claimants assigned. PR is now the next step.

### Changelog

- **v3.0-r15 (Gap 9, 2026-07-25):** switch `f64` to `octo_determin::Dfp` per RFC-0104. `SignalEvent.score_delta`, `ReputationAggregate.score_ewma`, `update_ewma` parameters/return, `NormalizerInput.delta`, all normalizer outputs, `CrossLayerResult.composite_score`, `SlidingWindowResult.score_delta`, and `ReplayRecord.aggregate_evolution` all move from `f64` to `octo_determin::Dfp`. SQL: `score_delta REAL` / `score_ewma REAL` / `score_ewma_at_checkpoint REAL` → `BLOB NOT NULL CHECK (length(...) = 24)` (canonical 24-byte `DfpEncoding::to_bytes()` form). Mission Phase 1 acceptance adds `octo-determin = { path = "../../determin" }` to `crates/oct-reputation/Cargo.toml` and adds feature-gated `oct-reputation` dependencies to `octo-network`, `quota-router-core`, and `octo-wallet`. Cross-replica determinism is achieved at the type level; no `f64` migration path exists. RFC-0104 DFP migration is no longer future work — `Dfp` is the v1.0 type.
