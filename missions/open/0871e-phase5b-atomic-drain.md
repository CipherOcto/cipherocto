# 0871e-phase5b — Atomic drain (Phase 5 paid-query MVP → real Phase 5)

**Status:** claimed 2026-08-10 (wave 2 step 1 of gap-closure backlog)
**Substrate:** RFC-0862 atomic transaction + RFC-0871 Phase 5 + RFC-0957 §Algorithms caveat verify
**Parent:** 0871e-paid-query-caveat (claimed)
**Closes:** Phase 5 MVP placeholder: `PaidQueryVerifyHandler` read-only → atomic drain with `PaymentReceipt` emission

## Scope

Phase 5 MVP delivers `verify_paid_query` as a read-only primitive returning `PaidQueryDecision`; the holder/wallet applies the decision via separate `RateLimitBudget::try_deduct` call. This sub-mission wires the drain into the verify flow so a `Proceed` decision atomically deducts budget + emits a `PaymentReceipt`.

1. `crates/octo-paid-query/src/ledger.rs` — NEW `SpendLedger` trait (`seed`, `try_deduct`, `balance`) + `InMemorySpendLedger` impl (replaces the free-function `RateLimitBudget::new()` / `seed` / `try_deduct` body but keeps the same public API for call-site compatibility).
2. `crates/octo-paid-query/src/lib.rs` — refactor `RateLimitBudget` to delegate to a `SpendLedger` backend (trait object via `Arc<dyn SpendLedger + Send + Sync>`), defaulting to `InMemorySpendLedger`.
3. `crates/octo-wallet-node/src/handlers/paid_query.rs` — `PaidQueryVerifyHandler` accepts a `SpendLedger` slot. On `Proceed`, calls `try_deduct` atomically. On `Partial` / `Reject`, no drain. New `PaymentReceipt { macaroon_id, drained_amount: MicroOCTO_W, remaining_budget, paid_query_decision, receipt_hmac }` field on `PaidQueryResponse`.
4. `crates/octo-wallet-node/src/node.rs` — `WalletNodeConfig` gains `spend_ledger: Arc<dyn SpendLedger>` slot; default to in-memory if not provided.
5. `crates/octo-wallet-node/src/handlers/mint.rs` — when minting a capability with a `PaymentCaveat`, seed the spend ledger with `(holder_did, macaroon_id) → caveat.budget`.
6. `crates/octo-cap-macaroon/src/caveat/payment.rs` — add `borsh` derives to `PaymentReceipt` (or define `PaymentReceipt` in `octo-paid-query` with borsh derives; the wire form is the wallet-node response boundary).

Adversary A10 (post-expiry queries) closed end-to-end: drain refuses on expired / exhausted / mismatched caveat (gates in `try_deduct`).

## Test vector discipline

- 2 new TV in `PaidQueryVerifyHandler::handle`:
  - TV1 — `Proceed` decision drains: response carries `PaymentReceipt { drained_amount: 250, remaining_budget: 750 }`.
  - TV2 — `Reject` decision no-drain: response carries `PaymentReceipt { drained_amount: 0, remaining_budget: 1000 }`.
- 1 new TV in `mint.rs`:
  - TV3 — mint with `PaymentCaveat` seeds ledger; verify-call deducts successfully.
- 1 cross-cut test: `RateLimitBudget` ledger fails closed on unknown holder (`PaidQueryError::UnknownHolder` → `Reject::BudgetExhausted`).

## Depends on

- 0957-phase2a (macaroon substrate, landed `90306f45`)
- 0957-phase2b (`PaymentCaveat`, landed `5cda2eb7`)
- 0957-phase2d (attenuation, landed `9fe9071c`)

## Blocks

- Production paid-query end-to-end
- 0871e-phase5c pricing policy (consumer of `SpendLedger`)
- 0957-f F1/F2/F3 (catalog federation depends on real drain)

## Layer direction

- `octo-paid-query` (Layer E) owns `SpendLedger` trait
- `octo-wallet-node` (Layer C) holds `Arc<dyn SpendLedger>` + drives drains
- `octo-cap-macaroon` (Layer 4) is the wire-format source for `PaymentCaveat`
- No reverse deps introduced

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib -p octo-paid-query -p octo-wallet-node -p octo-cap-macaroon`

## Cross-references

- `[[mission-gap-closure-priorities-2026-08-10]]` — Wave 2 plan
- `[[0957-phase2-unblocker-map]]` — master unblocker predecessor
- `[[mission-0871e-paid-query-caveat]]` — parent mission
