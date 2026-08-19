//! Cipherocto-side DAO for the `settlement_events` table (RFC-0959 §Event Sourcing).
//!
//! Persists `SettlementEvent` records (RFC-0959 §Data Structures) into the
//! canonical event log. Each row is a router-signed attestation of a settlement
//! event:
//! 1. Router computes `settlement_hash = BLAKE3(...)` from the canonical preimage.
//! 2. Router signs the canonical event bytes with its Ed25519 identity key.
//! 3. Row is inserted into `settlement_events`; the `settlement_hash` is UNIQUE
//!    so re-inserting the same event is idempotent (returns Ok(false)).
//! 4. The corresponding nonce is ALSO inserted into `consumed_receipt_index` (v003)
//!    via the `ConsumedReceiptRepository` — replay defense + audit linkage.
//!
//! Per [[stoolap-general-purpose-db]]: cipherocto owns this consumer schema;
//! the stoolap fork stays a general-purpose DB.

use crate::ask::{compute_settlement_hash, AxesConsumed, SettlementEvent};
use crate::migrations;
use crate::RepoError;

/// DAO for the `settlement_events` table.
///
/// Owns its embedded stoolap connection. All methods run queries via the
/// embedded DB; no caching layer at MVP.
#[derive(Clone)]
pub struct SettlementEventRepository {
    db: stoolap::Database,
}

/// Cargo for the settlement-event INSERT (RFC-0959 §Data Structures).
///
/// `asker_did` is denormalized into the row so per-asker queries (mesh router
/// dashboard, audit) don't require a JOIN against `asks`. The `cost` field
/// is 16-byte big-endian u128 since `u128` exceeds i64.
///
/// v2.0 fields (RFC-0959 v2.0 §Wire Format + mission 0959-c1): the
/// `cost_vault_id` + `chain_id` columns were added by migration v016
/// for cross-chain settlement reject (per review §20.7). Both are
/// `Option<[u8; 32]>` — pre-v2.0 settlements insert `None` (legacy
/// rows are gated by `SettlementError::CostVaultIdMissing` per the
/// v2.0 verify-time invariant).
pub struct SettlementEventInsert<'e> {
    /// Canonical event (RFC-0959 §Data Structures).
    pub event: &'e SettlementEvent,
    /// Ed25519 signature over `canonical_ser(event || nonce || settled_at_unix)`
    /// (RFC-0959 §Algorithms). 64 bytes.
    pub router_signature: [u8; 64],
    /// Asker DID (RFC-0009). Denormalized into the row for per-asker queries.
    pub asker_did: &'e str,
    /// Replay-defense nonce (16 bytes). Matches the corresponding
    /// `consumed_receipt_index.nonce` row.
    pub nonce: [u8; 16],
    /// v2.0 wire form — vault row the settlement draws against
    /// (RFC-0959 v2.0 §Wire Format + review §20.7). `None` for
    /// pre-v2.0 settlements (legacy rows; rejected by
    /// `verify_settlement_chain_match`).
    pub cost_vault_id: Option<[u8; 32]>,
    /// v2.0 wire form — chain scope of this settlement
    /// (RFC-0959 v2.0 §Wire Format + RFC-0010 v1.6 §ChainId 32-byte
    /// addendum). `None` for pre-v2.0 settlements.
    pub chain_id: Option<[u8; 32]>,
}

impl SettlementEventRepository {
    /// Open an in-memory DB, apply migrations, return the repository.
    /// Test-only convenience.
    /// # Errors
    /// Returns `RepoError::Db` on DB open failure, `RepoError::Migration` if migrations fail.
    pub fn open_in_memory() -> Result<Self, RepoError> {
        let db = stoolap::Database::open_in_memory()
            .map_err(|e| RepoError::Db(format!("open_in_memory: {e}")))?;
        migrations::apply_pending(&db)?;
        Ok(Self { db })
    }

    /// Open a file-backed DB at `path`, apply migrations, return the repository.
    /// # Errors
    /// Returns `RepoError::Db` on DB open failure, `RepoError::Migration` if migrations fail.
    pub fn open_path(path: &str) -> Result<Self, RepoError> {
        let db = stoolap::Database::open(path)
            .map_err(|e| RepoError::Db(format!("open({path}): {e}")))?;
        migrations::apply_pending(&db)?;
        Ok(Self { db })
    }

