# Mission: 0871e — Paid Query Caveat (RFC-0871 Phase 5)

## Status

Claimed + Phase 5 MVP shipped (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). RFC-0965 Accepted. Phase 5 paid-query **bridge** delivered — see MVP Disclosures below for the scope of this commit vs the full RFC-0871 §Implementation Phases Phase 5 surface.

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope
RFC-0965 (Economics): Capability Extension Format
RFC-0957 (Economics): Capability Token Format

**BLUEPRINT gate note:** All substrate RFCs Accepted. Mission 0871e implements Phase 5 of RFC-0871 §Implementation Phases. No new RFC required — caveat type fits within RFC-0965 reserved range + capability composition pattern.

This mission adds the `PaymentCaveat` caveat type (RFC-0965 reserved range 0x1A–0xCF per RFC-0871 §Implementation Phases Phase 5), the pricing policy extension to `RouterAnnouncePayload`, and the wallet authorization flow that ties them together. Pre-paid capacity subscription model that drains over time, implemented via RFC-0957 caveat composition over the `NodeEnvelope`.

## Summary

Implement paid query via caveat composition. Three components: (1) `PaymentCaveat` — new caveat type carrying `(prepaid_amount: MicroOCTO_W, drain_per_query: MicroOCTO_W, expires_at_unix_ms: u64)` semantics; composes with existing caveat chain per RFC-0957 §Algorithms (caveat verification step). (2) `RouterAnnouncePayload` extension: each announced payload_kind carries a pricing policy `(drain_per_query: MicroOCTO_W, accepted_payment_capabilities: HashSet<TokenId>)`. (3) Wallet authorization: wallet requests `RoutingDecision::Capability` with `Authorization::Capability(token)` carrying `PaymentCaveat`; quota router verifies caveat + drains capacity atomically per RFC-0862 atomic transaction substrate; rejects if capability expired or balance insufficient.

## Acceptance Criteria

### Top-level: Caveat type

- [x] **Bridge only (MVP):** `PaidQueryCaveat` lives in the new `crates/octo-paid-query/src/lib.rs` (Layer E extension crate), NOT inside `octo-cap-macaroon`. Shape: `(caveat_name: "paid-query/v1", budget: MicroOCTO_W, model: String, expires_at_unix_ms: u64)`.
- [ ] **Deferred (follow-on):** Migration of `PaidQueryCaveat` into `crates/octo-cap-macaroon/src/caveat/payment.rs` with discriminator 0x1A + `Caveat::verify` + `Caveat::attenuate` impls lands in mission 0957 Phase 2 follow-on (per mission file line 122 "Phase 4 extraction recommended"). Phase 5 MVP stands up the bridge crate so the caveat has a home before the per-extension migration completes.
- [ ] **Deferred (follow-on):** Caveat decoder in `octo-protocol::EnvelopeDispatcher::verify` recognizes `PaymentCaveat` discriminator. The dispatcher-level decode is not needed in Phase 5 MVP because the wallet handler decodes `PaidQueryRequest` directly via `borsh::from_slice` (no central enum — per [[cipherocto-design-principles]] §"Extension over enumeration").

### Top-level: Pricing policy

- [ ] **Deferred (follow-on):** `crates/quota-router-core/src/router_announce.rs::RouterAnnouncePayload` extension + `PricingPolicy { drain_per_query, accepted_payment_capabilities, settlement_recipient }`. Phase 5 MVP does not extend the router announce — pricing is a quota-router concern that lands in the atomic-drain follow-on mission.
- [ ] **Deferred (follow-on):** `RouterAnnouncePayload::broadcast` includes pricing policy per payload kind.
- [ ] **Deferred (follow-on):** borsh serde backward-compat (default empty `pricing_policy`).

### Top-level: Atomic drain

- [x] **Bridge only (MVP):** `RateLimitBudget::try_deduct(holder_did, macaroon_id, cost) -> Result<remaining, PaidQueryError>` lives in `crates/octo-paid-query/src/lib.rs`. In-memory storage only (`parking_lot::Mutex<HashMap>` equivalent — uses `std::sync::Mutex` to avoid a `parking_lot` dep). Follow-on mission replaces with `HolderRegistry`-backed ledger per RFC-0862 atomic transaction substrate.
- [ ] **Deferred (follow-on):** `crates/quota-router-core/src/payment/drain.rs` module + `PaymentReceipt` event emission per RFC-0862 atomic transaction. Proxy integration (`handle_request` post-`authenticate()` payment verification) is a separate workstream.

### Wallet authorization flow

