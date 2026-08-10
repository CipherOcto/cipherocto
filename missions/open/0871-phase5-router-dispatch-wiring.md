# 0871-phase5-router-dispatch-wiring — WalletNode→QuotaRouterNode announce delegation

**Status:** unassigned (wave 3a step 4; gap surfaced 2026-08-10)
**Substrate:** RFC-0870 announce wire + RFC-0862 gossip substrate
**Parent:** 0871e-phase5c pricing policy (closed `0a5570bb`)

## Scope

After wave 2 commits, 4 specialized nodes (wallet-node + capability-issuer-node + reputation-anchor-node + identity-resolver-node) added `quota-router-core` as a Cargo dep + construct `RouterAnnouncePayload` directly. But `QuotaRouterNode` (in `crates/quota-router-core/src/node/mod.rs`) ALREADY owns the canonical announce path. The 4 specialized nodes re-implement it byte-equivalently but do not share the code. Drift risk: future announce-shape changes land in `QuotaRouterNode` but miss the 4 specialized nodes.

Per cipherocto-design-principles §Stable Abstractions Principle + §No parallel abstractions, the announce path should live in ONE place. Recommended pattern: `QuotaRouterNode::broadcast_announce` is the single emitter; the 4 specialized nodes delegate.

1. `crates/quota-router-core/src/node/announce.rs` — extract `RouterAnnounceBuilder` struct (or equivalent) that both `QuotaRouterNode::broadcast_announce` AND the 4 specialized nodes use. The builder takes `(node_id, network_id, supported_models, capacities, pricing_policy)` + the network key, computes HMAC, serializes to bincode/JSON.
2. `crates/quota-router-core/src/node/mod.rs` — `QuotaRouterNode::broadcast_announce` refactored to use the builder.
3. `crates/octo-wallet-node/src/node.rs` — `WalletNode::broadcast_announce` calls `RouterAnnounceBuilder::new(...).build()` instead of inlining the struct literal.
4. `crates/octo-capability-issuer-node/src/node.rs` — same migration.
5. `crates/octo-reputation-anchor-node/src/node.rs` — same migration.
6. `crates/octo-identity-resolver-node/src/node.rs` — same migration.

## Test vector discipline

- 6 new TV: each of the 5 broadcast paths (QuotaRouterNode + 4 specialized nodes) produces a `RouterAnnouncePayload` whose wire bytes match a shared golden fixture. Drift detection TV: byte-equality across all 5 paths.
- 1 new TV: pricing_policy mutation changes HMAC across all 5 paths (regression guard).

## Depends on

- 0871e-phase5c pricing policy (closed `0a5570bb`) — `PricingPolicy` + `RouterAnnouncePayload.pricing_policy`
- 0959-placeholder-identity-binding (wave 3a step 3) — provides real `node_id` for the 4 specialized nodes

## Blocks

- Single-source-of-truth for announce shape (drift prevention)
- Any future announce-shape extension (e.g. settlement_recipient binding, network_id propagation)

## Layer direction

- `quota-router-core` (Layer A) owns the canonical `RouterAnnounceBuilder`
- 4 specialized nodes (Layer C) consume the builder via dep
- Per cipherocto-design-principles §Stable Abstractions Principle: stable substrate (Layer A) hosts the canonical type; per-node business logic (Layer C) stays minimal

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy -p quota-router-core -p octo-wallet-node -p octo-capability-issuer-node -p octo-reputation-anchor-node -p octo-identity-resolver-node --all-targets -- -D warnings`
- `cargo test --lib` for all 5 affected crates
- Byte-equality golden TV across 5 broadcast paths

## Cross-references

- [[wave-3-gaps-2026-08-10]] — gap surface context
- [[mission-0871e-phase5c-status]] — predecessor sub-mission (introduced duplication)
- [[0959-placeholder-identity-binding]] — sibling sub-mission (provides real node_id)
- [[cipherocto-design-principles]] — Stable Abstractions Principle + no parallel abstractions
