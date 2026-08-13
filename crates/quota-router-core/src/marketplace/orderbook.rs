//! Order book — price-time priority bid/ask book (RFC-0900 §Order Book).
//!
//! Generic over `Spec`: the spec is the orderable payload (e.g., an
//! `AskSpec { ask_id, asker_did, model }` for the quota marketplace,
//! or a `TaskSpec` for the inference task market — Gap 6).
//!
//! Internal storage: two `BTreeMap<(Price, Seq), Order<Spec>>` instances,
//! one for bids (best = highest price, FIFO at same price) and one for
//! asks (best = lowest price, FIFO at same price). Time priority
//! implemented by a monotonic `seq` counter per side; `ts_unix` is
//! preserved on the `Order` itself for content-addressing and display.
//! The seq counter guarantees uniqueness even when two orders are
//! placed in the same `ts_unix` second (Round 1 review fix: prior
//! BTreeMap key was `(price, ts_unix)` which silently overwrote the
//! earlier order on same-second placements).

use std::collections::BTreeMap;

/// 32-byte order identifier (BLAKE3 content hash of the canonical order).
pub type OrderId = [u8; 32];

/// Side of an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Bid,
    Ask,
}

/// An order resting in the book. `Spec` is the orderable payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order<Spec> {
    pub id: OrderId,
    pub spec: Spec,
    pub price: u128,
    pub qty: u64,
    pub owner: String,
    pub ts_unix: u64,
}

/// One executed match between a bid and an ask at the same or crossing price.
///
/// `price` = execution price (= ask.price; bid may be higher).
/// `qty` = matched quantity (min of bid.qty and ask.qty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchPair<Spec> {
    pub bid: Order<Spec>,
    pub ask: Order<Spec>,
    pub price: u128,
    pub qty: u64,
}

/// Price-time priority order book.
///
/// Bids are stored in `(Reverse<u128>, u64)`-keyed BTreeMap for "highest
/// price first, FIFO at same price". Asks are stored in `(u128, u64)`-
/// keyed BTreeMap for "lowest price first, FIFO at same price". The
/// second tuple element is a monotonic per-book sequence counter
/// (not `ts_unix`) so simultaneous same-second placements cannot
/// collide on the BTreeMap key.
#[derive(Debug, Clone)]
pub struct OrderBook<Spec> {
    bids: BTreeMap<(std::cmp::Reverse<u128>, u64), Order<Spec>>,
    asks: BTreeMap<(u128, u64), Order<Spec>>,
    /// Monotonic per-book counter; incremented on every `place_*` so
    /// the BTreeMap key is always unique even when `(price, ts_unix)`
    /// would collide.
    next_seq: u64,
}

impl<Spec> Default for OrderBook<Spec> {
    fn default() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            next_seq: 0,
        }
    }
}

impl<Spec: Clone> OrderBook<Spec> {
    /// Empty book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a bid at `price` for `qty` units of `spec`. `ts_unix` is
    /// recorded on the order (used by `OrderId` and visible to
    /// consumers) but does not participate in the BTreeMap key —
    /// per-book `seq` counter does, guaranteeing uniqueness.
    pub fn place_bid(
        &mut self,
        spec: Spec,
        price: u128,
        qty: u64,
        owner: impl Into<String>,
        ts_unix: u64,
    ) -> OrderId {
        let owner = owner.into();
        let id = order_id(price, qty, owner.as_bytes(), ts_unix, &Side::Bid);
        let seq = self.next_seq;
        self.next_seq += 1;
        let order = Order {
            id,
            spec,
            price,
            qty,
            owner,
            ts_unix,
        };
        self.bids.insert((std::cmp::Reverse(price), seq), order);
        id
    }

    /// Insert an ask at `price` for `qty` units of `spec`.
    pub fn place_ask(
        &mut self,
        spec: Spec,
        price: u128,
        qty: u64,
        owner: impl Into<String>,
        ts_unix: u64,
    ) -> OrderId {
        let owner = owner.into();
        let id = order_id(price, qty, owner.as_bytes(), ts_unix, &Side::Ask);
        let seq = self.next_seq;
        self.next_seq += 1;
        let order = Order {
            id,
            spec,
            price,
            qty,
            owner,
            ts_unix,
        };
        self.asks.insert((price, seq), order);
        id
    }

    /// Best (highest-price, earliest-seq) bid, if any.
    #[must_use]
    pub fn best_bid(&self) -> Option<&Order<Spec>> {
        self.bids.values().next()
    }

    /// Best (lowest-price, earliest-seq) ask, if any.
    #[must_use]
    pub fn best_ask(&self) -> Option<&Order<Spec>> {
        self.asks.values().next()
    }

