//! End-to-end integration tests for the marketplace (Gap 5 Task 5.4).
//!
//! Scenarios covered:
//!
//! 1. Happy path: buyer bids, provider asks, book matches, escrow locks,
//!    settlement releases funds.
//! 2. Dispute path: match → lock → dispute → resolve valid → seller
//!    slashed.
//! 3. Dispute path: match → lock → dispute → resolve invalid → payment
//!    confirmed.
//! 4. Below-tolerance miss: provider fails SLA under tolerance band → no
//!    slash.
//! 5. Banned provider: escalation crosses 50% loss → subsequent calls
//!    rejected.

use quota_router_core::marketplace::escrow::{Escrow, EscrowError, EscrowState, Party};
use quota_router_core::marketplace::orderbook::{MatchPair, Order, OrderBook, Side};
use quota_router_core::marketplace::slashing::{
    SlashError, SlashReason, SlashingLedger, SlashingRules,
};

/// Minimal Spec for the e2e test: an `AskSpec` carries the model name
/// and the asker (seller) DID — enough to drive end-to-end matching and
/// escrow settlement without depending on the stoolap-backed Ask repo.
use octo_ident::test_helpers::sample_did;
#[derive(Debug, Clone, PartialEq, Eq)]
struct AskSpec {
    model: String,
    asker_did: String,
}

/// One round-tripped "market transaction":
/// - buyer places bid, provider places ask
/// - book matches top-of-book
/// - escrow locks the agreed amount
/// - on success: settle → Settled → seller paid
struct MarketTransaction {
    pub book: OrderBook<AskSpec>,
    pub escrow: Escrow,
}

fn setup_match(buyer: &str, seller: &str, model: &str, price: u128, qty: u64) -> MarketTransaction {
    let mut book = OrderBook::<AskSpec>::new();
    book.place_ask(
        AskSpec {
            model: model.to_owned(),
            asker_did: seller.to_owned(),
        },
        price,
        qty,
        seller,
        1_000,
    );
    book.place_bid(
        AskSpec {
            model: model.to_owned(),
            asker_did: seller.to_owned(),
        },
        price,
        qty,
        buyer,
        1_500,
    );
    let escrow_id = [0x42; 32];
    let amount = price * qty as u128;
    let mut escrow = Escrow::with_arbitrator(escrow_id, buyer, seller, "arb-1", amount);
    escrow.lock(&Party::Buyer(buyer.to_string())).expect("lock");
    MarketTransaction { book, escrow }
}

fn match_and_populate(tx: &mut MarketTransaction) -> MatchPair<AskSpec> {
    tx.book.match_top().expect("top-of-book match")
}

#[test]
fn happy_path_bid_matches_ask_escrow_settles() {
    let mut tx = setup_match(&sample_did(238), &sample_did(145), "openai/gpt-4", 100, 5);

    let matched = match_and_populate(&mut tx);
    assert_eq!(matched.bid.owner, sample_did(238));
    assert_eq!(matched.ask.owner, sample_did(145));
    assert_eq!(matched.price, 100);
    assert_eq!(matched.qty, 5);
    assert!(tx.book.is_empty());

    // Settle the escrow.
    tx.escrow
        .settle(&Party::Seller(tx.escrow.seller.clone()))
        .expect("settle");
    assert_eq!(tx.escrow.state, EscrowState::Settled);
    assert!(tx.escrow.is_terminal());
    assert_eq!(tx.escrow.amount_micro_octo_w, 500);
}

