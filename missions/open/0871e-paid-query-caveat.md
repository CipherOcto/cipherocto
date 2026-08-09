# Mission: 0871e — Paid Query Caveat (RFC-0871 Phase 5)

## Status

Open (2026-08-09). RFC-0871 Accepted 2026-08-09 after R1–R7 adversarial review (R7 DRY). RFC-0965 Accepted. Phase 5 paid query mission.

## RFC

RFC-0871 (Networking): Specialized Node Protocol Envelope
RFC-0965 (Economics): Capability Extension Format
RFC-0957 (Economics): Capability Token Format

**BLUEPRINT gate note:** All substrate RFCs Accepted. Mission 0871e implements Phase 5 of RFC-0871 §Implementation Phases. No new RFC required — caveat type fits within RFC-0965 reserved range + capability composition pattern.

This mission adds the `PaymentCaveat` caveat type (RFC-0965 reserved range 0x1A–0xCF per RFC-0871 §Implementation Phases Phase 5), the pricing policy extension to `RouterAnnouncePayload`, and the wallet authorization flow that ties them together. Pre-paid capacity subscription model that drains over time, implemented via RFC-0957 caveat composition over the `NodeEnvelope`.

## Summary

Implement paid query via caveat composition. Three components: (1) `PaymentCaveat` — new caveat type carrying `(prepaid_amount: MicroOCTO_W, drain_per_query: MicroOCTO_W, expires_at_unix_ms: u64)` semantics; composes with existing caveat chain per RFC-0957 §caveat composition. (2) `RouterAnnouncePayload` extension: each announced payload_kind carries a pricing policy `(drain_per_query: MicroOCTO_W, accepted_payment_capabilities: HashSet<TokenId>)`. (3) Wallet authorization: wallet requests `RoutingDecision::Capability` with `Authorization::Capability(token)` carrying `PaymentCaveat`; quota router verifies caveat + drains capacity atomically per RFC-0862 atomic transaction; rejects if capability expired or balance insufficient.

## Acceptance Criteria

### Top-level: Caveat type

- [ ] NEW: `crates/octo-cap-macaroon/src/caveat/payment.rs` — `PaymentCaveat { prepaid_amount: MicroOCTO_W, drain_per_query: MicroOCTO_W, expires_at_unix_ms: u64 }` per RFC-0965 reserved discriminator 0x1A (first slot in 0x1A-0xCF range; subsequent payment variants may follow)
- [ ] `Caveat::verify` impl: rejects if `now_unix_ms > expires_at_unix_ms`; tracks remaining prepaid capacity per holder DID (via `HolderRegistry` lookup)
- [ ] `Caveat::attenuate` impl: supports attenuation by reducing `prepaid_amount` (holder can transfer part of prepaid capacity to sub-capability)
- [ ] Wire format: per RFC-0957 §Wire Format v1 + RFC-0965 §Encoding
- [ ] Caveat decoder in `octo-protocol::EnvelopeDispatcher::verify` recognizes `PaymentCaveat` discriminator

### Top-level: Pricing policy

- [ ] AMEND: `crates/quota-router-core/src/router_announce.rs::RouterAnnouncePayload` — add `pricing_policy: HashMap<PayloadKindId, PricingPolicy>` field
- [ ] `PricingPolicy { drain_per_query: MicroOCTO_W, accepted_payment_capabilities: HashSet<TokenId>, settlement_recipient: WireDid }`
- [ ] `RouterAnnouncePayload::broadcast` includes pricing policy per payload kind
- [ ] borsh serde with backward-compat: default empty `pricing_policy` for legacy announce payloads (RFC-0870 §NodeEnvelope Adoption transition window)

### Top-level: Atomic drain

- [ ] NEW: `crates/quota-router-core/src/payment/drain.rs` — `drain_payment(holder_did, drain_amount) -> Result<Receipt, PaymentError>` per RFC-0862 atomic transaction
- [ ] Atomic: deduct from holder's prepaid balance + emit `PaymentReceipt` event in single transaction (per RFC-0862 §Atomicity)
- [ ] Reject if balance < drain_amount: `PaymentError::InsufficientBalance`
- [ ] Reject if holder has no `PaymentCaveat` for this query: `PaymentError::NoPaymentCapability`
- [ ] Reject if `PaymentCaveat.expires_at_unix_ms < now`: `PaymentError::ExpiredCapability`

### Wallet authorization flow

- [ ] AMEND: `crates/octo-wallet-node/src/handlers/` — wallet's `WALLET_MINT_CAPABILITY` handler can mint capabilities with `PaymentCaveat` (per Phase 5 spec)
- [ ] AMEND: `crates/quota-router-core/src/proxy.rs::handle_request` — after `authenticate()` returns `RoutingDecision::Capability`, verify `Authorization::Capability(token)` carries valid `PaymentCaveat`; if so, call `drain_payment` before forwarding to model provider
- [ ] AMEND: `crates/quota-router-core/src/proxy.rs` — extend response envelope to include `PaymentReceipt` so wallet can track remaining balance

