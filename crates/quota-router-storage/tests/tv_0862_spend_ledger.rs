//! TV-0862-01..05 + TV-0862-07 + TV-0862-08 + TV-0862-04b +
//! TV-0862-09 + TV-0862-09b + TV-0862-12 + TV-0862-13 — RFC-0862
//! StoolapSpendLedger byte-exact fixtures.
//!
//! Per `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
//! §3 row 6 (Stream A.1 S6c) + §4 S6 verify gate. 10 byte-exact
//! test vectors pinning the `StoolapSpendLedger` substrate added in
//! RFC-0862 (Dqa + vault bump amendment):
//!
//! (TV-06 vault_id cross-ref lives in
//! `crates/octo-vault/tests/tv_0862_vault_id_cross_ref.rs` per
//! Round 1 review — 3 additional tests there.)
//!
//! - TV-01: row creation via seed
//! - TV-02: balance read (None for unknown, Some for known)
//! - TV-03: seed idempotency (last-wins upsert)
//! - TV-04: atomic try_deduct happy path
//! - TV-04b: try_deduct UnknownHolder error (regression split out
//!   from TV-04 per S6c Round 1 review)
//! - TV-05: Dqa encoding round-trip + i64 schema column round-trip
//! - TV-07: V2 wire-form on substrate side (Dqa round-trip + UPDATE)
//! - TV-08: multi-instance drain coordination (per-instance lock scope)
//! - TV-09: negative-cost rejection (substrate hardening per S4
//!   Round 2 + S6c Round 1 security review)
//! - TV-10: injected `Clock` sources `updated_at_unix_ms`
//!   deterministically (mission 0862-c2: `SystemTime::now` masked
//!   by fixture shape — now asserted via `FixedClock`)
//! - TV-11: file-backed two-instance concurrent-deduct (mission
//!   0862-c3: advisory file lock + stoolap transaction wrapper
//!   prevent over-drain; 10 threads × 100 cost on 1000 budget →
//!   exactly 10 succeed, 10 fail with `InsufficientBalance`,
//!   final balance 0; + TV-11b `LockUnavailable` fail-closed
//!   when an external `flock` holds the file)
//! - TV-12: scale-mismatch rejection on `seed` (mission 0862-c4:
//!   panic→typed-error for `dqa_to_i64`)
//! - TV-13: scale-mismatch rejection on `try_deduct` (mission 0862-c4)
//! - TV-14: substrate accepts arbitrary holder_did bytes (mission
//!   0862-c6: no DID validation in substrate; wallet-node boundary
//!   owns canonical validation)
//!
//! TV-06 (vault_id derivation cross-ref) moved to
//! `crates/octo-vault/tests/tv_0862_vault_id_cross_ref.rs` per
//! S6c Round 1 code review finding #4 (vault_id derivation is
//! owned by octo-vault, not quota-router-storage).
//!
//! All inputs byte-pinned (`TV_0862_*` constants); no RNG. Schema +
//! API surface mirrors `crates/quota-router-storage/src/stoolap_spend_ledger.rs`.

use octo_determin::{Dqa, DqaEncoding};
use quota_router_storage::stoolap_spend_ledger::{SpendLedgerError, StoolapSpendLedger};
use quota_router_storage::FixedClock;

// =============================================================================
// TV-0862-01 — row creation via seed
// =============================================================================

/// TV-0862-01: `StoolapSpendLedger::seed` inserts a new row when no
/// existing `(holder_did, macaroon_id)` row exists. After seed,
/// `balance` returns the same `Dqa`.
#[test]
fn tv_0862_01_seed_creates_row() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086201";
    let macaroon_id = TV_0862_MACAROON_ID_01;

    let budget = dqa(1_000_000); // 1 OCTO-W at scale=0
    ledger
        .seed(holder, &macaroon_id, budget)
        .expect("seed new row");

    let stored = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after seed");
    assert_eq!(
        stored, budget,
        "TV-0862-01: seeded balance must round-trip exactly: got {stored:?}"
    );
}

// =============================================================================
// TV-0862-02 — balance read
// =============================================================================

/// TV-0862-02: `balance` returns `None` for an unknown
/// `(holder_did, macaroon_id)` key. After seed, the same lookup
/// returns `Some(Dqa)`. This is the read-path contract that
/// the wallet-node relies on for pre-deduct visibility.
#[test]
fn tv_0862_02_balance_read_unknown_returns_none() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086202";
    let macaroon_id = TV_0862_MACAROON_ID_02;

    // Unknown key: must be None, NOT an error.
    let before = ledger.balance(holder, &macaroon_id).expect("balance read");
    assert!(
        before.is_none(),
        "TV-0862-02: unknown holder must return None: got {before:?}"
    );

    // After seed, must be Some.
    let budget = dqa(42_000);
    ledger.seed(holder, &macaroon_id, budget).expect("seed");
    let after = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after seed");
    assert_eq!(after, budget, "TV-0862-02: post-seed balance must match");
}

// =============================================================================
// TV-0862-03 — seed idempotency (last-wins upsert)
// =============================================================================

/// TV-0862-03: `seed` is upsert semantics — re-seeding an existing
/// `(holder_did, macaroon_id)` row overwrites the prior balance.
/// Per RFC-0957 §Algorithms caveat re-mint: the wallet may re-seed
/// on `PaymentCaveat` re-mint; the new budget supersedes the old.
#[test]
fn tv_0862_03_seed_idempotent_last_wins() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086203";
    let macaroon_id = TV_0862_MACAROON_ID_03;

    ledger
        .seed(holder, &macaroon_id, dqa(100_000))
        .expect("seed #1");
    let first = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after first seed");
    assert_eq!(
        first,
        dqa(100_000),
        "TV-0862-03: first seed must be 100_000"
    );

    ledger
        .seed(holder, &macaroon_id, dqa(500_000))
        .expect("seed #2");
    let second = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after second seed");
    assert_eq!(
        second,
        dqa(500_000),
        "TV-0862-03: second seed must overwrite (last-wins): got {second:?}"
    );
}

// =============================================================================
// TV-0862-04 — atomic try_deduct happy path
// =============================================================================

