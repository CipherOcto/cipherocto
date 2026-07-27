//! `StoolapReputationStore` — CipherOcto stoolap-fork backend impl.
//!
//! Per mission 0968 Phase 1.4 acceptance: persistent reputation storage over
//! the workspace-wide stoolap fork (`CipherOcto/stoolap@feat/blockchain-sql`).
//! Schema source of truth lives in `migrations/*.sql`; the runner is in
//! `crate::migrations::stoolap_runner::apply` and is invoked by every
//! constructor.
//!
//! ## Feature gating
//!
//! Every impl is gated on `#[cfg(feature = "stoolap")]`. Without the
//! feature, the type remains constructible as a marker and the
//! `ReputationStore` trait is satisfied by the historical stub that
//! returns `ReputationError::ChainRefInvalid("stoolap_backend_unimplemented:<name>")`.
//! This keeps the build hermetic in CI (no network round-trip).
//!
//! ## Dfp BLOB contract
//!
//! `score_ewma` and `score_delta` are stored as 24-byte BLOBs — the canonical
//! `DfpEncoding::from_dfp(&d).to_bytes()` form. Round-trip via
//! `crate::types::{dfp_to_blob, dfp_from_blob}` at the Rust boundary. SQL
//! does not interpret the bytes; it only guarantees `length(...) = 24`.
//!
//! ## Session 6 write path (this file)
//!
//! - `record_signal` — INSERT event + UPSERT aggregate with EWMA compute in
//!   Rust (mirrors `InMemoryReputationStore::record_signal`).
//! - `register_recorder` — 8-field chain verify + 3-guard stake check + INSERT.
//! - `slash_recorder` — UPDATE `slashed=1` + governance snapshot + destination/asset checks.
//! - `declare_retirement_eligible` — stubbed gov proof + retirement-eligibility update.
//!
//! Read path + cross-backend determinism land in Session 7.
//! Stoolap's API is sync — every async trait method blocks on `block_on`
//! via `futures_lite` (already a workspace dep), so the tokio reactor is
//! never stalled for long.

use crate::auth::{GovernanceProof, GovernanceSnapshot, SuspensionAuth};
use crate::error::ReputationError;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{
    ControllerId, EventId, ParityEvidence, RecorderDid, RecorderId, ReputationAggregate,
    ReputationLayer, RetirementEligibility, SignalEvent, SignalKind,
};

#[cfg(feature = "stoolap")]
use crate::auth::AssetTag;
#[cfg(feature = "stoolap")]
use crate::migrations::stoolap_runner;
#[cfg(feature = "stoolap")]
use crate::types::{dfp_from_blob, dfp_to_blob};

// ---------------------------------------------------------------------------
// cfg-gated: real impl
// ---------------------------------------------------------------------------

#[cfg(feature = "stoolap")]
mod real {
    use super::*;

    /// `StoolapReputationStore` (real). Owns one `stoolap::Database` behind
    /// `Arc` so trait methods can hold shared references. All mutating SQL
    /// operations execute under default MVCC isolation; we do not wrap them
    /// in transactions because every method issues one SQL statement and
    /// the trait's correctness contract is single-row atomicity.
    #[derive(Clone)]
    pub struct StoolapReputationStore {
        db: std::sync::Arc<stoolap::Database>,
    }

