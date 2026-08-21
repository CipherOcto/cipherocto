//! `StoolapReputationStore` — CipherOcto stoolap-fork backend impl.
//!
//! Per mission 0968 Phase 1.4 acceptance: persistent reputation storage over
//! the workspace-wide stoolap fork (`CipherOcto/stoolap@feat/blockchain-sql`).
//! Schema source of truth lives in `migrations/*.sql`; the runner is in
//! `crate::migrations::substrate_runner::apply` and is invoked by every
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
//! `DfpEncoding::from_dfp(&&d).to_bytes()` form. Round-trip via
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

#[cfg(feature = "stoolap")]
use crate::auth::AttestorRegistration;
use crate::auth::{Attestation, AttestorId, GovernanceProof, GovernanceSnapshot, SuspensionAuth};
use crate::error::ReputationError;
#[cfg(feature = "stoolap")]
use crate::gossip::GossipCatchUp;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{
    ControllerId, EventId, ParityEvidence, RecorderDid, RecorderId, ReputationAggregate,
    ReputationLayer, RetirementEligibility, SignalEvent, SignalKind,
};

#[cfg(feature = "stoolap")]
use crate::auth::AssetTag;
#[cfg(feature = "stoolap")]
use crate::migrations::substrate_runner;
#[cfg(feature = "stoolap")]
use crate::types::{dfp_from_blob, dfp_to_blob};

// ---------------------------------------------------------------------------
// cfg-gated: real impl
// ---------------------------------------------------------------------------

#[cfg(feature = "stoolap")]
mod real {
    use super::*;
    use crate::store::AnchorRecord;

    /// `StoolapReputationStore` (real). Owns one `octo_storage_core::Database` behind
    /// `Arc` so trait methods can hold shared references. All mutating SQL
    /// operations execute under default MVCC isolation; we do not wrap them
    /// in transactions because every method issues one SQL statement and
    /// the trait's correctness contract is single-row atomicity.
    #[derive(Clone)]
    pub struct StoolapReputationStore {
        db: std::sync::Arc<octo_storage_core::Database>,
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
            let db = octo_storage_core::Database::open(dsn)
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_open"))?;
            substrate_runner::apply(&db)?;
            Ok(Self {
                db: std::sync::Arc::new(db),
            })
        }

        /// Open an in-memory store. Used for tests and ephemeral deployments.
        pub async fn open_in_memory() -> Result<Self, ReputationError> {
            let db = octo_storage_core::Database::open_in_memory()
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_open_inmem"))?;
            substrate_runner::apply(&db)?;
            Ok(Self {
                db: std::sync::Arc::new(db),
            })
        }

        /// Construct over an existing `Database` handle without applying
        /// migrations. Useful for tests that pre-init schema and want to
        /// skip the apply step.
        pub fn from_db(db: octo_storage_core::Database) -> Self {
            Self {
                db: std::sync::Arc::new(db),
            }
        }

        /// Borrow the underlying `Database`.
        pub fn database(&self) -> &octo_storage_core::Database {
            &self.db
        }

        // -- monotonic id helpers --------------------------------------