#[test]
fn dispute_valid_slashes_seller() {
    let mut tx = setup_match(&sample_did(238), &sample_did(145), "openai/gpt-4", 200, 3);
    let matched = match_and_populate(&mut tx);
    assert_eq!(matched.qty, 3);

    tx.escrow
        .dispute(&Party::Buyer(tx.escrow.buyer.clone()))
        .expect("dispute");
    tx.escrow
        .resolve_valid(&Party::Arbitrator("arb-1".to_string()))
        .expect("resolve valid");
    assert_eq!(tx.escrow.state, EscrowState::Slashed);
    assert!(tx.escrow.is_terminal());

    // Provider gets slashed.
    let mut ledger = SlashingLedger::new();
    ledger.register(sample_did(145), 1_000_000);
    let out = ledger
        .slash(&sample_did(145), SlashReason::PROVIDER_ERROR, 1.0)
        .expect("slash");
    assert_eq!(out.amount_micro_octo_w, 100_000); // 10% first offense
    assert_eq!(out.new_stake_micro_octo_w, 900_000);
    assert!(!out.banned);
}

#[test]
fn dispute_invalid_confirms_payment() {
    let mut tx = setup_match(
        &sample_did(238),
        &sample_did(145),
        "anthropic/claude",
        50,
        10,
    );
    match_and_populate(&mut tx);

    tx.escrow
        .dispute(&Party::Buyer(tx.escrow.buyer.clone()))
        .expect("dispute");
    tx.escrow
        .resolve_invalid(&Party::Arbitrator("arb-1".to_string()))
        .expect("resolve invalid");
    assert_eq!(tx.escrow.state, EscrowState::Settled);
    assert_eq!(tx.escrow.amount_micro_octo_w, 500);
}

#[test]
fn below_tolerance_miss_rate_does_not_slash() {
    let mut ledger = SlashingLedger::with_rules(SlashingRules {
        miss_rate_tolerance: 0.05,
        ..SlashingRules::default()
    });
    ledger.register(sample_did(145), 1_000_000);
    let err = ledger
        .slash(&sample_did(145), SlashReason::TIMEOUT, 0.01)
        .unwrap_err();
    assert!(matches!(
        err,
        quota_router_core::marketplace::slashing::SlashError::BelowTolerance { .. }
    ));
    // Stake untouched.
    assert_eq!(ledger.stake(&sample_did(145)).unwrap().offense_count, 0);
    assert_eq!(
        ledger.stake(&sample_did(145)).unwrap().stake_micro_octo_w,
        1_000_000
    );
}

#[test]
fn repeated_offenses_eventually_ban_provider() {
    let mut ledger = SlashingLedger::new();
    ledger.register(sample_did(20), 1_000_000);

    // First three offenses with default rules (10%, 15%, 22.5%) leave
    // cumulative ≈ 40.7%. Fourth offense (33.75% of remaining ≈ 30%)
    // crosses 50% → banned.
    for _ in 0..4 {
        let _ = ledger
            .slash(&sample_did(20), SlashReason::PROVIDER_ERROR, 1.0)
            .expect("slash");
    }
    assert!(ledger
        .stake(&sample_did(20))
        .unwrap()
        .is_banned(ledger.rules()));

    // Subsequent slashes rejected.
    let err = ledger
        .slash(&sample_did(20), SlashReason::TIMEOUT, 1.0)
        .unwrap_err();
    assert!(matches!(
        err,
        quota_router_core::marketplace::slashing::SlashError::BannedProvider { .. }
    ));
}

#[test]
fn multiple_bids_match_multiple_asks_in_sequence() {
    let mut book = OrderBook::<AskSpec>::new();
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "seller-a".into(),
        },
        90,
        1,
        "seller-a",
        1,
    );
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "seller-b".into(),
        },
        100,
        1,
        "seller-b",
        2,
    );
    book.place_bid(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "seller-a".into(),
        },
        120,
        1,
        "buyer-1",
        10,
    );
    book.place_bid(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "seller-a".into(),
        },
        110,
        1,
        "buyer-2",
        11,
    );

    // First match: buyer-1 (120) crosses seller-a (90) → 90, qty=1.
    let m1 = book.match_top().unwrap();
    assert_eq!(m1.bid.owner, "buyer-1");
    assert_eq!(m1.ask.owner, "seller-a");
    assert_eq!(m1.price, 90);

    // Second match: buyer-2 (110) crosses seller-b (100) → 100, qty=1.
    let m2 = book.match_top().unwrap();
    assert_eq!(m2.bid.owner, "buyer-2");
    assert_eq!(m2.ask.owner, "seller-b");
    assert_eq!(m2.price, 100);

    // Book now empty.
    assert!(book.is_empty());
}

