//! Inference Task Market integration tests (RFC-0918, Gap 6).
//!
//! Set of end-to-end tests across the task_market module:
//! - constructor + enum test (Task 6.1)
//! - order book wrapping (Task 6.2)
//! - escrow + dispute paths (Task 6.3)
//! - slashing on underperformance (Task 6.4)
//! - full inference task flow (Task 6.5 — acceptance test)

// ---------------------------------------------------------------------------
// Task 6.1 — TaskType enum + TaskSpec constructors
// ---------------------------------------------------------------------------

use quota_router_core::task_market::{
    Dispute, DisputeReason, Evidence, TaskEscrow, TaskMarket, TaskSpec, TaskType,
};

#[test]
fn task_type_inference_constructor_is_distinct() {
    let a = TaskType::Inference;
    let b = TaskType::Embedding;
    assert_ne!(a, b);
}

#[test]
fn task_spec_inference_carries_model_and_max_price() {
    let spec = TaskSpec::new(TaskType::Inference, "openai/gpt-4", 100, 0, 1_000);
    assert_eq!(spec.task_type, TaskType::Inference);
    assert_eq!(spec.model, "openai/gpt-4");
    assert_eq!(spec.max_price, 100);
    assert_eq!(spec.deadline_unix, 0);
    assert_eq!(spec.quantity, 1_000);
}

#[test]
fn task_spec_embedding_carries_task_type() {
    let spec = TaskSpec::new(TaskType::Embedding, "text-embed-3-small", 50, 1_000_000, 1);
    assert_eq!(spec.task_type, TaskType::Embedding);
    assert_eq!(spec.model, "text-embed-3-small");
    assert_eq!(spec.max_price, 50);
    assert_eq!(spec.deadline_unix, 1_000_000);
    assert_eq!(spec.quantity, 1);
}

#[test]
fn task_spec_fine_tune_and_eval_are_distinct() {
    let ft = TaskSpec::new(TaskType::FineTune, "anthropic/claude", 1_000, 0, 1);
    let ev = TaskSpec::new(TaskType::Eval, "anthropic/claude", 1_000, 0, 1);
    assert_eq!(ft.task_type, TaskType::FineTune);
    assert_eq!(ev.task_type, TaskType::Eval);
    assert_ne!(ft.task_type, ev.task_type);
}

#[test]
fn task_market_empty_has_no_orders() {
    let market = TaskMarket::new();
    assert!(market.is_empty());
    assert_eq!(market.bid_count(), 0);
    assert_eq!(market.ask_count(), 0);
}

// ---------------------------------------------------------------------------
// Task 6.2 — order book wrapping (RFC-0918 §Order Book reusing
// marketplace::orderbook::OrderBook<TaskSpec>).
// ---------------------------------------------------------------------------

#[test]
fn task_market_best_ask_is_lowest_price() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "openai/gpt-4", 0, 0, 1);
    m.place_sell(spec.clone(), 200, 1, "seller-a", 1);
    m.place_sell(spec.clone(), 100, 1, "seller-b", 2);
    m.place_sell(spec, 300, 1, "seller-c", 3);

    let best = m.best_ask().expect("best ask");
    assert_eq!(best.price, 100);
    assert_eq!(best.owner, "seller-b");
}

#[test]
fn task_market_best_bid_is_highest_price() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "openai/gpt-4", 0, 0, 1);
    m.place_buy(spec.clone(), 100, 1, "buyer-a", 1);
    m.place_buy(spec.clone(), 200, 1, "buyer-b", 2);
    m.place_buy(spec, 150, 1, "buyer-c", 3);

    let best = m.best_bid().expect("best bid");
    assert_eq!(best.price, 200);
    assert_eq!(best.owner, "buyer-b");
}