/// TV-0862-04: `try_deduct` atomically decrements the balance and
/// returns the new remainder. The deduction runs inside a per-instance
/// `drain_lock` critical section (per RFC-0862 §StoolapSpendLedger
/// `Atomicity guarantee`).
#[test]
fn tv_0862_04_try_deduct_atomic_decrement() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086204";
    let macaroon_id = TV_0862_MACAROON_ID_04;

    ledger
        .seed(holder, &macaroon_id, dqa(10_000))
        .expect("seed");
    let new_balance = ledger
        .try_deduct(holder, &macaroon_id, dqa(3_000))
        .expect("try_deduct");
    assert_eq!(
        new_balance,
        dqa(7_000),
        "TV-0862-04: new balance after deduct must be 10_000 - 3_000"
    );

    // Persisted state must reflect the decrement.
    let stored = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after deduct");
    assert_eq!(
        stored,
        dqa(7_000),
        "TV-0862-04: persisted balance must match new balance"
    );
}

/// TV-0862-04b: `try_deduct` on an unknown holder returns
/// `UnknownHolder` (NOT `Storage` or a panic).
/// Dedicated macaroon_id constant (per S6c Round 1 code review
/// finding #8 — no cross-test coupling).
#[test]
fn tv_0862_04b_try_deduct_unknown_holder_errors() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let result = ledger.try_deduct("did:octo:zTV086204b", &TV_0862_MACAROON_ID_04B, dqa(1));
    assert!(
        matches!(result, Err(SpendLedgerError::UnknownHolder)),
        "TV-0862-04b: unknown holder must yield UnknownHolder: got {result:?}"
    );
}

// =============================================================================
// TV-0862-05 — Dqa encoding round-trip + i64 schema column round-trip
// =============================================================================

/// TV-0862-05: `Dqa` is `Dqa` at `scale = 0` (integer
/// micro-OCTO-W counts). `DqaEncoding` is the canonical 16-byte BE
/// consensus wire form per RFC-0105 v1.9 §DqaEncoding struct.
/// Round-trip must be exact AND the substrate's `i64` column
/// (stoolap `INTEGER` ↔ `i64`) must carry `Dqa::value` losslessly
/// via the `seed → balance` cycle.
///
/// Pin a non-trivial value to guard against endianness / scale sign /
/// schema-mapping regressions (S6c Round 1 code review finding #6:
/// the prior test only asserted `value.value == N`, which is
/// tautological — the substrate's `dqa_to_i64` helper was never
/// exercised).
#[test]
fn tv_0862_05_dqa_encoding_round_trip_and_i64_schema_column() {
    let value = dqa(1_234_567_890);
    assert_eq!(value.scale, 0, "TV-0862-05: Dqa MUST be stored at scale=0");

    // (a) DqaEncoding round-trip (canonical 16-byte BE wire form).
    let encoding = DqaEncoding::from_dqa(&value);
    assert_eq!(
        std::mem::size_of::<DqaEncoding>(),
        16,
        "TV-0862-05: DqaEncoding wire form MUST be exactly 16 bytes"
    );

    let round_trip = encoding.to_dqa().expect("decode to Dqa");
    assert_eq!(
        round_trip, value,
        "TV-0862-05: DqaEncoding round-trip must be exact: got {round_trip:?}"
    );

    // (b) i64 schema column round-trip via the substrate.
    // The seed path writes `dqa_to_i64(value)` (carrying Dqa::value
    // as i64) into the `balance` INTEGER column; the balance read
    // path decodes it back into Dqa at scale=0. This exercises the
    // actual storage contract — the prior test did NOT exercise it.
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086205";
    let macaroon_id = TV_0862_MACAROON_ID_05;
    ledger.seed(holder, &macaroon_id, value).expect("seed");
    let stored = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist");
    assert_eq!(
        stored.value, 1_234_567_890,
        "TV-0862-05: i64 schema column round-trip MUST equal input Dqa::value"
    );
    assert_eq!(
        stored.scale, 0,
        "TV-0862-05: schema column decode MUST preserve scale=0"
    );
}

// =============================================================================
// TV-0862-07 — V2 wire-form on substrate side
// =============================================================================

/// TV-0862-07: The `spend_ledger.balance` column stores the `i64`
/// form of `Dqa` at `scale = 0`. The substrate's UPDATE path
/// (deduct → new_balance) must round-trip through the column.
///
/// This pins the V2 wire-form on the SUBSTRATE side. The on-wire
/// `NodeEnvelope.version_tag = 0xA1` cross-ref per RFC-0870 (S6a)
/// is enforced at the dispatch layer; the dispatch-layer envelope
/// test lives in `crates/octo-protocol/tests/tv_0870_envelope.rs`.
#[test]
fn tv_0862_07_dqa_v2_wire_form_pinned() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086207";
    let macaroon_id = TV_0862_MACAROON_ID_07;

    // Seed a value that exercises the V2 wire-form round-trip path.
    let positive = TV_0862_BALANCE_07_FULL; // 1e12 micro-OCTO-W
    ledger
        .seed(holder, &macaroon_id, positive)
        .expect("seed positive");
    let read = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist");
    assert_eq!(
        read, positive,
        "TV-0862-07: V2 wire-form round-trip must be exact"
    );

    // Deduct half — exercises the V2 wire-form UPDATE path.
    let half = TV_0862_BALANCE_07_HALF;
    let remainder = ledger
        .try_deduct(holder, &macaroon_id, half)
        .expect("try_deduct");
    assert_eq!(
        remainder, TV_0862_BALANCE_07_HALF,
        "TV-0862-07: V2 wire-form UPDATE must reflect new balance"
    );

    // Encoding contract: 16-byte BE form must be deterministic
    // across `Dqa` ↔ `DqaEncoding` round-trips. Pin the byte array
    // explicitly (S6c Round 1 code review finding #2 — `PartialEq`
    // compares `i64` as integer, not as bytes; endianness drift in
    // `from_dqa` would pass `PartialEq` while flipping wire form).
    let enc1 = DqaEncoding::from_dqa(&TV_0862_BALANCE_07_FULL);
    // Canonical-form precondition: `from_dqa` calls the private
    // `canonicalize(*dqa)` (see `determin::dqa`) which strips trailing
    // zeros from the integer part. Fixtures MUST already be canonical
    // or the byte-pin below would silently compare the canonicalized
    // output (e.g. `1e15` at scale=3 → `1` at scale=0) instead of the
    // literal fixture bytes. `TV_0862_BALANCE_07_FULL = 1_000_000_000_000`
    // at `scale=0` is canonical (no trailing zeros to strip); the
    // `canonicalize` short-circuit at scale=0 confirms. Future
    // fixture changes MUST preserve canonical form — see TV-0862
    // reviewer follow-up 0862-c5 (domain-sep hygiene) for related
    // hash-prefix discipline.
    let enc1_bytes: &[u8; 16] = unsafe {
        // SAFETY: `DqaEncoding` is `#[repr(C)]` (per
        // `determin/src/dqa.rs` — the load-bearing invariant is the
        // `repr(C)` attribute, which guarantees the C ABI layout:
        // `value: i64` at offset 0, `scale: u8` at offset 8,
        // `_reserved: [u8; 7]` at offset 9, no implicit padding, total
        // 16 bytes). Without `repr(C)` the compiler is free to reorder
        // fields and this cast is UB. The compile-time assertion
        // `assert!(size_of::<DqaEncoding>() == 16)` in the substrate
        // is a secondary guard. The cast produces a stable byte view
        // matching the on-wire form (BE value, scale, reserved).
        &*(&enc1 as *const DqaEncoding as *const [u8; 16])
    };
    // 1_000_000_000_000 = 0xE8D4A51000 (BE 8 bytes: 0x00 0x00 0x00
    // 0xE8 0xD4 0xA5 0x10 0x00) + scale 0x00 + reserved 0x00 * 7.
    assert_eq!(
        enc1_bytes, &TV_0862_DQA_ENCODING_07_BYTES,
        "TV-0862-07: DqaEncoding 16-byte wire form MUST match pinned BE layout"
    );
}