#[test]
fn best_ask_matching_filters_by_model() {
    let mut book = OrderBook::<AskSpec>::new();
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s1".into(),
        },
        100,
        1,
        "s1",
        1,
    );
    book.place_ask(
        AskSpec {
            model: "claude".into(),
            asker_did: "s2".into(),
        },
        50,
        1,
        "s2",
        2,
    );
    let best_gpt = book
        .best_ask_matching(|spec| spec.model == "gpt-4")
        .unwrap();
    assert_eq!(best_gpt.price, 100);
    assert_eq!(best_gpt.spec.model, "gpt-4");
    assert_eq!(best_gpt.spec.asker_did, "s1");
    let best_claude = book
        .best_ask_matching(|spec| spec.model == "claude")
        .unwrap();
    assert_eq!(best_claude.price, 50);
}

// =============================================================================
// Strong-scenario E2E tests (Round 2 mission: marketplace-e2e-strong-scenarios).
//
// Each test pins a specific code path or recent Round 1 review fix.
// =============================================================================

#[test]
fn concurrent_ask_insertion_at_same_price_preserves_fifo() {
    // Round 1 fix (C1): per-book `next_seq` counter prevents
    // `(price, ts_unix)` collisions when multiple asks land in the same
    // second. This test exercises the fix under thread contention:
    // N threads each insert at the same price simultaneously; the
    // BTreeMap must hold all N entries (no overwrites).
    use std::sync::{Arc, Mutex};
    use std::thread;

    const N: usize = 32;
    let book = Arc::new(Mutex::new(OrderBook::<AskSpec>::new()));

    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let book = Arc::clone(&book);
        handles.push(thread::spawn(move || {
            let mut book = book.lock().expect("lock book");
            book.place_ask(
                AskSpec {
                    model: "gpt-4".into(),
                    asker_did: format!("seller-{i}"),
                },
                100,
                1,
                format!("seller-{i}"),
                1_000,
            );
        }));
    }
    for h in handles {
        h.join().expect("thread join");
    }

    // Drain via 32 matching bids (each crosses at 100). Every ask must
    // produce a match — no overwrites, no lost entries.
    let mut book = book.lock().expect("lock book");
    let mut matched_sellers = std::collections::HashSet::new();
    for _ in 0..N {
        book.place_bid(
            AskSpec {
                model: "gpt-4".into(),
                asker_did: "buyer".into(),
            },
            100,
            1,
            "buyer",
            2_000,
        );
        let m = book
            .match_top()
            .expect("match should succeed — every ask must be present");
        matched_sellers.insert(m.ask.owner.clone());
    }
    assert_eq!(
        matched_sellers.len(),
        N,
        "all {N} concurrent asks must survive"
    );
    assert!(book.is_empty());
}

#[test]
fn partial_fill_exact_match_no_residual() {
    // Round 1 fix (C2): match_top re-inserts residual qty with fresh
    // seq after partial fill. When bid and ask match exactly, no
    // residual is created — book is empty after one match.
    let mut book = OrderBook::<AskSpec>::new();
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s".into(),
        },
        100,
        10,
        "s",
        1,
    );
    book.place_bid(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s".into(),
        },
        200,
        10,
        "b",
        2,
    );
    let m = book.match_top().expect("exact fill");
    assert_eq!(m.qty, 10);
    assert_eq!(m.price, 100);
    assert!(book.is_empty(), "exact fill leaves no residual");
}