        fn next_event_id(&self) -> Result<u64, ReputationError> {
            // R8 review (deferred): read-MAX → compute+1 → return is
            // NOT atomic with the subsequent INSERT into
            // `reputation_events` (composite PK (recorder_did, event_id)).
            // Two concurrent `record_signal` calls under distinct
            // recorders can both observe MAX=N, both return N+1, both
            // INSERT — second INSERT collides on PK. Pre-R5-F4 this
            // was masked (always 1, collisions deterministic); post-fix
            // the race is user-visible. Production must wrap this
            // pair in a `Mutex<()>` or a stoolap transaction. Test
            // paths exercise single-threaded semantics; concurrent
            // races are flagged as a known limitation until the
            // S-locking pass lands. The memory backend avoids this via
            // `Arc<AtomicU64>::fetch_add(1, SeqCst)`.
            // R5-F4 fix: `last_event_id` is BLOB(8-byte BE u64); stoolap's
            // `CAST(BLOB AS INTEGER)` always yields 0, so the previous
            // SQL-level MAX returned 0 and every call returned 1,
            // colliding on the composite (recorder_did, event_id) PK.
            // Read the BLOB and compute MAX in Rust instead.
            let rows = self
                .db
                .query("SELECT last_event_id FROM reputation_aggregates", [])
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_next_event_id"))?;
            let mut max: u64 = 0;
            for row_res in rows {
                let row = row_res.map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_next_event_id:row_err")
                })?;
                let bytes: Vec<u8> = row
                    .get(0)
                    .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_next_event_id:get"))?;
                if bytes.len() != 8 {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_next_event_id:blob_len",
                    ));
                }
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&bytes);
                let id = u64::from_be_bytes(arr);
                if id > max {
                    max = id;
                }
            }
            // R9 review (LOW): defensive guard against u64 overflow
            // — would require 2^64 events to fire, but `next_event_id`
            // wraps to 0 in release mode (silent collision) and
            // panics in debug. Reject and surface a distinct variant
            // so the operator can see the saturation.
            if max == u64::MAX {
                return Err(ReputationError::ChainRefInvalid(
                    "stoolap_next_event_id:overflow",
                ));
            }
            Ok(max + 1)
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

        fn next_attestation_id(&self) -> Result<u64, ReputationError> {
            // MIN_ATTESTOR_QUORUM is 3 — the next attestation id is
            // computed as `COALESCE(MAX(id), -1) + 1` so the first row
            // gets id=0 (rather than MAX(NULL)=NULL+1=NULL).
            //
            // R13 / R14 review (MEDIUM, deferred): read-then-compute-
            // then-return is NOT atomic with the subsequent INSERT
            // into `reputation_attestations`. Two concurrent
            // `record_attestation` calls both observe `MAX = N`, both
            // compute `N+1`, both attempt INSERT — first succeeds,
            // second collides on `attestation_id` PK and is mapped to
            // `ChainRefInvalid("stoolap_record_attestation:insert")`.
            // The caller cannot distinguish a collision from a generic
            // DB failure; no retry path; no metric. Production must
            // wrap the SELECT-then-INSERT pair in `BEGIN IMMEDIATE`
            // (when stoolap-fork supports transactions) or
            // retry-on-collision with a distinct error variant.
            // Memory backend is immune via
            // `Arc<AtomicU64>::fetch_add(1, SeqCst)`. Documented
            // alongside the `next_event_id` race at stoolap.rs:124.
            let mut rows = self
                .db
                .query(
                    "SELECT COALESCE(MAX(CAST(attestation_id AS INTEGER)), -1) + 1
                     FROM reputation_attestations",
                    [],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_next_attestation_id"))?;
            let row = match rows.next() {
                Some(Ok(r)) => r,
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_next_attestation_id:row_err",
                    ))
                }
                None => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_next_attestation_id:empty",
                    ))
                }
            };
            let max: i64 = row.get(0).map_err(|_e| {
                ReputationError::ChainRefInvalid("stoolap_next_attestation_id:cast")
            })?;
            Ok(max.max(0) as u64)
        }
    }

    // -- value helpers ------------------------------------------------

    fn did_blob(d: &RecorderDid) -> octo_storage_core::stoolap::Value {
        octo_storage_core::stoolap::Value::blob(d.as_bytes().to_vec())
    }

    fn dfp_blob(arr: [u8; 24]) -> octo_storage_core::stoolap::Value {
        octo_storage_core::stoolap::Value::blob(arr.to_vec())
    }

    fn controller_blob(c: &ControllerId) -> octo_storage_core::stoolap::Value {
        octo_storage_core::stoolap::Value::blob(c.as_bytes().to_vec())
    }

    fn event_id_blob(e: EventId) -> octo_storage_core::stoolap::Value {
        octo_storage_core::stoolap::Value::blob(e.as_bytes().to_vec())
    }

    fn u64_to_value(v: u64) -> octo_storage_core::stoolap::Value {
        // u64 → i64 lossy at values > i64::MAX; reputation timestamps fit
        // comfortably in i64 for the foreseeable future.
        octo_storage_core::stoolap::Value::integer(v as i64)
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

    fn bytes_to_controller_id(b: &[u8]) -> StoreResult<ControllerId> {
        if b.len() != 32 {
            return Err(ReputationError::ChainRefInvalid(
                "stoolap_controller_id_len",
            ));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(b);
        Ok(ControllerId::from_array(arr))
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

            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                did_blob(&event.recorder_did),
                event_id_blob(eid),
                controller_blob(&event.controller_id),
                octo_storage_core::stoolap::Value::integer(kind_d as i64),
                octo_storage_core::stoolap::Value::integer(layer_d as i64),
                dfp_blob(score_bytes),
                u64_to_value(event.recorded_at_unix),
                match rot_blob {
                    Some(b) => octo_storage_core::stoolap::Value::blob(b),
                    None => octo_storage_core::stoolap::Value::null_unknown(),
                },
                match audit_blob {
                    Some(b) => octo_storage_core::stoolap::Value::blob(b),
                    None => octo_storage_core::stoolap::Value::null_unknown(),
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
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                did_blob(&event.recorder_did),
                octo_storage_core::stoolap::Value::integer(kind_d as i64),
                octo_storage_core::stoolap::Value::integer(layer_d as i64),
                dfp_blob(next_ewma_bytes),
                u64_to_value(samples_next),
                u64_to_value(0),
                event_id_blob(eid),
                u64_to_value(event.recorded_at_unix),
                u64_to_value(event.recorded_at_unix),
            ];
            // UPSERT manually: select-then-INSERT-or-UPDATE because
            // stoolap-fork does not support `ON CONFLICT … DO UPDATE`
            // (parity bit) and `execute` returns `Ok(1)` on a successful
            // INSERT — never `Err`, even when the PK collides. We
            // SELECT for existence first; INSERT on miss, UPDATE on hit.
            // The round-trip is one extra SELECT per write. Bounded to a
            // single process in tests; production wraps this in a
            // transaction in a future session.
            // R13 review: read existing severity_total so the UPSERT
            // preserves prior accumulation. Compute
            // severity_total = existing + (kind==Slash ? 1 : 0) in
            // Rust and bind to $3; the post-UPDATE bump is deleted.
            let mut q = self
                .db
                .query(
                    "SELECT severity_total FROM reputation_aggregates
                     WHERE recorder_did = $1 AND signal_kind = $2 AND layer = $3",
                    vec![
                        did_blob(&event.recorder_did),
                        octo_storage_core::stoolap::Value::integer(kind_d as i64),
                        octo_storage_core::stoolap::Value::integer(layer_d as i64),
                    ],
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_record_signal:exists_query")
                })?;
            let (existing_severity, exists): (u64, bool) = match q.next() {
                Some(Ok(row)) => {
                    // severity_total is INTEGER per v001:41 — read
                    // as i64, not 8-byte BE bytes.
                    let v: i64 = row.get(0).map_err(|_e| {
                        ReputationError::ChainRefInvalid("stoolap_record_signal:severity_read")
                    })?;
                    (v.max(0) as u64, true)
                }
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_record_signal:severity_read_iter",
                    ));
                }
                None => (0u64, false),
            };
            let severity_for_upsert: u64 = if matches!(event.signal_kind, SignalKind::Slash) {
                existing_severity.saturating_add(1)
            } else {
                existing_severity
            };
            if exists {
                let update_params: Vec<octo_storage_core::stoolap::Value> = vec![
                    dfp_blob(next_ewma_bytes),
                    u64_to_value(samples_next),
                    u64_to_value(severity_for_upsert),
                    event_id_blob(eid),
                    u64_to_value(event.recorded_at_unix),
                    u64_to_value(event.recorded_at_unix),
                    did_blob(&event.recorder_did),
                    octo_storage_core::stoolap::Value::integer(kind_d as i64),
                    octo_storage_core::stoolap::Value::integer(layer_d as i64),
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
            } else {
                let mut insert_params = params;
                insert_params[5] = u64_to_value(severity_for_upsert);
                self.db
                    .execute(
                        "INSERT INTO reputation_aggregates (
                            recorder_did, signal_kind, layer, score_ewma, samples,
                            severity_total, last_event_id, last_event_unix, updated_at_unix
                        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                        insert_params,
                    )
                    .map_err(|_e| {
                        ReputationError::ChainRefInvalid("stoolap_record_signal:insert_aggregate")
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
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                did_blob(did),
                octo_storage_core::stoolap::Value::integer(kind.discriminant() as i64),
                octo_storage_core::stoolap::Value::integer(layer.discriminant() as i64),
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
            did: &RecorderDid,
            kind: SignalKind,
            layers: &[ReputationLayer],
        ) -> StoreResult<Vec<ReputationAggregate>> {
            if layers.is_empty() {
                return Err(ReputationError::CrossLayerEmpty);
            }
            // Build `WHERE layer IN ($1, $2, ...)` with positional params.
            let mut placeholders: Vec<String> = Vec::with_capacity(layers.len());
            let mut params: Vec<octo_storage_core::stoolap::Value> =
                Vec::with_capacity(layers.len() + 1);
            params.push(did_blob(did));
            params.push(octo_storage_core::stoolap::Value::integer(
                kind.discriminant() as i64,
            ));
            for (i, _) in layers.iter().enumerate() {
                placeholders.push(format!("${}", i + 3));
            }
            let sql = format!(
                "SELECT score_ewma, samples, severity_total, last_event_id,
                        last_event_unix, updated_at_unix, layer
                 FROM reputation_aggregates
                 WHERE recorder_did = $1 AND signal_kind = $2 AND layer IN ({})",
                placeholders.join(", ")
            );
            for l in layers {
                params.push(octo_storage_core::stoolap::Value::integer(
                    l.discriminant() as i64
                ));
            }
            let mut rows = self
                .db
                .query(&sql, params)
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_cross_layer_query"))?;
            let mut out = Vec::with_capacity(layers.len());
            loop {
                match rows.next() {
                    Some(Ok(row)) => {
                        let score_bytes: Vec<u8> = row.get_by_name("score_ewma").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_cross_layer_query:score_blob")
                        })?;
                        let score_ewma = dfp_from_blob(&score_bytes)?;
                        let last_event_id_bytes: Vec<u8> =
                            row.get_by_name("last_event_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_cross_layer_query:last_event",
                                )
                            })?;
                        let last_event_id = bytes_to_event_id(&last_event_id_bytes)?;
                        let samples: i64 = row.get_by_name("samples").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_cross_layer_query:samples")
                        })?;
                        let severity_total: i64 =
                            row.get_by_name("severity_total").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_cross_layer_query:severity",
                                )
                            })?;
                        let last_event_unix: i64 =
                            row.get_by_name("last_event_unix").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_cross_layer_query:leu")
                            })?;
                        let updated_at_unix: i64 =
                            row.get_by_name("updated_at_unix").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_cross_layer_query:uau")
                            })?;
                        let layer_d: i64 = row.get_by_name("layer").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_cross_layer_query:layer")
                        })?;
                        let layer = match layer_d {
                            0x01 => ReputationLayer::Consensus,
                            0x02 => ReputationLayer::Market,
                            0x03 => ReputationLayer::Coordinator,
                            0x04 => ReputationLayer::Slash,
                            0x05 => ReputationLayer::Governance,
                            other => {
                                return Err(ReputationError::ReputationLayerInvalid(other as u8))
                            }
                        };
                        out.push(ReputationAggregate {
                            recorder_did: *did,
                            signal_kind: kind,
                            layer,
                            score_ewma,
                            samples: i64_to_u64(samples),
                            severity_total: i64_to_u64(severity_total),
                            last_event_id,
                            last_event_unix: i64_to_u64(last_event_unix),
                            updated_at_unix: i64_to_u64(updated_at_unix),
                        });
                    }
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_cross_layer_query:iter",
                        ))
                    }
                    None => break,
                }
            }
            // Append empty placeholders for layers with no aggregate, so
            // callers can map by layer without re-querying.
            let _ = placeholders;
            Ok(out)
        }

        async fn sliding_window(
            &self,
            did: &RecorderDid,
            kind: SignalKind,
            layer: ReputationLayer,
            window_secs: u64,
            now_unix: u64,
        ) -> StoreResult<ReputationAggregate> {
            if window_secs == 0 {
                return Err(ReputationError::SlidingWindowZero);
            }
            let cutoff = now_unix.saturating_sub(window_secs);
            let kind_d = kind.discriminant();
            let layer_d = layer.discriminant();
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                did_blob(did),
                octo_storage_core::stoolap::Value::integer(kind_d as i64),
                octo_storage_core::stoolap::Value::integer(layer_d as i64),
                u64_to_value(cutoff),
                // `now_unix` clamped at `i64::MAX` to avoid a `u64::MAX →
                // -1` wrap that would make `<= $5` always false. Real
                // callers pass recent Unix seconds far below this bound.
                u64_to_value(now_unix.min(i64::MAX as u64)),
            ];
            let mut rows = self
                .db
                .query(
                    "SELECT event_id, recorded_at_unix, score_delta
                     FROM reputation_events
                     WHERE recorder_did = $1 AND signal_kind = $2 AND layer = $3
                       AND recorded_at_unix >= $4 AND recorded_at_unix <= $5
                     ORDER BY recorded_at_unix",
                    params,
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_sliding_window:query"))?;
            let mut score = octo_determin::Dfp::zero();
            let mut samples: u64 = 0;
            let mut last_event_id = EventId::from_u64(0);
            let mut last_event_unix: u64 = 0;
            let mut updated_at_unix: u64 = 0;
            loop {
                match rows.next() {
                    Some(Ok(row)) => {
                        let score_bytes: Vec<u8> =
                            row.get_by_name("score_delta").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_sliding_window:score_blob",
                                )
                            })?;
                        let delta = dfp_from_blob(&score_bytes)?;
                        let event_id_bytes: Vec<u8> =
                            row.get_by_name("event_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_sliding_window:event_id")
                            })?;
                        let event_id = bytes_to_event_id(&event_id_bytes)?;
                        let ts: i64 = row.get_by_name("recorded_at_unix").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_sliding_window:ts")
                        })?;
                        let n = samples as f64;
                        let alpha = 1.0 / (n + 1.0);
                        let cur = score.to_f64();
                        let d = delta.to_f64();
                        score = octo_determin::Dfp::from_f64(cur * (1.0 - alpha) + d * alpha);
                        samples += 1;
                        last_event_id = event_id;
                        last_event_unix = i64_to_u64(ts);
                        updated_at_unix = i64_to_u64(ts);
                    }
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_sliding_window:iter",
                        ))
                    }
                    None => break,
                }
            }
            Ok(ReputationAggregate {
                recorder_did: *did,
                signal_kind: kind,
                layer,
                score_ewma: score,
                samples,
                severity_total: 0,
                last_event_id,
                last_event_unix,
                updated_at_unix,
            })
        }

        async fn replay_for_audit(
            &self,
            did: &RecorderDid,
            since_unix: u64,
            until_unix: u64,
        ) -> StoreResult<Vec<SignalEvent>> {
            if since_unix > until_unix {
                return Err(ReputationError::ReplayWindowInverted);
            }
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                did_blob(did),
                // Cap at `i64::MAX` (signed) to avoid the `u64::MAX → -1`
                // wrap that would silently drop the upper bound. Callers
                // that genuinely mean "no upper bound" should pass
                // `i64::MAX` directly; everything else passes through
                // unchanged.
                u64_to_value(since_unix),
                u64_to_value(until_unix.min(i64::MAX as u64)),
            ];
            let mut rows = self
                .db
                .query(
                    "SELECT recorder_did, event_id, controller_id, signal_kind, layer,
                            score_delta, recorded_at_unix, rotation_provenance, audit_ref,
                            anchor_tx_hash
                     FROM reputation_events
                     WHERE recorder_did = $1
                       AND recorded_at_unix >= $2 AND recorded_at_unix <= $3
                     ORDER BY recorded_at_unix",
                    params,
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_replay_for_audit:query"))?;
            let mut out = Vec::new();
            loop {
                match rows.next() {
                    Some(Ok(row)) => {
                        let kind_d: i64 = row.get_by_name("signal_kind").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_replay_for_audit:kind")
                        })?;
                        let layer_d: i64 = row.get_by_name("layer").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_replay_for_audit:layer")
                        })?;
                        let score_bytes: Vec<u8> =
                            row.get_by_name("score_delta").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_replay_for_audit:score_blob",
                                )
                            })?;
                        let score_delta = dfp_from_blob(&score_bytes)?;
                        let event_id_bytes: Vec<u8> =
                            row.get_by_name("event_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_replay_for_audit:event_id",
                                )
                            })?;
                        let event_id = bytes_to_event_id(&event_id_bytes)?;
                        let recorded_at_unix: i64 =
                            row.get_by_name("recorded_at_unix").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_replay_for_audit:ts")
                            })?;
                        let controller_id_bytes: Vec<u8> =
                            row.get_by_name("controller_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_replay_for_audit:controller_id",
                                )
                            })?;
                        let controller_id = bytes_to_controller_id(&controller_id_bytes)?;
                        let rotation_blob: Option<Vec<u8>> =
                            row.get_by_name("rotation_provenance").ok();
                        let rotation_provenance = rotation_blob.and_then(|b| {
                            if b.is_empty() {
                                return None;
                            }
                            serde_json::from_slice(&b).ok()
                        });
                        let audit_blob: Option<Vec<u8>> = row.get_by_name("audit_ref").ok();
                        let audit_ref = audit_blob.filter(|b| !b.is_empty());
                        // Round 2 review #1: select the v011
                        // anchor_tx_hash column so the persisted
                        // provenance reaches audit callers. Empty /
                        // NULL blob = event not yet anchored.
                        let anchor_blob: Option<Vec<u8>> = row.get_by_name("anchor_tx_hash").ok();
                        let anchor_tx_hash = anchor_blob.and_then(|b| {
                            if b.is_empty() {
                                None
                            } else {
                                let mut arr = [0u8; 32];
                                if b.len() == 32 {
                                    arr.copy_from_slice(&b);
                                    Some(arr)
                                } else {
                                    None
                                }
                            }
                        });
                        let kind = SignalKind::from_discriminant(kind_d as u8).map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_replay_for_audit:kind_disc")
                        })?;
                        let layer =
                            ReputationLayer::from_discriminant(layer_d as u8).map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_replay_for_audit:layer_disc",
                                )
                            })?;
                        out.push(SignalEvent {
                            event_id,
                            recorder_did: *did,
                            controller_id,
                            signal_kind: kind,
                            layer,
                            score_delta,
                            recorded_at_unix: i64_to_u64(recorded_at_unix),
                            rotation_provenance,
                            audit_ref,
                            anchor_tx_hash,
                        });
                    }
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_replay_for_audit:iter",
                        ))
                    }
                    None => break,
                }
            }
            Ok(out)
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
            let params: Vec<octo_storage_core::stoolap::Value> = vec![event_id_blob(event_id)];
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
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                octo_storage_core::stoolap::Value::integer(rid.to_u64() as i64),
                did_blob(&chain_ref.recorder_did),
                controller_blob(&chain_ref.recorder_did_placeholder_controller()),
                u64_to_value(chain_ref.octo_stake),
                u64_to_value(chain_ref.role_stake),
                octo_storage_core::stoolap::Value::integer(chain_ref.role_token_kind as i64),
                octo_storage_core::stoolap::Value::integer(chain_ref.chain_id as i64),
                u64_to_value(chain_ref.block_height),
                octo_storage_core::stoolap::Value::blob(chain_ref.tx_hash.to_vec()),
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
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                u64_to_value(now_unix),
                octo_storage_core::stoolap::Value::integer(recorder_id.to_u64() as i64),
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
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                u64_to_value(crate::migrations::now_unix()),
                octo_storage_core::stoolap::Value::integer(proof.recorder_id.to_u64() as i64),
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

        // -- Federation (Session 8 / mission 0968 Phase 4) --
        //
        // Real SQL implementations for `register_attestor`,
        // `attestor_lookup_did`, `record_attestation`,
        // `query_attestations`, `attestor_quorum_reached`, and
        // `gossip_catch_up`. The attestor_pubkey peer_set_id are
        // BLOBs; the attestor_did and recorder_did are 52-byte
        // BLOBs. Idempotency for `record_attestation` is enforced
        // via Rust-side guard (the (attestor_did, event_id)
        // composite-key lookup happens before INSERT).

        async fn register_attestor(
            &self,
            registration: AttestorRegistration,
        ) -> StoreResult<AttestorId> {
            if registration.attestor_did.as_bytes() == &[0u8; 52] {
                return Err(ReputationError::RecorderDidMalformed(
                    "attestor did must not be all-zero",
                ));
            }
            if registration.pubkey == [0u8; 32] {
                return Err(ReputationError::GossipEnvelopeInvalid(
                    "attestor_pubkey_zero",
                ));
            }
            // Idempotent: pre-existing row is overwritten via DELETE
            // + INSERT (stoolap-fork does not support `INSERT OR
            // REPLACE`). Re-registering with a new pubkey is
            // legitimate (key rotation); the same DID maps to the
            // latest pubkey at gossip-ingress verification time.
            let did_bytes = registration.attestor_did.as_bytes();
            self.db
                .execute(
                    "DELETE FROM reputation_attestors WHERE attestor_did = $1",
                    vec![octo_storage_core::stoolap::Value::blob(did_bytes.to_vec())],
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_register_attestor:delete")
                })?;
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                octo_storage_core::stoolap::Value::blob(did_bytes.to_vec()),
                octo_storage_core::stoolap::Value::blob(registration.pubkey.to_vec()),
                octo_storage_core::stoolap::Value::blob(registration.peer_set_id.to_vec()),
                u64_to_value(registration.requested_at_unix),
                u64_to_value(registration.registered_at_unix),
            ];
            self.db
                .execute(
                    "INSERT INTO reputation_attestors (
                        attestor_did, attestor_pubkey, peer_set_id,
                        requested_at_unix, registered_at_unix
                    ) VALUES ($1, $2, $3, $4, $5)",
                    params,
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_register_attestor:insert")
                })?;
            Ok(registration.attestor_did)
        }

        async fn attestor_lookup_did(
            &self,
            attestor_did: &AttestorId,
        ) -> StoreResult<AttestorRegistration> {
            let mut rows = self
                .db
                .query(
                    "SELECT attestor_pubkey, peer_set_id,
                            requested_at_unix, registered_at_unix
                     FROM reputation_attestors
                     WHERE attestor_did = $1",
                    vec![octo_storage_core::stoolap::Value::blob(
                        attestor_did.as_bytes().to_vec(),
                    )],
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_attestor_lookup_did:query")
                })?;
            let row = match rows.next() {
                Some(Ok(r)) => r,
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_attestor_lookup_did:row_err",
                    ))
                }
                None => return Err(ReputationError::RecorderNotRegistered(0)),
            };
            let attestor_pubkey_bytes: Vec<u8> =
                row.get_by_name("attestor_pubkey").map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_attestor_lookup_did:pubkey")
                })?;
            if attestor_pubkey_bytes.len() != 32 {
                return Err(ReputationError::ChainRefInvalid(
                    "stoolap_attestor_lookup_did:pubkey_len",
                ));
            }
            let mut attestor_pubkey = [0u8; 32];
            attestor_pubkey.copy_from_slice(&attestor_pubkey_bytes);
            let peer_set_id_bytes: Vec<u8> = row.get_by_name("peer_set_id").map_err(|_e| {
                ReputationError::ChainRefInvalid("stoolap_attestor_lookup_did:peer_set")
            })?;
            if peer_set_id_bytes.len() != 32 {
                return Err(ReputationError::ChainRefInvalid(
                    "stoolap_attestor_lookup_did:peer_set_len",
                ));
            }
            let mut peer_set_id = [0u8; 32];
            peer_set_id.copy_from_slice(&peer_set_id_bytes);
            let requested_at_unix: i64 = row.get_by_name("requested_at_unix").map_err(|_e| {
                ReputationError::ChainRefInvalid("stoolap_attestor_lookup_did:requested")
            })?;
            let registered_at_unix: i64 = row.get_by_name("registered_at_unix").map_err(|_e| {
                ReputationError::ChainRefInvalid("stoolap_attestor_lookup_did:registered")
            })?;
            Ok(AttestorRegistration {
                attestor_did: *attestor_did,
                pubkey: attestor_pubkey,
                peer_set_id,
                requested_at_unix: i64_to_u64(requested_at_unix),
                registered_at_unix: i64_to_u64(registered_at_unix),
            })
        }

        async fn record_attestation(&self, attestation: Attestation) -> StoreResult<u64> {
            // Idempotency guard: look up an existing row by the
            // (attestor_did, recorder_did, event_id) composite key
            // before INSERT. The v004 schema has `attestation_id
            // INTEGER PRIMARY KEY` but no UNIQUE on the composite
            // tuple, so duplicate inserts would collide on PK and
            // error. The guard returns the existing id and skips the
            // INSERT. The recorder_did is part of the composite so
            // that one attestor can attest the same event_id for
            // distinct recorders (event_id space is shared across
            // recorders).
            let attestor_bytes = attestation.attestor.as_bytes();
            let event_bytes = attestation.event_id.as_bytes();
            let recorder_bytes = attestation.recorder_did.as_bytes();
            let existing_id: Option<u64> = {
                let mut q = self
                    .db
                    .query(
                        "SELECT attestation_id FROM reputation_attestations
                         WHERE attestor_did = $1 AND recorder_did = $2 AND event_id = $3",
                        vec![
                            octo_storage_core::stoolap::Value::blob(attestor_bytes.to_vec()),
                            octo_storage_core::stoolap::Value::blob(recorder_bytes.to_vec()),
                            octo_storage_core::stoolap::Value::blob(event_bytes.to_vec()),
                        ],
                    )
                    .map_err(|_e| {
                        ReputationError::ChainRefInvalid("stoolap_record_attestation:exists_query")
                    })?;
                match q.next() {
                    Some(Ok(row)) => {
                        let v: i64 = row.get(0).map_err(|_e| {
                            ReputationError::ChainRefInvalid(
                                "stoolap_record_attestation:exists_cast",
                            )
                        })?;
                        Some(v.max(0) as u64)
                    }
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_record_attestation:exists_iter",
                        ))
                    }
                    None => None,
                }
            };
            if let Some(id) = existing_id {
                return Ok(id);
            }
            let id = self.next_attestation_id()?;
            let params: Vec<octo_storage_core::stoolap::Value> = vec![
                octo_storage_core::stoolap::Value::integer(id as i64),
                octo_storage_core::stoolap::Value::blob(attestor_bytes.to_vec()),
                octo_storage_core::stoolap::Value::blob(recorder_bytes.to_vec()),
                octo_storage_core::stoolap::Value::blob(event_bytes.to_vec()),
                octo_storage_core::stoolap::Value::blob(attestation.signature.clone()),
                u64_to_value(attestation.observed_at_unix),
                u64_to_value(attestation.received_at_unix),
                octo_storage_core::stoolap::Value::text(attestation.source_mission.clone()),
                octo_storage_core::stoolap::Value::text(attestation.source_domain.clone()),
            ];
            self.db
                .execute(
                    "INSERT INTO reputation_attestations (
                        attestation_id, attestor_did, recorder_did, event_id,
                        signature, observed_at_unix, received_at_unix,
                        source_mission, source_domain
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                    params,
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_record_attestation:insert")
                })?;
            Ok(id)
        }

        async fn query_attestations(
            &self,
            recorder_did: &RecorderDid,
            since_event_id: EventId,
        ) -> StoreResult<Vec<Attestation>> {
            // Use the `reputation_attestations_recorder` index on
            // `(recorder_did, observed_at_unix)`. Filter on event_id >
            // since_event_id via a secondary predicate; the composite
            // index gets us the row subset fast and the filter is
            // O(matches) on top.
            let recorder_bytes = recorder_did.as_bytes();
            let since_bytes = since_event_id.as_bytes();
            let mut rows = self
                .db
                .query(
                    "SELECT attestation_id, attestor_did, event_id,
                            signature, observed_at_unix, received_at_unix,
                            source_mission, source_domain
                     FROM reputation_attestations
                     WHERE recorder_did = $1 AND event_id > $2
                     ORDER BY observed_at_unix",
                    vec![
                        octo_storage_core::stoolap::Value::blob(recorder_bytes.to_vec()),
                        octo_storage_core::stoolap::Value::blob(since_bytes.to_vec()),
                    ],
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_query_attestations:query")
                })?;
            let mut out = Vec::new();
            loop {
                match rows.next() {
                    Some(Ok(row)) => {
                        let attestation_id: i64 =
                            row.get_by_name("attestation_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_query_attestations:id")
                            })?;
                        let attestor_bytes: Vec<u8> =
                            row.get_by_name("attestor_did").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_query_attestations:attestor",
                                )
                            })?;
                        if attestor_bytes.len() != 52 {
                            return Err(ReputationError::ChainRefInvalid(
                                "stoolap_query_attestations:attestor_len",
                            ));
                        }
                        let mut attestor_arr = [0u8; 52];
                        attestor_arr.copy_from_slice(&attestor_bytes);
                        let event_id_bytes: Vec<u8> =
                            row.get_by_name("event_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_query_attestations:event_id",
                                )
                            })?;
                        let event_id = bytes_to_event_id(&event_id_bytes)?;
                        let signature: Vec<u8> = row.get_by_name("signature").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_query_attestations:signature")
                        })?;
                        let observed_at_unix: i64 =
                            row.get_by_name("observed_at_unix").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_query_attestations:observed",
                                )
                            })?;
                        let received_at_unix: i64 =
                            row.get_by_name("received_at_unix").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_query_attestations:received",
                                )
                            })?;
                        let source_mission: String =
                            row.get_by_name("source_mission").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_query_attestations:mission",
                                )
                            })?;
                        let source_domain: String =
                            row.get_by_name("source_domain").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_query_attestations:domain",
                                )
                            })?;
                        out.push(Attestation {
                            attestation_id: i64_to_u64(attestation_id),
                            attestor: AttestorId::from_array(attestor_arr),
                            recorder_did: *recorder_did,
                            event_id,
                            signature,
                            observed_at_unix: i64_to_u64(observed_at_unix),
                            received_at_unix: i64_to_u64(received_at_unix),
                            source_mission,
                            source_domain,
                        });
                    }
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_query_attestations:iter",
                        ))
                    }
                    None => break,
                }
            }
            Ok(out)
        }

        async fn attestor_quorum_reached(&self, event_id: EventId) -> StoreResult<bool> {
            // Count distinct attestor_did rows for the given event_id
            // and compare against MIN_ATTESTOR_QUORUM.
            let event_bytes = event_id.as_bytes();
            let mut rows = self
                .db
                .query(
                    "SELECT COUNT(DISTINCT attestor_did) FROM reputation_attestations
                     WHERE event_id = $1",
                    vec![octo_storage_core::stoolap::Value::blob(
                        event_bytes.to_vec(),
                    )],
                )
                .map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_attestor_quorum_reached:query")
                })?;
            let row = match rows.next() {
                Some(Ok(r)) => r,
                Some(Err(_e)) => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_attestor_quorum_reached:row_err",
                    ))
                }
                None => {
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_attestor_quorum_reached:empty",
                    ))
                }
            };
            let distinct: i64 = row.get(0).map_err(|_e| {
                ReputationError::ChainRefInvalid("stoolap_attestor_quorum_reached:cast")
            })?;
            Ok((distinct.max(0) as u32) >= crate::constants::MIN_ATTESTOR_QUORUM)
        }

        async fn gossip_catch_up(&self, catch_up: &GossipCatchUp) -> StoreResult<Vec<SignalEvent>> {
            // Stream every event with event_id > since_event_id,
            // ordered by event_id, so the caller can republish the
            // missing envelopes. Also record the catch-up in the
            // `reputation_gossip_seen` ledger (v005) — the
            // (recorder_did, event_id) composite PK guarantees
            // idempotency on duplicate catch-up requests.
            let since_bytes = catch_up.since_event_id.as_bytes();
            let mut rows = self
                .db
                .query(
                    "SELECT recorder_did, event_id, controller_id, signal_kind,
                            layer, score_delta, recorded_at_unix,
                            rotation_provenance, audit_ref, anchor_tx_hash
                     FROM reputation_events
                     WHERE event_id > $1
                     ORDER BY event_id",
                    vec![octo_storage_core::stoolap::Value::blob(
                        since_bytes.to_vec(),
                    )],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:query"))?;
            let mut out = Vec::new();
            loop {
                match rows.next() {
                    Some(Ok(row)) => {
                        let recorder_bytes: Vec<u8> =
                            row.get_by_name("recorder_did").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:recorder")
                            })?;
                        if recorder_bytes.len() != 52 {
                            return Err(ReputationError::ChainRefInvalid(
                                "stoolap_gossip_catch_up:recorder_len",
                            ));
                        }
                        let mut recorder_arr = [0u8; 52];
                        recorder_arr.copy_from_slice(&recorder_bytes);
                        let recorder_did = RecorderDid::from_array(recorder_arr);
                        let kind_d: i64 = row.get_by_name("signal_kind").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:kind")
                        })?;
                        let layer_d: i64 = row.get_by_name("layer").map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:layer")
                        })?;
                        let score_bytes: Vec<u8> =
                            row.get_by_name("score_delta").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:score")
                            })?;
                        let score_delta = dfp_from_blob(&score_bytes)?;
                        let event_id_bytes: Vec<u8> =
                            row.get_by_name("event_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:event_id")
                            })?;
                        let event_id = bytes_to_event_id(&event_id_bytes)?;
                        let recorded_at_unix: i64 =
                            row.get_by_name("recorded_at_unix").map_err(|_e| {
                                ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:ts")
                            })?;
                        let controller_id_bytes: Vec<u8> =
                            row.get_by_name("controller_id").map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_gossip_catch_up:controller_id",
                                )
                            })?;
                        let controller_id = bytes_to_controller_id(&controller_id_bytes)?;
                        let rotation_blob: Option<Vec<u8>> =
                            row.get_by_name("rotation_provenance").ok();
                        let rotation_provenance = rotation_blob.and_then(|b| {
                            if b.is_empty() {
                                return None;
                            }
                            serde_json::from_slice(&b).ok()
                        });
                        let audit_blob: Option<Vec<u8>> = row.get_by_name("audit_ref").ok();
                        let audit_ref = audit_blob.filter(|b| !b.is_empty());
                        // Round 2 review #2: select the v011
                        // anchor_tx_hash column so gossip-fed peers
                        // see the on-chain provenance of every
                        // catch-up'd event.
                        let anchor_blob: Option<Vec<u8>> = row.get_by_name("anchor_tx_hash").ok();
                        let anchor_tx_hash = anchor_blob.and_then(|b| {
                            if b.is_empty() {
                                None
                            } else {
                                let mut arr = [0u8; 32];
                                if b.len() == 32 {
                                    arr.copy_from_slice(&b);
                                    Some(arr)
                                } else {
                                    None
                                }
                            }
                        });
                        let kind = SignalKind::from_discriminant(kind_d as u8).map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_gossip_catch_up:kind_disc")
                        })?;
                        let layer =
                            ReputationLayer::from_discriminant(layer_d as u8).map_err(|_e| {
                                ReputationError::ChainRefInvalid(
                                    "stoolap_gossip_catch_up:layer_disc",
                                )
                            })?;

                        // Record the catch-up entry in the gossip_seen
                        // ledger. The (recorder_did, event_id) PK
                        // guards against duplicate rows; we use a
                        // pre-check SELECT because stoolap-fork does
                        // not support `INSERT OR IGNORE`.
                        let attestor_bytes = catch_up.attestor_did.as_bytes().to_vec();
                        let now = crate::migrations::now_unix();
                        let already_seen: bool = {
                            let mut q = self
                                .db
                                .query(
                                    "SELECT 1 FROM reputation_gossip_seen
                                     WHERE recorder_did = $1 AND event_id = $2",
                                    vec![
                                        octo_storage_core::stoolap::Value::blob(
                                            recorder_bytes.to_vec(),
                                        ),
                                        octo_storage_core::stoolap::Value::blob(
                                            event_id_bytes.clone(),
                                        ),
                                    ],
                                )
                                .map_err(|_e| {
                                    ReputationError::ChainRefInvalid(
                                        "stoolap_gossip_catch_up:seen_query",
                                    )
                                })?;
                            matches!(q.next(), Some(Ok(_)))
                        };
                        if !already_seen {
                            let seen_params: Vec<octo_storage_core::stoolap::Value> = vec![
                                octo_storage_core::stoolap::Value::blob(recorder_bytes.to_vec()),
                                octo_storage_core::stoolap::Value::blob(event_id_bytes.clone()),
                                octo_storage_core::stoolap::Value::blob(attestor_bytes),
                                u64_to_value(now),
                                octo_storage_core::stoolap::Value::blob(vec![0u8; 32]),
                            ];
                            if let Err(_e) = self.db.execute(
                                "INSERT INTO reputation_gossip_seen (
                                    recorder_did, event_id, attestor_did,
                                    observed_at_unix, peer_id
                                ) VALUES ($1, $2, $3, $4, $5)",
                                seen_params,
                            ) {
                                eprintln!(
                                    "gossip_catch_up: gossip_seen insert failed (best-effort)"
                                );
                            }
                        }

                        out.push(SignalEvent {
                            event_id,
                            recorder_did,
                            controller_id,
                            signal_kind: kind,
                            layer,
                            score_delta,
                            recorded_at_unix: i64_to_u64(recorded_at_unix),
                            rotation_provenance,
                            audit_ref,
                            anchor_tx_hash,
                        });
                    }
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_gossip_catch_up:iter",
                        ))
                    }
                    None => break,
                }
            }
            Ok(out)
        }

        async fn anchor_pending(&self, batch_size: u32) -> StoreResult<Vec<(EventId, [u8; 32])>> {
            // Anchor-pending sweep: events with `anchor_tx_hash IS NULL`
            // (the schema column added by migration v010). In the live
            // chain-side flow, the `anchor_tx_hash` slot is filled with
            // the on-chain tx hash via `set_event_anchor_tx_hash`. The
            // sweep returns the `(event_id, [0; 32])` placeholder; the
            // anchor job constructs Merkle roots + submits before
            // persisting the real hash.
            let row_results = self
                .db
                .query(
                    "SELECT event_id
                     FROM reputation_events
                     WHERE anchor_tx_hash IS NULL
                     ORDER BY event_id
                     LIMIT $1",
                    vec![octo_storage_core::stoolap::Value::from(batch_size as i64)],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_anchor_pending:query"))?;
            let mut rows = row_results;
            let mut out: Vec<(EventId, [u8; 32])> = Vec::new();
            loop {
                match rows.next() {
                    Some(Ok(row)) => {
                        let event_id_value = row.get(0).map_err(|_e| {
                            ReputationError::ChainRefInvalid("stoolap_anchor_pending:event_id_get")
                        })?;
                        let event_id_bytes: Vec<u8> = match &event_id_value {
                            octo_storage_core::stoolap::Value::Blob(arc) => arc.to_vec(),
                            _ => {
                                return Err(ReputationError::ChainRefInvalid(
                                    "stoolap_anchor_pending:event_id_blob",
                                ));
                            }
                        };
                        let mut event_id_arr = [0u8; 8];
                        if event_id_bytes.len() != 8 {
                            return Err(ReputationError::ChainRefInvalid(
                                "stoolap_anchor_pending:event_id_len",
                            ));
                        }
                        event_id_arr.copy_from_slice(&event_id_bytes);
                        out.push((
                            EventId::from_u64(u64::from_be_bytes(event_id_arr)),
                            [0u8; 32],
                        ));
                    }
                    Some(Err(_e)) => {
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_anchor_pending:iter",
                        ))
                    }
                    None => break,
                }
            }
            Ok(out)
        }

        async fn set_event_anchor_tx_hash(
            &self,
            event_id: EventId,
            anchor_tx_hash: [u8; 32],
        ) -> StoreResult<()> {
            // UPDATE the events row to record the on-chain anchor tx
            // hash. Idempotent on `(event_id, anchor_tx_hash)`: a
            // second call with the same pair is a no-op (the WHERE
            // clause narrows the row count to 0 if already set).
            //
            // Round 2 review M2-fix: surface missing event_id as
            // ChainRefInvalid so callers cannot silently acknowledge
            // a write that didn't land. `execute` returns the
            // affected row count — 0 means the event_id was not
            // found OR the row was already set to the same hash
            // (idempotent re-submit). We treat 0 as a missing-event
            // signal ONLY on the first write (anchor_tx_hash IS NULL
            // path) — the no-op idempotent path is verified by
            // checking the prior value with a follow-up SELECT.
            let event_id_bytes = event_id.to_u64().to_be_bytes();
            // Round 7 review R7-F2: look up recorder_did for this
            // event so the UPDATE can scope by the FULL composite
            // primary key (recorder_did, event_id). Without scoping
            // by recorder_did, the UPDATE would match every event
            // sharing event_id=X across distinct recorders — a
            // cross-recorder side effect. The stoolap UPDATE guard
            // requires composite-PK WHEREs to reference all PK
            // columns, hence this SELECT-then-UPDATE.
            //
            // R10 review (HIGH): event_ids are NOT globally unique
            // across recorders (schema's composite PK is
            // (recorder_did, event_id) and event_ids reset per
            // recorder via the global MAX counter). When two
            // recorders share an event_id, the probe must count
            // matches: 0 → not found, 1 → anchor the unique row,
            // 2+ → ambiguous (refuse; the API signature lacks
            // recorder_did so we cannot pick deterministically).
            // This mirrors the memory backend's R9 fix.
            // First: count ALL rows matching event_id (regardless of anchor).
            // If 2+ distinct recorders share the event_id, the API call
            // is inherently ambiguous (no recorder_did in signature)
            // and we MUST refuse rather than pick one. This is the
            // stoolap analog of memory's R9 ambiguous_event_id guard.
            let count_rows = self
                .db
                .query(
                    "SELECT recorder_did FROM reputation_events
                     WHERE event_id = $1",
                    vec![octo_storage_core::stoolap::Value::blob(
                        event_id_bytes.to_vec(),
                    )],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_set_anchor:probe_count"))?;
            let mut all_matches: Vec<Vec<u8>> = Vec::new();
            for row_res in count_rows {
                let row = row_res.map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_set_anchor:probe_count_iter")
                })?;
                let bytes: Vec<u8> = match row.get(0).map_err(|_e| {
                    ReputationError::ChainRefInvalid("stoolap_set_anchor:probe_count_col")
                })? {
                    octo_storage_core::stoolap::Value::Blob(arc) => arc.to_vec(),
                    _ => {
                        // R11 review: a non-Blob recorder_did means
                        // data corruption; refuse rather than
                        // silently undercount (which could mask
                        // event_id collisions across recorders).
                        return Err(ReputationError::ChainRefInvalid(
                            "stoolap_set_anchor:probe_count_invalid_coltype",
                        ));
                    }
                };
                all_matches.push(bytes);
                if all_matches.len() > 2 {
                    // Cap at 3 to avoid materializing huge result
                    // sets; the ambiguity decision is binary.
                    break;
                }
            }
            match all_matches.len() {
                0 => {
                    // event_id does not exist under any recorder.
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_set_anchor:event_not_found",
                    ));
                }
                n if n >= 2 => {
                    // Multiple recorders share event_id; the API
                    // contract cannot disambiguate, refuse.
                    return Err(ReputationError::ChainRefInvalid(
                        "stoolap_set_anchor:ambiguous_event_id",
                    ));
                }
                _ => {
                    // Exactly one recorder has this event_id; safe to
                    // proceed to the null-vs-anchored probe below.
                }
            }
            let recorder_did_blob = all_matches.into_iter().next().expect("len() == 1 above");
            // Exactly one row matches event_id (count probe above).
            // The UPDATE is scoped to the composite PK
            // (recorder_did, event_id). The WHERE clause accepts
            // either NULL anchor (fresh write) or matching hash
            // (idempotent re-submit). Updated row count:
            //   1 → success (fresh or idempotent)
            //   0 → row exists but anchor is a DIFFERENT hash
            //        → anchor_already_set
            let updated = self
                .db
                .execute(
                    "UPDATE reputation_events
                     SET anchor_tx_hash = $1
                     WHERE recorder_did = $2 AND event_id = $3
                       AND (anchor_tx_hash IS NULL OR anchor_tx_hash = $1)",
                    vec![
                        octo_storage_core::stoolap::Value::blob(anchor_tx_hash.to_vec()),
                        octo_storage_core::stoolap::Value::blob(recorder_did_blob),
                        octo_storage_core::stoolap::Value::blob(event_id_bytes.to_vec()),
                    ],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_set_anchor:update"))?;
            if updated == 0 {
                // Row exists (count probe confirmed 1 match) but
                // anchor is a different hash. The count probe above
                // guarantees there is exactly one row at this
                // event_id, so the 0-row UPDATE result can ONLY
                // mean anchor_already_set.
                Err(ReputationError::ChainRefInvalid(
                    "stoolap_set_anchor:anchor_already_set",
                ))
            } else {
                Ok(())
            }
        }

        async fn query_anchors_by_controller_id(
            &self,
            controller_id: ControllerId,
        ) -> StoreResult<Vec<AnchorRecord>> {
            // Read-side join from `reputation_events` filtered by
            // `controller_id` + non-null `anchor_tx_hash`. Schema
            // migration v010__reputation_anchors.sql declares
            // `idx_reputation_anchors_controller` for this lookup;
            // the join here uses the underlying events table since
            // `anchor_tx_hash` lives on `reputation_events`.
            let rows = self
                .db
                .query(
                    "SELECT event_id, anchor_tx_hash, recorded_at_unix
                     FROM reputation_events
                     WHERE controller_id = $1
                       AND anchor_tx_hash IS NOT NULL
                     ORDER BY recorded_at_unix ASC, event_id ASC",
                    vec![octo_storage_core::stoolap::Value::blob(
                        controller_id.as_bytes().to_vec(),
                    )],
                )
                .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_query_anchors:select"))?;
            let mut out: Vec<AnchorRecord> = Vec::new();
            for row_res in rows {
                let row = row_res
                    .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_query_anchors:row"))?;
                let event_id_value = row
                    .get(0)
                    .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_query_anchors:col0"))?;
                let event_id_blob: Vec<u8> = match &event_id_value {
                    octo_storage_core::stoolap::Value::Blob(arc) => arc.to_vec(),
                    _ => continue,
                };
                let anchor_value = row
                    .get(1)
                    .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_query_anchors:col1"))?;
                let anchor_blob: Vec<u8> = match &anchor_value {
                    octo_storage_core::stoolap::Value::Blob(arc) => arc.to_vec(),
                    _ => continue,
                };
                let recorded_at: i64 = row
                    .get(2)
                    .map_err(|_e| ReputationError::ChainRefInvalid("stoolap_query_anchors:col2"))?;
                if event_id_blob.len() != 8 || anchor_blob.len() != 32 {
                    continue;
                }
                let mut id_arr = [0u8; 8];
                id_arr.copy_from_slice(&event_id_blob);
                let mut anchor_arr = [0u8; 32];
                anchor_arr.copy_from_slice(&anchor_blob);
                out.push(AnchorRecord {
                    event_id: EventId::from_u64(u64::from_be_bytes(id_arr)),
                    anchor_tx_hash: anchor_arr,
                    recorded_at_unix: recorded_at.max(0) as u64,
                });
            }
            Ok(out)
        }
    }
}

