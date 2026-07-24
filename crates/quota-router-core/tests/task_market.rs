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