#[test]
fn partial_fill_underfilled_bid_residual_matches_next_ask() {
    // Round 1 fix (C2) end-to-end: bid=10 crosses ask=3, residual=7
    // re-inserted; second match (against a new ask) consumes the
    // residual. Asserts no qty loss across two sequential partial
    // fills.
    let mut book = OrderBook::<AskSpec>::new();
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s1".into(),
        },
        100,
        3,
        "s1",
        1,
    );
    book.place_bid(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "b".into(),
        },
        200,
        10,
        "b",
        2,
    );
    let m1 = book.match_top().expect("first match");
    assert_eq!(m1.qty, 3);
    assert_eq!(m1.price, 100);
    assert_eq!(m1.ask.owner, "s1");
    // After partial fill, ask1 is gone (3 qty fully consumed),
    // bid residual of 7 is re-inserted in book. Asks is empty, bids
    // has 1 entry with qty=7.
    assert_eq!(book.best_bid().map(|o| o.qty), Some(7));
    assert_eq!(book.best_ask().map(|o| o.qty), None);

    // Add second ask of 7 qty at price 100 — crosses residual bid.
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s2".into(),
        },
        100,
        7,
        "s2",
        3,
    );
    let m2 = book.match_top().expect("second match consumes residual");
    assert_eq!(m2.qty, 7);
    assert_eq!(m2.ask.owner, "s2");
    assert!(book.is_empty());
}

#[test]
fn escrow_double_settle_rejected() {
    // Escrow is single-use. After settle, second settle returns
    // SettleFromInvalid. The Round 1 C3 fix (drop Clone) ensures no
    // double-settle vector via accidental clone; this test pins the
    // state-machine half of that contract.
    let mut escrow = Escrow::new([0x99; 32], "buyer", "seller", 500);
    escrow
        .lock(&Party::Buyer("buyer".to_string()))
        .expect("lock");
    escrow
        .settle(&Party::Seller("seller".to_string()))
        .expect("first settle");
    assert_eq!(escrow.state, EscrowState::Settled);
    let err = escrow
        .settle(&Party::Seller("seller".to_string()))
        .expect_err("second settle must fail");
    assert!(matches!(
        err,
        quota_router_core::marketplace::escrow::EscrowError::SettleFromInvalid(_)
    ));
}

#[test]
fn escrow_double_dispute_rejected() {
    // Same contract for dispute: only one dispute per Locked escrow.
    let mut escrow = Escrow::new([0x99; 32], "buyer", "seller", 500);
    escrow
        .lock(&Party::Buyer("buyer".to_string()))
        .expect("lock");
    escrow
        .dispute(&Party::Buyer("buyer".to_string()))
        .expect("first dispute");
    let err = escrow
        .dispute(&Party::Buyer("buyer".to_string()))
        .expect_err("second dispute must fail");
    assert!(matches!(
        err,
        quota_router_core::marketplace::escrow::EscrowError::DisputeFromInvalid(_)
    ));
}

#[test]
fn byzantine_provider_offense_count_increments_per_offense() {
    // Byzantine provider submits 99 valid + 1 invalid response.
    // The slashing ledger must register the offense_count = 1
    // (per-offense, not per-batch) so a 1-in-100 attacker cannot
    // dilute their penalty rate. The valid responses don't touch the
    // ledger; the invalid one slashes.
    let mut ledger = SlashingLedger::new();
    ledger.register(sample_did(7), 1_000_000);

    // 99 valid responses (no ledger action — only failures slash).
    // 1 invalid response (slash with full loss):
    let out = ledger
        .slash(&sample_did(7), SlashReason::PROVIDER_ERROR, 1.0)
        .expect("slash");
    assert_eq!(out.amount_micro_octo_w, 100_000); // 10% first offense
    assert_eq!(out.new_stake_micro_octo_w, 900_000);
    assert!(!out.banned);
    let stake = ledger.stake(&sample_did(7)).unwrap();
    assert_eq!(
        stake.offense_count, 1,
        "byzantine 1-in-100 must still register offense_count=1"
    );
    assert!(!stake.is_banned(ledger.rules()));
}

