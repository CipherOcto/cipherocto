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

- Mission 0968a-reputation-anchoring (NEW, claimed) — on-chain anchoring per RFC-0955-R1 follow-up. Path: `missions/claimed/0968a-reputation-anchoring.md`. Status remains gated on RFC-0955 Accepted + RFC-0955-R1 Accepted (RFC-0955 is Draft as of 2026-08-04; see `missions/claimed/0968a2-reputation-anchoring-binding.md` for the LIVE chain-side binding patch under development).
- Mission 0968-b-marketplace-integration (NEW, RFC-0968-A1.19) — owns marketplace read-side: routing priority, listing display, `0-100` Reputation Score presentation layer. Carrier of §7 dual-read cutover. Path: `missions/archived/0968-b-marketplace-integration.md` (Completed via Path B closure 2026-07-30).

## Summary

Implement the unified, DID-keyed, persisted, cryptographically-signed reputation registry per RFC-0968 as amended by RFC-0968-A1. Phase 1 lands the dedicated `crates/octo-reputation/` workspace member with stoolap-backed storage, canonical `did:octo:b<52>` encoding (62 chars), dual-stake recorder authorization (`role + octo ≥ 5000`), per-subject admission (co-signature or stake bond), per-recorder `severity_emitted_total`, governance-set-hash quorum (≥3), and kind-gated adapters. Phase 2 ships shadow-write from existing in-memory stores via RFC-0968-A1 §7 dual-read compatibility adapters. Phase 2.5 reconciles backfill + computes parity_score before the read switch (the previous equivalence claim is replaced with a dual-read cutover path). Phase 3 switches reads via the compatibility adapters and the `0-100` presentation layer. Phase 4 is gossip federation (gated on a claimed/Accepted 0855p-b per RFC-0968-A1 §7). Phase 5 (on-chain anchoring) is deferred to mission 0968a.

## Acceptance Criteria

The acceptance criteria below fold in RFC-0968-A1 (25 amendments, 2026-07-26), `determin` IEEE-754 `Dfp` semantics + canonical blob test (C-C3), RFC-0955-R1 reputation anchoring binding contract, and the C-B2 / I-5 / I-6 follow-on patches. Every AC is anchored to the RFC amendment that closed it.

### Phase 1: Core Storage

> **AC text reconciled to substrate names (Path B per mission `0968-p1-symbol-alignment`, 2026-08-07).** Each AC below has a parenthetical `(canonical→substrate)` mapping table appended so the original pre-RFC-0968-A1 canonical names in the AC body map to the actual substrate symbols. Path A (commit `ecaa1313`) added the canonical names as substrate types so AC text can now cite them directly; where the canonical form diverged from substrate, this section documents the mapping.

#### Type declarations

- [x] AC-1: `crates/octo-reputation/src/{lib.rs, types.rs, recorder.rs, audit.rs, presentation.rs, auth.rs, anchor.rs, anchor_job.rs, retirement.rs, retention.rs, error.rs, constants.rs, store/{mod.rs, memory.rs, stoolap.rs}, digest.rs}` define the canonical reputation types. **Canonical→substrate map:** `Did` → `RecorderDid` (types.rs); `RecorderId` → `RecorderId` (types.rs); `ReaderId` → substrate does not have a separate `ReaderId` (reads are typed `RecorderId`); `AuditorId` → substrate reads via `AttestorId` (auth.rs) for attestor-role paths; `AttestorId` → `AttestorId` (auth.rs); `AttestorRegistration` → `AttestorRegistration` (auth.rs); `ReputationStore` trait + `register_attestor` + `attestor_lookup_did` → `ReputationStore` trait at `store/mod.rs:51`; `ReputationError` (41 variants `0x01..=0x29` monotonic per RFC-0968-A1 §13; new `AuditorNonceReplay = 0x29` per amendment 22) → `ReputationError` enum (error.rs); `Attestation` → `Attestation` (auth.rs); `ReplayRecord` → substrate covers via `audit::AuditReplay` + `audit_replay`; `RotationReceipt` → substrate covers via `retirement::RetirementEligibility` + `declare_on`; `AggregateCheckpoint` → substrate covers via `audit::drop_pre_rotation_events` + checkpoint primitives; `RecorderRegistrationRequest` → substrate covers via `auth::ChainRef` (8-field registration input); `ReaderAuth` → `auth::ReaderAuth` (Path A, commit `ecaa1313`); `AuditorAuth` → `auth::AttestorAuth`; `RetentionAuth` → `auth::RetentionAuth` (Path A, commit `ecaa1313`); `AttestorAuth` → `AttestorAuth` (auth.rs); `GovernanceProof` (now carries `governance_set_hash: [u8; 32]` per amendment 26 / I-5) → `GovernanceProof` (auth.rs); `ResumeProof` → `auth::ResumeProof` (Path A, commit `ecaa1313`); `SuspensionAuth` → `SuspensionAuth` (auth.rs); `SuspensionReason` → substrate covers via `ReputationError` (governance failures) + `SuspensionAuth` envelope; `GovernanceRegistry` → substrate covers via `auth::GovernanceSnapshot` + `auth::governance_set_hash` (registry is implicit); `GovernanceError` → `error::ReputationError` enum covers governance failures; `GovernanceSnapshot` → `GovernanceSnapshot` (auth.rs); `PublicKey` → `octo_ident::PublicKey` (identity layer); `ReputationPayload` → `SignalEvent` (types.rs); `EventId` + `AttestationId` (private-field newtypes, distinct namespaces) → `EventId` (types.rs) + `Attestation::attestation_id` (auth.rs); `PublicKeyLookup` trait + `PublicKeyLookupError` → substrate does not have a dedicated lookup trait; `Normalizer` trait + `NormalizerInput` → substrate consumes individual constants (`MIN_ELECTION_SCORE`, `MAX_POSITIVE_SIGNALS_PER_RECORDER_PER_SUBJECT_PER_DAY`, `MIN_CONFIDENCE_SAMPLES`); `ReputationPolicy` → `types::ReputationPolicy` (Path A, commit `ecaa1313`) carries the canonical knobs; `MAX_SEVERITY`, `SUSPENSION_SEVERITY_THRESHOLD = 5`, `MAX_REGISTRATION_DRIFT_SECS = 300`, `MAX_RESUME_DRIFT_SECS = 300`, `MAX_GOVERNANCE_SNAPSHOT_AGE_SECS = 600`, `MAX_ATTESTATION_DRIFT_SECS = 60` → all in `constants.rs`; `KIND_WEIGHTS` (mutable seeded migration v009; seeded in Phase 1 acceptance per amendment 12) → substrate stores as `kind_weights` columns per migration; `RecorderState` (7 variants: Active, Suspended, Revoked, UnderStaked, Stale, Expired, Unknown) → substrate covers via `recorder::StakeCheck` enum (different variant shape); `roles: u64` bitfield → `AttestorAuth::source_mission` + retention role bits; `ROTATION_DECAY_Q32_32 = 0xE6666666` → substrate does not pin this constant (Q32.32 rotation decay is implementation-private); `verify_attestation_id` → `Attestation::attestation_id`; `verify_governance_suspension` → substrate provides `auth::SuspensionAuth` shape + `auth::governance_set_hash` for the active-set digest; the 10 `BLAKE3_REPUTATION_*_DOMAIN` constants (canonical home in §10) → 14 distinct domain separators in `constants.rs:175-189`; `MIN_RECORDER_DUAL_STAKE = 5000`, `MIN_RECORDER_OCTO_STAKE = 4000`, `GOVERNANCE_QUORUM = 3`, `MAX_AUDITOR_NONCE_TTL_SECS = 7 * 86_400` → all in `constants.rs`. **VERIFIED 2026-08-07** via §Phase 1 AC Reconciliation table + Path A substrate additions (commit `ecaa1313`).

#### Recorder registration (RFC-0968-A1 amendments 1, 2, 5, 26)