- [x] **Bridge only (MVP):** New `WALLET_PAID_QUERY_VERIFY` handler in `crates/octo-wallet-node/src/handlers/paid_query.rs` delegates to `octo_paid_query::verify_paid_query`. Handler is read-only — it does NOT mutate wallet state in Phase 5 MVP.
- [ ] **Deferred (follow-on):** `WALLET_MINT_CAPABILITY` handler accepting `PaymentCaveat` mint requests lands alongside the macaroon caveat chain migration (mission 0957 Phase 2 follow-on).
- [ ] **Deferred (follow-on):** `crates/quota-router-core/src/proxy.rs::handle_request` post-authenticate `Authorization::Capability(token)` verification + `drain_payment` invocation + `PaymentReceipt` response envelope. The quota-router proxy currently extracts `CapabilityToken` strings only (line 116 `extract_capability_token`); full integration requires `GatewayAuthenticator::authenticate()` plumbing which is out of scope for this mission.

### Test coverage

- [x] **Bridge only (MVP):** 15 unit tests in `crates/octo-paid-query/src/lib.rs` covering caveat construction, expiry predicate, model match (incl. wildcard), proceed / partial / reject decisions (4 rejection reasons), zero-macaroon-id defensive rejection, `RateLimitBudget` seed + deduct + isolation + unknown-holder, borsh round-trips for `PaidQueryRequest` + `PaidQueryResponse`, and the documented `PAID_QUERY_VERIFY` UUID.
- [x] **Bridge only (MVP):** 3 handler tests in `crates/octo-wallet-node/src/handlers/paid_query.rs` covering proceed-within-budget, reject-expired-caveat, and emits-correct-payload-kind.
- [x] 5 new `octo-protocol` tests for `PAID_QUERY_VERIFY` (UUID match, RFC namespace, predicate match, borsh round-trip, no-collision-with-other-namespaces).
- [ ] **Deferred (follow-on):** `crates/octo-cap-macaroon/tests/payment_caveat_roundtrip.rs` (lands with caveat migration).
- [ ] **Deferred (follow-on):** `crates/quota-router-core/tests/payment_atomic_drain.rs` (lands with drain mission).
- [ ] **Deferred (follow-on):** `crates/octo-wallet-node/tests/paid_query_e2e.rs` end-to-end test (lands once the proxy integration is wired).
- [x] `cargo test -p octo-paid-query --lib` green (15/15).
- [x] `cargo test -p octo-wallet-node --lib` green (17/17 — 14 pre-existing + 3 new).
- [x] `cargo test -p octo-protocol --lib` green (58/58 — no regression; 5 new).
- [x] `cargo clippy -p octo-paid-query -p octo-wallet-node --all-targets -- -D warnings` clean.
- [x] `cargo fmt --check -p octo-paid-query -p octo-wallet-node` clean.

### Adversary coverage (Phase 5 MVP subset)

- [x] **Defensive: zero-macaroon-id rejection.** All-zero `MacaroonId` is treated as uninitialised and rejected at the verifier (defense against buggy callers passing uninitialised identifiers).
- [x] **Expiry gate.** `verify_paid_query` rejects if `now_unix_ms > caveat.expires_at_unix_ms` (RFC-0871 §Adversary A10 — post-expiry queries). `u64::MAX` = never-expires is honoured.
- [x] **Model scope gate.** Caveat binds to a specific model (or wildcard `""`); mismatches are rejected with `PaidQueryRejectionReason::ModelMismatch`.
- [x] **Budget gate.** Empty budget (zero) or query-cost-exceeding-budget is rejected with `BudgetExhausted` / `Partial` (downgradeable).
- [x] **Envelope-level replay defense.** `EnvelopeDispatcher::verify_all` (run before handler dispatch) enforces envelope_id dedup + signature verification (RFC-0871 §Adversary A6). The new handler does not bypass this.
- [ ] **Deferred:** macaroon HMAC verification + `PaymentCaveat` chain verification per RFC-0957 §Algorithms. The bridge takes a `MacaroonId` (16-byte identifier) but does NOT re-verify the HMAC chain — that requires the caveat struct migration into `octo-cap-macaroon` (mission 0957 Phase 2 follow-on). Phase 5 MVP trusts the wallet's authority on its own caveat chain.
- [ ] **Deferred:** Atomic drain (RFC-0862). The `RateLimitBudget::try_deduct` primitive exists but is not yet wired into the proxy; in-memory storage means atomicity is single-process only. The follow-on mission replaces with the cross-process atomic substrate.
- [ ] **Deferred:** Pricing policy spoofing defense (`RouterAnnouncePayload` HMAC per RFC-0870). Not relevant until pricing policy lands.

### Backward compat