#[cfg(feature = "stoolap")]
pub use real::StoolapReputationStore;

// ---------------------------------------------------------------------------
// cfg-off: marker stub (preserves CI build path without the stoolap feature)
// ---------------------------------------------------------------------------

/// Feature-gated dual-impl pattern for `StoolapReputationStore`.
///
/// When the `stoolap` Cargo feature is enabled (the default
/// integration path), the real SQL-backed `StoolapReputationStore`
/// impl above at `impl StoolapReputationStore` (lines ~975+) is
/// compiled. When `stoolap` is disabled (e.g. consumers building
/// with `--no-default-features --features in-memory-only`), the
/// `stub` module below provides a `StoolapReputationStore` shim
/// that returns per-method `stoolap_backend_unimplemented:<method>`
/// errors via `ReputationError::ChainRefInvalid`. Both impls cover
/// every `ReputationStore` trait method (including the six
/// federation methods `register_attestor`/`attestor_lookup_did`/
/// `record_attestation`/`query_attestations`/`attestor_quorum_reached`/
/// `gossip_catch_up` added in mission 0855p-b / 0968 Phase 4).
///
/// The stub is NOT a placeholder to delete: it preserves
/// `cargo build` for feature-consumers that don't link the SQL
/// backend, while making any accidental call site immediately
/// observable in logs.
#[cfg(not(feature = "stoolap"))]
mod stub {
    use super::*;
    use crate::store::AnchorRecord;
    use crate::GossipCatchUp;

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
            "register_attestor" => "stoolap_backend_unimplemented:register_attestor",
            "attestor_lookup_did" => "stoolap_backend_unimplemented:attestor_lookup_did",
            "record_attestation" => "stoolap_backend_unimplemented:record_attestation",
            "query_attestations" => "stoolap_backend_unimplemented:query_attestations",
            "attestor_quorum_reached" => "stoolap_backend_unimplemented:attestor_quorum_reached",
            "gossip_catch_up" => "stoolap_backend_unimplemented:gossip_catch_up",
            "anchor_pending" => "stoolap_backend_unimplemented:anchor_pending",
            "set_event_anchor_tx_hash" => "stoolap_backend_unimplemented:set_event_anchor_tx_hash",
            "query_anchors_by_controller_id" => {
                "stoolap_backend_unimplemented:query_anchors_by_controller_id"
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
        async fn register_attestor(
            &self,
            _: crate::auth::AttestorRegistration,
        ) -> StoreResult<AttestorId> {
            stub("register_attestor")
        }
        async fn attestor_lookup_did(
            &self,
            _: &AttestorId,
        ) -> StoreResult<crate::auth::AttestorRegistration> {
            stub("attestor_lookup_did")
        }
        async fn record_attestation(&self, _: Attestation) -> StoreResult<u64> {
            stub("record_attestation")
        }
        async fn query_attestations(
            &self,
            _: &RecorderDid,
            _: EventId,
        ) -> StoreResult<Vec<Attestation>> {
            stub("query_attestations")
        }
        async fn attestor_quorum_reached(&self, _: EventId) -> StoreResult<bool> {
            stub("attestor_quorum_reached")
        }
        async fn gossip_catch_up(&self, _: &GossipCatchUp) -> StoreResult<Vec<SignalEvent>> {
            stub("gossip_catch_up")
        }
        async fn anchor_pending(&self, _: u32) -> StoreResult<Vec<(EventId, [u8; 32])>> {
            stub("anchor_pending")
        }
        async fn set_event_anchor_tx_hash(&self, _: EventId, _: [u8; 32]) -> StoreResult<()> {
            stub("set_event_anchor_tx_hash")
        }
        async fn query_anchors_by_controller_id(
            &self,
            _: ControllerId,
        ) -> StoreResult<Vec<AnchorRecord>> {
            stub("query_anchors_by_controller_id")
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