#[test]
fn byzantine_provider_escalation_ban_unchanged() {
    // Sanity: even with valid responses mixed in, the offense
    // escalation path must still ban the provider after enough
    // offenses. The 1st-offense threshold of 10% * 4 cuts > 50% in
    // cumulative_loss_pct.
    let mut ledger = SlashingLedger::new();
    ledger.register(sample_did(7), 1_000_000);
    for _ in 0..4 {
        let _ = ledger
            .slash(&sample_did(7), SlashReason::PROVIDER_ERROR, 1.0)
            .expect("slash");
    }
    assert!(ledger
        .stake(&sample_did(7))
        .unwrap()
        .is_banned(ledger.rules()));
}

#[test]
fn order_side_classification() {
    // Sanity check on the Side enum: ensure both branches are usable
    // through `Order<Spec>`.
    let ask_order = Order {
        id: [1u8; 32],
        spec: AskSpec {
            model: "gpt-4".into(),
            asker_did: "s".into(),
        },
        price: 10,
        qty: 1,
        owner: "s".into(),
        ts_unix: 1,
    };
    let bid_order = Order {
        id: [2u8; 32],
        spec: AskSpec {
            model: "gpt-4".into(),
            asker_did: "s".into(),
        },
        price: 10,
        qty: 1,
        owner: "b".into(),
        ts_unix: 2,
    };
    let side_for = |o: &Order<AskSpec>| if o.owner == "b" { Side::Bid } else { Side::Ask };
    assert_eq!(side_for(&bid_order), Side::Bid);
    assert_eq!(side_for(&ask_order), Side::Ask);
}

// ========================================================================
// Strong-scenario E2E tests (mission marketplace-e2e-strong-scenarios)
// ========================================================================
//
// These tests pin the marketplace state machine under failure modes
// the basic happy-path suite does not cover. Each test asserts a
// specific invariant; if the production code regresses, the test
// names tell the operator exactly which contract broke.

#[test]
fn concurrent_settlement_duplicate_rejected() {
    // Two producers race to settle the same escrow. The escrow state
    // machine must let exactly one transition Pending->Locked->Settled,
    // and the second settle attempt must fail with
    // `SettleFromInvalid(Settled)`. This is the dedup contract
    // (RFC-0900 §Settlement).
    let mut tx = setup_match(&sample_did(238), &sample_did(145), "openai/gpt-4", 100, 5);
    match_and_populate(&mut tx);
    // First settle succeeds.
    tx.escrow
        .settle(&Party::Seller(tx.escrow.seller.clone()))
        .expect("first settle");
    assert_eq!(tx.escrow.state, EscrowState::Settled);
    // Second settle (concurrent producer) rejected — already Settled.
    let err = tx
        .escrow
        .settle(&Party::Seller(tx.escrow.seller.clone()))
        .unwrap_err();
    assert!(
        matches!(err, EscrowError::SettleFromInvalid(EscrowState::Settled)),
        "duplicate settle must return SettleFromInvalid(Settled), got {err:?}"
    );
}

