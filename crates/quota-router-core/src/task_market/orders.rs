//! Task orders — `TaskType`, `TaskSpec`, and `TaskMarket` (RFC-0918).
//!
//! `TaskMarket` wraps a generic `OrderBook<TaskSpec>` from
//! `marketplace::orderbook` (Gap 5.1). All match/lookup semantics come
//! from that order book; this module adds the inference-task domain
//! types and ergonomic accessors.

use serde::{Deserialize, Serialize};

use crate::marketplace::orderbook::{MatchPair, OrderBook, OrderId};

/// Inference task categories per RFC-0918 §Task Types (simplified —
/// unit variants per Gap 6 plan).
///
/// The full RFC sketches richer payloads (`InferenceTask`, `ProofTask`,
/// `VerificationTask`, `AggregationTask`); Gap 6 ships the four-variant
/// spine that the order book and escrow layer can reason about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Standard inference request (LLM completion, embedding-free).
    Inference,
    /// Embedding generation (vector output).
    Embedding,
    /// Fine-tuning job (model mutation).
    FineTune,
    /// Evaluation / benchmark run.
    Eval,
}

/// The orderable payload for the inference task market.
///
/// One `TaskSpec` per bid/ask. `max_price` is the price the buyer is
/// willing to pay (for buy orders) or the price the seller is asking
/// (for sell orders); note that the order book stores the actual order
/// price separately, so `max_price` on the spec is informational — it
/// acts as a sanity guard when matching to enforce the buyer's cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSpec {
    pub task_type: TaskType,
    pub model: String,
    pub max_price: u128,
    pub deadline_unix: u64,
    pub quantity: u64,
}

impl TaskSpec {
    /// Construct a task spec. `quantity` is the number of tasks the
    /// order represents (RFC-0918 §Order Book: `OrderType::Buy.quantity`
    /// / `OrderType::Sell.capacity`).
    #[must_use]
    pub fn new(
        task_type: TaskType,
        model: impl Into<String>,
        max_price: u128,
        deadline_unix: u64,
        quantity: u64,
    ) -> Self {
        Self {
            task_type,
            model: model.into(),
            max_price,
            deadline_unix,
            quantity,
        }
    }

    /// True if this spec is an `Inference` task.
    #[must_use]
    pub fn is_inference(&self) -> bool {
        matches!(self.task_type, TaskType::Inference)
    }
}

/// Inference task market — wraps `OrderBook<TaskSpec>` (RFC-0900 §Order Book).
///
/// The book itself is in-memory, so no I/O. `parking_lot::Mutex` keeps
/// access exclusive across threads; the lock is held only for the
/// duration of a single mutating call so concurrent match/place
/// requests serialize cleanly.
#[derive(Debug, Default)]
pub struct TaskMarket {
    book: parking_lot::Mutex<OrderBook<TaskSpec>>,
}