    /// Open either an in-memory DB (when `path` is `None`) or a file-backed DB
    /// at `path`. CLI convenience wrapper.
    /// # Errors
    /// Returns `RepoError::Db` on DB open failure, `RepoError::Migration` if migrations fail.
    pub fn open(path: Option<&str>) -> Result<Self, RepoError> {
        match path {
            Some(p) => Self::open_path(p),
            None => Self::open_in_memory(),
        }
    }

    /// Wrap an existing stoolap connection (caller-owned). Caller is responsible
    /// for invoking `migrations::apply_pending(db)` at startup.
    #[must_use]
    pub fn from_db(db: stoolap::Database) -> Self {
        Self { db }
    }

    /// Insert a settlement event. Idempotent on UNIQUE `settlement_hash`
    /// collision (returns `Ok(false)`); other DB errors propagate.
    /// # Errors
    /// Returns `RepoError::Db` on non-UNIQUE stoolap failure.
    pub fn insert(&self, cargo: &SettlementEventInsert<'_>) -> Result<bool, RepoError> {
        let next_id = self.next_row_id()?;
        let axes_canonical = serde_json::to_vec(&cargo.event.axes_consumed)
            .map_err(|e| RepoError::Serde(format!("canonical_ser axes_consumed: {e}")))?;
        let cost_be = crate::dqa_serde::dqa_to_bytes(&cargo.event.cost).to_vec();
        // INSERT v1.0 columns (11 params). The Stoolap fork's `Params`
        // trait supports tuples up to a fixed arity; the v2.0 columns
        // (`cost_vault_id`, `chain_id`) are added via a follow-up
        // UPDATE to stay within that arity. Migration v016 added the
        // columns with no DEFAULT, so pre-v2.0 rows carry NULL
        // (legacy / pre-v2.0 settlements).
        let result = self.db.execute(
            "INSERT INTO settlement_events \
             (row_id, settlement_hash, cap_root_hash, ask_id, asker_did, \
              invocation_hash, axes_consumed_json, cost_micro_octo_w, \
              settled_at_unix, router_signature, nonce) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                next_id,
                cargo.event.settlement_hash_or_compute().to_vec(),
                cargo.event.cap_root_hash.to_vec(),
                cargo.event.ask_id.to_vec(),
                cargo.asker_did.to_owned(),
                cargo.event.invocation_hash.to_vec(),
                axes_canonical,
                cost_be,
                cargo.event.settled_at_unix as i64,
                cargo.router_signature.to_vec(),
                cargo.nonce.to_vec(),
            ),
        );
        let inserted = match result {
            Ok(_) => true,
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("UNIQUE")
                    || msg.contains("Duplicate")
                    || msg.contains("unique constraint")
                {
                    return Ok(false);
                }
                return Err(RepoError::Db(format!("insert settlement_events: {e}")));
            }
        };
        // v2.0 follow-up UPDATE for `cost_vault_id` + `chain_id` columns
        // (RFC-0959 v2.0 §Wire Format + review §20.7). Both columns
        // are NULLABLE (added by v016 with no DEFAULT); pre-v2.0
        // settlements leave them NULL.
        //
        // When BOTH are `None` we skip the UPDATE entirely (the v1.0
        // path stays a single round-trip). When EITHER is `Some` we
        // issue one UPDATE that sets both columns together (so the
        // row is never half-populated).
        if cargo.cost_vault_id.is_some() || cargo.chain_id.is_some() {
            let cost_vault_id_blob: Vec<u8> =
                cargo.cost_vault_id.map(|b| b.to_vec()).unwrap_or_default();
            let chain_id_blob: Vec<u8> = cargo.chain_id.map(|b| b.to_vec()).unwrap_or_default();
            self.db
                .execute(
                    "UPDATE settlement_events \
                     SET cost_vault_id = ?, chain_id = ? \
                     WHERE settlement_hash = ?",
                    (
                        cost_vault_id_blob,
                        chain_id_blob,
                        cargo.event.settlement_hash_or_compute().to_vec(),
                    ),
                )
                .map_err(|e| RepoError::Db(format!("update v2.0 settlement_events: {e}")))?;
        }
        Ok(inserted)
    }

    /// Fetch a settlement event by its canonical `settlement_hash`.
    /// Returns `Ok(None)` if the event is not present.
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    /// Returns `RepoError::Serde` if `axes_consumed_json` cannot be deserialized.
    pub fn get_by_settlement_hash(
        &self,
        settlement_hash: &[u8; 32],
    ) -> Result<Option<PersistedSettlementEvent>, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT cap_root_hash, ask_id, asker_did, invocation_hash, \
                 axes_consumed_json, cost_micro_octo_w, settled_at_unix, \
                 router_signature, nonce, cost_vault_id, chain_id \
                 FROM settlement_events WHERE settlement_hash = ?",
                (settlement_hash.to_vec(),),
            )
            .map_err(|e| RepoError::Db(format!("select settlement_events: {e}")))?;
        let mut iter = rows.into_iter();
        let Some(row_result) = iter.next() else {
            return Ok(None);
        };
        let row = row_result.map_err(|e| RepoError::Db(format!("row: {e}")))?;
        let event = deserialize_event(settlement_hash, &row)?;
        Ok(Some(event))
    }

    /// List all settlement events for an asker (idx_se_asker_did).
    /// Ordered by `settled_at_unix` ASC for chronological replay.
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    /// Returns `RepoError::Serde` if any row fails to deserialize.
    pub fn list_by_asker(
        &self,
        asker_did: &str,
    ) -> Result<Vec<PersistedSettlementEvent>, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT settlement_hash, cap_root_hash, ask_id, asker_did, \
                 invocation_hash, axes_consumed_json, cost_micro_octo_w, \
                 settled_at_unix, router_signature, nonce, cost_vault_id, chain_id \
                 FROM settlement_events WHERE asker_did = ? \
                 ORDER BY settled_at_unix ASC",
                (asker_did.to_owned(),),
            )
            .map_err(|e| RepoError::Db(format!("select by asker: {e}")))?;
        let mut out = Vec::new();
        for row_result in rows {
            let row = row_result.map_err(|e| RepoError::Db(format!("row: {e}")))?;
            let sh: Vec<u8> = row.get(0).unwrap_or_default();
            let sh_arr: [u8; 32] = sh
                .as_slice()
                .try_into()
                .map_err(|_| RepoError::Serde("settlement_hash length != 32".to_owned()))?;
            // SELECT order: 0=settlement_hash, then match `get_by_settlement_hash`
            // column order starting at index 1. Build the event by reading
            // columns 1..=9 directly.
            out.push(deserialize_event_at(&sh_arr, &row, 1)?);
        }
        Ok(out)
    }

    /// Count settlement events for an asker (used by dashboard + rate limiting).
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn count_by_asker(&self, asker_did: &str) -> Result<u64, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT COUNT(*) FROM settlement_events WHERE asker_did = ?",
                (asker_did.to_owned(),),
            )
            .map_err(|e| RepoError::Db(format!("count by asker: {e}")))?;
        let mut iter = rows.into_iter();
        let row = iter
            .next()
            .unwrap()
            .map_err(|e| RepoError::Db(format!("row: {e}")))?;
        let n: i64 = row.get(0).unwrap();
        Ok(n as u64)
    }

    /// Total events in the table (diagnostics / GC).
    /// # Errors
    /// Returns `RepoError::Db` on query failure.
    pub fn len(&self) -> Result<u64, RepoError> {
        let rows = self
            .db
            .query("SELECT COUNT(*) FROM settlement_events", ())
            .map_err(|e| RepoError::Db(format!("count: {e}")))?;
        let mut iter = rows.into_iter();
        let row = iter
            .next()
            .unwrap()
            .map_err(|e| RepoError::Db(format!("row: {e}")))?;
        let n: i64 = row.get(0).unwrap();
        Ok(n as u64)
    }

    /// Always false (table is empty until the first insert; queries are
    /// cheap even on populated tables — `len()` is the canonical signal).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self.len() {
            Ok(n) => n == 0,
            Err(_) => false,
        }
    }

    fn next_row_id(&self) -> Result<i64, RepoError> {
        let rows = self
            .db
            .query(
                "SELECT COALESCE(MAX(row_id), 0) + 1 FROM settlement_events",
                (),
            )
            .map_err(|e| RepoError::Db(format!("select max row_id: {e}")))?;
        let mut iter = rows.into_iter();
        let row = iter
            .next()
            .unwrap()
            .map_err(|e| RepoError::Db(format!("row: {e}")))?;
        let n: i64 = row.get(0).unwrap();
        Ok(n)
    }
}