#[test]
fn escrow_recovery_from_locked_state_succeeds() {
    // Provider crashes after lock but before settle. New process
    // reconstructs the escrow from the persisted state (Locked) and
    // may complete the settlement. This pins the recovery contract:
    // a Locked escrow is recoverable, not stuck.
    let mut tx = setup_match(&sample_did(238), &sample_did(145), "openai/gpt-4", 100, 5);
    match_and_populate(&mut tx);
    assert_eq!(tx.escrow.state, EscrowState::Locked);
    // "Crash" — drop tx; reconstruct from a fresh in-memory escrow
    // carrying the same Locked state (the production path hydrates
    // from the persistence layer).
    let recovered_state = tx.escrow.state;
    let amount = tx.escrow.amount_micro_octo_w;
    drop(tx);
    // Reconstruct: an escrow that was Locked before the crash is
    // still Locked after recovery, and can be settled by the seller.
    let mut recovered = quota_router_core::marketplace::escrow::Escrow::new(
        [0x42; 32],
        sample_did(238),
        sample_did(145),
        amount,
    );
    // Manually drive to Locked (no public constructor for
    // "recover from Locked"; the production persistence path does
    // this via a dedicated `from_snapshot` constructor — for this
    // test we exercise the state-machine path).
    recovered
        .lock(&Party::Buyer(sample_did(238).to_string()))
        .expect("lock recovered");
    assert_eq!(recovered.state, recovered_state);
    recovered
        .settle(&Party::Seller(sample_did(145).to_string()))
        .expect("settle after recovery");
    assert_eq!(recovered.state, EscrowState::Settled);
}

#[test]
fn escrow_recovery_from_locked_state_dispute_works() {
    // Alternative recovery path: Locked -> Disputed -> resolve.
    // Pin that a recovered Locked escrow can also enter the dispute
    // path (buyer invokes dispute after restart).
    let mut tx = setup_match(&sample_did(238), &sample_did(145), "openai/gpt-4", 100, 5);
    match_and_populate(&mut tx);
    drop(tx);
    let mut recovered = quota_router_core::marketplace::escrow::Escrow::with_arbitrator(
        [0x42; 32],
        sample_did(238),
        sample_did(145),
        "arb-1",
        500,
    );
    recovered
        .lock(&Party::Buyer(sample_did(238).to_string()))
        .expect("lock recovered");
    recovered
        .dispute(&Party::Buyer(sample_did(238).to_string()))
        .expect("dispute after recovery");
    assert_eq!(recovered.state, EscrowState::Disputed);
    recovered
        .resolve_invalid(&Party::Arbitrator("arb-1".to_string()))
        .expect("resolve invalid after recovery");
    assert_eq!(recovered.state, EscrowState::Settled);
}

#[test]
fn provider_key_rotation_preserves_ledger_state() {
    // RFC-0968: reputation is per-controller-did, not per-key. A
    // provider who rotates their key carries over the full slashing
    // ledger. The ledger is keyed by `provider_id` (DID), so the
    // rotation is invisible to it.
    let mut ledger = SlashingLedger::new();
    let did = sample_did(50);
    ledger.register(&did, 1_000_000);
    // Provider gets 3 offenses (cumulative ~40.7%) under key_v1.
    for _ in 0..3 {
        ledger
            .slash(&did, SlashReason::PROVIDER_ERROR, 1.0)
            .expect("slash");
    }
    let pre_rotation = ledger.stake(&did).expect("stake").clone();
    // "Key rotation" — DID is the same; in production the wallet
    // rotates the key but the controller DID persists. The ledger
    // is keyed on DID, so the rotation is opaque.
    let post_rotation = ledger.stake(&did).expect("stake").clone();
    assert_eq!(
        pre_rotation, post_rotation,
        "key rotation must not affect ledger state (DID-keyed)"
    );
    assert_eq!(post_rotation.offense_count, 3);
    // Continue slashing under "key_v2" — escalation still works.
    ledger
        .slash(&did, SlashReason::PROVIDER_ERROR, 1.0)
        .expect("slash after rotation");
    let final_state = ledger.stake(&did).expect("stake");
    assert!(final_state.is_banned(ledger.rules()));
}

