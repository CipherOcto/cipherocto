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
//! - TV-12: scale-mismatch rejection on `seed` (mission 0862-c4:
//!   panic→typed-error for `dqa_to_i64`)
//! - TV-13: scale-mismatch rejection on `try_deduct` (mission 0862-c4)
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

    let budget = dqa(1_000_000); // 1 OCTO_W at scale=0
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
/// micro-OCTO_W counts). `DqaEncoding` is the canonical 16-byte BE
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
    let positive = TV_0862_BALANCE_07_FULL; // 1e12 micro-OCTO_W
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
const TV_0862_MACAROON_ID_12: [u8; 16] = [
    0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF, 0xD0,
];
const TV_0862_MACAROON_ID_13: [u8; 16] = [
    0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE0,
];

/// TV-0862-07 balance fixtures (full + half deduction).
/// 1_000_000_000_000 = 1 OCTO_W times 1e6 micro-OCTO_W times 1e3 = 1e12.
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