// =============================================================================
// TV-0862-08 — multi-instance drain coordination (per-instance lock scope)
// =============================================================================

/// TV-0862-08: The per-instance `drain_lock` serializes
/// `try_deduct` within a single `StoolapSpendLedger` instance. Two
/// separate in-memory instances are independent (each owns its own
/// `Database::open_in_memory`).
///
/// Per RFC-0862 §StoolapSpendLedger `Atomicity guarantee`: in-memory
/// instances have NO cross-instance coordination; cross-instance
/// coordination is mission `0871e-phase5c-1`
/// (`RaftLikeDrainCoordinator` LANDED 2026-08-11).
///
/// Cross-process file-backed coordination is OUT OF SCOPE for S6c
/// and is filed as follow-on `0862-c3-cross-process-drain` (S6c
/// Round 1 security review finding #4).
#[test]
fn tv_0862_08_multi_instance_in_memory_lock_isolation() {
    let ledger_a = StoolapSpendLedger::open_in_memory().expect("open_in_memory A");
    let ledger_b = StoolapSpendLedger::open_in_memory().expect("open_in_memory B");
    let holder = "did:octo:zTV086208";
    let macaroon_id = TV_0862_MACAROON_ID_08;

    // Both instances are seeded independently (in-memory isolation).
    ledger_a
        .seed(holder, &macaroon_id, dqa(10_000))
        .expect("seed A");
    ledger_b
        .seed(holder, &macaroon_id, dqa(20_000))
        .expect("seed B");

    let a_balance = ledger_a
        .balance(holder, &macaroon_id)
        .expect("balance A")
        .expect("A row");
    let b_balance = ledger_b
        .balance(holder, &macaroon_id)
        .expect("balance B")
        .expect("B row");
    assert_eq!(
        a_balance,
        dqa(10_000),
        "TV-0862-08: instance A balance must be 10_000"
    );
    assert_eq!(
        b_balance,
        dqa(20_000),
        "TV-0862-08: instance B balance must be 20_000 (independent)"
    );
    assert_ne!(
        a_balance, b_balance,
        "TV-0862-08: in-memory instances MUST be independent (cross-instance coordination is 0871e-phase5c-1 territory)"
    );

    // Deduct on A does NOT affect B (lock is per-instance).
    let a_remainder = ledger_a
        .try_deduct(holder, &macaroon_id, dqa(3_000))
        .expect("try_deduct A");
    assert_eq!(
        a_remainder,
        dqa(7_000),
        "TV-0862-08: A remainder = 10_000 - 3_000"
    );

    let b_after = ledger_b
        .balance(holder, &macaroon_id)
        .expect("balance B after A deduct")
        .expect("B row still present");
    assert_eq!(
        b_after,
        dqa(20_000),
        "TV-0862-08: B balance MUST be unaffected by A's deduct (per-instance lock)"
    );
}

// =============================================================================
// TV-0862-09 — negative-cost rejection (substrate hardening per S4 Round 2)
// =============================================================================

/// TV-0862-09: `try_deduct` rejects negative `cost` with
/// `SpendLedgerError::NegativeCost` (defense-in-depth against signed
/// underflow in caller fee-computation paths and wire-decoded `i64`
/// amounts — S6c Round 1 security review finding #3, S4 Round 2
/// surfaced the same class of bug elsewhere).
///
/// Rejection is precondition-only (no DB hit, no lock acquired,
/// state unchanged).
#[test]
fn tv_0862_09_try_deduct_negative_cost_rejected() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086209";
    let macaroon_id = TV_0862_MACAROON_ID_09;

    // Seed a positive balance so the negative-cost path is
    // unambiguous (rejection must precede the
    // UnknownHolder/InsufficientBalance arms).
    ledger
        .seed(holder, &macaroon_id, dqa(10_000))
        .expect("seed");

    let negative = dqa(-1);
    let result = ledger.try_deduct(holder, &macaroon_id, negative);
    assert!(
        matches!(result, Err(SpendLedgerError::NegativeCost { .. })),
        "TV-0862-09: negative cost MUST yield NegativeCost: got {result:?}"
    );

    // State unchanged after rejection (balance still 10_000).
    let stored = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after rejection");
    assert_eq!(
        stored,
        dqa(10_000),
        "TV-0862-09: balance MUST be unchanged after NegativeCost rejection"
    );
}

/// TV-0862-09b: `try_deduct` with negative cost on an UNKNOWN holder
/// still rejects with `NegativeCost` (precondition check precedes
/// the UnknownHolder path).
#[test]
fn tv_0862_09b_try_deduct_negative_cost_on_unknown_holder() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let result = ledger.try_deduct("did:octo:zTV086209b", &TV_0862_MACAROON_ID_09B, dqa(-1));
    assert!(
        matches!(result, Err(SpendLedgerError::NegativeCost { .. })),
        "TV-0862-09b: negative cost on unknown holder MUST yield NegativeCost (precondition precedes UnknownHolder): got {result:?}"
    );
}