    /// Match the top-of-book bid with the top-of-book ask if they cross
    /// (`bid.price >= ask.price`). Returns the matched pair; any
    /// residual quantity on either side is re-inserted with a fresh
    /// seq counter so it remains in the book for the next match.
    /// If no cross, returns `None` and leaves the book untouched.
    pub fn match_top(&mut self) -> Option<MatchPair<Spec>> {
        let bid_entry = self.bids.values().next()?;
        let ask_entry = self.asks.values().next()?;
        if bid_entry.price < ask_entry.price {
            return None;
        }
        // Take ownership (clone) so we can remove from the maps.
        let bid = bid_entry.clone();
        let ask = ask_entry.clone();
        // Re-discover keys by linear scan — keys are (Reverse(price), seq)
        // and seq is opaque; scanning by content avoids storing it on
        // Order. Single entry per side per scan, cost is O(side_count).
        let bid_key = *self
            .bids
            .keys()
            .find(|k| k.0 == std::cmp::Reverse(bid.price) && self.bids[k].id == bid.id)
            .expect("bid key must exist while bid is in book");
        let ask_key = *self
            .asks
            .keys()
            .find(|k| k.0 == ask.price && self.asks[k].id == ask.id)
            .expect("ask key must exist while ask is in book");
        self.bids.remove(&bid_key);
        self.asks.remove(&ask_key);
        let qty = bid.qty.min(ask.qty);
        let price = ask.price; // execution at the ask's price (maker side)
                               // Re-insert residual quantity on either side (Round 1 review
                               // fix: prior code dropped both sides entirely even on partial
                               // fill, losing the residual).
        let bid_residual = bid.qty - qty;
        if bid_residual > 0 {
            let mut residual = bid.clone();
            residual.qty = bid_residual;
            let seq = self.next_seq;
            self.next_seq += 1;
            self.bids
                .insert((std::cmp::Reverse(residual.price), seq), residual);
        }
        let ask_residual = ask.qty - qty;
        if ask_residual > 0 {
            let mut residual = ask.clone();
            residual.qty = ask_residual;
            let seq = self.next_seq;
            self.next_seq += 1;
            self.asks.insert((residual.price, seq), residual);
        }
        Some(MatchPair {
            bid,
            ask,
            price,
            qty,
        })
    }

    /// Best ask whose `spec` satisfies `spec_pred` (e.g., matching model).
    #[must_use]
    pub fn best_ask_matching<P: Fn(&Spec) -> bool>(&self, spec_pred: P) -> Option<&Order<Spec>> {
        self.asks.values().find(|o| spec_pred(&o.spec))
    }

    /// All asks whose `spec` satisfies `spec_pred`, in price-time order
    /// (lowest price first, FIFO at the same price).
    ///
    /// Used by `Marketplace::cheapest_with_ranking` for latency-aware
    /// scanning (Gap 7.2).
    #[must_use]
    pub fn asks_matching<P: Fn(&Spec) -> bool>(&self, spec_pred: P) -> Vec<&Order<Spec>> {
        self.asks.values().filter(|o| spec_pred(&o.spec)).collect()
    }

    /// Total number of resting bids.
    #[must_use]
    pub fn bid_count(&self) -> usize {
        self.bids.len()
    }

    /// Total number of resting asks.
    #[must_use]
    pub fn ask_count(&self) -> usize {
        self.asks.len()
    }

    /// True if both sides are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() & self.asks.is_empty()
    }
}