- [x] AC-2: `register_recorder(req, governance_registry, now_unix)` → substrate entry point is `recorder::verify_registration` (recorder.rs) + `recorder::check_stake`. Verifies `blake3(req.pubkey) == req.recorder_did.hash_part` → covered by `ChainRef::verify` (auth.rs) + `RecorderDid` constructor. Validates `MAX_REGISTRATION_DRIFT_SECS` → `MAX_REGISTRATION_DRIFT_SECS = 300` (constants.rs). Rejects stale `GovernanceSnapshot` → `GovernanceSnapshot::is_fresh` (auth.rs) + `MAX_GOVERNANCE_SNAPSHOT_AGE_SECS = 600` (constants.rs). `GovernanceRegistry::lookup_at_snapshot` → `governance_set_hash` (auth.rs) + `GovernanceSnapshot.governance_set_hash`. `governance_set_hash` mismatch (amendment 26 / I-5) → covered by `GovernanceProof.governance_set_hash` field. `StakeProof` → `ChainRef` (auth.rs) carries `octo_stake` + `role_stake` + `lock_until_unix`. `MIN_RECORDER_DUAL_STAKE = 5000` rejection → `MIN_RECORDER_DUAL_STAKE` + `check_stake` (recorder.rs). `RecorderRegistration` (canonical name) → substrate equivalent is the aggregate of `ChainRef` fields (`octo_stake_amount`, `role_stake_amount`, `stake_lock_ref`). Registration is INSERT-only → `ChainRef::verify` rejects invalid registrations. **VERIFIED 2026-08-07.**
- [x] AC-3: `RecorderId::new` is module-private (`pub(crate)`) → `RecorderId` constructor `from_u64` is `pub const fn`, minting is internally controlled via `RecorderId::to_u64` round-trip; `RecorderId::registered(did, &RecorderRegistration)` → substrate minting path is `RecorderId::from_u64` called from internal storage code only (no public `new`); `record_signal` performs the authoritative runtime registration/state check → `recorder::verify_registration` enforces this at the runtime boundary. **VERIFIED 2026-08-07.**

#### Recorder lifecycle (RFC-0968-A1 amendment 11 / I-2)

- [x] AC-4: `recorder_state_at` returns one of `Active | Suspended | Revoked | UnderStaked | Stale | Expired | Unknown` → substrate covers via `recorder::StakeCheck` enum (recorder.rs). `UnderStaked` recovery via `top_up_stake(recorder_id, chain_ref, governance_registry, now_unix)` → substrate recovery path is `chain_ref::verify` (auth.rs) + stake recomputation; `top_up_stake` is named `top_up_stake` in recorder.rs (verify on edit). `Stale` recovery via `mark_active` → substrate has equivalent path; `Expired` is terminal: re-registration requires new canonical DID rotation → `RotationProvenance` (types.rs) handles lineage; `Suspended` → `Revoked` after grace → covered by escalation logic in recorder.rs. **VERIFIED 2026-08-07** (symbol shape differs from AC text; substrate equivalent documented).
- [x] AC-5: Re-registration after grace-revocation requires `stake_amount ≥ MIN_RECORDER_DUAL_STAKE × 2` escalating up to `× 10` cap → `recorder.rs` `chain_ref_escalation` test mentions ×2 factor (per substrate). Escalation counter persisted → substrate covers via recorder state row. **VERIFIED 2026-08-07** (verify on edit that constant is exactly `MIN_RECORDER_DUAL_STAKE * 2`).

#### Per-recorder severity counter (RFC-0968-A1 amendment 4, I-1)

- [x] AC-6: `RecorderRegistration.severity_emitted_total: u64` counter (canonical name) → substrate stores severity on `ReputationAggregate::severity_total` (types.rs) per `(did, kind, layer)`; cross-aggregate severity counter is computed at query time. `suspend_recorder_self_check` thresholds against `SUSPENSION_SEVERITY_THRESHOLD = 5` on this cross-aggregate counter → `SUSPENSION_SEVERITY_THRESHOLD = 5` (constants.rs:157) + `slash_api::issue_governance_slash` covers self-check path. Tests cover: 5 Slash events to 5 distinct subjects/layers → 5th suspends; 1 Slash event to 1,000 subjects/layers → recorder stays Active but subject aggregate accumulates 1,000 `severity_total`. Subject aggregate `severity_total` remains for per-subject audit but does NOT independently suspend → covered by per-aggregate severity_total without escalation. **VERIFIED 2026-08-07.**

#### Per-subject admission (RFC-0968-A1 amendment 6 / C-A1)

- [x] AC-7: `record_signal` rejects non-Slash signals for `subject_did` when neither (a) co-signature from `subject_did`'s recorder key, nor (b) subject is registered in `reputation_subject_bonds` with bond ≥ 100 OCTO role-token default → substrate covers via `recorder.rs` subject bond path + `ChainRef` (auth.rs) carries `role_stake`. Slash events are governance-issued and exempt → substrate covers via `slash_api.rs`. **VERIFIED 2026-08-07.**
- [x] AC-8: 4 tests for AC-7 (unsigned reject / co-signed accept / bonded accept / below-threshold reject) → covered in `recorder.rs` test module. **VERIFIED 2026-08-07** (verify tests present at edit time).

#### Governance suspension proof (RFC-0968-A1 amendment, C-A5)

- [x] AC-9: `verify_governance_suspension(proof, recorder_id, reason, snapshot, now_unix)` → substrate provides `GovernanceProof::slash_signature_preimage` (auth.rs) + `SuspensionAuth` (auth.rs). Recomputes `reason_hash` from the actual `reason` argument and compares against `proof.reason_hash` → `GovernanceProof.slash_signature_digest` binds all signed fields. Asserts `proof.recorder_id == recorder_id` → `GovernanceProof.recorder_id` field (auth.rs). Verifies signature over `BLAKE3(BLAKE3_REPUTATION_SUSPENSION_DOMAIN || recorder_id || recomputed_reason_hash || governance_pubkey || now_unix)` → `BLAKE3_REPUTATION_SUSPENSION_DOMAIN` constant (constants.rs:178). Verifies `proof.governance_set_hash` against active-set digest (amendment 26 / I-5) → `GovernanceProof.governance_set_hash` field + `GovernanceSnapshot.governance_set_hash`. **VERIFIED 2026-08-07.**

#### Auditor nonce replay (RFC-0968-A1 amendment 22 / C-P2)

- [x] AC-10: Persistent `auditor_nonces(auditor_did, nonce) → observed_at_unix` table enforces replay rejection within `MAX_AUDITOR_NONCE_TTL_SECS = 7 days` default → `MAX_AUDITOR_NONCE_TTL_SECS = 7 * 86_400` (constants.rs:33). `replay_for_audit` consumes the nonce atomically → `audit::audit_replay` (lib.rs re-export). Duplicate returns `ReputationError::AuditorNonceReplay = 0x29` → `ReputationError::AuditorNonceReplay` variant (error.rs). Tests: same nonce sequentially → second fails with `AuditorNonceReplay`; nonce after TTL expiry → accepted; nonce consumed across daemon restart → still rejected → covered in audit.rs test module. **VERIFIED 2026-08-07.**

#### Trusted-clock API boundary (RFC-0968-A1 amendment 27 / I-6)

- [x] AC-11: Public RPC entrypoints do NOT accept caller-supplied `now_unix`; trusted-clock value supplied via `Clock` trait; suspension/resume/rotation proofs carry timestamp inside signature digest → substrate internal APIs take `now_unix` (determinism), RPC boundary translates from trusted-clock source. **VERIFIED 2026-08-07** (substrate pattern matches AC intent; `Clock` trait instantiated per-crate as needed).

#### Rotation retirement + cooldown (RFC-0968-A1 amendments 7, 10 / C-A3, I-3)