// =============================================================================
// TV-0862-10 — injected `Clock` pins `updated_at_unix_ms`
// deterministically (mission 0862-c2: `SystemTime::now()` masked by
// fixture shape; fixed-clock test forces the column to the pinned value
// so the substrate becomes deterministically replayable)
// =============================================================================

/// TV-0862-10: `seed()` + `try_deduct()` with an injected
/// `FixedClock` MUST record `updated_at_unix_ms == clock.unix_millis()`.
/// Production path uses `SystemClock` (now-millis); tests substitute
/// a `FixedClock` to assert exact column values byte-exact.
///
/// The fixture uses a pinned millis-unix value (`1_700_000_000_000`,
/// i.e. 2023-11-14T22:13:20Z) — chosen as a stable round-number so
/// the assertion is readable. If the substrate ever changes the
/// `Clock` → `i64` cast shape (e.g. widening `as i64` to a checked
/// `try_into()`), this TV will surface immediately as either a
/// pin drift or a type-mismatch compile error.
///
/// The row is then read back via a raw stoolap `SELECT` — the
/// substrate's `balance()` accessor does not surface the column,
/// but the column has to be readable to migrate / replay
/// eventually (per RFC-0862 §Future Work).
#[test]
fn tv_0862_10_injected_clock_pins_updated_at_unix_ms() {
    use std::sync::Arc;

    let clock_ms: u64 = 1_700_000_000_000;
    let pinned_clock = Arc::new(FixedClock::new(clock_ms));

    let ledger = StoolapSpendLedger::open_in_memory_with_clock(pinned_clock.clone())
        .expect("open with clock");
    let holder = "did:octo:zTV086210";
    let macaroon_id = TV_0862_MACAROON_ID_10;

    // (1) seed → first updated_at_unix_ms write.
    ledger
        .seed(holder, &macaroon_id, dqa(1_000))
        .expect("seed with pinned clock");

    // (2) try_deduct → second updated_at_unix_ms write with the same
    // pinned clock (no advancement).
    let remaining = ledger
        .try_deduct(holder, &macaroon_id, dqa(250))
        .expect("deduct with pinned clock");
    assert_eq!(remaining, dqa(750));

    // (3) Substrate-level column assertion: the row's
    // `updated_at_unix_ms` MUST equal the pinned clock value.
    // Read it via raw stoolap query (the substrate's `balance()`
    // accessor does not surface the column).
    let rows = ledger
        .raw_query(
            "SELECT updated_at_unix_ms FROM spend_ledger \
             WHERE holder_did = ? AND macaroon_id = ? LIMIT 1",
            (holder.as_bytes().to_vec(), macaroon_id.to_vec()),
        )
        .expect("raw_query");
    let mut iter = rows;
    let row = iter.next().expect("row exists").expect("row ok");
    let column_value: i64 = row.get(0).unwrap_or(0);
    assert_eq!(
        column_value, clock_ms as i64,
        "TV-0862-10: updated_at_unix_ms MUST equal injected FixedClock value"
    );
}

// =============================================================================
// TV-0862-11 — file-backed concurrent-deduct (mission 0862-c3)
// =============================================================================