#[test]
fn task_market_time_priority_breaks_ties_oldest_first() {
    // Same price → earlier ts wins (price-time priority from Gap 5.1).
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "openai/gpt-4", 0, 0, 1);
    m.place_sell(spec.clone(), 100, 1, "late", 100);
    m.place_sell(spec, 100, 1, "early", 50);

    let best = m.best_ask().expect("best ask");
    assert_eq!(best.owner, "early");
}

#[test]
fn task_market_match_top_at_equal_price() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "gpt-4", 100, 0, 1);
    m.place_buy(spec.clone(), 100, 1, "b", 2);
    m.place_sell(spec, 100, 1, "s", 1);

    let mm = m.match_top().expect("match");
    assert_eq!(mm.price, 100);
    assert_eq!(mm.qty, 1);
    assert!(m.is_empty());
}

#[test]
fn task_market_repeated_matches_drain_book() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "gpt-4", 0, 0, 1);
    // Three crossings (best bid >= best ask) should drain via repeated
    // match_top() calls.
    m.place_buy(spec.clone(), 120, 1, "buyer-1", 10);
    m.place_buy(spec.clone(), 110, 1, "buyer-2", 11);
    m.place_buy(spec.clone(), 100, 1, "buyer-3", 12);
    m.place_sell(spec.clone(), 90, 1, "seller-a", 1);
    m.place_sell(spec.clone(), 100, 1, "seller-b", 2);
    m.place_sell(spec, 110, 1, "seller-c", 3);

    // Sequence of matches. After each match, top-of-book shifts.
    let m1 = m.match_top().expect("match 1");
    assert_eq!(m1.bid.owner, "buyer-1");
    assert_eq!(m1.ask.owner, "seller-a");
    assert_eq!(m1.price, 90);

    // Now top bid = 110, top ask = 100 → cross at 100.
    let m2 = m.match_top().expect("match 2");
    assert_eq!(m2.bid.owner, "buyer-2");
    assert_eq!(m2.ask.owner, "seller-b");
    assert_eq!(m2.price, 100);

    // Now top bid = 100, top ask = 110 → no cross (100 < 110).
    assert!(m.match_top().is_none());
    assert_eq!(m.bid_count(), 1);
    assert_eq!(m.ask_count(), 1);
}

#[test]
fn task_market_best_ask_matching_filters_by_spec() {
    let m = TaskMarket::new();
    let gpt = TaskSpec::new(TaskType::Inference, "openai/gpt-4", 0, 0, 1);
    let claude = TaskSpec::new(TaskType::Inference, "anthropic/claude", 0, 0, 1);
    m.place_sell(gpt.clone(), 100, 1, "gpt-seller", 1);
    m.place_sell(claude, 50, 1, "claude-seller", 2);

    let best = m
        .best_ask_matching(|spec| spec.model == "openai/gpt-4")
        .expect("best gpt");
    assert_eq!(best.price, 100);
    assert_eq!(best.ask.owner, "gpt-seller");
    assert_eq!(best.ask.spec.model, "openai/gpt-4");
}

#[test]
fn task_market_best_ask_matching_returns_none_when_no_match() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "gpt-4", 0, 0, 1);
    m.place_sell(spec, 100, 1, "s", 1);
    let result = m.best_ask_matching(|spec| spec.task_type == TaskType::FineTune);
    assert!(result.is_none());
}

#[test]
fn task_market_match_top_returns_none_when_no_cross() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "gpt-4", 0, 0, 1);
    m.place_buy(spec.clone(), 60, 1, "buyer", 1);
    m.place_sell(spec, 80, 1, "seller", 2);
    assert!(m.match_top().is_none());
    assert_eq!(m.bid_count(), 1);
    assert_eq!(m.ask_count(), 1);
}

// ---------------------------------------------------------------------------
// Task 6.3 — escrow + dispute resolution (RFC-0918 §Escrow Flow).
// ---------------------------------------------------------------------------

use quota_router_core::marketplace::escrow::EscrowState;

#[test]
fn task_escrow_new_starts_pending() {
    let e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    assert_eq!(e.state(), EscrowState::Pending);
    assert_eq!(e.task_id, [0x11; 32]);
    assert_eq!(e.request_id, [0x22; 32]);
    assert!(!e.is_terminal());
}