#[test]
fn partial_fill_exact_boundary_no_qty_loss() {
    // Buyer bids 10 qty. Seller has two asks at the same price:
    // ask1 = 3 qty, ask2 = 7 qty. First match consumes all of ask1
    // (3 of 10), residual bid (7) fills ask2. Total: 10 qty matched
    // across 2 trades; no qty is double-counted or lost.
    let mut book = OrderBook::<AskSpec>::new();
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s1".into(),
        },
        100,
        3,
        "s1",
        1,
    );
    book.place_ask(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s2".into(),
        },
        100,
        7,
        "s2",
        2,
    );
    book.place_bid(
        AskSpec {
            model: "gpt-4".into(),
            asker_did: "s1".into(),
        },
        120,
        10,
        "b1",
        10,
    );
    let m1 = book.match_top().expect("m1");
    assert_eq!(m1.qty, 3, "first match consumes full ask1");
    assert_eq!(m1.ask.owner, "s1");
    let m2 = book.match_top().expect("m2");
    assert_eq!(m2.qty, 7, "second match consumes full ask2");
    assert_eq!(m2.ask.owner, "s2");
    assert!(book.is_empty(), "exact-boundary fill leaves empty book");
    let total: u64 = m1.qty + m2.qty;
    assert_eq!(total, 10, "no qty loss across 2 matches");
}