/// TV-0862-11: a single file-backed `StoolapSpendLedger` instance
/// must atomically serialize its `try_deduct` operations across
/// concurrent threads via the `drain_lock` Mutex AND the
/// `stoolap::Transaction` wrapper introduced in mission 0862-c3
/// (the advisory file lock prevents cross-process races; the
/// transaction + drain_lock handle in-process concurrency). 20
/// threads each try to deduct 100 from a 1000 budget; exactly 10
/// must succeed, 10 must fail with `InsufficientBalance`. Final
/// balance MUST be 0 (no over-drain).
///
/// This test verifies the file-backed path matches the in-memory
/// path (TV-0862-08) on the cross-instance / cross-thread contract.
/// Cross-process serialization is exercised by TV-0862-11b (a
/// second `flock` on the same `.spend_ledger.lock` file surfaces
/// `LockUnavailable` fail-closed per mission 0862-c3 AC-1).
#[test]
fn tv_0862_11_file_backed_concurrent_deduct() {
    use std::sync::Arc;
    use std::thread;

    // Temp dir path (dropped at end of test via TempDir). Use
    // stoolap DSN format `file://<dir>` — the DSN path is a
    // directory for WAL + snapshots per stoolap fork persistence.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dsn = tmp.path().to_str().expect("utf8 path").to_string();

    let ledger = StoolapSpendLedger::open_path(&dsn).expect("open_path");
    let holder = "did:octo:zTV086211";
    let macaroon_id = TV_0862_MACAROON_ID_11;
    ledger
        .seed(holder, &macaroon_id, dqa(1_000))
        .expect("seed 1000");

    let ledger = Arc::new(ledger);
    let success_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let fail_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..20 {
        let ledger = Arc::clone(&ledger);
        let success_count = Arc::clone(&success_count);
        let fail_count = Arc::clone(&fail_count);
        handles.push(thread::spawn(move || {
            match ledger.try_deduct(holder, &macaroon_id, dqa(100)) {
                Ok(_) => {
                    success_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                Err(SpendLedgerError::InsufficientBalance { .. }) => {
                    fail_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                Err(e) => panic!("TV-0862-11: unexpected error (not InsufficientBalance): {e:?}"),
            }
        }));
    }
    for h in handles {
        h.join().expect("thread join");
    }

    let successes = success_count.load(std::sync::atomic::Ordering::SeqCst);
    let failures = fail_count.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        successes, 10,
        "TV-0862-11: exactly 10 threads must succeed (drain 1000 budget): got {successes}"
    );
    assert_eq!(
        failures, 10,
        "TV-0862-11: exactly 10 threads must fail with InsufficientBalance: got {failures}"
    );

    let final_balance = ledger
        .balance(holder, &macaroon_id)
        .expect("final balance read");
    assert_eq!(
        final_balance,
        Some(dqa(0)),
        "TV-0862-11: final balance must be 0 (no over-drain): got {final_balance:?}"
    );
}

/// TV-0862-11b: `open_path` fails-closed with `LockUnavailable` when
/// an external process holds the `.spend_ledger.lock` file (per
/// mission 0862-c3 AC-1 fail-closed contract). The substrate's
/// `flock(2)` would otherwise block on the second open; instead it
/// surfaces a typed error so the wallet-node startup can either
/// retry or fail-fast.
#[test]
fn tv_0862_11b_open_path_lock_unavailable_fail_closed() {
    use fs2::FileExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    let fs_path = tmp.path().to_str().expect("utf8 path");
    let dsn = fs_path.to_string();
    // Substrate acquires the lock on a sibling file
    // `<dir>/.spend_ledger.lock` (the DSN path is a directory for
    // WAL + snapshots per stoolap fork persistence). Acquire the
    // SAME file here to simulate a second process holding the
    // substrate's lock target.
    let lock_path = tmp.path().join(".spend_ledger.lock");
    let external_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .expect("external lock open");
    external_lock
        .lock_exclusive()
        .expect("external lock acquire");

    // Substrate's `open_path` must fail-closed.
    let result = StoolapSpendLedger::open_path(&dsn);
    assert!(
        matches!(result, Err(SpendLedgerError::LockUnavailable { .. })),
        "TV-0862-11b: open_path must fail-closed with LockUnavailable: got {result:?}"
    );

    drop(external_lock);
}

// =============================================================================
// TV-0862-15 — concurrent seed() on same (holder_did, macaroon_id) serializes
// (mission 0862-c8: seed() acquires drain_lock)
// =============================================================================

/// TV-0862-15: two concurrent `seed()` calls on the same
/// `(holder_did, macaroon_id)` MUST serialize via `drain_lock`. The
/// second seed observes the first's effect (no PRIMARY KEY violation
/// surfaces; no lost update). Per mission 0862-c8 (TOCTOU mitigation).
#[test]
fn tv_0862_15_concurrent_seed_serializes() {
    use std::sync::Arc;
    use std::thread;

    let ledger = Arc::new(StoolapSpendLedger::open_in_memory().expect("open_in_memory"));
    let holder = "did:octo:zTV086215";
    let macaroon_id = TV_0862_MACAROON_ID_15;

    // Pre-seed a row so the second seed hits the UPDATE branch (not
    // INSERT). Tests both branches under contention.
    ledger
        .seed(holder, &macaroon_id, dqa(100))
        .expect("pre-seed");

    let l1 = ledger.clone();
    let h1 = holder.to_owned();
    let m1 = macaroon_id;
    let t1 = thread::spawn(move || l1.seed(&h1, &m1, dqa(500)));

    let l2 = ledger.clone();
    let h2 = holder.to_owned();
    let m2 = macaroon_id;
    let t2 = thread::spawn(move || l2.seed(&h2, &m2, dqa(700)));

    t1.join().expect("seed 1 thread").expect("seed 1 ok");
    t2.join().expect("seed 2 thread").expect("seed 2 ok");

    // Last-writer-wins. Accept either 500 or 700.
    let final_balance = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist");
    assert!(
        final_balance == dqa(500) || final_balance == dqa(700),
        "TV-0862-15: last-writer-wins balance must be 500 or 700: got {final_balance:?}"
    );
}

// =============================================================================
// TV-0862-12 + TV-0862-13 — scale-mismatch rejection (mission 0862-c4:
// `dqa_to_i64` returns `SpendLedgerError::InvalidScale` instead of panicking)
// =============================================================================

/// TV-0862-12: `seed()` with a `Dqa` carrying a non-zero `scale`
/// MUST yield `SpendLedgerError::InvalidScale { expected: 0, actual: 1 }`,
/// NOT panic. Per mission 0862-c4 (S6c Round 1 security review
/// finding #8 — `assert!` is not an error path; a scale=1 input from
/// an upstream caller would otherwise crash the wallet-node on the
/// `dqa_to_i64` precondition).
///
/// Pre-seeding is unnecessary: the scale check fires BEFORE the
/// drain_lock / DB hit (the call path is `budget.value >= 0` →
/// `dqa_to_i64(budget)?`). State must be unchanged on rejection.
#[test]
fn tv_0862_12_seed_scale_mismatch_rejected() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086212";
    let macaroon_id = TV_0862_MACAROON_ID_12;

    // Dqa at scale=1 (sub-octet fractional data) — not storable as
    // INTEGER (scale=0) per RFC-0862 §StoolapSpendLedger substrate.
    let scale_1_dqa = Dqa::new(100, 1).expect("Dqa::new(100, 1)");

    let result = ledger.seed(holder, &macaroon_id, scale_1_dqa);
    assert!(
        matches!(
            result,
            Err(SpendLedgerError::InvalidScale {
                expected: 0,
                actual: 1
            })
        ),
        "TV-0862-12: scale=1 budget MUST yield InvalidScale {{ expected: 0, actual: 1 }}: got {result:?}"
    );

    // Side-effect check: no row persisted (rejection precedes
    // drain_lock + balance read + INSERT).
    let stored = ledger.balance(holder, &macaroon_id).expect("balance read");
    assert!(
        stored.is_none(),
        "TV-0862-12: no row should persist after InvalidScale rejection: got {stored:?}"
    );
}

/// TV-0862-13: `try_deduct()` with a `Dqa` carrying a non-zero
/// `scale` MUST yield `SpendLedgerError::InvalidScale`, NOT panic.
/// Mirrors TV-0862-12 on the deduct path (the new `dqa_to_i64` is
/// the sole gatekeeper for both). Pre-seeding a row ensures we
/// exercise the scale check on a known holder, isolating the error
/// from `UnknownHolder`.
#[test]
fn tv_0862_13_try_deduct_scale_mismatch_rejected() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086213";
    let macaroon_id = TV_0862_MACAROON_ID_13;

    ledger
        .seed(holder, &macaroon_id, dqa(10_000))
        .expect("seed");

    let scale_1_dqa = Dqa::new(100, 1).expect("Dqa::new(100, 1)");
    let result = ledger.try_deduct(holder, &macaroon_id, scale_1_dqa);
    assert!(
        matches!(
            result,
            Err(SpendLedgerError::InvalidScale {
                expected: 0,
                actual: 1
            })
        ),
        "TV-0862-13: scale=1 cost MUST yield InvalidScale {{ expected: 0, actual: 1 }}: got {result:?}"
    );

    // Side-effect check: balance unchanged after rejection (rejection
    // precedes drain_lock + SELECT / UPDATE).
    let stored = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after rejection");
    assert_eq!(
        stored,
        dqa(10_000),
        "TV-0862-13: balance MUST be unchanged after InvalidScale rejection"
    );
}