impl TaskMarket {
    /// Empty market.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True if both bids and asks are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.book.lock().is_empty()
    }

    /// Number of resting bids.
    #[must_use]
    pub fn bid_count(&self) -> usize {
        self.book.lock().bid_count()
    }

    /// Number of resting asks.
    #[must_use]
    pub fn ask_count(&self) -> usize {
        self.book.lock().ask_count()
    }

    /// Place a buy order (requester) at `max_price` for `qty` of `spec`.
    /// Returns the order id.
    pub fn place_buy(
        &self,
        spec: TaskSpec,
        max_price: u128,
        qty: u64,
        owner: impl Into<String>,
        ts_unix: u64,
    ) -> OrderId {
        let mut book = self.book.lock();
        book.place_bid(spec, max_price, qty, owner, ts_unix)
    }

    /// Place a sell order (worker) at `price` for `qty` of `spec`.
    pub fn place_sell(
        &self,
        spec: TaskSpec,
        price: u128,
        qty: u64,
        owner: impl Into<String>,
        ts_unix: u64,
    ) -> OrderId {
        let mut book = self.book.lock();
        book.place_ask(spec, price, qty, owner, ts_unix)
    }

    /// Match top-of-book bid with top-of-book ask if they cross
    /// (`bid.price >= ask.price`). Both sides removed on match.
    pub fn match_top(&self) -> Option<MatchPair<TaskSpec>> {
        self.book.lock().match_top()
    }

    /// Best ask whose spec satisfies `spec_pred`.
    pub fn best_ask_matching<P: Fn(&TaskSpec) -> bool>(
        &self,
        spec_pred: P,
    ) -> Option<MatchPair<TaskSpec>> {
        let book = self.book.lock();
        // The OrderBook::best_ask_matching returns &&Order; we surface
        // an Owned version via MatchPair cloning a single-order shape.
        let order = book.best_ask_matching(spec_pred)?;
        Some(MatchPair {
            bid: order.clone(), // buy-side placeholder; not used by callers
            ask: order.clone(),
            price: order.price,
            qty: order.qty,
        })
    }

    /// Best bid (highest price, earliest ts). Returns owned via clone.
    pub fn best_bid(&self) -> Option<crate::marketplace::orderbook::Order<TaskSpec>> {
        self.book.lock().best_bid().cloned()
    }

    /// Best ask (lowest price, earliest ts). Returns owned via clone.
    pub fn best_ask(&self) -> Option<crate::marketplace::orderbook::Order<TaskSpec>> {
        self.book.lock().best_ask().cloned()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_type_variants_are_distinct() {
        let variants = [
            TaskType::Inference,
            TaskType::Embedding,
            TaskType::FineTune,
            TaskType::Eval,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn task_spec_new_populates_all_fields() {
        let spec = TaskSpec::new(TaskType::FineTune, "gpt-4", 200, 1_000, 5);
        assert_eq!(spec.task_type, TaskType::FineTune);
        assert_eq!(spec.model, "gpt-4");
        assert_eq!(spec.max_price, 200);
        assert_eq!(spec.deadline_unix, 1_000);
        assert_eq!(spec.quantity, 5);
        assert!(!spec.is_inference());
    }

    #[test]
    fn is_inference_matches_inference_variant() {
        let inf = TaskSpec::new(TaskType::Inference, "x", 1, 0, 1);
        let emb = TaskSpec::new(TaskType::Embedding, "x", 1, 0, 1);
        assert!(inf.is_inference());
        assert!(!emb.is_inference());
    }

    #[test]
    fn task_market_place_buy_records_in_book() {
        let m = TaskMarket::new();
        let id = m.place_buy(
            TaskSpec::new(TaskType::Inference, "gpt-4", 100, 0, 1),
            100,
            1,
            "buyer",
            1,
        );
        assert_eq!(m.bid_count(), 1);
        assert_eq!(m.ask_count(), 0);
        let best = m.best_bid().unwrap();
        assert_eq!(best.id, id);
        assert_eq!(best.price, 100);
        assert_eq!(best.owner, "buyer");
    }

    #[test]
    fn task_market_place_sell_records_in_book() {
        let m = TaskMarket::new();
        m.place_sell(
            TaskSpec::new(TaskType::Inference, "gpt-4", 80, 0, 1),
            80,
            1,
            "seller",
            1,
        );
        let best = m.best_ask().unwrap();
        assert_eq!(best.price, 80);
        assert_eq!(best.owner, "seller");
    }

    #[test]
    fn task_market_matches_crossing_top_of_book() {
        let m = TaskMarket::new();
        m.place_buy(
            TaskSpec::new(TaskType::Inference, "gpt-4", 100, 0, 1),
            100,
            1,
            "buyer",
            1,
        );
        m.place_sell(
            TaskSpec::new(TaskType::Inference, "gpt-4", 80, 0, 1),
            80,
            1,
            "seller",
            1,
        );
        let mm = m.match_top().expect("match");
        assert_eq!(mm.bid.owner, "buyer");
        assert_eq!(mm.ask.owner, "seller");
        assert_eq!(mm.price, 80);
        assert!(m.is_empty());
    }

    #[test]
    fn task_market_no_cross_returns_none() {
        let m = TaskMarket::new();
        m.place_buy(
            TaskSpec::new(TaskType::Inference, "gpt-4", 60, 0, 1),
            60,
            1,
            "buyer",
            1,
        );
        m.place_sell(
            TaskSpec::new(TaskType::Inference, "gpt-4", 80, 0, 1),
            80,
            1,
            "seller",
            1,
        );
        assert!(m.match_top().is_none());
        assert_eq!(m.bid_count(), 1);
        assert_eq!(m.ask_count(), 1);
    }
}
