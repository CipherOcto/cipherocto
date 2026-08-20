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

use octo_storage_core::migrations::applied_version;
use octo_storage_core::open_in_memory;
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

/// Locks the composite PK `(chain_id, owner_did, asset_id)` per review
/// §20.3 Model B. A second row with the same composite PK must be rejected
/// at the substrate level (otherwise the runner records a migration whose
/// PK is silently broken — a wire-format break).
#[test]
fn vaults_composite_pk_rejects_duplicate() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply");

    let insert = || {
        db.execute(
            "INSERT INTO vaults \
             (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
              created_at_unix, metadata) \
             VALUES (?, ?, 'did:octo:alice', ?, '1.5', ?, 'Active', 1700000000, ?)",
            (
                [0u8; 32].as_slice(),
                [1u8; 32].as_slice(),
                [2u8; 32].as_slice(),
                vec![].as_slice(),
                vec![].as_slice(),
            ),
        )
    };
    insert().expect("first insert");
    let second = insert();
    assert!(
        second.is_err(),
        "composite PK (chain_id, owner_did, asset_id) must reject duplicate; got {:?}",
        second
    );
}

/// Locks the `vaults_vault_id_idx` UNIQUE index per review §20.3 Model B.
/// Two rows with DIFFERENT composite PKs but the same `vault_id` must be
/// rejected — vault_id is the central-registry identity key (§8.10).
#[test]
fn vaults_vault_id_unique_index_rejects_duplicate() {
    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply");

    // First row: (chain=1, owner=alice, asset=2)
    db.execute(
        "INSERT INTO vaults \
         (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
          created_at_unix, metadata) \
         VALUES (?, ?, 'did:octo:alice', ?, '1.5', ?, 'Active', 1700000000, ?)",
        (
            [42u8; 32].as_slice(),
            [1u8; 32].as_slice(),
            [2u8; 32].as_slice(),
            vec![].as_slice(),
            vec![].as_slice(),
        ),
    )
    .expect("first insert");

    // Second row: SAME vault_id, DIFFERENT composite PK
    // (chain=3, owner=bob, asset=4).
    let second = db.execute(
        "INSERT INTO vaults \
         (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
          created_at_unix, metadata) \
         VALUES (?, ?, 'did:octo:bob', ?, '1.5', ?, 'Active', 1700000000, ?)",
        (
            [42u8; 32].as_slice(),
            [3u8; 32].as_slice(),
            [4u8; 32].as_slice(),
            vec![].as_slice(),
            vec![].as_slice(),
        ),
    );
    assert!(
        second.is_err(),
        "UNIQUE INDEX vault_id must reject duplicate; got {:?}",
        second
    );
}

/// End-to-end wire alignment: derive vault_id via the public fn, insert,
/// SELECT, compare bytes. Catches silent BLOB(32)↔[u8;32]↔BLAKE3 truncation
/// or zero-padding in any Stoolap fork binding layer.
#[test]
fn vault_id_derive_insert_select_round_trips() {
    use octo_storage_core::migrations::applied_version;
    use octo_storage_core::open_in_memory;
    use octo_vault::{vault_id, AssetId, ChainId};

    let db = open_in_memory().expect("open in-memory");
    apply(&db).expect("apply");
    assert!(applied_version(&db, "schema_migrations")
        .expect("tracker")
        .contains(&13));

    let chain_id = ChainId::derive("cipherocto/testnet/v1");
    let asset_id = AssetId::derive("OCTO-W");
    let derived = vault_id(chain_id, "did:octo:round-trip-alice", asset_id);

    db.execute(
        "INSERT INTO vaults \
         (vault_id, chain_id, owner_did, asset_id, balance, policy, state, \
          created_at_unix, metadata) \
         VALUES (?, ?, 'did:octo:round-trip-alice', ?, '1.5', ?, 'Active', 1700000000, ?)",
        (
            derived.as_bytes(),
            chain_id.as_bytes(),
            asset_id.as_bytes(),
            vec![].as_slice(),
            vec![].as_slice(),
        ),
    )
    .expect("insert derived vault");

    let rows = db
        .query(
            "SELECT vault_id FROM vaults WHERE owner_did = 'did:octo:round-trip-alice'",
            (),
        )
        .expect("query derived vault");
    let mut found = None;
    for row in rows.into_iter() {
        let r = row.unwrap();
        let bytes: Vec<u8> = r.get(0).unwrap();
        found = Some(bytes);
    }
    let found = found.expect("derived vault row must exist");
    assert_eq!(
        found.as_slice(),
        derived.as_bytes(),
        "BLOB(32) round-trip must be byte-exact (32 bytes back, no truncation/zero-padding)"
    );
}