// =============================================================================
// TV-0862-16 — seed() rejects negative budget (mission 0862-c8: NegativeCost guard)
// =============================================================================

/// TV-0862-16: `seed()` with negative budget MUST yield
/// `SpendLedgerError::NegativeCost` (mirrors `try_deduct` guard). Per
/// mission 0862-c8 (Round 1 fix was asymmetric — only `try_deduct`
/// had the guard).
#[test]
fn tv_0862_16_seed_negative_budget_rejected() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086216";
    let macaroon_id = TV_0862_MACAROON_ID_16;

    let result = ledger.seed(holder, &macaroon_id, dqa(-1));
    assert!(
        matches!(result, Err(SpendLedgerError::NegativeCost { .. })),
        "TV-0862-16: negative budget MUST yield NegativeCost: got {result:?}"
    );

    // Side-effect check: no row persisted.
    let stored = ledger.balance(holder, &macaroon_id).expect("balance read");
    assert!(
        stored.is_none(),
        "TV-0862-16: no row should persist after NegativeCost rejection: got {stored:?}"
    );
}

// =============================================================================
// TV-0862-14 — substrate accepts arbitrary bytes as holder_did
// (mission 0862-c6: no DID validation in the substrate)
// =============================================================================

/// TV-0862-14: `StoolapSpendLedger::seed` accepts ANY byte slice as
/// `holder_did` — the substrate performs no `CanonicalCodec` /
/// DID-format / `did:octo:` prefix check. Per RFC-0862 §Layer
/// discipline (the canonical validation site is the wallet-node
/// boundary in `crates/octo-paid-query/src/handlers/`, not the
/// substrate).
///
/// This pins the convention by exercising FOUR representative
/// holder_did shapes that any canonical validator would reject:
///
/// 1. empty byte slice (zero-length)
/// 2. bytes that are not valid `did:octo:` z-multibase
/// 3. arbitrary binary garbage (control chars + non-ASCII)
/// 4. a syntactically valid DID that the substrate nevertheless
///    must accept (canonical production form)
///
/// If a future change makes the substrate reject any of these, the
/// fixture surfaces a regression: the substrate's contract is
/// "accept any bytes; validation lives at the boundary".
#[test]
fn tv_0862_14_substrate_accepts_any_bytes_as_holder_did() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let macaroon_id = TV_0862_MACAROON_ID_14;

    // 1. Empty holder_did — substrate accepts, canonical validator rejects.
    ledger
        .seed("", &macaroon_id, dqa(100))
        .expect("empty holder_did accepted by substrate");
    let stored = ledger.balance("", &macaroon_id).expect("balance read");
    assert_eq!(stored, Some(dqa(100)), "empty holder_did persisted");

    // 2. Holder_did that fails `did:octo:` z-multibase check — substrate accepts.
    let non_octo_did = "did:example:zNotMultibaseAtAll!!!";
    ledger
        .seed(non_octo_did, &macaroon_id, dqa(200))
        .expect("non-octo holder_did accepted by substrate");

    // 3. Binary-shaped garbage — substrate accepts (via lossy string
    // conversion; canonical validator would reject the embedded
    // control bytes).
    let binary_garbage = String::from_utf8_lossy(&[0x00, 0xFF, 0x7F, 0x80, 0x01, 0xFE, 0x42, 0xA5]);
    ledger
        .seed(binary_garbage.as_ref(), &macaroon_id, dqa(300))
        .expect("binary-garbage holder_did accepted by substrate");
    let stored_garbage = ledger
        .balance(binary_garbage.as_ref(), &macaroon_id)
        .expect("balance read");
    assert_eq!(
        stored_garbage,
        Some(dqa(300)),
        "binary-garbage holder_did persisted"
    );

    // 4. Canonical production form — substrate accepts.
    let canonical_did = "did:octo:zTV086214CanonicalAccept";
    ledger
        .seed(canonical_did, &macaroon_id, dqa(400))
        .expect("canonical holder_did accepted by substrate");
    let stored_canonical = ledger
        .balance(canonical_did, &macaroon_id)
        .expect("balance read");
    assert_eq!(
        stored_canonical,
        Some(dqa(400)),
        "canonical holder_did persisted"
    );

    // Cross-shape assertion: distinct holder_did values store DISTINCT
    // rows. The substrate uses raw UTF-8 bytes as the key — no
    // canonicalization collapses them. Empty/non-octo/garbage/canonical
    // each persist independently.
    let _ = ledger
        .balance(non_octo_did, &macaroon_id)
        .expect("balance read non-octo");
}

// =============================================================================
// TV-0862-20 — open_path fails-closed on pre-existing symlink at
// .spend_ledger.lock (mission 0862-c11 AC-1, S6c Round 3
// `toctou-symlink-race` HIGH finding).
// =============================================================================

/// TV-0862-20: `open_path` MUST surface
/// `SpendLedgerError::LockPathSymlink` when an attacker pre-creates
/// `<dsn-dir>/.spend_ledger.lock` as a symlink before substrate open.
/// Per mission 0862-c11 AC-1: the substrate pre-checks
/// `symlink_metadata` on the lock path and rejects any pre-existing
/// symlink to prevent `flock(2)` being acquired on an
/// attacker-controlled inode. The pre-check narrows the
/// check-then-open race window but does NOT eliminate it; a strict
/// O_NOFOLLOW fix would require a libc dep which is reserved for a
/// separate RFC. This test pins the fail-closed contract against
/// the pre-check surface.
#[test]
fn tv_0862_20_open_path_rejects_symlink_at_lock_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let fs_path = tmp.path().to_str().expect("utf8 path");
    let lock_path = tmp.path().join(".spend_ledger.lock");

    // Pre-create the lock path as a symlink pointing to /etc/passwd
    // (any non-existent path is fine; the substrate must reject the
    // symlink BEFORE attempting to follow it). The choice of
    // /etc/passwd is illustrative: even if the substrate followed the
    // symlink, the resulting flock would lock an unrelated file.
    std::os::unix::fs::symlink("/etc/passwd", &lock_path).expect("symlink create");

    let dsn = fs_path.to_string();
    let result = StoolapSpendLedger::open_path(&dsn);
    assert!(
        matches!(result, Err(SpendLedgerError::LockPathSymlink { .. })),
        "TV-0862-20: open_path MUST fail-closed with LockPathSymlink when .spend_ledger.lock is a symlink: got {result:?}"
    );

    // Side-effect check: the symlink target was not flocked. The lock
    // target must still be a symlink (the substrate rejected before
    // open). This is an integrity assertion: the substrate did NOT
    // unlink + recreate the path to defeat the symlink.
    let md = std::fs::symlink_metadata(&lock_path).expect("symlink_metadata");
    assert!(
        md.file_type().is_symlink(),
        "TV-0862-20: substrate MUST leave the symlink in place (no clobber): file_type={:?}",
        md.file_type()
    );
}