- [x] `cargo test -p octo-protocol --lib` green (58 tests, 5 new for paid-query).
- [x] `cargo test -p octo-wallet-node --lib` green (17 tests, 3 new for paid-query).
- [x] `cargo clippy -p octo-paid-query -p octo-wallet-node --all-targets -- -D warnings` clean.
- [x] `cargo fmt --check -p octo-paid-query -p octo-wallet-node` clean.
- [ ] **Not run (out of scope for this mission):** `cargo test -p octo-cap-macaroon --lib` — no regression expected (this mission does not amend the macaroon crate), but the mission did not exercise it.
- [ ] **Not run (out of scope for this mission):** `cargo test -p quota-router-core --lib` — Phase 5 MVP does not touch the quota-router proxy; the follow-on drain mission will.

## Type Coverage

Per BLUEPRINT §Mission template. RFC-0871 §Implementation Phases Phase 5 + RFC-0965 + RFC-0957 types mapped to this mission:

| RFC Type / Section | Implemented By |
|---|---|
| `PaymentCaveat` (RFC-0965 reserved discriminator 0x1A) | This mission — `crates/octo-cap-macaroon/src/caveat/payment.rs` |
| `PricingPolicy` extension | This mission — `crates/quota-router-core/src/router_announce.rs` (amends existing `RouterAnnouncePayload`) |
| `drain_payment` atomic primitive | This mission — `crates/quota-router-core/src/payment/drain.rs` |
| `PaymentReceipt` event | This mission — RFC-0959 §Specification (receipt fields per RFC-0959 Data Structures) |
| `WALLET_MINT_CAPABILITY` + `PaymentCaveat` flow | Mission `0871a-wallet-node.md` — wallet node's `mint.rs` handler extended to accept `PaymentCaveat` |
| `handle_request` post-authenticate payment verification | This mission — amends `crates/quota-router-core/src/proxy.rs` |
| `CapabilityToken` mint substrate (RFC-0957 §Algorithms) | Mission `0957-ext-macaroon-crate.md` — prerequisite (Phase 4 extraction); `PaymentCaveat` lives in extracted crate |
| `HolderRegistry` substrate | RFC-0957-A1 existing |
| `RouterAnnouncePayload` shape | Mission `0870-b-envelope-adoption.md` — RFC-0870 §NodeEnvelope Adoption |
| `NodeEnvelope` envelope shape (consumed) | Mission `0871-protocol-core-envelope.md` — Phase 1 prerequisite |
| Atomic transaction substrate | RFC-0862 existing |

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate (envelope shape)
- RFC-0965 — caveat discriminator + encoding
- RFC-0957 — capability token format + caveat composition
- RFC-0870 — `RouterAnnouncePayload` shape (specialized node pattern)
- RFC-0862 — atomic transaction substrate
- RFC-0959 — settlement receipt (paid query emits payment receipt per RFC-0959 §Specification, Data Structures — receipt carries prepaid_amount, drain_amount, holder_did, settlement_hash)
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-cap-macaroon` — macaroon substrate (Phase 4 extraction recommended: `0957-ext-macaroon-crate.md`)
- `crates/octo-wallet-node` — wallet node (Phase 2: `0871a-wallet-node.md`)

**Mission gates (sequential):**

- Mission `0871-protocol-core-envelope.md` MUST complete first (Phase 1 dependency)
- Mission `0871a-wallet-node.md` MUST complete first (Phase 2 — wallet node provides mint handler)
- Mission `0957-ext-macaroon-crate.md` MUST complete first (Phase 4 — `PaymentCaveat` lives in extracted `crates/octo-cap-macaroon/`, not in old monolithic `crates/octo-wallet/src/capability/macaroon.rs` path)

**Parallel with (no dependency):**

- Mission `0871b-identity-resolver-node.md` (Phase 3)
- Mission `0871c-reputation-anchor-node.md` (Phase 3)
- Mission `0871d-capability-issuer-node.md` (Phase 3)

**Not Requires:**

- RFC-0955 (fiat ramp) — paid query is off-chain settlement; on-chain settlement is separate concern
- New token (OCTO-W) — paid query uses existing OCTO-W (per `crates/quota-router-core/src/balance.rs` substrate)
- New RFC — `PaymentCaveat` fits within RFC-0965 reserved range; no new RFC needed

## Implementation Guide

- NEW file: `crates/octo-cap-macaroon/src/caveat/payment.rs` — `PaymentCaveat` struct + Caveat trait impl
- AMEND: `crates/octo-cap-macaroon/src/caveat/mod.rs` — register `PaymentCaveat` in `CAVEAT_REGISTRY` with discriminator 0x1A
- AMEND: `crates/quota-router-core/src/router_announce.rs` — add `pricing_policy` field with `#[serde(default)]` for backward compat
- NEW file: `crates/quota-router-core/src/payment/drain.rs` — `drain_payment` per RFC-0862 atomic transaction
- AMEND: `crates/quota-router-core/src/proxy.rs::handle_request` — post-authenticate payment verification
- AMEND: `crates/octo-wallet-node/src/handlers/mint.rs` — accept `PaymentCaveat` in mint request
- Test fixtures: deterministic `Clock` injection for expiry tests; in-memory `HolderRegistry` for atomic drain tests

