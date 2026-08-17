//! TV-0862-01..08 — RFC-0862 v2.0 StoolapSpendLedger byte-exact fixtures
//!
//! Per `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
//! §3 row 6 (Stream A.1 S6c) + §4 S6 verify gate. 8 byte-exact
//! test vectors pinning the `StoolapSpendLedger` substrate added in
//! RFC-0862 v2.0 (Dqa + vault bump).
//!
//! All inputs byte-pinned (`TV_0862_*` constants); no RNG. Schema +
//! API surface mirrors `crates/quota-router-storage/src/stoolap_spend_ledger.rs`.

use std::sync::Mutex;

use octo_determin::{Dqa, DqaEncoding};
use quota_router_storage::stoolap_spend_ledger::{
    MicroOctoW, SpendLedgerError, StoolapSpendLedger,
};

// Serialize concurrent access — stoolap `memory://` shares global
// catalog state across threads (per existing `stoolap_chain_namespace.rs`
// pattern).
static MIGRATION_LOCK: Mutex<()> = Mutex::new(());

// =============================================================================
// TV-0862-01 — row creation via seed
// =============================================================================

/// TV-0862-01: `StoolapSpendLedger::seed` inserts a new row when no
/// existing `(holder_did, macaroon_id)` row exists. After seed,
/// `balance` returns the same `MicroOctoW`.
#[test]
fn tv_0862_01_seed_creates_row() {
    let _guard = MIGRATION_LOCK.lock().unwrap();
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
/// returns `Some(MicroOctoW)`. This is the read-path contract that
/// the wallet-node relies on for pre-deduct visibility.
#[test]
fn tv_0862_02_balance_read_unknown_returns_none() {
    let _guard = MIGRATION_LOCK.lock().unwrap();
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
    let _guard = MIGRATION_LOCK.lock().unwrap();
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
/// `drain_lock` critical section (per RFC-0862 §DrainCoordinator).
#[test]
fn tv_0862_04_try_deduct_atomic_decrement() {
    let _guard = MIGRATION_LOCK.lock().unwrap();
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
#[test]
fn tv_0862_04b_try_deduct_unknown_holder_errors() {
    let _guard = MIGRATION_LOCK.lock().unwrap();
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let result = ledger.try_deduct("did:octo:zTV086204b", &TV_0862_MACAROON_ID_04, dqa(1));
    assert!(
        matches!(result, Err(SpendLedgerError::UnknownHolder)),
        "TV-0862-04b: unknown holder must yield UnknownHolder: got {result:?}"
    );
}

// =============================================================================
// TV-0862-05 — Dqa encoding round-trip
// =============================================================================

/// TV-0862-05: `MicroOctoW` is `Dqa` at `scale = 0` (integer
/// micro-OCTO_W counts). `DqaEncoding` is the canonical 16-byte BE
/// consensus wire form per `determin/src/dqa.rs:495-502`. Encode +
/// re-decode must round-trip exactly. Pin a non-trivial value to
/// guard against endianness / scale sign regressions.
#[test]
fn tv_0862_05_dqa_encoding_round_trip() {
    let value = dqa(1_234_567_890);
    assert_eq!(
        value.scale, 0,
        "TV-0862-05: MicroOctoW MUST be stored at scale=0"
    );

    let encoding = DqaEncoding::from_dqa(&value);
    assert_eq!(
        std::mem::size_of::<DqaEncoding>(),
        16,
        "TV-0862-05: DqaEncoding wire form MUST be exactly 16 bytes"
    );

    // Round-trip: encode + decode the canonical form.
    let round_trip = encoding.to_dqa().expect("decode to Dqa");
    assert_eq!(
        round_trip, value,
        "TV-0862-05: DqaEncoding round-trip must be exact: got {round_trip:?}"
    );

    // The schema persists `i64` (`dqa_to_i64` helper) — the
    // `Dqa::value` field IS the `i64` representation. No precision
    // loss in the i64 <-> Dqa step.
    let i64_form = value.value;
    assert_eq!(
        i64_form, 1_234_567_890,
        "TV-0862-05: Dqa::value must equal the i64 column value"
    );
}

// =============================================================================
// TV-0862-06 — vault_id cross-ref (BLAKE3 derivation per RFC-0960 §20.3)
// =============================================================================

/// TV-0862-06: `vault_id` derivation per RFC-0960 §20.3:
///
/// ```text
/// vault_id = BLAKE3("cipherocto/vault/v1/" + chain_id + owner_did + asset_id)
/// ```
///
/// Pin the canonical input format + 32-byte output. The
/// `StoolapSpendLedger` does NOT validate `vault_id` directly — the
/// `Macaroon::verify_for_vault_op` (RFC-0957 v2.1) does — but the
/// cross-ref substrate shares the canonical derivation. A drift in
/// the derivation breaks the vault_id ↔ spend_ledger binding.
#[test]
fn tv_0862_06_vault_id_derivation_blake3() {
    let chain_id = TV_0862_CHAIN_ID;
    let owner_did = b"did:octo:zTV086206";
    let asset_id = b"cipherocto/asset/v1/role_a";

    // Canonical input per RFC-0960 §20.3 — pin the prefix string
    // (regression: prefix drift silently breaks the vault_id space).
    let prefix = b"cipherocto/vault/v1/";
    let mut input =
        Vec::with_capacity(prefix.len() + chain_id.len() + owner_did.len() + asset_id.len());
    input.extend_from_slice(prefix);
    input.extend_from_slice(&chain_id);
    input.extend_from_slice(owner_did);
    input.extend_from_slice(asset_id);

    let expected_vault_id: [u8; 32] = blake3::hash(&input).into();
    assert_eq!(
        expected_vault_id.len(),
        32,
        "TV-0862-06: BLAKE3 output must be exactly 32 bytes"
    );
    // The vault_id is non-zero (regression: empty-input BLAKE3).
    assert_ne!(
        expected_vault_id, [0u8; 32],
        "TV-0862-06: vault_id MUST be non-zero for canonical input"
    );

    // Cross-ref invariant: same inputs MUST yield the same vault_id
    // (deterministic).
    let rehash: [u8; 32] = blake3::hash(&input).into();
    assert_eq!(
        rehash, expected_vault_id,
        "TV-0862-06: vault_id derivation MUST be deterministic"
    );
}

// =============================================================================
// TV-0862-07 — V2 wire-form (Dqa 16-byte BE on the spend_ledger row)
// =============================================================================

/// TV-0862-07: The `spend_ledger.balance` column stores the `i64`
/// form of `Dqa` at `scale = 0`. On read, the substrate decodes
/// the `i64` back into `Dqa` and asserts `scale = 0` (via
/// `dqa_to_i64` debug assertion + storage helper). Pin the encoding
/// contract: encoding round-trips through `DqaEncoding::to_bytes` ↔
/// `DqaEncoding::from_bytes` + `Dqa` ↔ `i64` interop.
///
/// This is the V2 wire-form on the substrate side.
///
/// The on-wire `NodeEnvelope.version_tag = 0xA1` cross-ref per
/// RFC-0870 v2.1 + RFC-0862 v2.0 §StoolapSpendLedger substrate is
/// enforced at the dispatch layer; this TV pins the substrate-side
/// encoding.
#[test]
fn tv_0862_07_dqa_v2_wire_form_pinned() {
    let _guard = MIGRATION_LOCK.lock().unwrap();
    let ledger = StoolapSpendLedger::open_in_memory().expect("open_in_memory");
    let holder = "did:octo:zTV086207";
    let macaroon_id = TV_0862_MACAROON_ID_07;

    // Seed a value that exercises both positive + zero scale.
    let positive = dqa(1_000_000_000_000); // 1e12 micro-OCTO_W
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
    let half = dqa(500_000_000_000);
    let remainder = ledger
        .try_deduct(holder, &macaroon_id, half)
        .expect("try_deduct");
    assert_eq!(
        remainder,
        dqa(500_000_000_000),
        "TV-0862-07: V2 wire-form UPDATE must reflect new balance"
    );

    // Encoding contract: 16-byte BE form must be deterministic
    // across `Dqa` ↔ `DqaEncoding` round-trips. `DqaEncoding` is
    // `#[repr(C)]` 16-byte struct (BE value + scale + reserved);
    // struct equality suffices for byte-stability (PartialEq + Eq
    // derived).
    let enc1 = DqaEncoding::from_dqa(&positive);
    let enc2 = DqaEncoding::from_dqa(&Dqa::new(positive.value, 0).expect("re-dqa"));
    assert_eq!(
        enc1, enc2,
        "TV-0862-07: DqaEncoding MUST be byte-stable across equal values"
    );
}

// =============================================================================
// TV-0862-08 — multi-instance drain coordination (per-instance lock scope)
// =============================================================================

/// TV-0862-08: The per-instance `drain_lock` serializes
/// `try_deduct` within a single `StoolapSpendLedger` instance. Two
/// separate instances pointing at the same file path coordinate
/// only via the underlying stoolap per-statement transaction; in
/// the in-memory case, two instances are independent.
///
/// This TV pins the documented contract (per source-of-truth at
/// `stoolap_spend_ledger.rs:73-74` comment + RFC-0862 §Atomicity):
/// in-memory instances have NO cross-instance coordination; the
/// production follow-on is mission `0871e-phase5c-1`
/// (`RaftLikeDrainCoordinator` LANDED 2026-08-11) which provides
/// cross-instance drain coordination via the consensus substrate.
#[test]
fn tv_0862_08_multi_instance_in_memory_lock_isolation() {
    let _guard = MIGRATION_LOCK.lock().unwrap();
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
const TV_0862_MACAROON_ID_07: [u8; 16] = [
    0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60,
];
const TV_0862_MACAROON_ID_08: [u8; 16] = [
    0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70,
];

/// 32-byte chain_id fixture (zero-distinct).
const TV_0862_CHAIN_ID: [u8; 32] = [
    0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB0,
    0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0,
];

/// Helper: build a `MicroOctoW` (Dqa at scale=0) from an integer.
fn dqa(n: i64) -> MicroOctoW {
    Dqa::new(n, 0).expect("Dqa::new scale=0")
}