### Test coverage

- [ ] `crates/octo-cap-macaroon/tests/payment_caveat_roundtrip.rs` — borsh round-trip + attenuation + expiry
- [ ] `crates/quota-router-core/tests/payment_atomic_drain.rs` — atomic drain + insufficient balance + expired capability
- [ ] `crates/octo-wallet-node/tests/paid_query_e2e.rs` — end-to-end: wallet mints capability with `PaymentCaveat` → quota router verifies + drains → model provider serves → wallet receives response with `PaymentReceipt`
- [ ] All tests green; clippy clean; fmt clean

### Adversary coverage

- [ ] Replay of paid query: envelope_id dedup + capability's `PaymentCaveat` debit-once semantics prevents double-spend
- [ ] Capability forgery: macaroon HMAC + `PaymentCaveat` chain verified per RFC-0957 §Verification
- [ ] Atomic drain bypass: drain is part of RFC-0862 atomic transaction — no partial drain possible
- [ ] Stale payment capability: `PaymentCaveat.expires_at_unix_ms` check at every handler invocation
- [ ] Pricing policy spoofing: `RouterAnnouncePayload` signed per RFC-0870 §HMAC; unsigned announcements rejected

### Backward compat

- [ ] `cargo test -p octo-cap-macaroon --lib` continues green (no regression in existing caveat tests)
- [ ] `cargo test -p quota-router-core --lib` continues green (no regression in existing router tests)
- [ ] `cargo test -p octo-wallet-node --lib` green (new paid-query handler)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires:**

- RFC-0871 — accepted substrate (envelope shape)
- RFC-0965 — caveat discriminator + encoding
- RFC-0957 — capability token format + caveat composition
- RFC-0870 — `RouterAnnouncePayload` shape (specialized node pattern)
- RFC-0862 — atomic transaction substrate
- RFC-0959 — settlement receipt (paid query emits payment receipt per RFC-0959 §Schema)
- `crates/octo-protocol` — Phase 1 envelope types (mission `0871-protocol-core-envelope.md`)
- `crates/octo-cap-macaroon` — macaroon substrate (Phase 4 extraction recommended: `0957-ext-macaroon-crate.md`)
- `crates/octo-wallet-node` — wallet node (Phase 2: `0871a-wallet-node.md`)

**Mission gates (sequential):**

- Mission `0871-protocol-core-envelope.md` MUST complete first (Phase 1 dependency)
- Mission `0871a-wallet-node.md` MUST complete first (Phase 2 — wallet node provides mint handler)
- Mission `0957-ext-macaroon-crate.md` SHOULD complete first (Phase 4 — `PaymentCaveat` lives in extracted crate)

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

Per RFC-0871 §Implementation Phases Phase 5 + RFC-0957 §caveat composition + RFC-0959 §Schema:

- [x] RFCs Accepted (RFC-0871, RFC-0965, RFC-0957)
- [ ] Mission filed (this file)
- [ ] Phase 1 foundation complete: `0871-protocol-core-envelope.md`
- [ ] Phase 2 wallet node complete: `0871a-wallet-node.md`
- [ ] Phase 4 macaroon extraction complete: `0957-ext-macaroon-crate.md` (recommended)
- [ ] `PaymentCaveat` implemented + registered
- [ ] `RouterAnnouncePayload` pricing policy extension
- [ ] Atomic drain via RFC-0862
- [ ] End-to-end paid query test green

## Claimant

@unassigned

## Pull Request

#

## Notes

- Layer E extension (caveat variant). Stability: per-extension. `PaymentCaveat` is one variant; future payment variants (subscription, per-token billing) follow the same registration pattern.
- Atomicity is critical — drain MUST be in same RFC-0862 transaction as the forward-to-model-provider call. Partial drain + forward leaves the holder debited without service rendered (or vice versa). Per RFC-0862 §Atomicity, the entire request handling is one transaction.
- Caveat composition: `PaymentCaveat` composes with existing caveat chain per RFC-0957 §caveat composition. A capability can carry `[MaxPerEpochCaveat, PaymentCaveat, RevocationCaveat]` simultaneously — all verified atomically.
- Backward compat: `RouterAnnouncePayload.pricing_policy` defaults to empty for legacy announce payloads (RFC-0870 §NodeEnvelope Adoption transition window). Legacy routers can still announce; new routers reject queries against legacy announces if `PaymentCaveat` is required.
- This is OFF-CHAIN settlement (capability drain). On-chain settlement integration (per RFC-0959 future RFC) is a separate concern; `PaymentReceipt` can be later anchored to on-chain settlement.
- Per CLAUDE.md crate stability: `PaymentCaveat` lives in `octo-cap-macaroon` (Layer E extension). Future payment variants (subscription, per-token) land in `octo-cap-payment/` or similar — each extension is its own crate per RFC-0871 §Layer E model.