fn order_id(price: u128, qty: u64, owner: &[u8], ts: u64, side: &Side) -> OrderId {
    let mut msg = Vec::with_capacity(16 + 8 + owner.len() + 8 + 1);
    msg.extend_from_slice(&price.to_le_bytes());
    msg.extend_from_slice(&qty.to_le_bytes());
    msg.extend_from_slice(owner);
    msg.extend_from_slice(&ts.to_le_bytes());
    msg.push(match side {
        Side::Bid => 0,
        Side::Ask => 1,
    });
    *blake3::hash(&msg).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_bid_records_in_book() {
        let mut book = OrderBook::<u32>::new();
        let id = book.place_bid(42u32, 100, 5, "buyer-1", 1_000);
        assert_eq!(book.bid_count(), 1);
        assert_eq!(book.ask_count(), 0);
        let best = book.best_bid().unwrap();
        assert_eq!(best.id, id);
        assert_eq!(best.price, 100);
        assert_eq!(best.qty, 5);
        assert_eq!(best.owner, "buyer-1");
        assert_eq!(best.ts_unix, 1_000);
        assert_eq!(best.spec, 42);
    }

    #[test]
    fn place_ask_records_in_book() {
        let mut book = OrderBook::<&str>::new();
        book.place_ask("gpt-4", 50, 3, "seller-1", 2_000);
        let best = book.best_ask().unwrap();
        assert_eq!(best.price, 50);
        assert_eq!(best.qty, 3);
        assert_eq!(best.owner, "seller-1");
        assert_eq!(best.spec, "gpt-4");
    }

    #[test]
    fn best_ask_is_lowest_price() {
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 200, 1, "s1", 1);
        book.place_ask((), 100, 1, "s2", 2);
        book.place_ask((), 300, 1, "s3", 3);
        let best = book.best_ask().unwrap();
        assert_eq!(best.price, 100);
        assert_eq!(best.owner, "s2");
    }

    #[test]
    fn best_bid_is_highest_price() {
        let mut book = OrderBook::<()>::new();
        book.place_bid((), 50, 1, "b1", 1);
        book.place_bid((), 150, 1, "b2", 2);
        book.place_bid((), 100, 1, "b3", 3);
        let best = book.best_bid().unwrap();
        assert_eq!(best.price, 150);
        assert_eq!(best.owner, "b2");
    }

    #[test]
    fn fifo_breaks_ties_first_inserted_wins() {
        // Same price → first-inserted wins (FIFO at price). The per-book
        // `seq` counter breaks ties; `ts_unix` is preserved on the Order
        // but does not participate in the BTreeMap key (Round 1 fix:
        // prior `(price, ts_unix)` key silently overwrote same-second
        // placements; the seq counter guarantees uniqueness regardless
        // of `ts_unix` collisions).
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 100, 1, "late", 100);
        book.place_ask((), 100, 1, "early", 50);
        let best = book.best_ask().unwrap();
        assert_eq!(best.owner, "late");
    }

    #[test]
    fn match_top_returns_crossing_pair_exact_fill() {
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 90, 3, "seller", 1);
        book.place_bid((), 100, 3, "buyer", 2);
        let m = book.match_top().unwrap();
        assert_eq!(m.bid.price, 100);
        assert_eq!(m.ask.price, 90);
        assert_eq!(m.price, 90); // executed at ask price
        assert_eq!(m.qty, 3);
        assert!(book.is_empty());
    }

    #[test]
    fn match_top_partial_fill_reinserts_ask_residual() {
        // Round 1 fix: prior code dropped both sides entirely on a
        // partial fill, losing the residual quantity. With qty=3 bid
        // vs qty=5 ask, the bid is exhausted and the ask should remain
        // in the book with qty=2.
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 90, 5, "seller", 1);
        book.place_bid((), 100, 3, "buyer", 2);
        let m = book.match_top().unwrap();
        assert_eq!(m.qty, 3);
        assert_eq!(m.ask.price, 90);
        // Bid is gone (qty fully consumed); ask residual = 2 remains.
        assert_eq!(book.bid_count(), 0);
        assert_eq!(book.ask_count(), 1);
        let residual = book.best_ask().unwrap();
        assert_eq!(residual.qty, 2);
        assert_eq!(residual.owner, "seller");
    }

    #[test]
    fn match_top_partial_fill_reinserts_bid_residual() {
        // Mirror of the ask-residual test: bid larger than ask, ask is
        // exhausted, bid residual remains.
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 90, 3, "seller", 1);
        book.place_bid((), 100, 5, "buyer", 2);
        let m = book.match_top().unwrap();
        assert_eq!(m.qty, 3);
        assert_eq!(book.bid_count(), 1);
        assert_eq!(book.ask_count(), 0);
        let residual = book.best_bid().unwrap();
        assert_eq!(residual.qty, 2);
        assert_eq!(residual.owner, "buyer");
    }

    #[test]
    fn place_ask_same_second_does_not_overwrite() {
        // Round 1 fix: BTreeMap key collision on `(price, ts_unix)`
        // silently overwrote the earlier order. The per-book `seq`
        // counter guarantees uniqueness — two orders placed in the
        // same second with the same price now both land.
        let mut book = OrderBook::<()>::new();
        let id_alice = book.place_ask((), 100, 1, "alice", 1_700_000_000);
        let id_bob = book.place_ask((), 100, 1, "bob", 1_700_000_000);
        assert_ne!(id_alice, id_bob);
        assert_eq!(book.ask_count(), 2);
    }

    #[test]
    fn match_top_returns_none_when_no_cross() {
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 200, 1, "seller", 1);
        book.place_bid((), 100, 1, "buyer", 2);
        assert!(book.match_top().is_none());
        // Book untouched.
        assert_eq!(book.bid_count(), 1);
        assert_eq!(book.ask_count(), 1);
    }

    #[test]
    fn match_top_at_equal_price() {
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 100, 1, "s", 1);
        book.place_bid((), 100, 1, "b", 2);
        let m = book.match_top().unwrap();
        assert_eq!(m.price, 100);
        assert_eq!(m.qty, 1);
        assert!(book.is_empty());
    }

    #[test]
    fn best_ask_matching_filters_by_spec_predicate() {
        let mut book = OrderBook::<&str>::new();
        book.place_ask("gpt-4", 50, 1, "a", 1);
        book.place_ask("claude", 40, 1, "b", 2);
        let best = book.best_ask_matching(|m| *m == "gpt-4").unwrap();
        assert_eq!(best.spec, "gpt-4");
        assert_eq!(best.price, 50);
        assert!(book.best_ask_matching(|m| *m == "missing").is_none());
    }

    #[test]
    fn empty_book_returns_none() {
        let book: OrderBook<()> = OrderBook::new();
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
        assert!(book.is_empty());
    }

    #[test]
    fn id_is_deterministic_for_same_inputs() {
        let id_a = {
            let mut book = OrderBook::<()>::new();
            book.place_bid((), 100, 5, "buyer", 1)
        };
        let id_b = {
            let mut book = OrderBook::<()>::new();
            book.place_bid((), 100, 5, "buyer", 1)
        };
        assert_eq!(id_a, id_b);
    }
}