## Acceptance Cross-Ref

Per RFC-0871 §Implementation Phases Phase 5 + RFC-0957 §Algorithms (caveat verification) + RFC-0959 §Specification:

- [x] RFCs Accepted (RFC-0871, RFC-0965, RFC-0957)
- [x] Mission filed (this file)
- [x] Phase 1 foundation complete: `0871-protocol-core-envelope.md`
- [x] Phase 2 wallet node complete: `0871a-wallet-node.md`
- [x] Phase 4 macaroon extraction complete (Phase 1): `0957-ext-macaroon-crate.md`
- [x] **Phase 5 MVP bridge shipped:** `PaidQueryCaveat` + `verify_paid_query` + `RateLimitBudget` + `WALLET_PAID_QUERY_VERIFY` handler
- [ ] Caveat migration into `octo-cap-macaroon` + `Caveat::verify`/`attenuate` impls (follow-on: mission 0957 Phase 2)
- [ ] `RouterAnnouncePayload` pricing policy extension (follow-on)
- [ ] Atomic drain via RFC-0862 (follow-on)
- [ ] End-to-end paid query test green (follow-on: requires proxy integration)

## Claimant

@mmacedoeu (Phase 5 MVP, 2026-08-09)

## Pull Request

# (local-only — no push per [[git-workflow]])

## Closure Record

### Files changed

| File | Change |
|---|---|
| `crates/octo-protocol/src/payload_kind.rs` | AMEND — added `PAID_QUERY_VERIFY` const, `PAID_QUERY_PAYLOAD_KINDS` array, `is_paid_query_payload_kind` predicate, 5 unit tests. |
| `crates/octo-paid-query/Cargo.toml` | NEW — Layer E extension crate manifest, deps + lints per `octo-cap-macaroon` pattern. |
| `crates/octo-paid-query/src/lib.rs` | NEW — `PaidQueryCaveat`, `verify_paid_query`, `PaidQueryDecision`, `PaidQueryRejectionReason`, `RateLimitBudget`, `PaidQueryError`, `PaidQueryRequest`, `PaidQueryResponse` types + 15 unit tests. |
| `crates/octo-wallet-node/Cargo.toml` | AMEND — added `octo-paid-query` dep with layer-rationale comment. |
| `crates/octo-wallet-node/src/lib.rs` | AMEND — re-exported `PaidQueryVerifyHandler` + `PaidQueryVerifyRequest`; added `PAID_QUERY_VERIFY` to `WALLET_PAYLOAD_KINDS`; updated module doc. |
| `crates/octo-wallet-node/src/handlers/mod.rs` | AMEND — added `pub mod paid_query` + `pub use`. |
| `crates/octo-wallet-node/src/handlers/paid_query.rs` | NEW — `PaidQueryVerifyHandler` (delegates to `octo_paid_query::verify_paid_query`) + 3 unit tests. |
| `crates/octo-wallet-node/src/node.rs` | AMEND — added `k if k == octo_paid_query::PAID_QUERY_VERIFY` match arm in `handle_envelope`. |
| `missions/claimed/0871e-paid-query-caveat.md` | NEW (moved from `missions/open/`) — closure record + AC checkboxes + MVP disclosures. |

### Verification

| Check | Result |
|---|---|
| `cargo build -p octo-paid-query -p octo-wallet-node` | green |
| `cargo test -p octo-paid-query --lib` | 15/15 pass |
| `cargo test -p octo-wallet-node --lib` | 17/17 pass (14 pre-existing + 3 new) |
| `cargo test -p octo-protocol --lib` (regression) | 58/58 pass (53 pre-existing + 5 new) |
| `cargo clippy -p octo-paid-query --all-targets -- -D warnings` | clean |
| `cargo fmt --check -p octo-paid-query -p octo-wallet-node` | clean |

### MVP disclosures (Phase 5 scope vs full RFC-0871 §Implementation Phases Phase 5)

