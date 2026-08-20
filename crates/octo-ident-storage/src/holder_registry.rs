//! `StoolapHolderRegistry` reference impl (RFC-0957-A1 §Schema).
//!
//! Backed by a `octo_storage_core::Database` with two tables:
//! - `holder_registry` (PK = `cap_root_hash` BLOB)
//! - `outbox` (atomic at-least-once delivery retry queue)

// Pedantic lints were clean in quota-router-storage (no `pedantic`
// config there). Mission 0206-003 v3.0 moves this file to
// `octo-ident-storage` which enables `pedantic`. Relax the lints
// that produce noise but not bugs (carried forward from the
// pre-move code; documented in mission scope).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::needless_pass_by_value,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]
//!
//! Schema per RFC-0957-A1 §StoolapHolderRegistry Schema. Cipherocto-side
//! migration lives at `crates/quota-router-storage/migrations/v005__create_holder_registry.sql`
//! and `v006__create_outbox.sql` per [[stoolap-general-purpose-db]].
//!
//! Constraint semantics per RFC §Stoolap compatibility note: the parser
//! doesn't support `UNIQUE ... WHERE`; we rely on `NULL` semantics — NULL
//! `ask_id` rows are excluded from the UNIQUE constraint, so multiple
//! non-market Bearer/V1 records are allowed; market-bound records
//! (ask_id IS NOT NULL) are uniquely keyed by (ask_id, kind).

use octo_cap_macaroon::{Clock, HolderKind, HolderRecord, HolderRegistry, RegistryError};
use quota_router_storage::migrations::apply_pending;

/// Canonical INSERT statement for the `holder_registry` table.
/// Shared by `insert` (single-record) + `insert_dual` (atomic pair) so
/// the SQL stays in lockstep — a column added to one MUST be added to
/// the other. Mission 0969-b1 R3-N1 invariant.
const INSERT_HOLDER_SQL: &str = "INSERT INTO holder_registry \
     (cap_root_hash, kind, holder_did, holder_pub, audience_did, \
      caveats_canonical, ask_id, mint_at_millis_unix, ttl_millis_unix, \
      revoked_at_millis_unix) \
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// Parameter tuple for `INSERT_HOLDER_SQL` (10 columns; see schema
/// `v005__create_holder_registry.sql`). Named alias to satisfy clippy
/// `type_complexity`.
type InsertParams = (
    Vec<u8>,
    i64,
    String,
    Vec<u8>,
    String,
    Vec<u8>,
    Option<Vec<u8>>,
    i64,
    i64,
    Option<i64>,
);

/// Build the parameter tuple for `INSERT_HOLDER_SQL` from a `HolderRecord`.
fn insert_params(record: &HolderRecord) -> InsertParams {
    (
        record.cap_root_hash.to_vec(),
        record.kind.as_byte() as i64,
        record.holder_did.clone(),
        record.holder_pub.to_vec(),
        record.audience_did.clone(),
        record.caveats_canonical.clone(),
        record.ask_id.map(|a| a.to_vec()),
        record.mint_at_millis_unix as i64,
        record.ttl_millis_unix as i64,
        record.revoked_at_millis_unix.map(|v| v as i64),
    )
}

/// Map a Stoolap `execute` error to the canonical `RegistryError`
/// taxonomy. `AlreadyExists` on UNIQUE/PK collisions; `Storage` for
/// everything else.
fn classify_insert_err(e: stoolap::Error) -> RegistryError {
    let msg = format!("{e}");
    if msg.contains("UNIQUE")
        || msg.contains("unique")
        || msg.contains("PRIMARY")
        || msg.contains("PrimaryKey")
    {
        RegistryError::AlreadyExists
    } else {
        RegistryError::Storage(msg)
    }
}

/// Execute `INSERT_HOLDER_SQL` against a `octo_storage_core::Database`.
fn execute_insert_db(
    db: &octo_storage_core::Database,
    record: &HolderRecord,
) -> Result<(), RegistryError> {
    db.execute(INSERT_HOLDER_SQL, insert_params(record))
        .map(|_| ())
        .map_err(classify_insert_err)
}

/// Execute `INSERT_HOLDER_SQL` against a `stoolap::ApiTransaction`.
/// Used by `insert_dual` so both records run in the same Stoolap tx.
fn execute_insert_tx(
    tx: &mut stoolap::ApiTransaction,
    record: &HolderRecord,
) -> Result<(), RegistryError> {
    tx.execute(INSERT_HOLDER_SQL, insert_params(record))
        .map(|_| ())
        .map_err(classify_insert_err)
}