/// Decoded settlement event row (cargo + canonical event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedSettlementEvent {
    /// Canonical settlement_hash (BCD).
    pub settlement_hash: [u8; 32],
    /// BLAKE3 capability root.
    pub cap_root_hash: [u8; 32],
    /// Content-addressable AskId.
    pub ask_id: [u8; 32],
    /// Asker DID (denormalized).
    pub asker_did: String,
    /// BLAKE3 of the invocation.
    pub invocation_hash: [u8; 32],
    /// Per-axis consumption (canonical BTreeMap ordering preserved).
    pub axes_consumed: AxesConsumed,
    /// Cost in micro-OCTO-W (parsed from 16-byte BE `DqaEncoding` blob).
    pub cost_micro_octo_w: octo_determin::Dqa,
    /// Settlement timestamp (Unix seconds).
    pub settled_at_unix: u64,
    /// Router Ed25519 signature (64 bytes).
    pub router_signature: Vec<u8>,
    /// Replay-defense nonce (16 bytes).
    pub nonce: Vec<u8>,
    /// v2.0 wire form — vault row the settlement draws against
    /// (RFC-0959 v2.0 §Wire Format + review §20.7). `None` for
    /// pre-v2.0 settlements.
    pub cost_vault_id: Option<[u8; 32]>,
    /// v2.0 wire form — chain scope of this settlement
    /// (RFC-0959 v2.0 §Wire Format + RFC-0010 v1.6). `None` for
    /// pre-v2.0 settlements.
    pub chain_id: Option<[u8; 32]>,
}

