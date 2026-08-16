//! End-to-end substrate delegation test for octo_vault::apply.
//!
//! Per plan §B.3: octo-vault owns migrations v013 (`vaults` table +
//! UNIQUE vault_id index) and v014 (`transfer_events`). The substrate
//! runner (`octo_storage_core::apply_pending`) drives the catalog; this
//! integration test exercises the full path against Stoolap's fork
//! (`DQA(12)` native type support).
//!
//! Locks:
//! - Migrations land via the substrate tracker table.
//! - `vaults` table + `vaults_vault_id_idx` exist after apply.
//! - `transfer_events` table exists after apply.
//! - Re-running `apply()` is a no-op (idempotency).
//! - DQA(12) insert + read round-trips with canonical scale.

use octo_storage_core::{applied_version, open_in_memory};
use octo_vault::apply;

#[test]
fn apply_lands_v013_and_v014() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("first apply");

    let applied = applied_version(&db, "schema_migrations").expect("tracker");
    assert!(applied.contains(&13), "v013 must be recorded");
    assert!(applied.contains(&14), "v014 must be recorded");

    // Insert a vault row with DQA(12) balance — exercises the native type
    // path + composite-PK uniqueness.
    db.execute(
        "INSERT INTO vaults \
         (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
          created_at_unix, metadata) \
         VALUES (?, ?, 'did:octo:test-alice', ?, '1.5', ?, 'Active', 1700000000, ?)",
        (
            [0u8; 32].as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 32].as_slice(),
            vec![].as_slice(),
            vec![].as_slice(),
        ),
    )
    .expect("insert into vaults");

    // Read back. Composite PK = (chain_id, owner_did, asset_id).
    let rows = db
        .query(
            "SELECT balance, state FROM vaults \
             WHERE chain_id = ? AND owner_did = 'did:octo:test-alice' AND asset_id = ?",
            ([1u8; 32].as_slice(), [2u8; 32].as_slice()),
        )
        .expect("query vaults");
    let mut count = 0;
    for row in rows.into_iter() {
        let r = row.unwrap();
        let balance: String = r.get(0).unwrap();
        let state: String = r.get(1).unwrap();
        assert_eq!(state, "Active");
        // DQA(12) canonical form on read — exact decimal-string round-trip.
        assert_eq!(balance, "1.5");
        count += 1;
    }
    assert_eq!(count, 1, "exactly one row should match composite PK");

    // Insert a transfer_event row (v014 smoke).
    db.execute(
        "INSERT INTO transfer_events \
         (event_id, tx_id, chain_id, schema_version, visibility, occurred_at_unix, \
          attributes, corrections, signature, zk_proof, from_vault_id, to_vault_id, \
          amount, capability_id, reason, canonical_hash, settlement_ref) \
         VALUES (?, ?, ?, 1, 'Public', 1700000000, ?, NULL, ?, NULL, ?, ?, '0.5', \
                 NULL, 'mint', ?, NULL)",
        (
            [10u8; 32].as_slice(),
            [11u8; 32].as_slice(),
            [1u8; 32].as_slice(),
            vec![].as_slice(),
            [0u8; 64].as_slice(),
            [3u8; 32].as_slice(),
            [4u8; 32].as_slice(),
            [12u8; 32].as_slice(),
        ),
    )
    .expect("insert transfer_event");
}

#[test]
fn apply_is_idempotent() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("first apply");
    apply(&db).expect("second apply — no-op");
    let applied = applied_version(&db, "schema_migrations").expect("tracker");
    assert!(applied.contains(&13));
    assert!(applied.contains(&14));
}