/// Stoolap-backed registry implementation.
pub struct StoolapHolderRegistry {
    db: octo_storage_core::Database,
}

impl std::fmt::Debug for StoolapHolderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapHolderRegistry")
            .finish_non_exhaustive()
    }
}

impl StoolapHolderRegistry {
    /// Open a fresh in-memory database with the holder_registry schema applied.
    pub fn open_in_memory() -> Result<Self, RegistryError> {
        let db = octo_storage_core::Database::open_in_memory()
            .map_err(|e| RegistryError::Storage(format!("open_in_memory: {e}")))?;
        apply_pending(&db).map_err(|e| RegistryError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self { db })
    }

    /// Wrap an existing `octo_storage_core::Database` (does NOT call `apply_pending`).
    pub fn from_database(db: octo_storage_core::Database) -> Self {
        Self { db }
    }

    fn row_to_record(row: stoolap::ResultRow) -> Result<HolderRecord, RegistryError> {
        let kind_byte: i64 = row
            .get(1)
            .map_err(|e| RegistryError::Storage(format!("kind: {e}")))?;
        let kind = HolderKind::from_byte(kind_byte as u8).unwrap_or(HolderKind::V1);
        let ask_id: Option<Vec<u8>> = row.get(6).ok();
        let ask_id_arr = ask_id
            .and_then(|v| if v.len() == 32 { Some(v) } else { None })
            .and_then(|v| v.try_into().ok());
        let revoked: Option<i64> = row.get(9).ok();
        let cap_root_hash_raw: Vec<u8> = row
            .get(0)
            .map_err(|e| RegistryError::Storage(format!("cap_root_hash: {e}")))?;
        let holder_pub_raw: Vec<u8> = row
            .get(3)
            .map_err(|e| RegistryError::Storage(format!("holder_pub: {e}")))?;
        let holder_did: String = row
            .get(2)
            .map_err(|e| RegistryError::Storage(format!("holder_did: {e}")))?;
        let audience_did: String = row
            .get(4)
            .map_err(|e| RegistryError::Storage(format!("audience_did: {e}")))?;
        let caveats_canonical: Vec<u8> = row
            .get(5)
            .map_err(|e| RegistryError::Storage(format!("caveats_canonical: {e}")))?;
        let mint_at_millis_unix: i64 = row
            .get(7)
            .map_err(|e| RegistryError::Storage(format!("mint_at_millis_unix: {e}")))?;
        let ttl_millis_unix: i64 = row
            .get(8)
            .map_err(|e| RegistryError::Storage(format!("ttl_millis_unix: {e}")))?;
        let mut cap_root_hash = [0u8; 32];
        if cap_root_hash_raw.len() == 32 {
            cap_root_hash.copy_from_slice(&cap_root_hash_raw);
        }
        let mut holder_pub = [0u8; 32];
        if holder_pub_raw.len() == 32 {
            holder_pub.copy_from_slice(&holder_pub_raw);
        }
        Ok(HolderRecord {
            cap_root_hash,
            kind,
            holder_did,
            holder_pub,
            audience_did,
            caveats_canonical,
            ask_id: ask_id_arr,
            mint_at_millis_unix: mint_at_millis_unix as u64,
            ttl_millis_unix: ttl_millis_unix as u64,
            revoked_at_millis_unix: revoked.map(|v| v as u64),
        })
    }
}

impl HolderRegistry for StoolapHolderRegistry {
    fn lookup(&self, cap_root_hash: &[u8; 32]) -> Result<Option<HolderRecord>, RegistryError> {
        let rows = self
            .db
            .query(
                "SELECT cap_root_hash, kind, holder_did, holder_pub, audience_did, \
             caveats_canonical, ask_id, mint_at_millis_unix, ttl_millis_unix, \
             revoked_at_millis_unix \
             FROM holder_registry WHERE cap_root_hash = ?",
                (cap_root_hash.to_vec(),),
            )
            .map_err(|e| RegistryError::Storage(format!("lookup: {e}")))?;
        let mut iter = rows.into_iter();
        match iter.next() {
            None => Ok(None),
            Some(row_result) => {
                let row = row_result.map_err(|e| RegistryError::Storage(format!("row: {e}")))?;
                Ok(Some(Self::row_to_record(row)?))
            }
        }
    }

