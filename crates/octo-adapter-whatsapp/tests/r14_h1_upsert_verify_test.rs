// R14-H1 fix verification: UPSERT on the SAME composite UNIQUE index
// that `app_state_mutation_macs` uses (UNIQUE(name, index_mac, device_id))
// must work end-to-end in the new stoolap commit (1fc5bc2).
//
// The previous test file at
// /home/mmacedoeu/_w/databases/stoolap/tests/mission_0850_r14_regression_test.rs
// covered the same bug at the stoolap level. This test covers it at the
// cipherocto consumer level: it must work with the schema and SQL the
// adapter actually issues.

use stoolap::Database;

#[test]
fn r14_h1_upsert_on_composite_unique_works_in_cipherocto() {
    // Build the same schema as the adapter
    let db = Database::open("memory://r14_h1_upsert_test").expect("open db");
    db.execute(
        "CREATE TABLE app_state_mutation_macs (
            name TEXT NOT NULL,
            version BIGINT NOT NULL,
            index_mac BLOB NOT NULL,
            value_mac BLOB NOT NULL,
            device_id BIGINT NOT NULL,
            UNIQUE (name, index_mac, device_id)
        )",
        vec![],
    )
    .expect("create table");

    // First insert (plain INSERT, no tx wrap)
    let r = db
        .execute(
            "INSERT INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id)
             VALUES ($1, $2, $3, $4, $5)",
            vec![
                "critical_block".to_string().into(),
                1i64.into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                stoolap::core::Value::blob(vec![0x11; 32]),
                42i64.into(),
            ],
        )
        .expect("first insert should succeed");
    assert_eq!(r, 1, "first insert affects 1 row");

    // UPSERT on same composite unique — must NOT raise UniqueConstraint
    let r = db
        .execute(
            "INSERT INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id)
             VALUES ($1, $2, $3, $4, $5)
             ON DUPLICATE KEY UPDATE
                 version = $2,
                 value_mac = $4",
            vec![
                "critical_block".to_string().into(),
                2i64.into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                stoolap::core::Value::blob(vec![0x22; 32]),
                42i64.into(),
            ],
        )
        .expect("UPSERT on composite unique must succeed (R14-H1 fix)");
    assert_eq!(r, 1, "UPSERT affects 1 row (updated)");

    // Verify the row was updated (not duplicated)
    let count: i64 = db
        .query_one(
            "SELECT COUNT(*) FROM app_state_mutation_macs
             WHERE name = $1 AND index_mac = $2 AND device_id = $3",
            vec![
                "critical_block".to_string().into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                42i64.into(),
            ],
        )
        .expect("count should succeed");
    assert_eq!(count, 1, "UPSERT must UPDATE, not INSERT a duplicate");

    // Verify the values were updated
    let value_mac: Vec<u8> = db
        .query_one(
            "SELECT value_mac FROM app_state_mutation_macs
             WHERE name = $1 AND index_mac = $2 AND device_id = $3",
            vec![
                "critical_block".to_string().into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                42i64.into(),
            ],
        )
        .expect("query should succeed");
    assert_eq!(value_mac, vec![0x22; 32], "value_mac should be updated to 0x22");
}

#[test]
fn r14_h1_upsert_in_transaction_works_in_cipherocto() {
    // R15 fix: put_mutation_macs wraps the whole batch in a single
    // transaction. The UPSERT inside the tx must also work.
    let db = Database::open("memory://r14_h1_upsert_tx_test").expect("open db");
    db.execute(
        "CREATE TABLE app_state_mutation_macs (
            name TEXT NOT NULL,
            version BIGINT NOT NULL,
            index_mac BLOB NOT NULL,
            value_mac BLOB NOT NULL,
            device_id BIGINT NOT NULL,
            UNIQUE (name, index_mac, device_id)
        )",
        vec![],
    )
    .expect("create table");

    // Seed: first tx with plain INSERT
    {
        let mut tx = db.begin().expect("begin seed");
        tx.execute(
            "INSERT INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id)
             VALUES ($1, $2, $3, $4, $5)",
            vec![
                "critical_block".to_string().into(),
                1i64.into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                stoolap::core::Value::blob(vec![0x11; 32]),
                42i64.into(),
            ],
        )
        .expect("seed insert");
        tx.commit().expect("seed commit");
    }

    // Second tx: UPSERT (this is what put_mutation_macs does)
    {
        let mut tx = db.begin().expect("begin upsert");
        tx.execute(
            "INSERT INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id)
             VALUES ($1, $2, $3, $4, $5)
             ON DUPLICATE KEY UPDATE
                 version = $2,
                 value_mac = $4",
            vec![
                "critical_block".to_string().into(),
                2i64.into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                stoolap::core::Value::blob(vec![0x22; 32]),
                42i64.into(),
            ],
        )
        .expect("UPSERT in tx must succeed (R14-H1 fix)");
        tx.commit().expect("upsert commit");
    }

    // Verify the row was updated
    let count: i64 = db
        .query_one(
            "SELECT COUNT(*) FROM app_state_mutation_macs
             WHERE name = $1 AND index_mac = $2 AND device_id = $3",
            vec![
                "critical_block".to_string().into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                42i64.into(),
            ],
        )
        .expect("count should succeed");
    assert_eq!(count, 1, "UPSERT in tx must UPDATE, not INSERT a duplicate");

    let value_mac: Vec<u8> = db
        .query_one(
            "SELECT value_mac FROM app_state_mutation_macs
             WHERE name = $1 AND index_mac = $2 AND device_id = $3",
            vec![
                "critical_block".to_string().into(),
                stoolap::core::Value::blob(vec![0xAA; 32]),
                42i64.into(),
            ],
        )
        .expect("query should succeed");
    assert_eq!(value_mac, vec![0x22; 32], "value_mac should be updated");
}

#[test]
fn r14_h1_tx_delete_insert_works_in_cipherocto() {
    // Build a smaller composite-unique table to exercise the tx-local
    // delete visibility fix. This is the same pattern
    // `put_mutation_macs` uses (delete-then-insert within a transaction).
    let db = Database::open("memory://r14_h1_tx_test").expect("open db");
    db.execute(
        "CREATE TABLE kv (
            k TEXT NOT NULL,
            v BIGINT NOT NULL,
            device_id BIGINT NOT NULL,
            UNIQUE (k, device_id)
        )",
        vec![],
    )
    .expect("create table");

    // Seed: insert a row
    db.execute(
        "INSERT INTO kv (k, v, device_id) VALUES ($1, $2, $3)",
        vec!["key1".to_string().into(), 100i64.into(), 1i64.into()],
    )
    .expect("seed insert");

    // Now: DELETE then INSERT in a single transaction with the same unique value.
    // R14-H1 fix: check_unique_constraints must filter out the locally-deleted row.
    let mut tx = db.begin().expect("begin tx");
    tx.execute(
        "DELETE FROM kv WHERE k = $1 AND device_id = $2",
        vec!["key1".to_string().into(), 1i64.into()],
    )
    .expect("delete should succeed");
    tx.execute(
        "INSERT INTO kv (k, v, device_id) VALUES ($1, $2, $3)",
        vec!["key1".to_string().into(), 200i64.into(), 1i64.into()],
    )
    .expect("insert after delete in same tx must succeed (R14-H1 fix)");
    tx.commit().expect("commit");

    // Verify the new value
    let v: i64 = db
        .query_one(
            "SELECT v FROM kv WHERE k = $1 AND device_id = $2",
            vec!["key1".to_string().into(), 1i64.into()],
        )
        .expect("query should succeed");
    assert_eq!(v, 200, "value should be the new value 200 after tx commit");
}