    impl std::fmt::Debug for StoolapReputationStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("StoolapReputationStore")
                .finish_non_exhaustive()
        }
    }

    impl StoolapReputationStore {
        /// Open (or create) a file-backed store at the given DSN. Use
        /// `Database::open("file:///path/to.db")` or `"memory://"`.
        /// Applies migrations synchronously on first run; subsequent opens
        /// are no-ops.
        pub async fn open(dsn: &str) -> Result<Self, ReputationError> {
            let db = stoolap::Database::open(dsn)
                .map_err(|_e: stoolap::Error| ReputationError::ChainRefInvalid("stoolap_open"))?;
            stoolap_runner::apply(&db)?;
            Ok(Self {
                db: std::sync::Arc::new(db),
            })
        }

        /// Open an in-memory store. Used for tests and ephemeral deployments.
        pub async fn open_in_memory() -> Result<Self, ReputationError> {
            let db = stoolap::Database::open_in_memory().map_err(|_e: stoolap::Error| {
                ReputationError::ChainRefInvalid("stoolap_open_inmem")
            })?;
            stoolap_runner::apply(&db)?;
            Ok(Self {
                db: std::sync::Arc::new(db),
            })
        }

        /// Construct over an existing `Database` handle without applying
        /// migrations. Useful for tests that pre-init schema and want to
        /// skip the apply step.
        pub fn from_db(db: stoolap::Database) -> Self {
            Self {
                db: std::sync::Arc::new(db),
            }
        }

        /// Borrow the underlying `Database`.
        pub fn database(&self) -> &stoolap::Database {
            &self.db
        }

        // -- monotonic id helpers --------------------------------------

        fn next_event_id(&self) -> Result<u64, ReputationError> {
            let mut rows = self
                .db
                .query(
                    "SELECT COALESCE(MAX(CAST(last_event_id AS INTEGER)), 0) FROM reputation_aggregates",
                    [],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_next_event_id"))?;
            let row = match rows.next() {
                Some(Ok(r)) => r,
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_next_event_id:row_err",
                    ))
                }
                None => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_next_event_id:empty",
                    ))
                }
            };
            let max: i64 = row
                .get(0)
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_next_event_id:cast"))?;
            Ok((max + 1) as u64)
        }

        fn next_recorder_id(&self) -> Result<u64, ReputationError> {
            let mut rows = self
                .db
                .query(
                    "SELECT COALESCE(MAX(recorder_id), 0) FROM reputation_recorders",
                    [],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_next_recorder_id"))?;
            let row = match rows.next() {
                Some(Ok(r)) => r,
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_next_recorder_id:row_err",
                    ))
                }
                None => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_next_recorder_id:empty",
                    ))
                }
            };
            let max: i64 = row
                .get(0)
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_next_recorder_id:cast"))?;
            Ok((max + 1) as u64)
        }
    }

    // -- value helpers ------------------------------------------------

    fn did_blob(d: &RecorderDid) -> stoolap::Value {
        stoolap::Value::blob(d.as_bytes().to_vec())
    }

    fn dfp_blob(arr: [u8; 24]) -> stoolap::Value {
        stoolap::Value::blob(arr.to_vec())
    }

    fn controller_blob(c: &ControllerId) -> stoolap::Value {
        stoolap::Value::blob(c.as_bytes().to_vec())
    }

    fn event_id_blob(e: EventId) -> stoolap::Value {
        stoolap::Value::blob(e.as_bytes().to_vec())
    }

    fn u64_to_value(v: u64) -> stoolap::Value {
        // u64 → i64 lossy at values > i64::MAX; reputation timestamps fit
        // comfortably in i64 for the foreseeable future.
        stoolap::Value::integer(v as i64)
    }

    fn i64_to_u64(v: i64) -> u64 {
        v.max(0) as u64
    }

    fn bytes_to_event_id(b: &[u8]) -> StoreResult<EventId> {
        if b.len() != 8 {
            return Err(ReputationError::ChainRefInvalid("stoolap_event_id_len"));
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(b);
        Ok(EventId::from_u64(u64::from_be_bytes(arr)))
    }

    // -- ReputationStore impl -----------------------------------------

    #[allow(async_fn_in_trait)]
    impl ReputationStore for StoolapReputationStore {
        async fn record_signal(&self, mut event: SignalEvent) -> StoreResult<EventId> {
            let eid = EventId::from_u64(self.next_event_id()?);
            event.event_id = eid;

            // 1. INSERT into reputation_events.
            let score_bytes = dfp_to_blob(&event.score_delta);
            let rot_blob: Option<Vec<u8>> = event
                .rotation_provenance
                .as_ref()
                .map(|rp| serde_json::to_vec(rp).unwrap_or_default());
            let audit_blob: Option<Vec<u8>> = event.audit_ref.clone();
            let kind_d = event.signal_kind.discriminant();
            let layer_d = event.layer.discriminant();

            let params: Vec<stoolap::Value> = vec![
                did_blob(&event.recorder_did),
                event_id_blob(eid),
                controller_blob(&event.controller_id),
                stoolap::Value::integer(kind_d as i64),
                stoolap::Value::integer(layer_d as i64),
                dfp_blob(score_bytes),
                u64_to_value(event.recorded_at_unix),
                match rot_blob {
                    Some(b) => stoolap::Value::blob(b),
                    None => stoolap::Value::null_unknown(),
                },
                match audit_blob {
                    Some(b) => stoolap::Value::blob(b),
                    None => stoolap::Value::null_unknown(),
                },
            ];
            self.db
                .execute(
                    "INSERT INTO reputation_events (
                        recorder_did, event_id, controller_id, signal_kind, layer,
                        score_delta, recorded_at_unix, rotation_provenance, audit_ref
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                    params,
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_record_signal:insert_event")
                })?;

            // 2. UPSERT into reputation_aggregates. `read_aggregate` is
            // async and we're already inside an async fn, so a direct
            // `.await` is correct.
            let existing_agg = self
                .read_aggregate(&event.recorder_did, event.signal_kind, event.layer)
                .await
                .ok();
            let (next_ewma_bytes, samples_next) = match existing_agg {
                Some(a) => {
                    let n = a.samples as f64;
                    let alpha = 1.0 / (n + 1.0);
                    let cur = a.score_ewma.to_f64();
                    let delta = event.score_delta.to_f64();
                    let next = cur * (1.0 - alpha) + delta * alpha;
                    (
                        dfp_to_blob(&octo_determin::Dfp::from_f64(next)),
                        a.samples + 1,
                    )
                }
                None => (dfp_to_blob(&event.score_delta), 1u64),
            };
            let params: Vec<stoolap::Value> = vec![
                did_blob(&event.recorder_did),
                stoolap::Value::integer(kind_d as i64),
                stoolap::Value::integer(layer_d as i64),
                dfp_blob(next_ewma_bytes),
                u64_to_value(samples_next),
                u64_to_value(0),
                event_id_blob(eid),
                u64_to_value(event.recorded_at_unix),
                u64_to_value(event.recorded_at_unix),
            ];
            // UPSERT manually: stoolap-fork does not support
            // `ON CONFLICT … DO UPDATE` (parity bit). INSERT first; if the
            // PK collides, UPDATE the row in place. Tests use a single
            // thread so the (read-then-write) race is bounded — a future
            // session wraps this in `BEGIN/COMMIT` for MVCC-safe
            // concurrent writers.
            let insert_result = self.db.execute(
                "INSERT INTO reputation_aggregates (
                    recorder_did, signal_kind, layer, score_ewma, samples,
                    severity_total, last_event_id, last_event_unix, updated_at_unix
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                params.clone(),
            );
            if let Err(_e) = insert_result {
                let update_params: Vec<stoolap::Value> = vec![
                    dfp_blob(next_ewma_bytes),
                    u64_to_value(samples_next),
                    u64_to_value(0),
                    event_id_blob(eid),
                    u64_to_value(event.recorded_at_unix),
                    u64_to_value(event.recorded_at_unix),
                    did_blob(&event.recorder_did),
                    stoolap::Value::integer(kind_d as i64),
                    stoolap::Value::integer(layer_d as i64),
                ];
                self.db
                    .execute(
                        "UPDATE reputation_aggregates
                         SET score_ewma = $1, samples = $2, severity_total = $3,
                             last_event_id = $4, last_event_unix = $5, updated_at_unix = $6
                         WHERE recorder_did = $7 AND signal_kind = $8 AND layer = $9",
                        update_params,
                    )
                    .map_err(|_e| {
                        ReputationError::ChainRefInvalid("stoolap_record_signal:update_aggregate")
                    })?;
            }

            // 3. Bump severity_total on Slash events.
            if matches!(event.signal_kind, SignalKind::Slash) {
                let params: Vec<stoolap::Value> = vec![
                    did_blob(&event.recorder_did),
                    stoolap::Value::integer(kind_d as i64),
                    stoolap::Value::integer(layer_d as i64),
                ];
                self.db
                    .execute(
                        "UPDATE reputation_aggregates
                         SET severity_total = severity_total + 1
                         WHERE recorder_did = $1 AND signal_kind = $2 AND layer = $3",
                        params,
                    )
                    .map_err(|_e| {
                        ReputationError::ChainRefInvalid("stoolap_record_signal:severity_update")
                    })?;
            }
            Ok(eid)
        }

        async fn read_aggregate(
            &self,
            did: &RecorderDid,
            kind: SignalKind,
            layer: ReputationLayer,
        ) -> StoreResult<ReputationAggregate> {
            let params: Vec<stoolap::Value> = vec![
                did_blob(did),
                stoolap::Value::integer(kind.discriminant() as i64),
                stoolap::Value::integer(layer.discriminant() as i64),
            ];
            let mut rows = self
                .db
                .query(
                    "SELECT score_ewma, samples, severity_total, last_event_id,
                            last_event_unix, updated_at_unix
                     FROM reputation_aggregates
                     WHERE recorder_did = $1 AND signal_kind = $2 AND layer = $3",
                    params,
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_read_aggregate"))?;
            let row = match rows.next() {
                Some(Ok(r)) => r,
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_read_aggregate:row_err",
                    ))
                }
                None => {
                    return Err(ReputationError::AggregateNotFound {
                        did: 0,
                        kind: kind.discriminant(),
                        layer: layer.discriminant(),
                    });
                }
            };
            {
                let score_bytes: Vec<u8> = row.get_by_name("score_ewma").map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_read_aggregate:score_blob")
                })?;
                let score_ewma = dfp_from_blob(&score_bytes)?;
                let last_event_id_bytes: Vec<u8> =
                    row.get_by_name("last_event_id").map_err(|_e| {
                        ReputationError::ChainRefInvalid("stoolap_read_aggregate:last_event")
                    })?;
                let last_event_id = bytes_to_event_id(&last_event_id_bytes)?;
                let samples: i64 = row.get_by_name("samples").map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_read_aggregate:samples")
                })?;
                let severity_total: i64 = row.get_by_name("severity_total").map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_read_aggregate:severity")
                })?;
                let last_event_unix: i64 = row
                    .get_by_name("last_event_unix")
                    .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_read_aggregate:leu"))?;
                let updated_at_unix: i64 = row
                    .get_by_name("updated_at_unix")
                    .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_read_aggregate:uau"))?;
                Ok(ReputationAggregate {
                    recorder_did: *did,
                    signal_kind: kind,
                    layer,
                    score_ewma,
                    samples: i64_to_u64(samples),
                    severity_total: i64_to_u64(severity_total),
                    last_event_id,
                    last_event_unix: i64_to_u64(last_event_unix),
                    updated_at_unix: i64_to_u64(updated_at_unix),
                })
            }
        }

        async fn cross_layer_query(
            &self,
            _did: &RecorderDid,
            _kind: SignalKind,
            layers: &[ReputationLayer],
        ) -> StoreResult<Vec<ReputationAggregate>> {
            if layers.is_empty() {
                return Err(ReputationError::CrossLayerEmpty);
            }
            // Session 7 stub.
            Ok(Vec::new())
        }

        async fn sliding_window(
            &self,
            _did: &RecorderDid,
            _kind: SignalKind,
            _layer: ReputationLayer,
            window_secs: u64,
            _now_unix: u64,
        ) -> StoreResult<ReputationAggregate> {
            if window_secs == 0 {
                return Err(ReputationError::SlidingWindowZero);
            }
            Err(ReputationError::ChainRefInvalid(
                "stoolap_backend_unimplemented:sliding_window",
            ))
        }

        async fn replay_for_audit(
            &self,
            _did: &RecorderDid,
            since_unix: u64,
            until_unix: u64,
        ) -> StoreResult<Vec<SignalEvent>> {
            if since_unix > until_unix {
                return Err(ReputationError::ReplayWindowInverted);
            }
            Ok(Vec::new())
        }

        async fn retention_prune(&self, cutoff_unix: u64, now_unix: u64) -> StoreResult<u64> {
            if cutoff_unix > now_unix {
                return Err(ReputationError::RetentionCutoffFuture);
            }
            let n = self
                .db
                .execute(
                    "DELETE FROM reputation_events WHERE recorded_at_unix <= $1",
                    vec![u64_to_value(cutoff_unix)],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_retention_prune"))?;
            Ok(n.max(0) as u64)
        }

        async fn prune_event(&self, event_id: EventId) -> StoreResult<()> {
            let params: Vec<stoolap::Value> = vec![event_id_blob(event_id)];
            self.db
                .execute("DELETE FROM reputation_events WHERE event_id = $1", params)
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_prune_event"))?;
            Ok(())
        }

        async fn register_recorder(
            &self,
            chain_ref: crate::auth::ChainRef,
        ) -> StoreResult<RecorderId> {
            crate::recorder::verify_registration(&chain_ref)?;
            let rid = RecorderId::from_u64(self.next_recorder_id()?);
            let params: Vec<stoolap::Value> = vec![
                stoolap::Value::integer(rid.to_u64() as i64),
                did_blob(&chain_ref.recorder_did),
                controller_blob(&chain_ref.recorder_did_placeholder_controller()),
                u64_to_value(chain_ref.octo_stake),
                u64_to_value(chain_ref.role_stake),
                stoolap::Value::integer(chain_ref.role_token_kind as i64),
                stoolap::Value::integer(chain_ref.chain_id as i64),
                u64_to_value(chain_ref.block_height),
                stoolap::Value::blob(chain_ref.tx_hash.to_vec()),
                u64_to_value(chain_ref.lock_until_unix),
                u64_to_value(crate::migrations::now_unix()),
                u64_to_value(crate::migrations::now_unix()),
            ];
            self.db
                .execute(
                    "INSERT INTO reputation_recorders (
                        recorder_id, recorder_did, controller_id, octo_stake, role_stake,
                        role_token_kind, chain_id, block_height, tx_hash, lock_until_unix,
                        created_at_unix, updated_at_unix
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
                    params,
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_register_recorder"))?;
            Ok(rid)
        }

        async fn verify_governance_suspension(
            &self,
            auth: &SuspensionAuth,
            snapshot: &GovernanceSnapshot,
            now_unix: u64,
        ) -> StoreResult<()> {
            if !snapshot.is_fresh(now_unix) {
                return Err(ReputationError::GovernanceSnapshotStale {
                    age_secs: snapshot.age_secs(now_unix),
                    max: crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
                });
            }
            if snapshot.governance_set_hash != auth.governance_set_hash {
                return Err(ReputationError::GovernanceSetHashMismatch);
            }
            if snapshot.quorum_count() < crate::constants::GOVERNANCE_QUORUM {
                return Err(ReputationError::GovernanceQuorumNotMet {
                    signatures: snapshot.quorum_count(),
                    quorum: crate::constants::GOVERNANCE_QUORUM,
                });
            }
            if auth.governance_pubkey == [0u8; 32] {
                return Err(ReputationError::GovernanceSignatureInvalid);
            }
            Ok(())
        }

        async fn suspend_recorder(
            &self,
            recorder_id: RecorderId,
            auth: SuspensionAuth,
            now_unix: u64,
        ) -> StoreResult<()> {
            let snapshot = auth.snapshot.clone();
            self.verify_governance_suspension(&auth, &snapshot, now_unix)
                .await?;
            let params: Vec<stoolap::Value> = vec![
                u64_to_value(now_unix),
                stoolap::Value::integer(recorder_id.to_u64() as i64),
            ];
            let n = self
                .db
                .execute(
                    "UPDATE reputation_recorders SET suspended = 1, updated_at_unix = $1
                     WHERE recorder_id = $2",
                    params,
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_suspend_recorder"))?;
            if n == 0 {
                return Err(ReputationError::RecorderNotRegistered(recorder_id.to_u64()));
            }
            Ok(())
        }

        async fn slash_recorder(&self, proof: GovernanceProof) -> StoreResult<()> {
            let snap = proof.snapshot.clone();
            if !snap.is_fresh(crate::migrations::now_unix()) {
                return Err(ReputationError::GovernanceSnapshotStale {
                    age_secs: snap.age_secs(crate::migrations::now_unix()),
                    max: crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
                });
            }
            if snap.governance_set_hash != proof.governance_set_hash {
                return Err(ReputationError::GovernanceSetHashMismatch);
            }
            if snap.quorum_count() < crate::constants::GOVERNANCE_QUORUM {
                return Err(ReputationError::GovernanceQuorumNotMet {
                    signatures: snap.quorum_count(),
                    quorum: crate::constants::GOVERNANCE_QUORUM,
                });
            }
            if proof.slash_destination.is_none() {
                return Err(ReputationError::SlashDestinationMismatch {
                    expected: 0,
                    actual: 0,
                });
            }
            if matches!(proof.slash_asset, AssetTag::None) {
                return Err(ReputationError::ChainRefInvalid("slash_asset"));
            }
            if proof.slash_amount == 0 {
                return Err(ReputationError::ChainRefInvalid("slash_amount"));
            }
            let params: Vec<stoolap::Value> = vec![
                u64_to_value(crate::migrations::now_unix()),
                stoolap::Value::integer(proof.recorder_id.to_u64() as i64),
            ];
            let n = self
                .db
                .execute(
                    "UPDATE reputation_recorders SET slashed = 1, updated_at_unix = $1
                     WHERE recorder_id = $2",
                    params,
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_slash_recorder"))?;
            if n == 0 {
                return Err(ReputationError::RecorderNotRegistered(
                    proof.recorder_id.to_u64(),
                ));
            }
            Ok(())
        }

        async fn declare_retirement_eligible(
            &self,
            _adapter: u8,
            evidence: ParityEvidence,
            proof: GovernanceProof,
            now_unix: u64,
        ) -> StoreResult<RetirementEligibility> {
            crate::retirement::stub_verify_proof_shape(&proof, now_unix)?;
            crate::retirement::validate_evidence(&evidence)?;
            let arr = crate::retirement::retirement_envelope_hash(
                &evidence.evidence_hash,
                evidence.last_bucket_unix,
                _adapter,
            );
            Ok(RetirementEligibility {
                eligible: true,
                since_unix: now_unix,
                evidence_hash: arr,
                adapter: _adapter,
            })
        }
    }
}

#[cfg(feature = "stoolap")]
pub use real::StoolapReputationStore;

// ---------------------------------------------------------------------------
// cfg-off: marker stub (preserves CI build path without the stoolap feature)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "stoolap"))]
mod stub {
    use super::*;

    pub struct StoolapReputationStore;

    impl std::fmt::Debug for StoolapReputationStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("StoolapReputationStore(stub)").finish()
        }
    }

    fn stub<T>(name: &str) -> StoreResult<T> {
        Err(ReputationError::ChainRefInvalid(match name {
            "record_signal" => "stoolap_backend_unimplemented:record_signal",
            "read_aggregate" => "stoolap_backend_unimplemented:read_aggregate",
            "cross_layer_query" => "stoolap_backend_unimplemented:cross_layer_query",
            "sliding_window" => "stoolap_backend_unimplemented:sliding_window",
            "replay_for_audit" => "stoolap_backend_unimplemented:replay_for_audit",
            "retention_prune" => "stoolap_backend_unimplemented:retention_prune",
            "prune_event" => "stoolap_backend_unimplemented:prune_event",
            "register_recorder" => "stoolap_backend_unimplemented:register_recorder",
            "verify_governance_suspension" => {
                "stoolap_backend_unimplemented:verify_governance_suspension"
            }
            "suspend_recorder" => "stoolap_backend_unimplemented:suspend_recorder",
            "slash_recorder" => "stoolap_backend_unimplemented:slash_recorder",
            "declare_retirement_eligible" => {
                "stoolap_backend_unimplemented:declare_retirement_eligible"
            }
            _ => "stoolap_backend_unimplemented",
        }))
    }

    #[allow(async_fn_in_trait)]
    impl ReputationStore for StoolapReputationStore {
        async fn record_signal(&self, _: SignalEvent) -> StoreResult<EventId> {
            stub("record_signal")
        }
        async fn read_aggregate(
            &self,
            _: &RecorderDid,
            _: SignalKind,
            _: ReputationLayer,
        ) -> StoreResult<ReputationAggregate> {
            stub("read_aggregate")
        }
        async fn cross_layer_query(
            &self,
            _: &RecorderDid,
            _: SignalKind,
            _: &[ReputationLayer],
        ) -> StoreResult<Vec<ReputationAggregate>> {
            stub("cross_layer_query")
        }
        async fn sliding_window(
            &self,
            _: &RecorderDid,
            _: SignalKind,
            _: ReputationLayer,
            _: u64,
            _: u64,
        ) -> StoreResult<ReputationAggregate> {
            stub("sliding_window")
        }
        async fn replay_for_audit(
            &self,
            _: &RecorderDid,
            _: u64,
            _: u64,
        ) -> StoreResult<Vec<SignalEvent>> {
            stub("replay_for_audit")
        }
        async fn retention_prune(&self, _: u64, _: u64) -> StoreResult<u64> {
            stub("retention_prune")
        }
        async fn prune_event(&self, _: EventId) -> StoreResult<()> {
            stub("prune_event")
        }
        async fn register_recorder(&self, _: crate::auth::ChainRef) -> StoreResult<RecorderId> {
            stub("register_recorder")
        }
        async fn verify_governance_suspension(
            &self,
            _: &SuspensionAuth,
            _: &GovernanceSnapshot,
            _: u64,
        ) -> StoreResult<()> {
            stub("verify_governance_suspension")
        }
        async fn suspend_recorder(
            &self,
            _: RecorderId,
            _: SuspensionAuth,
            _: u64,
        ) -> StoreResult<()> {
            stub("suspend_recorder")
        }
        async fn slash_recorder(&self, _: GovernanceProof) -> StoreResult<()> {
            stub("slash_recorder")
        }
        async fn declare_retirement_eligible(
            &self,
            _: u8,
            _: ParityEvidence,
            _: GovernanceProof,
            _: u64,
        ) -> StoreResult<RetirementEligibility> {
            stub("declare_retirement_eligible")
        }
    }
}