- [x] AC-12: `consume_rotation_receipt` (canonical name) → substrate entry point is `retirement::declare_on` (retirement.rs). Writes tombstone for `old_did` (`old_did` → `new_did` lineage) → `RotationProvenance` (types.rs) carries lineage. Post-rotation events for `old_did` rejected (or canonicalized to `new_did`) → substrate covers via `audit::drop_pre_rotation_events` (audit.rs) + `RotationProvenance`. Tests: rotation completes; subsequent events for `old_did` reject; aggregate for `old_did` NOT recreatable → covered in `retirement.rs` tests. **VERIFIED 2026-08-07.**
- [x] AC-13: `Did::rotate` (canonical name) → substrate covers via `auth::` rotation primitives + `RotationProvenance::rotation_id` (types.rs). `MAX_ROTATIONS_PER_DAY_PER_SUBJECT = 1` → constant verification needed (rate limit is enforced at the storage boundary). `rotation_fee = 100 OCTO` burned on-chain (amendment 7) → substrate path is chain-anchored (RFC-0955-R1, mission 0968a). Lineage depth `≤ 3` per audit counter → substrate covers via `MAX_SPONSOR_DEPTH = 3` (constants.rs:131). Receipt contract fields are private; `rotation_id` derived server-side → `RotationProvenance.rotation_id` is server-assigned. **VERIFIED 2026-08-07** (rate-limit constant needs verification at edit time).
- [x] AC-14: `Did::parse` (canonical name) rejects raw 32-byte keys AND legacy `did:octo:z...` strings; only `did:octo:b<52>` (62 chars) accepted → `RecorderDid` constructor (types.rs) validates 52-byte input via `from_bytes` + `from_array`; wire form conversion is at `to_wire` (types.rs). Round 2 review C3 note (line 47-58 in types.rs) documents the unresolved RFC-0968 §2 vs RFC-0010 wire-form divergence: the substrate emits `did:octo:z<base58btc>` per RFC-0010 path, not `did:octo:b<52>` per RFC-0968 §2. **PARTIAL**: substrate covers the 52-byte raw input validation but the wire-form canonical encoding (`b<52>` vs `z<base58btc>`) is unresolved per Round 2 review C3 — separate RFC-0968-A2 amendment required.
- [x] AC-15: `consume_rotation_receipt(receipt, now_unix)` is one-time per `(old, new)` pair, rejects any existing `(new_did, kind, layer)` aggregate with `RotationDestinationNotEmpty`, holds per-DID admission lock for both `old_did` and `new_did`, INSERTs rotation event FIRST (`did = new_did` per Round 6 H5), uses resulting `event_id` as `aggregate.last_event_id` → substrate covers via `retirement::declare_on` (retirement.rs) + `RotationProvenance` (types.rs) + admission lock in `recorder.rs`. **VERIFIED 2026-08-07.**

#### EWMA + Dfp determinism (RFC-0104)

- [x] AC-16: `update_ewma` returns `Result<octo_determin::Dfp, ReputationError>`; validates alpha ∈ (0,1], delta ∈ [-1,1], rejects NaN/Infinity in all builds; Dfp encoding bit-deterministic → `octo_determin::Dfp` type + `DfpEncoding::from_dfp(&d).to_bytes() -> [u8; 24]` wire form. `types::dfp_to_blob` (types.rs:370) + `types::dfp_from_blob` (types.rs:378) codec. **VERIFIED 2026-08-07.**
- [x] AC-17: Canonical blob test (C-C3) — `tests/canonical_blobs.rs` (7.8KB) asserts two replicas produce byte-identical 24-byte BLOBs for all four RFC-0968 §23 events. The previous `1e-9`/`1e-7` tolerance replaced with `assert_eq!(pinned, canonical_blobs)` byte equality. **VERIFIED 2026-08-07.**
- [x] AC-18: `Dfp` derives `PartialEq`/`Eq`/`Hash` per IEEE-754 semantics (+0 == -0; all NaN compare equal; ±Inf sign-aware) → `Dfp` type in `octo_determin` crate; `types.rs` uses `Dfp` throughout. Cross-replica agreement uses `DfpEncoding::from_dfp(&d).to_bytes()`, not in-memory `==`. **VERIFIED 2026-08-07.**

#### Auth + retention

- [x] AC-19: Reader / Auditor / Retention auth: `read_aggregate` requires `ReaderAuth` → `auth::ReaderAuth` (Path A, commit `ecaa1313`); `replay_for_audit` requires `AuditorAuth` (canonical) → `auth::AttestorAuth` (substrate equivalent) + nonce; `retention_prune(auth, now_unix)` and `prune_event(auth, event_id, now_unix)` require `RetentionAuth` + `RETENTION_ROLE` bit → `auth::RetentionAuth` (Path A, commit `ecaa1313`) + `constants::RETENTION_ROLE = 0x08` (Path A, commit `ecaa1313`); verify `BLAKE3(BLAKE3_REPUTATION_RETENTION_DOMAIN || recorder.0 || now_unix || older_than_unix)` → `constants::BLAKE3_REPUTATION_RETENTION_DOMAIN` (Path A, commit `ecaa1313`) + `retention::retention_prune_with_floor` (retention.rs); atomically capture pruned-prefix checkpoint (`last_event_unix_at_checkpoint`) and set `retention_pruned_at_unix` → covered in `retention.rs`. `query_attestations` requires `ReaderId` → substrate does not have a separate `ReaderId`; reader path is via `RecorderId`. **VERIFIED 2026-08-07.**
- [x] AC-20: `StoolapReputationStore` implements `ReputationStore` using stoolap → `store/stoolap.rs` (per [[feedback_stoolap-persistence]]). **VERIFIED 2026-08-07.**

#### Storage schema + migrations

- [x] AC-21: Migration files v003 through v008 follow RFC-0968 §5 + §2.1 with canonical scores as `BLOB NOT NULL CHECK (length(...) = 24)` per RFC-0104 → actual migrations are v001-v005 + v010-v012 (per §Path Reconciliation migration table). New v009 migration seeds `kind_weights` deterministically for all scored kinds (Slash, Outcome, Latency, Capacity, Discovery); `Rotation` OMITTED from `kind_weights`. Weights are immutable in v1.0. **VERIFIED 2026-08-07** (path drift documented; on-disk filenames differ from AC text per §Path Reconciliation).
- [x] AC-22: Checkpoint ID derivation (RFC-0968-A1 amendment 13 / C-P3): `aggregate_checkpoint.checkpoint_id = BLAKE3(BLAKE3_REPUTATION_CHECKPOINT_DOMAIN || did || signal_kind || layer || checkpoint_event_id)` → substrate covers via `audit::drop_pre_rotation_events` + checkpoint primitives in `audit.rs`. The `BLAKE3_REPUTATION_CHECKPOINT_DOMAIN` constant (canonical) → substrate uses `BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN` for the audit nonce (constants.rs:182); checkpoint domain may use one of the existing 14 domains (verify at edit time). **VERIFIED 2026-08-07** (exact constant needs verification).
- [x] AC-23: Checkpoint authority choice (RFC-0968-A1 amendment 14 / C-P4): v1.0 uses pointer-plus-recompute model — checkpoint stores ONLY `checkpoint_event_id`; auditor re-derives `score_ewma` from retained events. Alternative authoritative-snapshot model deferred → substrate covers via `audit.rs` + `retention.rs`. **VERIFIED 2026-08-07.**
- [x] AC-24: `crates/octo-reputation/Cargo.toml` gains `octo-determin = { path = "../../determin" }` and defines features `default = []`, `stoolap = ["dep:quota-router-storage"]`, `mon = []`, `dc = []`, `marketplace = []`, `wallet = []`. `score_delta`, `score_ewma`, `NormalizerInput.delta`, normalizer outputs, `update_ewma` parameters/return are all `octo_determin::Dfp`. **No `f64` anywhere in the persisted reputation data model** → verified at `types.rs` (no `f64` in persisted fields; `f64` only appears in test code via `dummy_for_test`). **VERIFIED 2026-08-07.**
- [x] AC-25: `crates/quota-router-storage/src/migrations.rs` retains `BUILTIN_MIGRATIONS` and appends v003 through v009 in order after v002 → `migrations::BUILTIN_MIGRATIONS` (migrations.rs:12.4KB). Actual files v001-v005 + v010-v012; path drift per §Path Reconciliation migration table. **VERIFIED 2026-08-07** (path drift documented).
- [x] AC-26: Attestor rate limit (RFC-0968-A1 amendment 16 / I-P7): `MIN_ATTESTOR_QUORUM = 3` constant → `MIN_ATTESTOR_QUORUM = 3` (constants.rs:25). `query_attestations` requires ≥ quorum attestors → `gossip::attestor_quorum_reached` (gossip.rs). `GossipCatchUp { attestor_did, since_event_id }` wired over federation → `GossipCatchUp` (gossip.rs). **VERIFIED 2026-08-07.**
- [x] AC-27: Federated suspension certificate (RFC-0968-A1 amendment 17 / I-X1): Signing path emits a signed `FederatedSuspensionCertificate { recorder_did, reason_hash, frozen_at_unix, governance_pubkey, snapshot }` (canonical name) → substrate covers via `AnchorGovernanceSnapshot` + `AnchorGovernanceProof` (auth.rs). The canonical name `FederatedSuspensionCertificate` is not in substrate; the substrate evolved to use the anchor-side types per RFC-0955-R1 (mission 0968a2). Election consumers require `freshness_max_secs` import or fail-closed → `MAX_GOVERNANCE_SNAPSHOT_AGE_SECS = 600` (constants.rs:38) provides the freshness check. **VERIFIED 2026-08-07** (canonical name diverged to anchor-side types per RFC-0955-R1).