/// Helper: compute the canonical `settlement_hash` for an event. The
/// `SettlementEvent` struct has `settled_at_unix` + canonical axes, so the
/// hash is deterministic regardless of whether the caller pre-filled it.
trait SettlementEventHash {
    fn settlement_hash_or_compute(&self) -> [u8; 32];
}

impl SettlementEventHash for SettlementEvent {
    fn settlement_hash_or_compute(&self) -> [u8; 32] {
        // The struct doesn't store a self-hash field; compute from canonical
        // preimage. `compute_settlement_hash` returns `Result` but the only
        // failure path (canonical_ser on AxesConsumed) cannot happen for
        // the typed fields we hold, so unwrap is safe.
        compute_settlement_hash(self).expect("compute_settlement_hash")
    }
}

fn deserialize_event(
    settlement_hash: &[u8; 32],
    row: &stoolap::ResultRow,
) -> Result<PersistedSettlementEvent, RepoError> {
    deserialize_event_at(settlement_hash, row, 0)
}

/// Deserialize a row whose columns match the `get_by_settlement_hash` SELECT
/// starting at column `offset`. `offset = 0` for `get_by_settlement_hash` (the
/// SELECT starts at `cap_root_hash`); `offset = 1` for `list_by_asker` (the
/// SELECT starts with `settlement_hash` followed by `cap_root_hash`).
fn deserialize_event_at(
    settlement_hash: &[u8; 32],
    row: &stoolap::ResultRow,
    offset: usize,
) -> Result<PersistedSettlementEvent, RepoError> {
    let cap_root_hash: Vec<u8> = row.get(offset).unwrap_or_default();
    let ask_id: Vec<u8> = row.get(offset + 1).unwrap_or_default();
    let asker_did: String = row.get(offset + 2).unwrap_or_default();
    let invocation_hash: Vec<u8> = row.get(offset + 3).unwrap_or_default();
    let axes_consumed_json: Vec<u8> = row.get(offset + 4).unwrap_or_default();
    let cost_be: Vec<u8> = row.get(offset + 5).unwrap_or_default();
    let settled_at_unix: i64 = row.get(offset + 6).unwrap_or(0_i64);
    let router_signature: Vec<u8> = row.get(offset + 7).unwrap_or_default();
    let nonce: Vec<u8> = row.get(offset + 8).unwrap_or_default();
    // v2.0 columns (migration v016): cost_vault_id + chain_id.
    // NULL → None (pre-v2.0 legacy rows).
    let cost_vault_id_bytes: Option<Vec<u8>> = row.get(offset + 9).ok();
    let chain_id_bytes: Option<Vec<u8>> = row.get(offset + 10).ok();

    let axes_consumed: AxesConsumed = serde_json::from_slice(&axes_consumed_json)
        .map_err(|e| RepoError::Serde(format!("axes_consumed_json: {e}")))?;
    let cost_arr: [u8; 16] = cost_be
        .as_slice()
        .try_into()
        .map_err(|_| RepoError::Serde("cost_micro_octo_w length != 16".to_owned()))?;
    let cost_micro_octo_w = crate::dqa_serde::dqa_from_bytes(&cost_arr)
        .map_err(|e| RepoError::Serde(format!("cost_micro_octo_w decode: {e:?}")))?;

    let cap_root_hash_arr: [u8; 32] = cap_root_hash
        .as_slice()
        .try_into()
        .map_err(|_| RepoError::Serde("cap_root_hash length != 32".to_owned()))?;
    let ask_id_arr: [u8; 32] = ask_id
        .as_slice()
        .try_into()
        .map_err(|_| RepoError::Serde("ask_id length != 32".to_owned()))?;
    let invocation_hash_arr: [u8; 32] = invocation_hash
        .as_slice()
        .try_into()
        .map_err(|_| RepoError::Serde("invocation_hash length != 32".to_owned()))?;

    // v2.0 column decode: 32-byte BLOB → [u8; 32]. Stoolap may
    // return NULL → None, or a 32-byte Vec → Some([u8; 32]). For
    // v016-migrated rows the column is present (NULL for legacy
    // pre-v2.0 rows); the migration is the gate.
    let cost_vault_id: Option<[u8; 32]> = match cost_vault_id_bytes {
        Some(v) if v.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&v);
            Some(arr)
        }
        Some(_) | None => None,
    };
    let chain_id: Option<[u8; 32]> = match chain_id_bytes {
        Some(v) if v.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&v);
            Some(arr)
        }
        Some(_) | None => None,
    };

    Ok(PersistedSettlementEvent {
        settlement_hash: *settlement_hash,
        cap_root_hash: cap_root_hash_arr,
        ask_id: ask_id_arr,
        asker_did,
        invocation_hash: invocation_hash_arr,
        axes_consumed,
        cost_micro_octo_w,
        settled_at_unix: settled_at_unix as u64,
        router_signature,
        nonce,
        cost_vault_id,
        chain_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample_event(ask_id: [u8; 32], _nonce: [u8; 16]) -> SettlementEvent {
        let mut axes = BTreeMap::new();
        axes.insert("input_tokens_per_1k".to_owned(), 1000_u32);
        SettlementEvent {
            cap_root_hash: [0x01; 32],
            ask_id,
            invocation_hash: [0xab; 32],
            axes_consumed: AxesConsumed {
                axes,
                cache_key_hash: None,
            },
            cost: octo_determin::Dqa::new(30_000, 0).expect("non-overflow"),
            settled_at_unix: 1_700_000_000,
        }
    }

    fn sample_cargo<'e>(
        event: &'e SettlementEvent,
        asker: &'e str,
        nonce: [u8; 16],
    ) -> SettlementEventInsert<'e> {
        SettlementEventInsert {
            event,
            router_signature: [0u8; 64],
            asker_did: asker,
            nonce,
            // Legacy pre-v2.0 test cargo: cost_vault_id + chain_id
            // are None (verifier rejects with CostVaultIdMissing per
            // RFC-0959 v2.0 §Cross-Chain Settlement Reject).
            cost_vault_id: None,
            chain_id: None,
        }
    }

    #[test]
    fn insert_then_get_by_settlement_hash_roundtrip() {
        let repo = SettlementEventRepository::open_in_memory().unwrap();
        let event = sample_event([0x42; 32], [0x55; 16]);
        let cargo = sample_cargo(&event, "did:octo:asker1", [0x55; 16]);
        let inserted = repo.insert(&cargo).unwrap();
        assert!(inserted, "first insert must report inserted=true");
        let sh = compute_settlement_hash(&event).unwrap();
        let persisted = repo.get_by_settlement_hash(&sh).unwrap().expect("row");
        assert_eq!(persisted.settlement_hash, sh);
        assert_eq!(persisted.ask_id, event.ask_id);
        assert_eq!(persisted.asker_did, "did:octo:asker1");
        assert_eq!(
            persisted.cost_micro_octo_w,
            octo_determin::Dqa::new(30_000, 0).expect("non-overflow")
        );
        assert_eq!(persisted.axes_consumed.axes.len(), 1);
    }

    #[test]
    fn duplicate_insert_is_idempotent() {
        let repo = SettlementEventRepository::open_in_memory().unwrap();
        let event = sample_event([0x42; 32], [0x55; 16]);
        let cargo = sample_cargo(&event, "did:octo:asker1", [0x55; 16]);
        assert!(repo.insert(&cargo).unwrap());
        let second = repo.insert(&cargo).unwrap();
        assert!(!second, "duplicate insert must return Ok(false)");
        assert_eq!(repo.len().unwrap(), 1, "table must hold exactly 1 row");
    }

    #[test]
    fn list_by_asker_returns_chronological_events() {
        let repo = SettlementEventRepository::open_in_memory().unwrap();
        let e1 = sample_event([0x42; 32], [0x55; 16]);
        let cargo1 = sample_cargo(&e1, "did:octo:asker1", [0x55; 16]);
        repo.insert(&cargo1).unwrap();

        // Second event: later settled_at_unix, same asker.
        let mut e2 = sample_event([0x66; 32], [0x99; 16]);
        e2.settled_at_unix = 1_700_000_001;
        let cargo2 = sample_cargo(&e2, "did:octo:asker1", [0x99; 16]);
        repo.insert(&cargo2).unwrap();

        // Third event: different asker.
        let e3 = sample_event([0x77; 32], [0xaa; 16]);
        let cargo3 = sample_cargo(&e3, "did:octo:asker2", [0xaa; 16]);
        repo.insert(&cargo3).unwrap();

        let asker1_events = repo.list_by_asker("did:octo:asker1").unwrap();
        assert_eq!(asker1_events.len(), 2);
        assert_eq!(asker1_events[0].ask_id, [0x42; 32]);
        assert_eq!(asker1_events[1].ask_id, [0x66; 32]);
        assert_eq!(repo.count_by_asker("did:octo:asker1").unwrap(), 2);
        assert_eq!(repo.count_by_asker("did:octo:asker2").unwrap(), 1);
        assert_eq!(
            repo.count_by_asker(&octo_ident::test_helpers::sample_did(19))
                .unwrap(),
            0
        );
    }

    #[test]
    fn cost_roundtrips_through_16_byte_be_u128() {
        let repo = SettlementEventRepository::open_in_memory().unwrap();
        let mut event = sample_event([0x42; 32], [0x55; 16]);
        event.cost = octo_determin::Dqa::new(i64::MAX, 0).expect("non-overflow");
        let cargo = sample_cargo(&event, "did:octo:asker1", [0x55; 16]);
        repo.insert(&cargo).unwrap();
        let sh = compute_settlement_hash(&event).unwrap();
        let persisted = repo.get_by_settlement_hash(&sh).unwrap().expect("row");
        assert_eq!(
            persisted.cost_micro_octo_w,
            octo_determin::Dqa::new(i64::MAX, 0).expect("non-overflow")
        );
    }

    #[test]
    fn axis_consumed_with_cache_key_roundtrip() {
        let repo = SettlementEventRepository::open_in_memory().unwrap();
        let mut event = sample_event([0x42; 32], [0x55; 16]);
        event.axes_consumed.cache_key_hash = Some([0xee; 32]);
        let cargo = sample_cargo(&event, "did:octo:asker1", [0x55; 16]);
        repo.insert(&cargo).unwrap();
        let sh = compute_settlement_hash(&event).unwrap();
        let persisted = repo.get_by_settlement_hash(&sh).unwrap().expect("row");
        assert_eq!(persisted.axes_consumed.cache_key_hash, Some([0xee; 32]));
    }
}
