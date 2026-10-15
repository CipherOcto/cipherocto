---
name: marketplace-order-book-accepts-zero-quantity-orders
description: "OrderBook::place_ask and place_bid accept qty = 0 with no validation, and match_top then returns a phantom match: qty resolves to the minimum of the two sides, so a zero-qty order produces Some(MatchPair) with qty = 0 while the counterparty is left untouched and the zero-qty order is silently consumed. Measured: a zero-qty ask against a 5-unit bid returned Some((seller-zero, buyer, 0)) and removed the ask, so a caller sees a completed match for nothing."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0900
---

# Mission `marketplace-order-book-accepts-zero-quantity-orders`

## Status

Deferred — the fix requires changing a public signature
(`place_ask` / `place_bid` return `OrderId`, not `Result`), and none of the
candidate behaviours is obviously correct. Evidence gathered and
reproduced below; nothing actioned.

## Finding

Neither placement entry point validates quantity:

```rust
pub fn place_ask(&mut self, spec: Spec, price: u128, qty: u64, owner: impl Into<String>, ts_unix: u64) -> OrderId
pub fn place_bid(&mut self, spec: Spec, price: u128, qty: u64, owner: impl Into<String>, ts_unix: u64) -> OrderId
```

Both insert unconditionally. A `qty` of `0` produces a resting order.

`match_top` then computes:

```rust
let qty = bid.qty.min(ask.qty);
```

With one side at `0`, the fill is `0`. The residual logic is consistent
with that — `bid_residual = bid.qty - 0 = bid.qty` and
`ask_residual = 0 - 0 = 0` — so the zero-qty side is **removed from the
book** and the counterparty is **re-inserted at full quantity**. The
function still returns `Some(MatchPair)`.

## Measured

```rust
let mut book = OrderBook::<u32>::new();
book.place_ask(1u32, 100, 0, "seller-zero", 1_000);
book.place_bid(2u32, 100, 5, "buyer", 2_000);
let m = book.match_top();
```

```text
before: asks=1 bids=1
match  = Some(("seller-zero", "buyer", 0))
after:  asks=0 bids=1
residual bid: owner=buyer qty=5
```

A caller receives a completed match against `seller-zero` for **zero
units**. The seller's listing is gone; the buyer still holds all 5. No
quantity changed hands and no error was returned.

This is the same shape as the class of defect already recorded elsewhere
in this audit — a function that reports a completed unit of work that did
not happen. Here the unreported unit of work is a trade.

## Severity today: latent

`Marketplace::put` hardcodes the quantity:

```rust
book.place_ask(AskSpec { ... }, price, 1, ask.asker_did.clone(), now);
```

and the facade exposes **no** bid placement path at all — `place_bid` has
no `Marketplace` wrapper. So no production path can place a zero-qty
order today, and this matches the standing caveat that the marketplace has
zero production callers.

It becomes reachable the moment order placement is exposed rather than
synthesised, which is exactly what settlement wiring will do.

## Why this was not simply fixed

`place_ask` and `place_bid` return `OrderId`, not `Result`. Rejecting a
zero quantity properly means changing both signatures to return
`Result<_, OrderBookError>`, which is a breaking public API change. Three
behaviours are available once it does, and none is obviously right:

| Option                        | Effect                                             | Problem                                                   |
| ----------------------------- | -------------------------------------------------- | --------------------------------------------------------- |
| Reject at placement           | `Err(ZeroQuantity)`                                | breaking signature change; caller must handle a new error |
| Silently skip zero-qty orders | order never rests                                  | fabricates success for a placement that did not happen    |
| Skip in `match_top`           | zero-qty orders never match but stay in the book   | they occupy the top of the book and block real matches    |
| Consume silently (today)      | phantom `Some(MatchPair { qty: 0 })`, listing lost | the defect                                                |

The third option deserves particular care: because `match_top` reads
`bids.values().next()` and `asks.values().next()`, a zero-qty order at the
top of either side would permanently shadow every real order behind it.
That is strictly worse than today's behaviour, and it is the reason a
naive "just skip them in match_top" fix should not be taken.

The correct resolution — reject at placement with a typed error — is the
only one that both fails closed and leaves no stuck state, which is why
this is filed rather than patched.

## What the RFC owner needs to decide

- Should `place_ask` / `place_bid` return `Result` and reject `qty == 0`
  with a typed error? This is the option that fails closed.
- Or is zero quantity meaningful in some listing model — a placeholder
  order, a withdrawal marker, a market-maker probe? If so, the RFC should
  say so, because today it is indistinguishable from a bug.
- When the facade gains a real bid path, should `Marketplace` validate
  quantity at its own boundary as well, or rely entirely on the book?
- Should `match_top` defensively skip zero-qty orders _in addition_ to a
  placement-time rejection, so a book populated by an older writer cannot
  deadlock the top of the queue?

## Reproduction

```bash
# no validation on either placement path
grep -n -A8 "pub fn place_ask" crates/quota-router-core/src/marketplace/orderbook.rs
grep -n -A8 "pub fn place_bid" crates/quota-router-core/src/marketplace/orderbook.rs

# the fill that can be zero
grep -n "bid.qty.min(ask.qty)" crates/quota-router-core/src/marketplace/orderbook.rs

# the facade cannot reach it today: qty is hardcoded
grep -n "book.place_ask" crates/quota-router-core/src/marketplace/mod.rs
```

## Cross-references

- RFC-0900 §Purchase Lifecycle — the trade this match claims to have made
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade` —
  the same latent-because-unwired shape, for escrow
- Mission `the-marketplace-facade-has-no-production-caller` — why nothing
  shipping is affected today
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the second
  index, whose `select_ask` has its own quantity handling