#[test]
fn task_escrow_happy_path_lock_then_settle() {
    let mut e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    e.lock().expect("lock");
    assert_eq!(e.state(), EscrowState::Locked);
    e.settle().expect("settle");
    assert_eq!(e.state(), EscrowState::Settled);
    assert!(e.is_terminal());
}

#[test]
fn task_escrow_dispute_valid_slashes_seller() {
    let mut e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    e.lock().expect("lock");
    let dispute = Dispute::new(
        [0xaa; 32],
        "did:octo:buyer",
        DisputeReason::ResultMismatch,
        Some(Evidence {
            hash: [0x99; 32],
            description: "result did not match commitment".into(),
        }),
    );
    assert_eq!(dispute.escrow_id, [0xaa; 32]);
    assert_eq!(dispute.raised_by, "did:octo:buyer");
    assert_eq!(dispute.reason, DisputeReason::ResultMismatch);
    assert_eq!(dispute.evidence.expect("evidence").hash, [0x99; 32]);

    e.dispute().expect("dispute");
    assert_eq!(e.state(), EscrowState::Disputed);
    e.resolve_valid().expect("resolve valid");
    assert_eq!(e.state(), EscrowState::Slashed);
    assert!(e.is_terminal());
}

#[test]
fn task_escrow_dispute_invalid_confirms_payment() {
    let mut e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    e.lock().expect("lock");
    e.dispute().expect("dispute");
    e.resolve_invalid().expect("resolve invalid");
    assert_eq!(e.state(), EscrowState::Settled);
    assert!(e.is_terminal());
}

#[test]
fn task_escrow_rejects_lock_from_non_pending() {
    let mut e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    e.lock().expect("lock");
    assert!(e.lock().is_err());
}

#[test]
fn task_escrow_rejects_settle_from_pending() {
    let mut e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    assert!(e.settle().is_err());
}

#[test]
fn task_escrow_rejects_dispute_from_pending() {
    let mut e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    assert!(e.dispute().is_err());
}

#[test]
fn task_escrow_rejects_resolve_from_non_disputed() {
    let mut e = TaskEscrow::new(
        [0xaa; 32],
        [0x11; 32],
        [0x22; 32],
        "did:octo:buyer",
        "did:octo:seller",
        100_000,
    );
    e.lock().expect("lock");
    assert!(e.resolve_valid().is_err());
    assert!(e.resolve_invalid().is_err());
}

#[test]
fn dispute_without_evidence_allowed() {
    let d = Dispute::new(
        [0x01; 32],
        "did:octo:buyer",
        DisputeReason::ProviderTimeout,
        None,
    );
    assert!(d.evidence.is_none());
    assert_eq!(d.reason, DisputeReason::ProviderTimeout);
}

#[test]
fn task_market_match_then_full_escrow_happy_path() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "gpt-4", 100, 0, 1);
    m.place_buy(spec.clone(), 100, 1, "buyer", 1);
    m.place_sell(spec, 90, 1, "seller", 2);

    let matched = m.match_top().expect("match");
    assert_eq!(matched.price, 90);

    let mut escrow = TaskEscrow::new(
        [0x42; 32],
        [0x11; 32],
        [0x22; 32],
        matched.bid.owner.clone(),
        matched.ask.owner.clone(),
        matched.price * matched.qty as u128,
    );
    escrow.lock().expect("lock");
    escrow.settle().expect("settle");
    assert_eq!(escrow.state(), EscrowState::Settled);
    assert_eq!(escrow.base.amount_micro_octo_w, 90);
}

