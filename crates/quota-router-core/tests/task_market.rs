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

use quota_router_core::task_market::{TaskMarket, TaskSpec, TaskType};

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
