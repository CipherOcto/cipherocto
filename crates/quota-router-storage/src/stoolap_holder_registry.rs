//! `StoolapHolderRegistry` reference impl (RFC-0957-A1 §Schema).
//!
//! Backed by a `stoolap::Database` with two tables:
//! - `holder_registry` (PK = `cap_root_hash` BLOB)
//! - `outbox` (atomic at-least-once delivery retry queue)
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

use crate::clock::Clock;
use crate::holder_kind::HolderKind;
use crate::holder_record::HolderRecord;
use crate::holder_registry::{HolderRegistry, RegistryError};
use crate::migrations::apply_pending;

/// Stoolap-backed registry implementation.
pub struct StoolapHolderRegistry {
    db: stoolap::Database,
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
        let db = stoolap::Database::open_in_memory()
            .map_err(|e| RegistryError::Storage(format!("open_in_memory: {e}")))?;
        apply_pending(&db).map_err(|e| RegistryError::Storage(format!("apply_pending: {e}")))?;
        Ok(Self { db })
    }

    /// Wrap an existing `stoolap::Database` (does NOT call `apply_pending`).
    pub fn from_database(db: stoolap::Database) -> Self {
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
        let kind = record.kind.as_byte();
        let ask_id: Option<Vec<u8>> = record.ask_id.map(|a| a.to_vec());
        let result = self.db.execute(
            "INSERT INTO holder_registry \
             (cap_root_hash, kind, holder_did, holder_pub, audience_did, \
              caveats_canonical, ask_id, mint_at_millis_unix, ttl_millis_unix, \
              revoked_at_millis_unix) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                record.cap_root_hash.to_vec(),
                kind as i64,
                record.holder_did.clone(),
                record.holder_pub.to_vec(),
                record.audience_did.clone(),
                record.caveats_canonical.clone(),
                ask_id,
                record.mint_at_millis_unix as i64,
                record.ttl_millis_unix as i64,
                record.revoked_at_millis_unix.map(|v| v as i64),
            ),
        );
        match result {
            Ok(_affected) => Ok(()),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("UNIQUE")
                    || msg.contains("unique")
                    || msg.contains("PRIMARY")
                    || msg.contains("PrimaryKey")
                {
                    Err(RegistryError::AlreadyExists)
                } else {
                    Err(RegistryError::Storage(msg))
                }
            }
        }
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
    use crate::bearer_capsule_stub::BearerCapsule;
    use crate::clock::FixedClock;

    fn bearer() -> BearerCapsule {
        BearerCapsule {
            bearer_capsule_hash: [0x42; 32],
            encrypted_capsule: vec![0x01],
            seller_signature: [0x55; 64],
        }
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
            &crate::holder_record::CapabilityTokenLike {
                cap_root_hash: [0x11; 32],
                class: crate::holder_record::CapabilityClass::V1,
            },
            &[0x22; 32],
            "did:octo:v1",
            None,
            1_700_000_000_000,
        );
        r.insert(v1.clone()).unwrap();
        let zk = HolderRecord::from_capability(
            &crate::holder_record::CapabilityTokenLike {
                cap_root_hash: [0x44; 32],
                class: crate::holder_record::CapabilityClass::ZKBearing,
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
            holder_did: octo_ident::test_helpers::sample_did(104).into(),
            holder_pub: [0x77; 32],
            audience_did: octo_ident::test_helpers::sample_did(23).into(),
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
            &BearerCapsule {
                bearer_capsule_hash: [0x99; 32],
                encrypted_capsule: vec![],
                seller_signature: [0x55; 64],
            },
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
            &BearerCapsule {
                bearer_capsule_hash: [0xAA; 32],
                encrypted_capsule: vec![],
                seller_signature: [0x55; 64],
            },
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
}