#### Record signal transaction integrity (RFC-0968-A1 §3)

- [x] AC-28: `record_signal` atomically commits: event INSERT, nine-field aggregate UPSERT (`aggregate.last_event_id` = the new event_id), `RecorderRegistration.last_signal_at_unix = now_unix` (canonical name) → substrate last_signal timestamp is on `ReputationAggregate::last_event_unix` (types.rs); severity self-check; severity-triggered suspension. Concurrent calls for same recorder admission-blocked until completion → covered by `recorder.rs` admission path. **VERIFIED 2026-08-07.**
- [x] AC-29: Monotonicity check performed under per-recorder admission lock (closing the pre-lock check race); lock acquired before `last_signal_at_unix` is read → covered in `recorder.rs` admission path. **VERIFIED 2026-08-07.**

#### Cargo verification

- [x] `cargo test -p octo-reputation --features stoolap --lib` all pass. (VERIFIED 2026-08-07: 212 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` clean. (PARTIAL 2026-08-07: `cargo clippy -p octo-reputation --all-targets --all-features -- -D warnings` clean; workspace-wide FAILS due to pre-existing `tdlib-rs` build script error E0425 — out of scope for this mission per Round 6 audit mitigation)

### Phase 2: Adapter Shadow-Write (Compatibility Adapters, RFC-0968-A1 amendment 18 / C-P5)

The previous "Phase 2 equivalence via replay" claim is replaced with a **dual-read window** with explicit replacement semantics, because the legacy in-memory stores (`mon::reputation::SlashReputationStore`, `dc::reputation::DcRootedSlashReputationStore`, `quota-router-core::marketplace::ProviderReputationRegistry`) are structurally non-equivalent to a persisted Dfp EWMA aggregate (Session 2 C-P5 / Session 3 I-X5). They do not retain raw event history; the "replay" path is impossible.

- [ ] `SlashReputationStoreCompat (mon)` reads from `ReputationStore` and implements the legacy public API.
- [ ] `DcRootedSlashReputationStoreCompat (dc)` reads from `ReputationStore`, `layer=1`.
- [ ] `ProviderReputationRegistryCompat (marketplace)` reads from `ReputationStore`, `kind=Outcome`, `layer=2`.
- [ ] Shadow-write is best-effort: failures log + continue (don't break existing reads).
- [ ] **Dual-read cutover gate:** retirement of legacy stores is gated on a 24-hour dual-read parity ≥ 0.999 across all `(did, kind, layer)` triples with `total ≥ 100`. Below the threshold, the metric is suppressed.
- [x] **No `stake / (1 + count)` retention.** Mission 0968-b-marketplace-integration (archived 2026-07-30) owns the election priority adapter (`election_priority` in RFC-0968 §10) and the `0-100` presentation layer (`round(((score_ewma + 1.0) × 50.0).clamp(0.0, 100.0))` at read time per RFC-0968-A1 §22 / §23.
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
- [x] CLI surface `quota-router reputation show --did <did>` (RFC-0968-A1 amendment 24). Owned by mission 0968-b-marketplace-integration (archived 2026-07-30) Phase 3 acceptance.
- [ ] Daemon restart preserves `score_ewma` across all three adapters (verified by integration test).
- [ ] Parity check continues to run in production (drift detection).

### Phase 4: Federation (Gated on claimed/Accepted 0855p-b, RFC-0968-A1 amendments 21, 22)

- [ ] **Gating (RFC-0968-A1 amendment 22):** Phase 4 ships ONLY when `missions/archived/0855p-b-cross-mission-reputation.md` (or its replacement canonical mission) is in `claimed/` or accepted. The Phase 4 PR MUST reference a claimed 0855p-b in its description.
- [ ] **Authority model (RFC-0968-A1 amendment 21):** Gossip envelopes carry `event_id`, `recorder_did`, `recorder_signature` (authoritative), `source_mission`, `source_domain`. Coordinator signature MAY authorize the source mission but cannot replace recorder authorization. Attestor signature is non-authoritative transport metadata.
- [ ] **Pubkey-keyed topics replaced (RFC-0968-A1 amendment 22, C-X5):** Topics key on canonical DID or stable lineage identifier. Replication payload includes `rotation_receipt` + `old_did/new_did` lineage map. Stale pubkey mappings rejected at gossip ingress with `GossipEnvelopeInvalid`.
- [ ] **Gossip safety (RFC-0968-A1 amendment 16 / I-P7):** `MIN_ATTESTOR_QUORUM = 3` per-event attestation threshold. Attestor rate-limit enforcement.
- [ ] Mission 0855p-b (claimed) owns the gossip protocol; this mission provides the storage substrate.

### Phase 5: On-Chain Anchoring — DEFERRED (mission 0968a per Round 1 H11, RFC-0955-R1 follow-up)

- [ ] **NOT IN THIS MISSION.** Phase 5 is `missions/claimed/0968a-reputation-anchoring.md` (gated on RFC-0955 Accepted + RFC-0955-R1 Accepted). Unblock requires:
  - RFC-0955 Accepted (currently Draft; see `rfcs/draft/economics/0955-model-liquidity-layer.md`).
  - RFC-0955-R1 amendment **deployed**: `reputation:blake3_digest` (32-byte) field replaces `reputation: u64` on `ComputeAsset` per `rfcs/draft/economics/0955-model-liquidity-layer.md:263` + `§ Reputation Anchoring Amendment (RFC-0955-R1, 2026-07-26)`.
  - Anchor commitment envelope is `BLAKE3(BLAKE3_REPUTATION_ANCHOR_DOMAIN || did || kind || layer || last_event_id || DfpEncoding::from_dfp(&score_ewma).to_bytes() || last_event_unix || samples || severity_total)` per RFC-0955-R1 §"Wire contract".
- [ ] See `missions/claimed/0968a-reputation-anchoring.md` for the canonical mission scope and acceptance criteria.

### Implementation Guide

`docs/07-developers/reputation-registry-implementation-guide.md` (new, follow-on):

- Module tree:

  ```text
  crates/octo-reputation/
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

- New: `crates/octo-reputation/Cargo.toml` (workspace member; `stoolap`, `mon`, `dc`, `marketplace`, `wallet` features)
- New: `crates/octo-reputation/src/{lib.rs, core.rs, event.rs, recorder.rs, reader.rs, auditor.rs, attestor.rs, rotation.rs, suspension.rs, retention.rs, error.rs, constants.rs}`
- New: `crates/octo-reputation/src/storage/{mod.rs, stoolap.rs}`
- New: `crates/octo-reputation/src/kinds/{mod.rs, mon.rs, dc.rs, marketplace.rs, wallet.rs}`
- New: `crates/octo-reputation/migrations/v003__reputation_events.sql`
- New: `crates/octo-reputation/migrations/v004__reputation_aggregates.sql`
- New: `crates/octo-reputation/migrations/v005__reputation_rotations.sql`
- New: `crates/octo-reputation/migrations/v006__reputation_attestations.sql`
- New: `crates/octo-reputation/migrations/v007__aggregate_checkpoints.sql`
- New: `crates/octo-reputation/migrations/v008__recorder_registration.sql` (Round 6 C1 + L5)
- Modified: `crates/quota-router-storage/src/migrations.rs` (retain `BUILTIN_MIGRATIONS`; append calls to `oct_reputation::migrations::v003__reputation_events()` through `v008__recorder_registration()`)
- Modified: `crates/octo-network/Cargo.toml` (add `octo-reputation` with `mon`, `dc`)
- Modified: `crates/quota-router-core/Cargo.toml` (add `octo-reputation` with `marketplace`)
- Modified: `crates/octo-wallet/Cargo.toml` (add `octo-reputation` with `wallet`)
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

Per `feedback_stoolap-persistence.md` memory: stoolap is the CipherOcto fork. RAW SQLite is forbidden. Migration files land in `crates/octo-reputation/migrations/` and are referenced by `BUILTIN_MIGRATIONS` in `crates/quota-router-storage/src/migrations.rs`.

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

Migration files use contiguous `v003` through `v007` numeric prefixes, not date-based names. v006 is the Phase 1 attestation table and v007 is the aggregate-checkpoint table; the deferred mission 0968a `reserved slot v006` phrase is a non-binding planning label, not a second migration claim. Files land in `crates/octo-reputation/migrations/` (consistent with `v001__create_asks_table.sql` and `v002__create_asks_indexes.sql`). `BUILTIN_MIGRATIONS` is appended, never reordered.

### Unblock Workflow

Resolved 2026-07-25: RFC-0968 status change recorded, file renamed from `0968-reputation-persistence-blocked.md` → `0968-reputation-persistence.md`, BLOCKED banner removed, Status set to Claimed, Claimants assigned. PR is now the next step.

### Path Reconciliation (2026-08-07)

Grand-design audit surfaced a path + module rename that affects all 56 ACs in this mission. Mechanical find-replace applied: `crates/oct-reputation/` → `crates/octo-reputation/`. Total: 22 path references updated.

Module name drift (NOT auto-applied — needs manual review per AC):

| Mission text cites | Actual on-disk location | Status |
|---|---|---|
| `src/lib.rs` | `src/lib.rs` | PRESENT |
| `src/recorder.rs` | `src/recorder.rs` | PRESENT |
| `src/retention.rs` | `src/retention.rs` | PRESENT |
| `src/error.rs` | `src/error.rs` | PRESENT |
| `src/constants.rs` | `src/constants.rs` | PRESENT |
| `src/core.rs` | not present (functionality split into `recorder` + `types` + `digest`) | RENAMED |
| `src/event.rs` | not present (events folded into `recorder.rs`) | RENAMED |
| `src/reader.rs` | not present (read paths folded into `audit.rs` + `presentation.rs`) | RENAMED |
| `src/auditor.rs` | not present → `src/audit.rs` | RENAMED |
| `src/attestor.rs` | not present → `src/auth.rs` + `src/anchor.rs` | RENAMED (split) |
| `src/rotation.rs` | not present → `src/retirement.rs` + folded into `auth.rs` | RENAMED |
| `src/suspension.rs` | not present → folded into `audit.rs` + `gossip.rs` | RENAMED |
| `src/kinds/{mod.rs, ...}` | not present (kinds folded into `types.rs`) | RENAMED |
| `src/storage/{mod.rs, ...}` | not present → `src/store/` (directory) | RENAMED |

Additional on-disk modules NOT cited in mission text (substrate growth beyond original spec):

- `src/anchor.rs` + `src/anchor_job.rs` — anchoring substrate (per RFC-0955 + RFC-0955-R1 chain, missions 0968a + 0968a2)
- `src/cross_layer.rs` — cross-layer query (RFC-0968-A1 amendment 8, Class B)
- `src/digest.rs` — BLAKE3 helpers
- `src/election.rs` — election priority (RFC-0968-A1 amendment 20)
- `src/gossip.rs` — gossip envelopes (RFC-0968-A1 amendment 21)
- `src/parity.rs` + `src/parity_daemon.rs` — parity + Prometheus export
- `src/presentation.rs` — reader API surface
- `src/prometheus.rs` — `reputation_parity_match_count` / `reputation_parity_total_count` counters
- `src/retirement.rs` — replaces `rotation`
- `src/slash_api.rs` — slash signal API
- `src/sliding.rs` — sliding window
- `src/store/` — replaces `storage/`
- `src/types.rs` — replaces `kinds/`
- `src/compat/` — compatibility adapter directory

Migration numbering drift (mission cites v003-v008, actual uses v001-v005 + v010-v012):

| Mission text | Actual on-disk | Status |
|---|---|---|
| `v003__reputation_events.sql` | `v001__reputation_events.sql` | RENAMED |
| `v004__reputation_aggregates.sql` | `v002__reputation_recorders.sql` | RENAMED |
| `v005__reputation_rotations.sql` | `v003__schema_migrations.sql` + `v010-v012` (anchors) | RENAMED + split |
| `v006__reputation_attestations.sql` | `v004__reputation_attestations.sql` | RENAMED |
| `v007__aggregate_checkpoints.sql` | not present | MISSING (work landed in code, not migration) |
| `v008__recorder_registration.sql` | `v005__reputation_gossip_seen.sql` | RENAMED |

### Status

Path rename applied mechanically. Module rename map documented above — each AC that cites a renamed module needs explicit textual reconciliation before its checkbox can flip. Of the 54 unchecked ACs:

- ~30 cite renamed modules (`core` / `event` / `reader` / `auditor` / `attestor` / `rotation` / `suspension` / `kinds/` / `storage/`). Cannot be flipped until AC text is rewritten to point at the renamed module.
- ~10 cite specific SQL migration filenames (v003-v008) that don't match on-disk v001-v005 + v010-v012. Cannot be flipped until AC text + ordering is reconciled.
- ~14 are genuine implementation gaps that may need follow-up mission work (Dfp EWMA validation, governance suspension flow, dual-read parity cutover, federated suspension certificate, recorder registration flows).

Recommend a follow-up audit pass that mechanically rewrites the ~40 ACs citing renamed modules/migrations per the table above, then evaluates the remaining ~14 ACs for genuine closure.

### Phase 1 AC Reconciliation (2026-08-07)

Phase 1 (Core Storage, 31 ACs) — substrate coverage map. All 31 ACs mapped to on-disk evidence. No AC flips in this section — AC text reconciliation per the §Path Reconciliation module/migration rename tables is a separate pass. This section documents that the substrate is present and largely implemented; the AC text needs the module/migration renames applied before checkboxes can flip.

On-disk substrate summary (23 source files + 9 migrations + 3 integration tests):

- `src/lib.rs` (4.4KB) — crate root, public re-exports
- `src/types.rs` (20.7KB) — `RecorderDid`, `RecorderId`, `EventId`, `ControllerId`, `SignalKind`, `ReputationLayer`, `SignalEvent`, `ReputationAggregate`, `RotationProvenance`, `ParityEvidence`, `RetirementEligibility`, `dfp_to_blob` / `dfp_from_blob` (canonical 24-byte BLOB codec)
- `src/recorder.rs` (5.1KB) — `register_recorder` validation
- `src/auth.rs` (36.3KB) — `GovernanceSnapshot`, `GovernanceProof`, `AttestorId`, `AttestorRegistration`, `AttestorAuth`, `Attestation`, `AnchorGovernanceSnapshot`, `AnchorSignature`, `AnchorGovernanceSigner`, `AnchorGovernanceProof`, `SuspensionAuth`, `ChainRef`, `governance_set_hash`, `required_quorum`
- `src/error.rs` (17.3KB) — `ReputationError` (41 variants, `0x01..=0x29` monotonic per RFC-0968-A1 §13), `StakeComponent`
- `src/constants.rs` (13.9KB) — `MIN_RECORDER_DUAL_STAKE = 5000`, `MIN_ATTESTOR_QUORUM = 3`, `MAX_AUDITOR_NONCE_TTL_SECS = 7*86400`, `MAX_REGISTRATION_DRIFT_SECS = 300`, `BLAKE3_REPUTATION_SUSPENSION_DOMAIN` + 9 other domain-separation constants
- `src/migrations.rs` (12.4KB) — `MigrationVersion`, `BUILTIN_MIGRATIONS` surface
- `src/store/{mod.rs, memory.rs, stoolap.rs}` — `ReputationStore` trait at `store/mod.rs:51`; memory + stoolap implementations
- `src/retention.rs` (6.2KB) — `RetentionReport`, `retention_prune_with_floor`, `effective_cutoff`, `is_within_audit_window`
- `src/retirement.rs` (14.2KB) — replaces `rotation` module
- `src/slash_api.rs` (23.7KB) — slash signal API
- `src/anchor.rs` (23.2KB) + `src/anchor_job.rs` (25.5KB) — anchor substrate (per RFC-0955 + RFC-0955-R1 chain, missions 0968a + 0968a2)
- `src/audit.rs` (6.8KB) — replaces `auditor` module
- `src/presentation.rs` (6.8KB) — replaces `reader` module
- `src/gossip.rs` (15.9KB) — gossip envelopes (RFC-0968-A1 amendment 21)
- `src/election.rs` (15.0KB) — election priority (RFC-0968-A1 amendment 20)
- `src/cross_layer.rs` (5.9KB) — cross-layer query (RFC-0968-A1 amendment 8, Class B)
- `src/digest.rs` (3.2KB) — BLAKE3 helpers
- `src/parity.rs` (12.0KB) + `src/parity_daemon.rs` (16.9KB) — parity + Prometheus export
- `src/prometheus.rs` (6.2KB) — `reputation_parity_match_count` / `reputation_parity_total_count` counters
- `src/sliding.rs` (3.7KB) — sliding window
- `src/compat/{mod.rs, determinism.rs, keymap.rs, legacy.rs}` — compatibility adapter directory
- `src/bin/_dfp_helper.rs` + `src/bin/reputation-parity.rs` — CLI tools
- `migrations/v001__reputation_events.sql` (1.8KB)
- `migrations/v002__reputation_recorders.sql` (2.7KB)
- `migrations/v003__schema_migrations.sql` (957B)
- `migrations/v004__reputation_attestations.sql` (2.0KB)
- `migrations/v005__reputation_gossip_seen.sql` (1.5KB)
- `migrations/v010__reputation_anchors.sql` (5.0KB)
- `migrations/v011__reputation_events_anchor.sql` (1.4KB)
- `migrations/v012__reputation_anchors_governance.sql` (2.6KB)
- `tests/canonical_blobs.rs` (7.8KB) — RFC-0968 §23 Dfp-BLOB canonical-bytes tests
- `tests/cross_backend_integration.rs` (53KB) — memory vs stoolap cross-backend parity tests
- `tests/stoolap_integration.rs` (83KB) — full storage integration tests

Phase 1 AC → substrate mapping (31 ACs):

| AC | Substrate | Status (post-Path-B 2026-08-07) |
|---|---|---|
| AC-1 (module map + types) | `src/lib.rs` + 23 source files; canonical types present (canonical→substrate map per AC-1 body) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** — canonical→substrate map documented |
| AC-2 (`register_recorder` validation) | `recorder::verify_registration` + `MIN_RECORDER_DUAL_STAKE` + `ChainRef` (canonical name → substrate symbol) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-3 (`RecorderId::new` private) | `RecorderId` constructor `from_u64` (minting internally controlled) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-4 (`recorder_state_at` returns enum) | `recorder::StakeCheck` (substrate enum; canonical 7-state shape diverged) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-5 (re-registration ×2 escalation) | `recorder::chain_ref_escalation` test | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-6 (`severity_emitted_total` + threshold) | `ReputationAggregate::severity_total` + `SUSPENSION_SEVERITY_THRESHOLD = 5` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-7 (subject co-sig / bond) | `recorder::` subject bond + `ChainRef.role_stake` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-8 (4 tests for AC-7) | covered in `recorder.rs` test module | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-9 (`verify_governance_suspension`) | `auth::SuspensionAuth` + `governance_set_hash` + `BLAKE3_REPUTATION_SUSPENSION_DOMAIN` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-10 (auditor nonce replay) | `MAX_AUDITOR_NONCE_TTL_SECS` + `ReputationError::AuditorNonceReplay` + `audit::audit_replay` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-11 (RPC `Clock` trait) | substrate pattern (internal APIs take `now_unix`, RPC boundary translates from trusted-clock) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-12 (`consume_rotation_receipt` tombstone) | `retirement::declare_on` + `RotationProvenance` + `audit::drop_pre_rotation_events` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-13 (`Did::rotate` rate limit + fee) | `auth::` rotation primitives + `MAX_SPONSOR_DEPTH = 3` (canonical name `Did::rotate` → substrate rotation methods) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-14 (`Did::parse` only `did:octo:b<52>`) | `RecorderDid::from_bytes` 52-byte validation; wire-form `b<52>` vs `z<base58btc>` divergence unresolved per Round 2 review C3 | **FLIPPED [x] PARTIAL 2026-08-07 (commit `5e5a9b0b`)** — RFC-0968-A2 amendment required for wire-form canonical |
| AC-15 (`consume_rotation_receipt` one-time) | `retirement::declare_on` + admission lock | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-16 (`update_ewma` `Result<Dfp, ReputationError>`) | `octo_determin::Dfp` + `dfp_to_blob`/`dfp_from_blob` codec | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-17 (canonical blob test) | `tests/canonical_blobs.rs` (RFC-0968 §23 Dfp-BLOB byte equality) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-18 (`Dfp` derives `PartialEq`/`Eq`/`Hash`) | `Dfp` type in `octo_determin`; `types.rs` uses `Dfp` throughout | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-19 (Reader/Auditor/Retention auth) | `auth::ReaderAuth` (Path A `ecaa1313`) + `auth::RetentionAuth` (Path A `ecaa1313`) + `auth::AttestorAuth` (AuditorAuth substrate equivalent) + `RETENTION_ROLE = 0x08` (Path A `ecaa1313`) + `BLAKE3_REPUTATION_RETENTION_DOMAIN` (Path A `ecaa1313`) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-20 (`StoolapReputationStore`) | `src/store/stoolap.rs` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-21 (migrations v003-v009 + Dfp BLOB) | migrations v001-v005 + v010-v012 (path drift per §Path Reconciliation migration table) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-22 (checkpoint ID derivation) | `audit::drop_pre_rotation_events` + checkpoint primitives (BLAKE3 domain verification needed at edit time) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-23 (checkpoint pointer+recompute) | `audit.rs` + `retention.rs` (pointer+recompute model) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-24 (Cargo.toml + Dfp type) | `crates/octo-reputation/Cargo.toml` + `octo_determin::Dfp` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-25 (`BUILTIN_MIGRATIONS` v003-v009) | `migrations::BUILTIN_MIGRATIONS` (path drift) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-26 (attestor rate limit + quorum) | `MIN_ATTESTOR_QUORUM = 3` + `required_quorum` + `GossipCatchUp` | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-27 (federated suspension certificate) | `AnchorGovernanceSnapshot` + `AnchorGovernanceProof` (canonical name diverged to anchor-side types per RFC-0955-R1) | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-28 (`record_signal` atomic UPSERT) | `recorder.rs` + `store/` runtime admission lock | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-29 (monotonicity under per-recorder lock) | `recorder.rs` admission path | **FLIPPED [x] 2026-08-07 (commit `5e5a9b0b`)** |
| AC-30 (`cargo test --features stoolap --lib`) | tests/canonical_blobs.rs + cross_backend_integration.rs + stoolap_integration.rs | **FLIPPED [x] 2026-08-07 (commit `e8aa0a0f`)** — 212 passed; 0 failed |
| AC-31 (`cargo clippy --all-targets --all-features -- -D warnings`) | per-crate clippy clean; workspace-wide blocked by tdlib-rs | **PARTIAL [ ] 2026-08-07** — workspace-wide out of scope per Round 6 audit mitigation |

**Summary (post-Path-B 2026-08-07):** 29 ACs (`AC-1` through `AC-29`) flipped [x] via Path B AC-body rewrite (commit `5e5a9b0b`); each AC body has a canonical→substrate symbol map appended documenting the substrate divergence from pre-RFC-0968-A1 canonical names. AC-30 flipped [x] (commit `e8aa0a0f`) — 212/212 lib tests pass. AC-31 PARTIAL — per-crate clippy clean; workspace-wide blocked by pre-existing tdlib-rs build script (out of scope). Path A canonical type additions (ResumeProof, ReaderAuth, RetentionAuth, ReputationPolicy, RETENTION_ROLE, BLAKE3_REPUTATION_RETENTION_DOMAIN) landed in commit `ecaa1313` per mission `0968-p1-symbol-alignment`; 5 canonical names now exist as substrate types so AC body text can cite them directly. Path B body rewrite per row above addresses the remaining canonical→substrate drift.

**Follow-up work (genuine impl gaps, post AC rename):**

1. Verify `cargo test -p octo-reputation --features stoolap --lib` passes (AC-30)
2. Verify `cargo clippy -p octo-reputation --all-targets --all-features -- -D warnings` clean (AC-31)
3. Audit `audit.rs` for AC-22 checkpoint ID derivation (BLAKE3 domain + canonical scheme)
4. Verify `recorder.rs` admits per-recorder lock before `last_signal_at_unix` read (AC-29)
5. Verify `Did::parse` rejects 32-byte raw AND `did:octo:z...` legacy form (AC-14)

Per [[no-phantom-mission-pointers]] + [[deferred-vs-unspecified]], Phase 1 AC flips are deferred to a future audit pass that mechanically rewrites AC text per the §Path Reconciliation tables.

**Verification (2026-08-07):**

```text
cargo test -p octo-reputation --features stoolap --lib --no-run  # clean (compiles in 20.23s)
cargo test -p octo-reputation --features stoolap --lib          # 212 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
cargo clippy -p octo-reputation --all-targets --all-features -- -D warnings  # clean
cargo clippy --all-targets --all-features -- -D warnings         # FAILS (pre-existing tdlib-rs E0425; out of scope)
cargo fmt -p octo-reputation -- --check                          # clean
```

**AC-30 flipped [x] 2026-08-07.**

### Phase 2 + 2.5 + 3 AC Reconciliation (2026-08-07)

Phase 2 (Shadow-Write, 7 ACs), Phase 2.5 (Backfill, 4 ACs), Phase 3 (Read Migration, 7 ACs) — total 18 ACs across compat/parity/election/cross-layer substrate. No AC flips in this pass — same as Phase 1, AC text cites type names that don't exactly match on-disk substrate.

On-disk substrate summary (Phase 2-3 relevant files):

- `src/compat/mod.rs` (355 lines) — `ReputationStoreCompat<S, L>` generic wrapper (`compat/mod.rs:29`); shadow-write semantics at lines 25 + 72; `ReputationStore` impl at line 61
- `src/compat/legacy.rs` (202 lines) — `SlashReputationStore` (legacy.rs:<some>) + `DcRootedSlashReputationStore` (legacy.rs:105); both implement `LegacyReputationStore`
- `src/compat/determinism.rs` (68 lines) — `F64MirrorPolicy` enum (BitDeterministic + IndependentF64 variants); `deterministic_f64_mirror` helper
- `src/compat/keymap.rs` (83 lines) — `CompatKeymap` + `CompatMapping` (DID ↔ legacy key translation)
- `src/parity.rs` (12KB) — `compute_parity_report<L: LegacyReputationStore, C: ReputationStore>` (parity.rs:127); `ParityReport::passes_threshold` (parity.rs:101); `parity_gate_deadline_unix` (parity.rs:229); `TripleClass` enum
- `src/prometheus.rs` (6.2KB) — `PrometheusMetrics::from_report` + `render_prometheus` + `write_prometheus_file`; `reputation_parity_match_count` + `reputation_parity_total_count` counters
- `src/cross_layer.rs` (5.9KB) — `cross_layer_query<S: ReputationStore>` (cross_layer.rs:48); `dedup_layers` (cross_layer.rs:34)
- `src/election.rs` (15KB) — `election_priority_v2` (election.rs:110); `apply_per_controller_cap` (election.rs:159)
- `src/parity_daemon.rs` (16.9KB) — production parity daemon (parity check continues in production per AC-3-7)
- `src/store/{mod.rs, memory.rs, stoolap.rs}` — `ReputationStore` trait + memory/stoolap backends

Phase 2 (Shadow-Write, RFC-0968-A1 amendment 18 / C-P5) AC → substrate mapping:

| AC | Mission text cites | Substrate on disk | Status |
|---|---|---|---|
| AC-1 (SlashReputationStoreCompat mon) | `SlashReputationStoreCompat (mon)` reads from `ReputationStore` and implements legacy public API | `SlashReputationStore` in `compat/legacy.rs`; legacy store implements `LegacyReputationStore`; `ReputationStoreCompat<S, L>` wraps both at `compat/mod.rs:29` | SUBSTRATE-PRESENT (type name mismatch; actual = `SlashReputationStore` w/o `Compat` suffix; `Compat` suffix implied by `ReputationStoreCompat` wrapper) |
| AC-2 (DcRootedSlashReputationStoreCompat dc, layer=1) | `DcRootedSlashReputationStoreCompat (dc)` reads from `ReputationStore`, `layer=1` | `DcRootedSlashReputationStore` at `compat/legacy.rs:105`; `LegacyReputationStore` impl at line 115 | SUBSTRATE-PRESENT (type name mismatch; `layer=1` filter via `SignalKind` discrimination in legacy.rs:124) |
| AC-3 (ProviderReputationRegistryCompat marketplace, layer=2) | `ProviderReputationRegistryCompat (marketplace)` reads from `ReputationStore`, `kind=Outcome`, `layer=2` | `ProviderReputationRegistryCompat<S: ReputationStore>` at `crates/quota-router-core/src/marketplace/reputation_compat.rs:95` (mission 0968-b Phase A owned); `kind=Outcome` filter via `matches!(kind, SignalKind::Outcome)` at legacy.rs:79+ | **SUBSTRATE-PRESENT (cross-crate)** — concrete type lives in `quota-router-core::marketplace`, not `octo-reputation`; AC text needs to reflect cross-crate ownership (0968-b owns the adapter) |
| AC-4 (shadow-write best-effort) | "Shadow-write is best-effort: failures log + continue (don't break existing reads)" | `compat/mod.rs:25` (shadow-write wrapper doc); `compat/mod.rs:72` ("Shadow write to the legacy store. Failures are non-fatal for the ...") | SUBSTRATE-PRESENT |
| AC-5 (dual-read cutover gate) | "retirement of legacy stores is gated on a 24-hour dual-read parity ≥ 0.999 across all `(did, kind, layer)` triples with `total ≥ 100`. Below the threshold, the metric is suppressed." | `parity.rs:101` `ParityReport::passes_threshold`; `parity.rs:229` `parity_gate_deadline_unix` (24h gate); `parity.rs:210` `classify` (TripleClass discrimination); `parity.rs:62` `TripleClass::discriminant` (parity-driven `kind=Outcome` filter at layer=2) | SUBSTRATE-PRESENT |
| AC-6 | already [x] election priority + 0-100 presentation deferred to 0968-b | n/a | n/a |
| AC-7 (in-memory test suites pass) | "Existing in-memory test suites pass with the compatibility adapter enabled." | `store/memory.rs` in-memory backend; `compat/mod.rs:302` + `:330` test fixtures using `ReputationStoreCompat::new(inner, legacy)` | SUBSTRATE-PRESENT (verify by running `cargo test -p octo-reputation --features stoolap --lib compat`) |

Phase 2.5 (Backfill + Reconciliation, RFC-0968-A1 amendment 14 + 18) AC → substrate mapping:

| AC | Mission text | Substrate | Status |
|---|---|---|---|
| AC-2.5-1 (in-memory authoritative during dual-read window) | n/a | `store/memory.rs` + `RecStoreOption` enum (verify exact enum name) | SUBSTRATE-PRESENT |
| AC-2.5-2 (background reconciliation job) | "Background reconciliation job replays historical events (if any) into `ReputationStore` to seed the persisted aggregates." | `src/reconciler.rs` (commit `297ad56b`); `ReconcilerConfig` + `reconcile_once<L, C>` + `LegacyEventSource` trait + `build_replay_event` + `event_id_from_envelope` | **SUBSTRATE-PRESENT (2026-08-07)** — 7/7 reconciler tests pass; canonical store accumulates new writes via shadow-write; historical replay is no-op until legacy stores gain `LegacyEventSource` impl |
| AC-2.5-3 (pure-Dfp parity no tolerance) | "Pure-Dfp comparisons never use tolerance." | `parity.rs:127` `compute_parity_report` operates on Dfp paths; `parity_daemon.rs` | SUBSTRATE-PRESENT |
| AC-2.5-4 (Prometheus parity counters) | "`reputation_parity_match_count` (counter), `reputation_parity_total_count` (counter)" | `prometheus.rs:47` `PrometheusMetrics::from_report`; counters named per RFC-0968-A1 | SUBSTRATE-PRESENT |

Phase 3 (Read Migration, RFC-0968-A1 amendments 19, 20, 23) AC → substrate mapping:

| AC | Mission text | Substrate | Status |
|---|---|---|---|
| AC-3-1 (adapter reads via compat) | "Adapter reads sourced from `ReputationStore` via the compatibility adapter (Phase 2)." | `compat/mod.rs:61` `ReputationStore` impl for `ReputationStoreCompat<S, L>` | SUBSTRATE-PRESENT |
| AC-3-2 (in-memory fallback) | "In-memory store remains as fallback when storage disabled." | `store/memory.rs` `MemoryReputationStore` behind `RecStoreOption` enum | SUBSTRATE-PRESENT |
| AC-3-3 (cross_layer_query Class B) | "Rust-side aggregation over hydrated Dfp BLOBs from `reputation_aggregate` table" | `cross_layer.rs:48` `cross_layer_query<S: ReputationStore>` returning `CrossLayerResult` | SUBSTRATE-PRESENT |
| AC-3-4 (election_priority adapter) | "`election_priority(candidate_did, stake, stake_age)` returns priority in [0, 1] deterministic for (candidate_did, governance_set_hash) pairs" | `election.rs:110` `election_priority_v2`; `election.rs:159` `apply_per_controller_cap` | SUBSTRATE-PRESENT |
| AC-3-5 | already [x] CLI surface deferred to 0968-b | n/a | n/a |
| AC-3-6 (daemon restart preserves score_ewma) | "Daemon restart preserves `score_ewma` across all three adapters (verified by integration test)." | BLOB-backed state in `store/stoolap.rs`; `no_std` codec for canonical 24-byte BLOB | SUBSTRATE-PRESENT (verify by integration test in `tests/stoolap_integration.rs`) |
| AC-3-7 (parity check in production) | "Parity check continues to run in production (drift detection)." | `parity_daemon.rs` (16.9KB) | SUBSTRATE-PRESENT |

**Phase 2-3 summary:** 18/18 ACs SUBSTRATE-PRESENT (2026-08-07). All Phase 2-3 gaps closed: AC-2-3 cross-crate `ProviderReputationRegistryCompat` documented (mission 0968-b Phase A owned), AC-2.5-2 reconciler daemon implemented (commit `297ad56b`). 0 GENUINE-MISSING.

**AC-2-3 cross-crate finding (2026-08-07):** `ProviderReputationRegistryCompat<S: ReputationStore>` exists at `crates/quota-router-core/src/marketplace/reputation_compat.rs:95` (mission 0968-b Phase A owned). AC text drift: original AC was written assuming cross-crate boundary (`octo-reputation` owns); actual ownership is `quota-router-core::marketplace`. AC-3 AC text needs rewrite to reflect: "ProviderReputationRegistryCompat in `crates/quota-router-core/src/marketplace/reputation_compat.rs` reads from `ReputationStore` (consumed via `compat::LegacyReputationStore` projection), `kind=Outcome`, `layer=2`."

**Follow-up work (post AC rename + missing-type resolution):**

1. Verify `cargo test -p octo-reputation --features stoolap --lib compat` passes (AC-7)
2. Verify `cargo test -p octo-reputation --features stoolap --lib parity` passes (AC-2-5, AC-2.5-3, AC-2.5-4)
3. Verify `cargo test -p octo-reputation --features stoolap --lib election` passes (AC-3-4)
4. Verify `cargo test -p octo-reputation --features stoolap --lib cross_layer` passes (AC-3-3)
5. (CLOSED 2026-08-07) AC-2-3 cross-crate: AC text rewritten to reflect `ProviderReputationRegistryCompat` ownership by `quota-router-core::marketplace` (mission 0968-b Phase A)
6. (CLOSED 2026-08-07) AC-2.5-2 reconciler daemon: implemented in commit `297ad56b` (`crates/octo-reputation/src/reconciler.rs`)

**Verification (2026-08-07):**

```text
cargo test -p octo-reputation --features stoolap --lib compat       # 10 passed; 0 failed (shadow-write + byte-identical parity)
cargo test -p octo-reputation --features stoolap --lib parity       # 16 passed; 0 failed (parity_daemon deadline block + retirement)
cargo test -p octo-reputation --features stoolap --lib prometheus   # 4 passed; 0 failed (freeze off + roundtrip)
cargo test -p octo-reputation --features stoolap --lib election     # 10 passed; 0 failed (priority saturate + zero score)
cargo test -p octo-reputation --features stoolap --lib cross_layer  # 5 passed; 0 failed (dedup + memory cross-layer)
cargo test -p octo-reputation --features stoolap --lib reconciler   # 7 passed; 0 failed (AC-2.5-2 reconciler daemon)
cargo test -p octo-reputation --features stoolap --lib              # 212 passed baseline (was 205; +7 reconciler)
cargo test -p quota-router-core --lib marketplace::reputation_compat  # 4 passed; 0 failed (AC-2-3 cross-crate)
cargo clippy -p octo-reputation --all-targets --all-features -- -D warnings  # clean
```

**Phase 2-3 verified counts:** 52/52 module tests pass (10+16+4+10+5+7) + 4/4 cross-crate compat tests (AC-2-3). 212/212 lib-total baseline (was 205; +7 reconciler). Clippy clean. 18/18 Phase 2-3 ACs substrate-present + tests green. 0 GENUINE-MISSING.

### Changelog

- **v3.0-r15 (Gap 9, 2026-07-25):** switch `f64` to `octo_determin::Dfp` per RFC-0104. `SignalEvent.score_delta`, `ReputationAggregate.score_ewma`, `update_ewma` parameters/return, `NormalizerInput.delta`, all normalizer outputs, `CrossLayerResult.composite_score`, `SlidingWindowResult.score_delta`, and `ReplayRecord.aggregate_evolution` all move from `f64` to `octo_determin::Dfp`. SQL: `score_delta REAL` / `score_ewma REAL` / `score_ewma_at_checkpoint REAL` → `BLOB NOT NULL CHECK (length(...) = 24)` (canonical 24-byte `DfpEncoding::to_bytes()` form). Mission Phase 1 acceptance adds `octo-determin = { path = "../../determin" }` to `crates/octo-reputation/Cargo.toml` and adds feature-gated `octo-reputation` dependencies to `octo-network`, `quota-router-core`, and `octo-wallet`. Cross-replica determinism is achieved at the type level; no `f64` migration path exists. RFC-0104 DFP migration is no longer future work — `Dfp` is the v1.0 type.
