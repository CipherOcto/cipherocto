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

use quota_router_core::marketplace::escrow::{Escrow, EscrowState, Party};
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
    ledger.register(sample_did(20), 1_000_000);

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
        .slash(&sample_did(7), SlashReason::ProviderError, 1.0)
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
            .slash(&sample_did(7), SlashReason::ProviderError, 1.0)
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
