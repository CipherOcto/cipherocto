# 0957-phase2b — PaymentCaveat migration + dispatcher decode

**Status:** claimed 2026-08-10 (wave 1 step 2 of gap-closure backlog)
**Substrate:** RFC-0871 §Implementation Phases Phase 5 + RFC-0957 §Algorithms + RFC-0965 §3 caveats
**Closes:** 0871e deferred items #1 (caveat migration) + #2 (dispatcher-level decode) + #6 (macaroon HMAC verification + PaymentCaveat chain verify) + #7 (WALLET_MINT_CAPABILITY handler accepting PaymentCaveat mint requests)

## Scope

`PaidQueryCaveat` currently lives in the Layer E extension crate
`crates/octo-paid-query/src/lib.rs`. Phase 2b migrates the caveat
data type + verify + attenuation semantics into the Layer 4 macaroon
substrate `crates/octo-cap-macaroon/src/caveat/payment.rs`, with
`octo-paid-query` becoming a thin re-export wrapper preserving all
existing call sites.

The central `Caveat` enum in `octo_cap_macaroon::caveat` gains a new
`Payment(PaymentCaveat)` variant (discriminator `0x1A` per
RFC-0965 reserved range). `CaveatName::Payment` is added with the
wire-stable identifier `cipherocto/cap/v1/caveat/payment`.

## Implementation

1. Create `crates/octo-cap-macaroon/src/caveat/payment.rs` with the
   `PaymentCaveat` struct (moved verbatim from `octo-paid-query`):
   - `(caveat_name, budget, model, expires_at_unix_ms)` fields
   - `PAID_QUERY_CAVEAT_NAME` discriminator string constant
   - `is_expired(now_unix_ms)` + `matches_model(query_model)` predicates
   - `verify(query_cost, query_model, now_unix_ms) -> PaidQueryDecision`
     (the `verify_paid_query` semantics — budget gate, model gate,
     expiry gate; returns Proceed/Partial/Reject)
   - `attenuate(new_budget, new_expires_at) -> PaymentCaveat`
     (monotonic narrowing — `new_budget <= self.budget`,
     `new_expires_at <= self.expires_at_unix_ms`)

2. Add `Caveat::Payment(PaymentCaveat)` variant to the central enum
   in `crates/octo-cap-macaroon/src/caveat.rs`. Add
   `CaveatName::Payment` variant + wire identifier
   `cipherocto/cap/v1/caveat/payment`.

3. Update `crates/octo-cap-macaroon/src/lib.rs` to export
   `payment::{PaymentCaveat, PaidQueryDecision, PaidQueryRejectionReason, PAID_QUERY_CAVEAT_NAME}`.

4. Update `crates/octo-paid-query/src/lib.rs` to re-export
   `PaymentCaveat` + `PAID_QUERY_CAVEAT_NAME` from cap-macaroon
   (preserves all existing call sites + tests). The crate keeps
   `RateLimitBudget` + `verify_paid_query` + `PaidQueryRequest` /
   `PaidQueryResponse` (Phase 5 MVP primitives).

5. Update `crates/octo-wallet-node/src/handlers/mint.rs` to accept
   an optional `payment_caveat: Option<PaymentCaveat>` field on
   `MintRequest`. When present, the handler passes it as the initial
   caveat via `CapabilityToken::mint(..., &[Caveat::Payment(p)])`.

## Test vector discipline

Per `[[mission-gap-closure-priorities-2026-08-10]]` §Test vector
discipline + `[[0957-phase2-unblocker-map]]` phase2b scope, byte-exact
TV preservation across migration:

- All 14 existing `octo-paid-query` tests pass unchanged (the crate
  re-exports the migrated type, no schema change).
- 6 new `octo-cap-macaroon` TV:
  - TV1 — `Caveat::Payment(p)` serde_json roundtrip preserves
    all 4 fields.
  - TV2 — `Caveat::Payment(p).name() == CaveatName::Payment` and
    `.as_str() == "cipherocto/cap/v1/caveat/payment"`.
  - TV3 — `set_subsumes([Payment{parent}], [Payment{child}])`
    accepts narrowing (child budget ≤ parent budget, child
    expiry ≤ parent expiry, parent model empty OR matches child).
  - TV4 — `PaymentCaveat::attenuate(narrower_budget, narrower_expiry)`
    returns a PaymentCaveat with the narrower fields; rejects
    widening via panic (debug-only) or return Err (production).
  - TV5 — `PaymentCaveat::verify(query_cost, query_model, now_unix_ms)`
    covers all 4 decision branches (Proceed / Partial / Reject
    Expired / Reject ModelMismatch / Reject BudgetExhausted).
  - TV6 — wallet-node `MintRequest { payment_caveat: Some(p) }`
    mint + wire + deserialize yields a macaroon whose caveats
    list contains exactly one `Caveat::Payment(p)` entry.

## Depends on

- 0957-phase2a (landed commit `90306f45`) — real wire form substrate
- 0957-ext-macaroon Phase 1 + 2b + 2c substrate (landed)

## Blocks

- 0871e-phase5b (atomic drain) — needs PaymentCaveat in caveat chain
- 0871e-phase5c (pricing policy) — needs PaymentCaveat
- 0957-f F1/F2/F3 — needs PaymentCaveat discriminator in Caveat enum

## Layer direction

- `octo-cap-macaroon` (Layer 4) — owns PaymentCaveat (the canonical home)
- `octo-paid-query` (Layer E) — re-exports from cap-macaroon
- `octo-wallet-node` (Layer C) → `octo-cap-macaroon` (Layer 4) ✓
- `octo-wallet-node` → `octo-paid-query` (Layer E) ✓

No new reverse dependencies. The octo-paid-query crate's role is
now reduced to "Phase 5 MVP primitives (RateLimitBudget, request/
response envelopes)" — the caveat data type lives in cap-macaroon.

## Validation

- `cargo fmt -p octo-cap-macaroon -p octo-paid-query -p octo-wallet-node --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib -p octo-cap-macaroon` (existing 160 + 6 new)
- `cargo test --lib -p octo-paid-query` (existing 14 unchanged)
- `cargo test --lib -p octo-wallet-node` (existing 21 + 1 new)
- `cargo test --lib -p octo-wallet` (no regressions on the
  re-export path)

## Cross-references

- `[[0957-phase2-unblocker-map]]` — phase2b sub-mission
- `[[cipherocto-design-principles]]` — per-extension crate model
  (caveat data type lives in Layer 4 substrate; the E-extension
  crate is reduced to primitives)
- `[[mission-gap-closure-priorities-2026-08-10]]` — Wave 1 plan
- `[[mission-0957-ext-macaroon-phase2b-status]]` — substrate predecessor