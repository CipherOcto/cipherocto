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

use quota_router_core::marketplace::escrow::{Escrow, EscrowState};
use quota_router_core::marketplace::orderbook::{MatchPair, Order, OrderBook, Side};
use quota_router_core::marketplace::slashing::{SlashReason, SlashingLedger, SlashingRules};

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
    let mut escrow = Escrow::new(escrow_id, buyer, seller, amount);
    escrow.lock().expect("lock");
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
    tx.escrow.settle().expect("settle");
    assert_eq!(tx.escrow.state, EscrowState::Settled);
    assert!(tx.escrow.is_terminal());
    assert_eq!(tx.escrow.amount_micro_octo_w, 500);
}

#[test]
fn dispute_valid_slashes_seller() {
    let mut tx = setup_match(&sample_did(238), &sample_did(145), "openai/gpt-4", 200, 3);
    let matched = match_and_populate(&mut tx);
    assert_eq!(matched.qty, 3);

    tx.escrow.dispute().expect("dispute");
    tx.escrow.resolve_valid().expect("resolve valid");
    assert_eq!(tx.escrow.state, EscrowState::Slashed);
    assert!(tx.escrow.is_terminal());

    // Provider gets slashed.
    let mut ledger = SlashingLedger::new();
    ledger.register(&sample_did(145), 1_000_000);
    let out = ledger
        .slash(&sample_did(145), SlashReason::ProviderError, 1.0)
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

    tx.escrow.dispute().expect("dispute");
    tx.escrow.resolve_invalid().expect("resolve invalid");
    assert_eq!(tx.escrow.state, EscrowState::Settled);
    assert_eq!(tx.escrow.amount_micro_octo_w, 500);
}

#[test]
fn below_tolerance_miss_rate_does_not_slash() {
    let mut ledger = SlashingLedger::with_rules(SlashingRules {
        miss_rate_tolerance: 0.05,
        ..SlashingRules::default()
    });
    ledger.register(&sample_did(145), 1_000_000);
    let err = ledger
        .slash(&sample_did(145), SlashReason::Timeout, 0.01)
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
    ledger.register(&sample_did(20), 1_000_000);

    // First three offenses with default rules (10%, 15%, 22.5%) leave
    // cumulative ≈ 40.7%. Fourth offense (33.75% of remaining ≈ 30%)
    // crosses 50% → banned.
    for _ in 0..4 {
        let _ = ledger
            .slash(&sample_did(20), SlashReason::ProviderError, 1.0)
            .expect("slash");
    }
    assert!(ledger
        .stake(&sample_did(20))
        .unwrap()
        .is_banned(ledger.rules()));

    // Subsequent slashes rejected.
    let err = ledger
        .slash(&sample_did(20), SlashReason::Timeout, 1.0)
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