| Surface | Phase 5 MVP | Full Phase 5 (follow-on missions) |
|---|---|---|
| Caveat home | New `crates/octo-paid-query/` Layer E extension crate | Migration into `octo-cap-macaroon::caveat::payment` with discriminator 0x1A + `Caveat::verify`/`attenuate` |
| Verification | `verify_paid_query` (read-only; takes a `MacaroonId` and a `PaidQueryCaveat`) | Full macaroon HMAC + `PaymentCaveat` chain verification per RFC-0957 §Algorithms |
| Rate-limit storage | `RateLimitBudget` in-memory (`std::sync::Mutex<HashMap>`) | `HolderRegistry`-backed ledger per RFC-0862 atomic transaction substrate (cross-process) |
| Handler | `WALLET_PAID_QUERY_VERIFY` on the existing `WalletNode` (read-only delegator) | Dedicated `paid-query-node` specialized node (Layer C) per RFC-0871 §Layer C specialized-node pattern |
| Pricing policy | Not amended | `RouterAnnouncePayload::pricing_policy` extension + `PricingPolicy { drain_per_query, accepted_payment_capabilities, settlement_recipient }` |
| Receipt event | Not emitted | `PaymentReceipt` event per RFC-0959 §Data Structures |
| Proxy integration | Not wired (`WALLET_PAID_QUERY_VERIFY` reachable only via direct envelope) | `crates/quota-router-core/src/proxy.rs::handle_request` post-`authenticate()` `Authorization::Capability(token)` verification + `drain_payment` + response envelope with `PaymentReceipt` |
| End-to-end test | None | `crates/octo-wallet-node/tests/paid_query_e2e.rs` + `crates/quota-router-core/tests/payment_atomic_drain.rs` + `crates/octo-cap-macaroon/tests/payment_caveat_roundtrip.rs` |

### Why the bridge pattern, not the full caveat chain

Per [[cipherocto-design-principles]] §"Extension over enumeration" + "User extensibility": the paid-query bridge is the smallest atomic increment that (a) allocates the `PAID_QUERY_VERIFY` payload kind namespace, (b) proves the per-extension crate shape, and (c) wires a verifier primitive the holder can call before a quota-router follow-on ships. Deferring the full `PaymentCaveat` chain migration + atomic drain to follow-on missions keeps each commit a single atomic review unit (per [[cipherocto-design-principles]] §"Discipline at first call site pays off"). The full surface lands in two follow-on missions:

1. **Mission 0957 Phase 2 follow-on** — migrate `PaidQueryCaveat` into `octo-cap-macaroon::caveat::payment` with discriminator 0x1A + `Caveat::verify`/`attenuate` impls per RFC-0957 §Algorithms. Update `octo-paid-query` to depend on the migrated caveat.
2. **Mission 0871e Phase 6 follow-on** (to be filed) — `RouterAnnouncePayload::pricing_policy` extension + `crates/quota-router-core/src/payment/drain.rs` atomic drain per RFC-0862 + proxy integration + `PaymentReceipt` event emission + dedicated `paid-query-node` specialized node + end-to-end tests.

## Notes

- Layer E extension (caveat variant). Stability: per-extension. `PaidQueryCaveat` is one variant; future payment variants (subscription, per-token billing) follow the same registration pattern.
- Atomicity is critical — drain MUST be in same RFC-0862 transaction as the forward-to-model-provider call. Partial drain + forward leaves the holder debited without service rendered (or vice versa). Per RFC-0862 atomic transaction substrate, the entire request handling is one transaction.
- Caveat composition: `PaidQueryCaveat` composes with existing caveat chain per RFC-0957 §Algorithms (caveat verification step). A capability can carry `[MaxPerEpochCaveat, PaidQueryCaveat, RevocationCaveat]` simultaneously — all verified atomically.
- Backward compat: `RouterAnnouncePayload.pricing_policy` defaults to empty for legacy announce payloads (RFC-0870 §NodeEnvelope Adoption transition window). Legacy routers can still announce; new routers reject queries against legacy announces if `PaidQueryCaveat` is required.
- This is OFF-CHAIN settlement (capability drain). On-chain settlement integration (per RFC-0959 future RFC) is a separate concern; `PaymentReceipt` can be later anchored to on-chain settlement.
- Per CLAUDE.md crate stability: `PaidQueryCaveat` lives in `octo-paid-query` for Phase 5 MVP. Follow-on migration moves it into `octo-cap-macaroon` (Layer 4) per the per-extension crate extraction roadmap (RFC-0957 v2.0 §Per-Extension Crate Layout). Future payment variants land in `octo-cap-payment/` or similar — each extension is its own crate per RFC-0871 §Layer E model.
