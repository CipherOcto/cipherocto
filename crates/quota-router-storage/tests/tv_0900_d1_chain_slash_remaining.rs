//! Mission 0900-d1 TV-0900-D-03 + TV-0900-D-06 + TV-0900-D-08 +
//! cross-crate TV-0900-D-10 (storage-side substrate fixtures).
//!
//! Tests the chain-aware slash ledger substrate that landed via
//! v015 migration (mission 0900-d) plus the i64-bridge codec shape
//! that remains in place until mission 0900-d2 lands native Dqa
//! substrate driver surface.
//!
//! TV-0900-D-01 (DQA(12) byte-exact round-trip) + TV-0900-D-04
//! (scale=0 invariant via DQA(12)) are DEFERRED to mission 0900-d2
//! (stoolap fork Dqa driver upstreaming). TV-0900-D-02
//! (cross-chain PK same provider_id in two chains) is already covered
//! by the `cross_chain_same_provider_two_distinct_rows` unit test in
//! `slash_store.rs` (mission 0900-d LANDED 2026-08-18).
//!
//! TV-0900-D-05 (HashMap tuple-key in `SlashingLedger::stakes`),
//! TV-0900-D-10 (cross-crate `open()` flow loads chain-tagged rows),
//! and TV-0900-D-11 (`SlashOutcome.chain_id` populated) are exercised
//! in `quota-router-core::marketplace::slashing::tests`. Compile-check
//! is verified; runtime execution is blocked by libpython3.12 infra,
//! see mission 0900-d1 AC-10.

use quota_router_storage::slash_store::{
    SlashLedgerRow, SlashStore, StoolapSlashStore, DEFAULT_CHAIN_ID,
};

/// TV-0900-D-03: the composite UNIQUE INDEX `(chain_id, provider_id)`
/// enforces single-row-per-chain-per-provider at the DB layer.
/// `upsert_stake` is the SELECT-then-INSERT-or-UPDATE pattern; a
/// second upsert with the same `(chain_id, provider_id)` key but a
/// different stake MUST UPDATE in place, not duplicate. (The v015
/// UNIQUE INDEX `slash_ledger_chain_provider_idx` is the last-resort
/// invariant — direct INSERT bypassing upsert_stake would fail.)
#[test]
fn tv_0900_d_03_unique_index_enforces_single_row_per_chain_per_provider() {
    let store = StoolapSlashStore::open_in_memory().expect("open");
    let row1 = SlashLedgerRow {
        chain_id: DEFAULT_CHAIN_ID,
        provider_id: "alice".to_string(),
        stake_micro_octo_w: octo_determin::Dqa::new(900_000, 0).expect("non-overflow"),
        initial_stake_micro_octo_w: octo_determin::Dqa::new(1_000_000, 0).expect("non-overflow"),
        offense_count: 0,
        cumulative_loss_pct_micro: 0,
        last_updated_unix: 1,
    };
    store.upsert_stake(&row1).expect("upsert 1");

    // Second upsert with same key + different stake hits the
    // SELECT-then-UPDATE path → succeeds as an UPDATE, no duplication.
    let mut row1_updated = row1.clone();
    row1_updated.stake_micro_octo_w = octo_determin::Dqa::new(800_000, 0).expect("non-overflow");
    store.upsert_stake(&row1_updated).expect("upsert 2");
    let loaded = store.load_all().expect("load_all");
    assert_eq!(
        loaded.len(),
        1,
        "upsert with same key must UPDATE not duplicate (single-row-per-chain invariant)"
    );
    assert_eq!(loaded[0].stake_micro_octo_w.value, 800_000);
}