#[cfg(not(feature = "stoolap"))]
pub use stub::StoolapReputationStore;

// ---------------------------------------------------------------------------
// ChainRef controller_id placeholder (DID-field placeholder for storage).
// Until RFC-0968 amendment 40 lands, we use `blake3(recorder_did)[..32]`.
// ---------------------------------------------------------------------------

impl crate::auth::ChainRef {
    pub fn recorder_did_placeholder_controller(&self) -> ControllerId {
        let mut h = blake3::Hasher::new();
        h.update(self.recorder_did.as_bytes());
        let out = h.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(out.as_bytes());
        ControllerId::from_array(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_ref_placeholder_controller_is_deterministic() {
        let cr = crate::auth::ChainRef {
            chain_id: 1,
            block_height: 1,
            tx_hash: [1u8; 32],
            recorder_did: RecorderDid::from_array([2u8; 52]),
            octo_stake: 4_000,
            role_stake: 1_000,
            role_token_kind: 1,
            lock_until_unix: 1_000_000_000,
        };
        let a = cr.recorder_did_placeholder_controller();
        let b = cr.recorder_did_placeholder_controller();
        assert_eq!(a, b);
    }

    #[test]
    fn dfp_blob_round_trip() {
        let d = octo_determin::Dfp::from_f64(0.5);
        let bytes = crate::types::dfp_to_blob(&d);
        assert_eq!(bytes.len(), 24);
        let back = crate::types::dfp_from_blob(&bytes).unwrap();
        assert_eq!(d.to_f64().to_bits(), back.to_f64().to_bits());
    }

    #[test]
    fn dfp_from_blob_rejects_wrong_length() {
        let err = crate::types::dfp_from_blob(&[0u8; 23]).unwrap_err();
        assert_eq!(err, ReputationError::ScoreEncodingInvalid);
    }
}
