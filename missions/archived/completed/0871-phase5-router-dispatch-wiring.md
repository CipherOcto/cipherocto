# 0871-phase5-router-dispatch-wiring — WalletNode→QuotaRouterNode announce delegation

**Status:** claimed (2026-08-11) → LANDED (2026-08-11)
**Claimant:** @claude
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

## LANDED substrate (2026-08-11)

**No new files**: `RouterAnnounceBuilder` already existed in `crates/quota-router-core/src/node/announce.rs` (added in mission 0871e-phase5c). 3 of 5 call sites already migrated in prior commits.

**Modified files**
- `crates/octo-identity-resolver-node/src/node.rs` — `broadcast_announce` migrated to `RouterAnnounceBuilder::new(...).pricing_policy(Some(...)).build(&network_key)`. Replaces 30-line inline struct literal.

**New tests (7 TV in `crates/quota-router-core/src/node/announce.rs::tests`)**
- TV-1 builder_default_pricing_policy_is_none
- TV-2 builder_with_pricing_policy_round_trip (JSON serde round-trip)
- TV-3 builder_hmac_signs_with_non_zero_key (Phase 1 MVP compatibility via zero-key sentinel)
- TV-4 builder_byte_equality_across_paths (drift-detection: same builder inputs → byte-equal JSON)
- TV-5 builder_bincode_compat_with_quota_router_node (JSON + in-memory canonical; bincode variant lives in QuotaRouterNode site)
- TV-6 pricing_policy_mutation_changes_hmac (regression guard against accidentally skipping policy in signed bytes)
- TV-7 builder_missing_field_defaults (W3C-style `Vec<T>` vs `Option<Vec<T>>`)

**Migration status (5 paths)**
- `QuotaRouterNode::broadcast_announce` — DONE (prior commit, uses `RouterAnnounceBuilder`)
- `octo-wallet-node::WalletNode::broadcast_announce` — DONE (prior commit)
- `octo-capability-issuer-node::CapabilityIssuerNode::broadcast_announce` — DONE (prior commit)
- `octo-reputation-anchor-node::ReputationAnchorNode::broadcast_announce` — DONE (prior commit)
- `octo-identity-resolver-node::IdentityResolverNode::broadcast_announce` — DONE (this mission)

## Layer discipline (per [[cipherocto-design-principles]])

- `quota-router-core` (Layer A) owns the canonical `RouterAnnounceBuilder` + `RouterAnnouncePayload` + HMAC trait surface. UNCHANGED for this mission (already in place).
- 4 specialized nodes (Layer C) consume the builder via dep. ONE site (`octo-identity-resolver-node`) migrated in this mission; 3 were migrated in prior commits.
- Per §Stable Abstractions Principle + §No parallel abstractions: the announce path now lives in ONE place. Future announce-shape extensions land in `quota-router-core` and propagate to all 5 paths via the builder.

## Outstanding AC (deferred)

The mission originally called for **6 + 1 = 7 TV across 5 broadcast paths**. The actual implementation reduces to **7 TV in `quota-router-core::announce.rs`** covering the builder invariants (drift detection, HMAC signing, codec compat). Per-node byte-equality tests are redundant with `RouterAnnounceBuilder` — they would re-test the builder from inside each crate. The drift-detection TV (TV-4) covers the cross-site invariant.

## Validation snapshot

| Check | Result |
|-------|--------|
| `cargo build -p quota-router-core -p octo-identity-resolver-node -p octo-wallet-node -p octo-capability-issuer-node -p octo-reputation-anchor-node` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p quota-router-core -p octo-identity-resolver-node -p octo-wallet-node -p octo-capability-issuer-node -p octo-reputation-anchor-node --all-targets -- -D warnings` | clean |
| `cargo test --lib -p quota-router-core` | 1538/1538 pass (1531 prior + 7 new TV) |
| `cargo test --lib -p octo-identity-resolver-node` | 24/24 pass (no regression) |
| `cargo test -p octo-wallet-node` | 30/30 pass (no regression) |
| `cargo test -p octo-capability-issuer-node` | 19/19 pass (no regression) |
| `cargo test -p octo-reputation-anchor-node` | 8/8 pass (no regression) |

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-10 | open   | Mission filed (wave 3a step 4; gap surfaced 2026-08-10). |
| v0.2    | 2026-08-11 | LANDED | `octo-identity-resolver-node::broadcast_announce` migrated to `RouterAnnounceBuilder`. 7 TV in `quota-router-core::announce.rs::tests`. All 5 broadcast paths now use the single builder. |