#[test]
fn task_market_match_then_dispute_path() {
    let m = TaskMarket::new();
    let spec = TaskSpec::new(TaskType::Inference, "gpt-4", 200, 0, 1);
    m.place_buy(spec.clone(), 200, 1, "buyer", 1);
    m.place_sell(spec, 150, 1, "seller", 2);

    let matched = m.match_top().expect("match");
    let mut escrow = TaskEscrow::new(
        [0x42; 32],
        [0x11; 32],
        [0x22; 32],
        matched.bid.owner.clone(),
        matched.ask.owner.clone(),
        matched.price * matched.qty as u128,
    );
    escrow.lock().expect("lock");
    escrow.dispute().expect("dispute");
    escrow.resolve_valid().expect("resolve valid");
    assert_eq!(escrow.state(), EscrowState::Slashed);
}

// ---------------------------------------------------------------------------
// Task 6.4 — slashing (RFC-0918 §Slashing Model — reuses Gap 5.3
// SlashingLedger).
// ---------------------------------------------------------------------------

use quota_router_core::marketplace::slashing::SlashReason;
use quota_router_core::task_market::TaskMarketSlashing;

#[test]
fn task_market_slashing_register_then_slash_deducts_stake() {
    let mut slashing = TaskMarketSlashing::new();
    slashing.register("did:octo:worker", 1_000_000);
    let out = slashing
        .slash("did:octo:worker", SlashReason::Timeout, 1.0)
        .expect("slash");
    assert_eq!(out.amount_micro_octo_w, 100_000);
    assert_eq!(out.new_stake_micro_octo_w, 900_000);
    assert!(!out.banned);
}

#[test]
fn task_market_slashing_repeated_offenses_escalate() {
    let mut slashing = TaskMarketSlashing::new();
    slashing.register("did:octo:flaky", 1_000_000);
    let o1 = slashing
        .slash("did:octo:flaky", SlashReason::ProviderError, 1.0)
        .expect("slash 1");
    assert_eq!(o1.amount_micro_octo_w, 100_000);
    let o2 = slashing
        .slash("did:octo:flaky", SlashReason::ProviderError, 1.0)
        .expect("slash 2");
    // 0.10 * 1.5 = 0.15 → 0.15 * 900_000 = 135_000
    assert_eq!(o2.amount_micro_octo_w, 135_000);
    assert!(!o2.banned);
}

#[test]
fn task_market_slashing_eventually_bans_provider() {
    let mut slashing = TaskMarketSlashing::new();
    slashing.register("did:octo:bad", 1_000_000);
    // 4 consecutive offenses at default rules → banned.
    for _ in 0..4 {
        let _ = slashing
            .slash("did:octo:bad", SlashReason::ProviderError, 1.0)
            .expect("slash");
    }
    let err = slashing
        .slash("did:octo:bad", SlashReason::Timeout, 1.0)
        .unwrap_err();
    assert!(matches!(
        err,
        quota_router_core::task_market::TaskSlashError::Slash(
            quota_router_core::marketplace::slashing::SlashError::BannedProvider { .. }
        )
    ));
}

#[test]
fn task_market_slashing_below_tolerance_does_not_slash() {
    let mut slashing =
        TaskMarketSlashing::with_rules(quota_router_core::marketplace::slashing::SlashingRules {
            miss_rate_tolerance: 0.05,
            ..quota_router_core::marketplace::slashing::SlashingRules::default()
        });
    slashing.register("did:octo:ok", 1_000_000);
    let err = slashing
        .slash("did:octo:ok", SlashReason::Timeout, 0.01)
        .unwrap_err();
    assert!(matches!(
        err,
        quota_router_core::task_market::TaskSlashError::Slash(
            quota_router_core::marketplace::slashing::SlashError::BelowTolerance { .. }
        )
    ));
}

#[test]
fn task_market_slashing_unknown_provider_errors() {
    let mut slashing = TaskMarketSlashing::new();
    let err = slashing
        .slash("ghost", SlashReason::Timeout, 1.0)
        .unwrap_err();
    assert!(matches!(
        err,
        quota_router_core::task_market::TaskSlashError::Slash(
            quota_router_core::marketplace::slashing::SlashError::UnknownProvider(_)
        )
    ));
}