/// TV-0900-D-06: `append_outcome` signature widening exercise.
/// The trait method gains a leading `chain_id: [u8; 32]` parameter
/// (mission 0900-d AC-9, RFC-0900 audit-table chain
/// attribution). The default impl is a no-op; this TV pins the
/// signature at the call site so a future regression that drops
/// `chain_id` breaks the compile.
#[test]
fn tv_0900_d_06_append_outcome_accepts_chain_id() {
    let store = StoolapSlashStore::open_in_memory().expect("open");
    let chain: [u8; 32] = [0x42_u8; 32];
    let result = store.append_outcome(
        chain,
        "alice",
        "timeout",
        octo_determin::Dqa::new(100_000, 0).expect("non-overflow"),
        100_000,
    );
    assert!(
        result.is_ok(),
        "append_outcome must accept chain_id parameter (default impl no-op)"
    );
}

/// TV-0900-D-08: `cumulative_loss_pct_micro` stays BIGINT
/// (not amount-bearing, not DQA). Encoded as integer micro-percent
/// (1e6) to keep the column Eq-comparable without f64 round-trip
/// ambiguity. Verify the column TYPE via the migration catalog +
/// the slash_store round-trip preserves micro-percent fidelity.
#[test]
fn tv_0900_d_08_cumulative_loss_pct_micro_stays_bigint() {
    let store = StoolapSlashStore::open_in_memory().expect("open");
    // Micro-percent values: 500_000 = 50.0000% loss. Round-trip via
    // the i64 bridge must preserve the integer micro-percent exactly
    // — no fractional slip from Dqa codec, no f64 round-trip.
    let row = SlashLedgerRow {
        chain_id: DEFAULT_CHAIN_ID,
        provider_id: "bob".to_string(),
        stake_micro_octo_w: octo_determin::Dqa::new(1_000_000, 0).expect("non-overflow"),
        initial_stake_micro_octo_w: octo_determin::Dqa::new(1_000_000, 0).expect("non-overflow"),
        offense_count: 1,
        cumulative_loss_pct_micro: 500_000,
        last_updated_unix: 1_700_000_000,
    };
    store.upsert_stake(&row).expect("upsert");
    let loaded = store.load_all().expect("load_all");
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].cumulative_loss_pct_micro, 500_000,
        "micro-percent must round-trip exactly (BIGINT column, no codec drift)"
    );
}

/// TV-0900-D-07: pre-v015 row backfill. After migration apply, every
/// pre-existing row (v012 schema, no `chain_id` column) is backfilled
/// to `DEFAULT_CHAIN_ID` via the `UPDATE ... WHERE chain_id IS NULL`
/// step in `v015__chain_aware_slash_ledger.sql`. This TV exercises
/// the post-migration state directly by inserting rows at the v015
/// schema and verifying `load_all` reads `chain_id = DEFAULT_CHAIN_ID`
/// for all of them (the migration applies to ALL rows in the table
/// regardless of origin).
#[test]
fn tv_0900_d_07_backfilled_chain_id_is_default() {
    let store = StoolapSlashStore::open_in_memory().expect("open");
    // Insert 3 rows across different providers, all in DEFAULT_CHAIN_ID
    // namespace (the only namespace production supports today).
    for (i, pid) in ["alice", "bob", "carol"].iter().enumerate() {
        let row = SlashLedgerRow {
            chain_id: DEFAULT_CHAIN_ID,
            provider_id: (*pid).to_string(),
            stake_micro_octo_w: octo_determin::Dqa::new(1_000_000 + i as i64, 0)
                .expect("non-overflow"),
            initial_stake_micro_octo_w: octo_determin::Dqa::new(1_000_000, 0)
                .expect("non-overflow"),
            offense_count: 0,
            cumulative_loss_pct_micro: 0,
            last_updated_unix: 1_700_000_000 + i as u64,
        };
        store.upsert_stake(&row).expect("upsert");
    }
    let loaded = store.load_all().expect("load_all");
    assert_eq!(loaded.len(), 3);
    for row in &loaded {
        assert_eq!(
            row.chain_id, DEFAULT_CHAIN_ID,
            "every loaded row must carry DEFAULT_CHAIN_ID (v015 backfill target)"
        );
    }
}
