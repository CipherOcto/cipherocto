# 0871e-phase5c — Pricing policy announce (Phase 5 paid-query pricing surface)

**Status:** claimed 2026-08-10 (wave 2 step 2 of gap-closure backlog)
**Substrate:** RFC-0870 §RouterAnnouncePayload + RFC-0871 Phase 5 + RFC-0862 atomic transaction
**Parent:** 0871e-paid-query-caveat (claimed)
**Closes:** Stub announce bodies (`CIPHEROCTO_*_ANNOUNCE_V1:*`) across 4 specialized nodes → real `RouterAnnouncePayload` with `pricing_policy`

## Scope

`RouterAnnouncePayload` (RFC-0870) carries the public pricing surface that wallets consume to construct `PaymentCaveat` chains. The quota-router-node already constructs a real payload (per `crates/quota-router-core/src/node/mod.rs:427`); the other 4 specialized nodes emit stub bytes. This sub-mission:

1. `crates/quota-router-core/src/node/announce.rs` — add `pricing_policy: Option<PricingPolicy>` field to `RouterAnnouncePayload`. `PricingPolicy { drain_per_query: MicroOCTO_W, accepted_payment_capabilities: HashSet<MacaroonId>, settlement_recipient: Option<WireDid> }`. `serde(default)` for backward compat.
2. `crates/quota-router-core/src/node/announce.rs::SignedPayload` — `compute_hmac` already canonicalizes via `serde_json::to_vec` so adding the field auto-extends HMAC coverage. Defensive TV: pin that two payloads differing only in `pricing_policy` produce different HMACs.
3. `crates/quota-router-core/src/node/mod.rs` — populate `pricing_policy` from `config.providers` (e.g. default `drain_per_query = 1000` per announced model, `accepted_payment_capabilities = empty` until F1 catalog federation lands).
4. `crates/octo-wallet-node/src/node.rs` — `WalletNode::broadcast_announce` emits real `RouterAnnouncePayload` (replaces `CIPHEROCTO_WALLET_ANNOUNCE_V1:4_payload_kinds` stub). Empty `pricing_policy` (wallet does not charge for verify operations).
5. `crates/octo-capability-issuer-node/src/node.rs` — same migration (replaces `CIPHEROCTO_CAPABILITY_ISSUER_ANNOUNCE_V1:2_payload_kinds`).
6. `crates/octo-reputation-anchor-node/src/node.rs` — same migration (replaces `CIPHEROCTO_REPUTATION_ANCHOR_ANNOUNCE_V1:1_payload_kind`).
7. `crates/octo-identity-resolver-node/src/node.rs` — same migration (replaces `CIPHEROCTO_IDENTITY_RESOLVER_ANNOUNCE_V1:1_payload_kind`).

## Test vector discipline

- 1 new TV in `announce.rs`: pricing_policy presence changes HMAC.
- 4 new TV across the migrated nodes: each node's `broadcast_announce` produces a bincode-encodable `RouterAnnouncePayload` with `node_id` matching the node's bound identity and `pricing_policy` = `None` for non-quota-router nodes.

## Depends on

- 0871e-phase5b (SpendLedger — pricing_policy is consumed at verify time which uses the ledger)
- RFC-0870 acceptance (already Accepted)
- RFC-0871 Phase 5 substrate (Phase 1 MVP landed via mission 0871-protocol-core-envelope)

## Blocks

- Cross-node paid-query discovery (wallets learn pricing by listening to announces)
- 0959-c4 CompositeCapabilityCatalog (catalog federation cites pricing_policy)

## Layer direction

- `quota-router-core` (Layer A core) owns the canonical type + HMAC trait
- 4 specialized nodes (Layer C) construct + emit
- `octo-paid-query` (Layer E extension) consumes via cross-node gossip
- No reverse deps introduced

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib -p quota-router-core -p octo-wallet-node -p octo-capability-issuer-node -p octo-reputation-anchor-node -p octo-identity-resolver-node`

## Cross-references

- `[[mission-gap-closure-priorities-2026-08-10]]` — Wave 2 plan
- `[[mission-0871e-paid-query-caveat]]` — parent mission
- `[[mission-0871e-phase5b-atomic-drain]]` — sibling sub-mission (this Wave 2 step 1)