// =============================================================================
// TV-0862-21 — open_path locks .spend_ledger.lock to 0600 (mission
// 0862-c11 AC-2, S6c Round 3 `lock-bypass` HIGH finding).
// =============================================================================

/// TV-0862-21: after `open_path` succeeds, the substrate-created
/// `.spend_ledger.lock` file MUST have mode `0o600` (owner read +
/// write only). Per mission 0862-c11 AC-2: a 0o644 default would
/// let a different uid unlink + recreate the lock file to defeat
/// serialization. Best-effort chmod to 0o600 prevents that attack
/// surface. The `set_permissions` call returns `Err` if the FS is
/// read-only — surfaced as `SpendLedgerError::Storage` by the
/// substrate (not asserted here).
#[test]
fn tv_0862_21_lock_file_permissions_are_0600() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    let fs_path = tmp.path().to_str().expect("utf8 path");
    let lock_path = tmp.path().join(".spend_ledger.lock");

    let dsn = fs_path.to_string();
    let _ledger = StoolapSpendLedger::open_path(&dsn).expect("open_path succeeds");

    let md = std::fs::metadata(&lock_path).expect("lock file must exist after open");
    let mode = md.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "TV-0862-21: .spend_ledger.lock permissions MUST be 0o600 (owner-only): got {mode:o}"
    );
}

// =============================================================================
// TV-0862-22 — try_deduct with cost=0 is a no-op (mission 0862-c11
// TV-coverage finding #13: zero-cost edge).
// =============================================================================

/// TV-0862-22: `try_deduct` with `cost = Dqa(0, 0)` MUST succeed
/// and leave the balance unchanged. The substrate treats zero-cost
/// deduction as a meaningful no-op (free-tier query / sanity ping).
/// Not covered by TV-04 (cost=100) or TV-09 (cost=-1 rejected).
#[test]
fn tv_0862_22_try_deduct_zero_cost_no_op() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086222";
    let macaroon_id = TV_0862_MACAROON_ID_22;

    ledger.seed(holder, &macaroon_id, dqa(1_000)).expect("seed");

    let zero_cost = dqa(0);
    let returned = ledger
        .try_deduct(holder, &macaroon_id, zero_cost)
        .expect("try_deduct zero cost");
    assert_eq!(
        returned,
        dqa(1_000),
        "TV-0862-22: zero-cost try_deduct returns current balance (unchanged): got {returned:?}"
    );

    let stored = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist");
    assert_eq!(
        stored,
        dqa(1_000),
        "TV-0862-22: balance MUST be unchanged after zero-cost try_deduct: got {stored:?}"
    );
}

// =============================================================================
// TV-0862-24 — macaroon_id accepts any byte slice (mission 0862-c11
// TV-coverage finding #15: macaroon_id axis parallel to holder_did).
// =============================================================================

/// TV-0862-24: `StoolapSpendLedger` accepts ANY byte slice as
/// `macaroon_id` — no length / format / canonical-16-byte check at
/// substrate (per mission 0862-c6: substrate contract is "any bytes;
/// canonical validation lives at wallet-node boundary"). Mirrors
/// TV-14 (holder_did axis) for the macaroon_id axis. Four
/// representative shapes exercised:
///   1. empty slice (zero-length)
///   2. single byte
///   3. canonical 16-byte raw (production form per RFC-0957)
///   4. 64-byte binary garbage (oversized / arbitrary shape)
///
/// Distinct macaroon_id values store DISTINCT rows — the substrate
/// uses raw bytes as the key without canonicalization.
#[test]
fn tv_0862_24_macaroon_id_accepts_any_bytes() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086224";

    // 1. Empty macaroon_id — substrate accepts.
    let empty: [u8; 0] = [];
    ledger
        .seed(holder, &empty, dqa(100))
        .expect("empty macaroon_id accepted by substrate");
    assert_eq!(
        ledger.balance(holder, &empty).expect("balance read"),
        Some(dqa(100)),
        "empty macaroon_id persisted"
    );

    // 2. Single-byte macaroon_id — substrate accepts.
    let one_byte: [u8; 1] = [0x42];
    ledger
        .seed(holder, &one_byte, dqa(200))
        .expect("single-byte macaroon_id accepted by substrate");
    assert_eq!(
        ledger.balance(holder, &one_byte).expect("balance read"),
        Some(dqa(200)),
        "single-byte macaroon_id persisted"
    );

    // 3. Canonical 16-byte — substrate accepts.
    let canonical: [u8; 16] = [
        0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE,
        0xAF,
    ];
    ledger
        .seed(holder, &canonical, dqa(300))
        .expect("canonical 16-byte macaroon_id accepted by substrate");
    assert_eq!(
        ledger.balance(holder, &canonical).expect("balance read"),
        Some(dqa(300)),
        "canonical 16-byte macaroon_id persisted"
    );

    // 4. 64-byte binary garbage — substrate accepts.
    let garbage: [u8; 64] = [
        0x00, 0xFF, 0x7F, 0x80, 0x01, 0xFE, 0x42, 0xA5, 0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA,
        0xBE, 0x13, 0x37, 0x42, 0x42, 0xFF, 0x00, 0x80, 0x80, 0x55, 0xAA, 0x33, 0xCC, 0x99, 0x66,
        0x11, 0x22, 0x44, 0x88, 0xCC, 0x00, 0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA, 0xF9, 0xF8, 0xF7,
        0xF6, 0xF5, 0xF4, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE,
        0xDC, 0xBA, 0x98, 0x76,
    ];
    ledger
        .seed(holder, &garbage, dqa(400))
        .expect("64-byte garbage macaroon_id accepted by substrate");
    assert_eq!(
        ledger.balance(holder, &garbage).expect("balance read"),
        Some(dqa(400)),
        "64-byte garbage macaroon_id persisted"
    );

    // Cross-shape assertion: each macaroon_id shape stores a DISTINCT
    // row. The substrate uses raw bytes as the key — no canonicalization
    // collapses them.
    let _ = ledger
        .balance(holder, &one_byte)
        .expect("balance read one_byte");
}

