//! Order book — price-time priority bid/ask book (RFC-0900 §Order Book).
//!
//! Generic over `Spec`: the spec is the orderable payload (e.g., an
//! `AskSpec { ask_id, asker_did, model }` for the quota marketplace,
//! or a `TaskSpec` for the inference task market — Gap 6).
//!
//! Internal storage: two `BTreeMap<(Price, Ts), Order<Spec>>` instances,
//! one for bids (best = highest price, FIFO at same price) and one for
//! asks (best = lowest price, FIFO at same price). Time priority
//! implemented by including a monotonic `ts_unix` in the key; ties break
//! by insertion order (earlier wins).

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
/// keyed BTreeMap for "lowest price first, FIFO at same price".
#[derive(Debug, Clone)]
pub struct OrderBook<Spec> {
    bids: BTreeMap<(std::cmp::Reverse<u128>, u64), Order<Spec>>,
    asks: BTreeMap<(u128, u64), Order<Spec>>,
}

impl<Spec> Default for OrderBook<Spec> {
    fn default() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }
}

impl<Spec: Clone> OrderBook<Spec> {
    /// Empty book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a bid at `price` for `qty` units of `spec`. `ts_unix` sets time
    /// priority (earlier wins on ties).
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
        let order = Order {
            id,
            spec,
            price,
            qty,
            owner,
            ts_unix,
        };
        self.bids.insert((std::cmp::Reverse(price), ts_unix), order);
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
        let order = Order {
            id,
            spec,
            price,
            qty,
            owner,
            ts_unix,
        };
        self.asks.insert((price, ts_unix), order);
        id
    }

    /// Best (highest-price, earliest-ts) bid, if any.
    #[must_use]
    pub fn best_bid(&self) -> Option<&Order<Spec>> {
        self.bids.values().next()
    }

    /// Best (lowest-price, earliest-ts) ask, if any.
    #[must_use]
    pub fn best_ask(&self) -> Option<&Order<Spec>> {
        self.asks.values().next()
    }

    /// Match the top-of-book bid with the top-of-book ask if they cross
    /// (`bid.price >= ask.price`). Returns the matched pair; both sides are
    /// removed from the book. If no cross, returns `None` and leaves the
    /// book untouched.
    pub fn match_top(&mut self) -> Option<MatchPair<Spec>> {
        let bid_entry = self.bids.values().next()?;
        let ask_entry = self.asks.values().next()?;
        if bid_entry.price < ask_entry.price {
            return None;
        }
        // Take ownership (clone) so we can remove from the maps.
        let bid = bid_entry.clone();
        let ask = ask_entry.clone();
        let bid_key = (std::cmp::Reverse(bid.price), bid.ts_unix);
        let ask_key = (ask.price, ask.ts_unix);
        // SAFETY: we just observed these keys via `values().next()`.
        self.bids.remove(&bid_key);
        self.asks.remove(&ask_key);
        let qty = bid.qty.min(ask.qty);
        let price = ask.price; // execution at the ask's price (maker side)
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
        self.bids.is_empty() && self.asks.is_empty()
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
    fn time_priority_breaks_ties_oldest_first() {
        // Same price → earlier ts wins.
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 100, 1, "late", 100);
        book.place_ask((), 100, 1, "early", 50);
        let best = book.best_ask().unwrap();
        assert_eq!(best.owner, "early");
    }

    #[test]
    fn match_top_returns_crossing_pair() {
        let mut book = OrderBook::<()>::new();
        book.place_ask((), 90, 5, "seller", 1);
        book.place_bid((), 100, 3, "buyer", 2);
        let m = book.match_top().unwrap();
        assert_eq!(m.bid.price, 100);
        assert_eq!(m.ask.price, 90);
        assert_eq!(m.price, 90); // executed at ask price
        assert_eq!(m.qty, 3);
        assert!(book.is_empty());
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