    fn lookup_by_ask(
        &self,
        ask_id: &[u8; 32],
        kind: HolderKind,
    ) -> Result<Option<HolderRecord>, RegistryError> {
        let rows = self
            .db
            .query(
                "SELECT cap_root_hash, kind, holder_did, holder_pub, audience_did, \
             caveats_canonical, ask_id, mint_at_millis_unix, ttl_millis_unix, \
             revoked_at_millis_unix \
             FROM holder_registry WHERE ask_id = ? AND kind = ?",
                (ask_id.to_vec(), kind.as_byte() as i64),
            )
            .map_err(|e| RegistryError::Storage(format!("lookup_by_ask: {e}")))?;
        let mut iter = rows.into_iter();
        match iter.next() {
            None => Ok(None),
            Some(row_result) => {
                let row = row_result.map_err(|e| RegistryError::Storage(format!("row: {e}")))?;
                Ok(Some(Self::row_to_record(row)?))
            }
        }
    }

    fn lookup_active(
        &self,
        cap_root_hash: &[u8; 32],
        clock: &dyn Clock,
    ) -> Result<Option<HolderRecord>, RegistryError> {
        match self.lookup(cap_root_hash)? {
            None => Ok(None),
            Some(r) => {
                if r.is_active_at(clock.unix_millis()) {
                    Ok(Some(r))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn insert(&self, record: HolderRecord) -> Result<(), RegistryError> {
        execute_insert_db(&self.db, &record)
    }

    /// Atomic dual-record insert (RFC-0969 §Phase 2 atomicity invariant).
    /// Both records persist or neither does. Mission 0969-b1.
    ///
    /// Uses Stoolap `Database::begin()` to open a transaction, inserts
    /// the bearer + capability records via the same SQL the single-record
    /// `insert` uses, then commits. On any failure the `Transaction`
    /// wrapper auto-rolls back on `Drop` (per Stoolap `Drop` impl), so
    /// neither record is visible to subsequent queries.
    ///
    /// Error mapping mirrors `insert`:
    /// - `RegistryError::AlreadyExists` if either PK collides (UNIQUE
    ///   constraint on `cap_root_hash` or `(ask_id, kind)`).
    /// - `RegistryError::Storage(reason)` for any other failure
    ///   (transaction begin, SQL execution, commit).
    fn insert_dual(
        &self,
        bearer: HolderRecord,
        capability: HolderRecord,
    ) -> Result<(), RegistryError> {
        // Open Stoolap transaction (auto-rollback on Drop if not committed).
        let mut tx = self
            .db
            .begin()
            .map_err(|e| RegistryError::Storage(format!("insert_dual begin: {e}")))?;

        // Insert bearer first. If this fails, the tx is dropped → auto-rollback,
        // capability is never attempted.
        execute_insert_tx(&mut tx, &bearer)?;

        // Insert capability. If this fails, the tx is dropped → auto-rollback,
        // bearer is rolled back too.
        execute_insert_tx(&mut tx, &capability)?;

        // Commit. If commit fails, the tx is still in scope and Drop
        // attempts rollback (defense in depth — Stoolap's commit failure
        // path leaves the tx usable per `Database::begin` contract).
        tx.commit()
            .map_err(|e| RegistryError::Storage(format!("insert_dual commit: {e}")))?;
        Ok(())
    }

    fn revoke(&self, cap_root_hash: &[u8; 32], clock: &dyn Clock) -> Result<(), RegistryError> {
        // Idempotent: only update if revoked_at_millis_unix IS NULL.
        let now = clock.unix_millis() as i64;
        let _updated = self
            .db
            .execute(
                "UPDATE holder_registry SET revoked_at_millis_unix = ? \
             WHERE cap_root_hash = ? AND revoked_at_millis_unix IS NULL",
                (now, cap_root_hash.to_vec()),
            )
            .map_err(|e| RegistryError::Storage(format!("revoke: {e}")))?;
        Ok(())
    }

    fn sync_peers(&self) -> Result<(), RegistryError> {
        // RFC-0862 sync. CipherOcto-side fan-out is owned by 0959-c / 0862 integration.
        // 0957-c ships the trait method; concrete impl is a stub that returns Ok.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::{BearerCapsule, FixedClock};

    fn bearer() -> BearerCapsule {
        BearerCapsule::new([0x42; 32], vec![0x01], [0x55; 64])
    }

    fn reg() -> StoolapHolderRegistry {
        StoolapHolderRegistry::open_in_memory().unwrap()
    }

    fn rec_with_pub(pub_key: [u8; 32]) -> HolderRecord {
        HolderRecord::from_bearer(
            &bearer(),
            &pub_key,
            &octo_ident::test_helpers::sample_did(234),
            [0x33; 32],
            1_700_000_000_000,
        )
    }

    #[test]
    fn tv1_lookup_hit() {
        let r = reg();
        let rec = rec_with_pub([0x77; 32]);
        r.insert(rec.clone()).unwrap();
        let got = r.lookup(&rec.cap_root_hash).unwrap();
        assert_eq!(
            got,
            Some(rec),
            "TV1: Lookup Hit must return inserted record"
        );
    }

    #[test]
    fn tv2_lookup_miss() {
        let r = reg();
        let got = r.lookup(&[0xAA; 32]).unwrap();
        assert_eq!(got, None, "TV2: Lookup Miss must return None");
    }

    #[test]
    fn tv3_insert_duplicate_pk() {
        let r = reg();
        let rec = rec_with_pub([0x77; 32]);
        r.insert(rec.clone()).unwrap();
        let err = r.insert(rec.clone()).unwrap_err();
        assert!(
            matches!(err, RegistryError::AlreadyExists),
            "TV3: second insert must fail with AlreadyExists, got {err:?}"
        );
    }

    #[test]
    fn tv4_revoke_then_lookup_active_returns_none() {
        let r = reg();
        let rec = rec_with_pub([0x77; 32]);
        r.insert(rec.clone()).unwrap();
        let clock = FixedClock::new(1_700_000_000_500);
        r.revoke(&rec.cap_root_hash, &clock).unwrap();
        let after = r.lookup(&rec.cap_root_hash).unwrap();
        assert!(
            after.is_some(),
            "TV4: lookup after revoke must still return the record"
        );
        assert_eq!(
            after.unwrap().revoked_at_millis_unix,
            Some(1_700_000_000_500)
        );
        let active = r
            .lookup_active(&rec.cap_root_hash, &FixedClock::new(1_700_000_000_600))
            .unwrap();
        assert!(
            active.is_none(),
            "TV4: lookup_active on revoked must return None"
        );
    }

    #[test]
    fn tv6_four_kind_agnosticism() {
        let r = reg();
        // Insert one per HolderKind.
        let b = HolderRecord::from_bearer(
            &bearer(),
            &[0x77; 32],
            "did:octo:b",
            [0x33; 32],
            1_700_000_000_000,
        );
        r.insert(b.clone()).unwrap();
        let v1 = HolderRecord::from_capability(
            &octo_cap_macaroon::CapabilityTokenLike {
                cap_root_hash: [0x11; 32],
                class: octo_cap_macaroon::CapabilityClass::V1,
            },
            &[0x22; 32],
            "did:octo:v1",
            None,
            1_700_000_000_000,
        );
        r.insert(v1.clone()).unwrap();
        let zk = HolderRecord::from_capability(
            &octo_cap_macaroon::CapabilityTokenLike {
                cap_root_hash: [0x44; 32],
                class: octo_cap_macaroon::CapabilityClass::ZKBearing,
            },
            &[0x55; 32],
            &octo_ident::test_helpers::sample_did(196),
            None,
            1_700_000_000_000,
        );
        r.insert(zk.clone()).unwrap();
        // HopCapability is owned by 0970-a; we test it via direct insert.
        let hop = HolderRecord {
            cap_root_hash: [0x66; 32],
            kind: HolderKind::HopCapability,
            holder_did: octo_ident::test_helpers::sample_did(104),
            holder_pub: [0x77; 32],
            audience_did: octo_ident::test_helpers::sample_did(23),
            caveats_canonical: vec![],
            ask_id: None,
            mint_at_millis_unix: 1_700_000_000_000,
            ttl_millis_unix: 1_700_000_000_000,
            revoked_at_millis_unix: None,
        };
        r.insert(hop.clone()).unwrap();

        // TV6: each round-trips; lookup returns the same kind byte.
        for expected in [&b, &v1, &zk, &hop] {
            let got = r.lookup(&expected.cap_root_hash).unwrap().unwrap();
            assert_eq!(got.kind, expected.kind, "TV6: kind byte must round-trip");
        }
    }

    #[test]
    fn tv12_lookup_by_ask_unique() {
        let r = reg();
        let ask_id = [0x33; 32];
        let b = HolderRecord::from_bearer(
            &bearer(),
            &[0x77; 32],
            "did:octo:b",
            ask_id,
            1_700_000_000_000,
        );
        r.insert(b.clone()).unwrap();
        // Same (ask_id, kind) → must fail; UNIQUE on (ask_id, kind).
        let b2 = HolderRecord::from_bearer(
            &BearerCapsule::new([0x99; 32], vec![], [0x55; 64]),
            &[0x88; 32],
            "did:octo:b2",
            ask_id,
            1_700_000_000_000,
        );
        let err = r.insert(b2).unwrap_err();
        assert!(
            matches!(err, RegistryError::AlreadyExists),
            "TV12: duplicate (ask_id, kind) must fail AlreadyExists, got {err:?}"
        );
        // Different ask_id, same kind → allowed.
        let b3 = HolderRecord::from_bearer(
            &BearerCapsule::new([0xAA; 32], vec![], [0x55; 64]),
            &[0x77; 32],
            "did:octo:b",
            [0x44; 32],
            1_700_000_000_000,
        );
        r.insert(b3).unwrap();
        // lookup_by_ask returns the first inserted.
        let got = r.lookup_by_ask(&ask_id, HolderKind::Bearer).unwrap();
        assert_eq!(got, Some(b), "TV12: lookup_by_ask returns unique record");
    }

    #[test]
    fn tv13_debug_redaction_holds_across_schema() {
        let r = reg();
        let rec = rec_with_pub([0x77; 32]);
        r.insert(rec.clone()).unwrap();
        // Insert + lookup: ensure the schema round-trip preserves the Debug
        // redaction (manual impl is on the Rust type, not the schema).
        let got = r.lookup(&rec.cap_root_hash).unwrap().unwrap();
        let s = format!("{:?}", got);
        assert!(
            s.contains("redacted"),
            "TV13: expected redaction marker: {s}"
        );
        assert!(!s.contains("7777"), "TV13: leaked holder_pub bytes: {s}");
        assert!(!s.contains("4242"), "TV13: leaked cap_root_hash bytes: {s}");
    }

    #[test]
    fn tv14_revoked_distinct_from_ttl() {
        // Record with ttl_millis_unix=0 + revoked_at_millis_unix=None:
        //   lookup_active at any now MUST return Some (perpetual).
        let r = reg();
        let mut rec =
            HolderRecord::from_bearer(&bearer(), &[0x77; 32], "did:octo:b", [0x33; 32], 0);
        rec.revoked_at_millis_unix = None;
        r.insert(rec.clone()).unwrap();
        let clock = FixedClock::new(u64::MAX);
        let active = r.lookup_active(&rec.cap_root_hash, &clock).unwrap();
        assert!(
            active.is_some(),
            "TV14: ttl=0 + not revoked = perpetual active"
        );

        // Revoke at a known timestamp.
        let revoke_at = 1_700_000_000_000_u64;
        r.revoke(&rec.cap_root_hash, &FixedClock::new(revoke_at))
            .unwrap();
        let clock = FixedClock::new(revoke_at + 1);
        let active = r.lookup_active(&rec.cap_root_hash, &clock).unwrap();
        assert!(active.is_none(), "TV14: revoked record must not be active");
    }

    // ---- Mission 0969-b1: TV9-I1/I2/I3 atomic insert_dual tests ----

    fn capability_v1(cap_root_hash: [u8; 32]) -> HolderRecord {
        HolderRecord::from_capability(
            &octo_cap_macaroon::CapabilityTokenLike {
                cap_root_hash,
                class: octo_cap_macaroon::CapabilityClass::V1,
            },
            &[0x88; 32],
            &octo_ident::test_helpers::sample_did(207),
            Some([0x33; 32]),
            1_700_000_000_000,
        )
    }

    fn bearer_with_pub_and_hash(
        capsule_hash: [u8; 32],
        holder_pub: [u8; 32],
        ask_id: [u8; 32],
    ) -> HolderRecord {
        HolderRecord::from_bearer(
            &BearerCapsule::new(capsule_hash, vec![0x01, 0x02, 0x03], [0x55; 64]),
            &holder_pub,
            &octo_ident::test_helpers::sample_did(118),
            ask_id,
            1_700_000_000_000,
        )
    }

    /// TV9-I1: `insert_dual` happy path — both records persist atomically.
    /// Mission 0969-b1 AC: `lookup_by_ask(ask_id, HolderKind::Bearer)` returns
    /// the bearer record AND `lookup_by_ask(ask_id, HolderKind::V1)` returns
    /// the capability record after a single `insert_dual` call.
    #[test]
    fn tv9_i1_insert_dual_happy_path() {
        let r = reg();
        let ask_id = [0x33; 32];
        let bearer = bearer_with_pub_and_hash([0x42; 32], [0x77; 32], ask_id);
        let capability = capability_v1([0x11; 32]);

        r.insert_dual(bearer.clone(), capability.clone()).unwrap();

        let bearer_got = r
            .lookup_by_ask(&ask_id, HolderKind::Bearer)
            .unwrap()
            .expect("TV9-I1: bearer must persist after dual insert");
        let cap_got = r
            .lookup_by_ask(&ask_id, HolderKind::V1)
            .unwrap()
            .expect("TV9-I1: capability must persist after dual insert");

        assert_eq!(bearer_got.cap_root_hash, bearer.cap_root_hash);
        assert_eq!(bearer_got.kind, HolderKind::Bearer);
        assert_eq!(cap_got.cap_root_hash, capability.cap_root_hash);
        assert_eq!(cap_got.kind, HolderKind::V1);
    }

    /// TV9-I2: atomicity failure path — capability insert forced to fail
    /// (PK collision with a pre-existing record). Assert: the bearer record
    /// MUST NOT persist. Mission 0969-b1 AC.
    #[test]
    fn tv9_i2_insert_dual_rollback_on_capability_failure() {
        let r = reg();
        let ask_id = [0x33; 32];

        // Pre-existing capability record with the SAME cap_root_hash as the
        // one we're about to dual-insert. This forces a PK collision on the
        // capability insert (UNIQUE on cap_root_hash).
        let pre_cap = capability_v1([0x11; 32]);
        r.insert(pre_cap.clone()).unwrap();

        let bearer = bearer_with_pub_and_hash([0x42; 32], [0x77; 32], ask_id);
        let capability = capability_v1([0x11; 32]); // SAME cap_root_hash → collision

        let err = r
            .insert_dual(bearer.clone(), capability.clone())
            .unwrap_err();
        assert!(
            matches!(err, RegistryError::AlreadyExists),
            "TV9-I2: capability PK collision must surface AlreadyExists, got {err:?}"
        );

        // Bearer MUST NOT persist (atomicity guarantee).
        let bearer_got = r.lookup(&bearer.cap_root_hash).unwrap();
        assert!(
            bearer_got.is_none(),
            "TV9-I2: bearer MUST be rolled back on capability failure, got {bearer_got:?}"
        );
        let bearer_by_ask = r.lookup_by_ask(&ask_id, HolderKind::Bearer).unwrap();
        assert!(
            bearer_by_ask.is_none(),
            "TV9-I2: bearer (by ask_id) MUST NOT persist, got {bearer_by_ask:?}"
        );

        // Pre-existing capability still present (not touched by failed dual).
        let pre_cap_got = r.lookup(&pre_cap.cap_root_hash).unwrap();
        assert_eq!(
            pre_cap_got,
            Some(pre_cap),
            "TV9-I2: pre-existing capability must remain"
        );
    }

    /// TV9-I3: bearer PK collision — `insert_dual` returns AlreadyExists,
    /// capability is never attempted. Mission 0969-b1 AC.
    #[test]
    fn tv9_i3_insert_dual_aborts_on_bearer_pk_collision() {
        let r = reg();
        let ask_id = [0x33; 32];

        // Pre-existing bearer with the SAME cap_root_hash AND same ask_id
        // as the dual-insert input — forces collision on BOTH the PK
        // (`cap_root_hash`) and the `(ask_id, kind)` UNIQUE index.
        let pre_bearer = bearer_with_pub_and_hash([0x42; 32], [0x77; 32], ask_id);
        r.insert(pre_bearer.clone()).unwrap();

        let bearer = bearer_with_pub_and_hash([0x42; 32], [0xAA; 32], ask_id);
        let capability = capability_v1([0x11; 32]);

        let err = r
            .insert_dual(bearer.clone(), capability.clone())
            .unwrap_err();
        assert!(
            matches!(err, RegistryError::AlreadyExists),
            "TV9-I3: bearer PK collision must surface AlreadyExists, got {err:?}"
        );

        // Capability was never attempted → not present.
        let cap_got = r.lookup(&capability.cap_root_hash).unwrap();
        assert!(
            cap_got.is_none(),
            "TV9-I3: capability MUST NOT be attempted on bearer PK collision, got {cap_got:?}"
        );

        // Pre-existing bearer untouched.
        let pre_got = r.lookup(&pre_bearer.cap_root_hash).unwrap();
        assert_eq!(
            pre_got,
            Some(pre_bearer),
            "TV9-I3: pre-existing bearer must remain"
        );
    }
}