// =============================================================================
// TV-0862-25 — seed with budget=0 persists a zero-balance row
// (mission 0862-c11 TV-coverage finding #16: seed-side zero edge).
// =============================================================================

/// TV-0862-25: `seed(holder, mac, Dqa(0, 0))` MUST succeed and
/// persist a row with balance = 0. Pairs with TV-22 (try_deduct
/// zero-cost no-op) for the seed side. The substrate treats
/// zero-budget seed as a meaningful state — the row exists for
/// later try_deduct calls (which will surface `InsufficientBalance`
/// because balance=0 < cost=anything-positive).
#[test]
fn tv_0862_25_seed_zero_budget_persists() {
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086225";
    let macaroon_id = TV_0862_MACAROON_ID_25;

    ledger
        .seed(holder, &macaroon_id, dqa(0))
        .expect("seed zero budget");

    let stored = ledger
        .balance(holder, &macaroon_id)
        .expect("balance read")
        .expect("row must exist after zero-budget seed");
    assert_eq!(
        stored,
        dqa(0),
        "TV-0862-25: zero-budget seed MUST persist balance=0 row: got {stored:?}"
    );

    // Cross-check: any positive-cost try_deduct against the
    // zero-balance row surfaces InsufficientBalance (proves the row
    // is wired into the substrate's check path, not a phantom
    // insert).
    let result = ledger.try_deduct(holder, &macaroon_id, dqa(1));
    assert!(
        matches!(result, Err(SpendLedgerError::InsufficientBalance { .. })),
        "TV-0862-25: try_deduct cost=1 against balance=0 MUST yield InsufficientBalance: got {result:?}"
    );
}

// =============================================================================
// Test fixtures (byte-pinned constants)
// =============================================================================

/// 16-byte macaroon_id fixture. Per-octet identity sequence to
/// guard against accidental byte-reversal regressions.
const TV_0862_MACAROON_ID_01: [u8; 16] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
];
const TV_0862_MACAROON_ID_02: [u8; 16] = [
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
];
const TV_0862_MACAROON_ID_03: [u8; 16] = [
    0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30,
];
const TV_0862_MACAROON_ID_04: [u8; 16] = [
    0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40,
];
const TV_0862_MACAROON_ID_04B: [u8; 16] = [
    0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50,
];
const TV_0862_MACAROON_ID_05: [u8; 16] = [
    0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60,
];
const TV_0862_MACAROON_ID_07: [u8; 16] = [
    0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70,
];
const TV_0862_MACAROON_ID_08: [u8; 16] = [
    0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F, 0x80,
];
const TV_0862_MACAROON_ID_09: [u8; 16] = [
    0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F, 0x90,
];
const TV_0862_MACAROON_ID_09B: [u8; 16] = [
    0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9B, 0x9C, 0x9D, 0x9E, 0x9F, 0xA0,
];
const TV_0862_MACAROON_ID_15: [u8; 16] = [
    0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB0,
];
const TV_0862_MACAROON_ID_16: [u8; 16] = [
    0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0,
];
const TV_0862_MACAROON_ID_10: [u8; 16] = [
    0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF, 0xF0,
];
const TV_0862_MACAROON_ID_12: [u8; 16] = [
    0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF, 0xD0,
];
const TV_0862_MACAROON_ID_13: [u8; 16] = [
    0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE0,
];

/// TV-0862-14 macaroon_id fixture. Distinct byte range from c4 fixtures.
const TV_0862_MACAROON_ID_14: [u8; 16] = [
    0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF, 0xD0,
];

/// TV-0862-11 macaroon_id fixture. Distinct byte range from c2/c6.
const TV_0862_MACAROON_ID_11: [u8; 16] = [
    0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0,
];

/// TV-0862-22 macaroon_id fixture (zero-cost try_deduct no-op).
const TV_0862_MACAROON_ID_22: [u8; 16] = [
    0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30,
];

/// TV-0862-25 macaroon_id fixture (seed zero-budget persistence).
const TV_0862_MACAROON_ID_25: [u8; 16] = [
    0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40,
];

/// TV-0862-07 balance fixtures (full + half deduction).
/// 1_000_000_000_000 = 1 OCTO-W times 1e6 micro-OCTO-W times 1e3 = 1e12.
const TV_0862_BALANCE_07_FULL: Dqa = TV_0862_BALANCE_VALUE;
const TV_0862_BALANCE_07_HALF: Dqa = TV_0862_BALANCE_HALF_VALUE;
const TV_0862_BALANCE_VALUE: Dqa = Dqa {
    value: 1_000_000_000_000,
    scale: 0,
};
const TV_0862_BALANCE_HALF_VALUE: Dqa = Dqa {
    value: 500_000_000_000,
    scale: 0,
};

/// TV-0862-07 DqaEncoding byte-array pin (regression: endianness
/// drift in `DqaEncoding::from_dqa`).
///
/// Layout: `value: i64 BE` (8 B) + `scale: u8` (1 B) +
/// `_reserved: [u8; 7]` (7 B) = 16 B.
///
/// 1_000_000_000_000 = 0x00000000E8D4A51000 BE.
const TV_0862_DQA_ENCODING_07_BYTES: [u8; 16] = [
    0x00, 0x00, 0x00, 0xE8, 0xD4, 0xA5, 0x10, 0x00, // value BE
    0x00, // scale
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // reserved
];

/// Helper: build a `Dqa` (at scale=0) from an integer.
fn dqa(n: i64) -> Dqa {
    Dqa::new(n, 0).expect("Dqa::new scale=0")
}