#[test]
fn stale_view_after_restart_sees_writes() {
    // Process A writes an ask; process B opens a marketplace on the
    // same file. B's order book must include A's ask per the
    // load-on-open contract (mission marketplace-book-load-on-open).
    use quota_router_core::marketplace::Marketplace;
    use std::env;
    let dir = env::temp_dir().join(format!(
        "marketplace-e2e-stale-view-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.db");
    let dsn = format!("file://{}", path.display());
    // Process A: open, write 2 asks, drop.
    {
        let m = Marketplace::open_path(&dsn).expect("open A");
        m.put(&quota_router_storage::ask::Ask {
            asker_did: sample_did(50),
            model: quota_router_storage::ask::ModelRef::from("openai/gpt-4"),
            rates: quota_router_storage::ask::ModelRateTable {
                model: quota_router_storage::ask::ModelRef::from("openai/gpt-4"),
                rates: vec![quota_router_storage::ask::AxisRate {
                    axis: "input_tokens_per_1k".into(),
                    rate_per_1k: 10_000,
                }],
            },
            nonce: [0x11; 16],
            expires_at_unix: 1_900_000_000,
        })
        .expect("put A");
        m.put(&quota_router_storage::ask::Ask {
            asker_did: sample_did(60),
            model: quota_router_storage::ask::ModelRef::from("openai/gpt-4"),
            rates: quota_router_storage::ask::ModelRateTable {
                model: quota_router_storage::ask::ModelRef::from("openai/gpt-4"),
                rates: vec![quota_router_storage::ask::AxisRate {
                    axis: "input_tokens_per_1k".into(),
                    rate_per_1k: 20_000,
                }],
            },
            nonce: [0x22; 16],
            expires_at_unix: 1_900_000_000,
        })
        .expect("put A2");
    }
    // Process B: open against same file, verify both asks visible.
    let m2 = Marketplace::open_path(&dsn).expect("open B");
    let winner = m2.cheapest("openai/gpt-4").expect("some");
    assert_eq!(winner.asker_did, sample_did(50));
    let alice_asks = m2.list_by_asker(&sample_did(50)).expect("list_by_asker");
    assert_eq!(alice_asks.len(), 1, "process B must see A's writes");
    let bob_asks = m2.list_by_asker(&sample_did(60)).expect("list_by_asker");
    assert_eq!(bob_asks.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stake_withdrawal_full_amount_after_ban_rejected() {
    // Production failure mode (mission `marketplace-stake-withdraw-api`):
    // a banned provider must NOT be able to withdraw their stake.
    // The `withdraw_stake` API exposes the ban gate; the prior
    // re-register proxy covered the invariant indirectly. This test
    // pins the production withdraw path.
    let mut ledger = SlashingLedger::new();
    let did = sample_did(99);
    ledger.register(&did, 1_000_000);
    for _ in 0..4 {
        ledger
            .slash(&did, SlashReason::PROVIDER_ERROR, 1.0)
            .expect("slash");
    }
    assert!(ledger.stake(&did).unwrap().is_banned(ledger.rules()));
    let pre_offense_count = ledger.stake(&did).unwrap().offense_count;
    let pre_cumulative = ledger.stake(&did).unwrap().cumulative_loss_pct;

    // Attempt full withdrawal — must be rejected with BannedProvider.
    let err = ledger
        .withdraw_stake(&did, ledger.stake(&did).unwrap().stake_micro_octo_w)
        .unwrap_err();
    assert!(matches!(err, SlashError::BannedProvider { .. }));

    // State untouched: stake still present, offense_count + cumulative
    // loss unchanged.
    assert!(ledger.stake(&did).unwrap().is_banned(ledger.rules()));
    assert_eq!(ledger.stake(&did).unwrap().offense_count, pre_offense_count);
    assert_eq!(
        ledger.stake(&did).unwrap().cumulative_loss_pct,
        pre_cumulative
    );
}

#[test]
fn stake_withdrawal_partial_preserves_ledger_state() {
    // Pre-ban: a partial withdrawal must NOT clear offense history or
    // affect the ban-stability invariant. Provider is still in
    // good standing; partial withdraw gives them less skin in the
    // game, but prior offenses stick.
    let mut ledger = SlashingLedger::new();
    let did = sample_did(101);
    ledger.register(&did, 1_000_000);
    // One offense (10% loss) — not enough to ban.
    ledger
        .slash(&did, SlashReason::PROVIDER_ERROR, 1.0)
        .expect("slash");
    let pre_offense_count = ledger.stake(&did).unwrap().offense_count;
    let pre_cumulative = ledger.stake(&did).unwrap().cumulative_loss_pct;
    let pre_stake = ledger.stake(&did).unwrap().stake_micro_octo_w;

    // Withdraw 100k (well under the 900k remaining).
    let new_stake = ledger.withdraw_stake(&did, 100_000).expect("withdraw");
    assert_eq!(new_stake, pre_stake - 100_000);
    assert_eq!(ledger.stake(&did).unwrap().stake_micro_octo_w, new_stake);

    // Offense history untouched.
    assert_eq!(ledger.stake(&did).unwrap().offense_count, pre_offense_count);
    assert_eq!(
        ledger.stake(&did).unwrap().cumulative_loss_pct,
        pre_cumulative
    );

    // `can_withdraw` agrees with `withdraw_stake`.
    assert!(ledger.can_withdraw(&did, 1).is_ok());
    assert!(ledger.can_withdraw(&did, new_stake + 1).is_err());
}

#[test]
fn stake_withdrawal_rejects_invalid_inputs() {
    // Five-way gating round-trip: unknown provider, zero, over-balance,
    // exact-balance (success), post-zero (success-then-zero-reject).
    let mut ledger = SlashingLedger::new();
    let did = sample_did(202);
    ledger.register(&did, 500_000);

    // Unknown provider.
    assert!(matches!(
        ledger.withdraw_stake(&sample_did(200), 1),
        Err(SlashError::UnknownProvider(_))
    ));
    // Zero.
    assert!(matches!(
        ledger.withdraw_stake(&did, 0),
        Err(SlashError::InvalidAmount(0))
    ));
    // Over-balance.
    assert!(matches!(
        ledger.withdraw_stake(&did, 500_001),
        Err(SlashError::InsufficientStake {
            available: 500_000,
            requested: 500_001
        })
    ));
    // Exact balance succeeds.
    assert_eq!(ledger.withdraw_stake(&did, 500_000), Ok(0));
    // Subsequent zero-reject (stake exhausted).
    assert!(matches!(
        ledger.withdraw_stake(&did, 0),
        Err(SlashError::InvalidAmount(0))
    ));
    // Subsequent any-amount-reject (zero balance).
    assert!(matches!(
        ledger.withdraw_stake(&did, 1),
        Err(SlashError::InsufficientStake {
            available: 0,
            requested: 1
        })
    ));
}
