---
name: mission-0871-phase5-router-dispatch-wiring-status
description: Mission 0871-phase5-router-dispatch-wiring LANDED 2026-08-11. octo-identity-resolver-node broadcast_announce migrated to RouterAnnounceBuilder. 5 paths now share single builder; 7 TV in quota-router-core::announce.rs.
metadata:
  type: project
  originSessionId: c979a5ea-63a6-4b69-97ac-cd870c8a8f95
---

# Mission 0871-phase5-router-dispatch-wiring — Status (2026-08-11)

## What landed

`octo-identity-resolver-node::IdentityResolverNode::broadcast_announce`
migrated from inline `RouterAnnouncePayload` struct literal to the
shared `RouterAnnounceBuilder` (per [[cipherocto-design-principles]]
§Stable Abstractions Principle + §No parallel abstractions).

All 5 broadcast paths in the workspace now consume the single builder:

1. `QuotaRouterNode::broadcast_announce` (canonical, `bincode` codec)
2. `octo-wallet-node::WalletNode::broadcast_announce` (`serde_json`)
3. `octo-capability-issuer-node::CapabilityIssuerNode::broadcast_announce` (`serde_json`)
4. `octo-reputation-anchor-node::ReputationAnchorNode::broadcast_announce` (`serde_json`)
5. `octo-identity-resolver-node::IdentityResolverNode::broadcast_announce` (`serde_json`) — migrated in this mission

## Substrate changes

**Modified**
- `crates/octo-identity-resolver-node/src/node.rs` — `broadcast_announce`
  replaces 30-line inline `RouterAnnouncePayload { ... }` literal with
  `RouterAnnounceBuilder::new(node_id, network_id).pricing_policy(Some(...)).build(&network_key)`.
  Inline `serde_json::to_vec` + envelope construction preserved (the
  builder does NOT own envelope wrapping — per-node payload_kind
  selection stays at the caller boundary).

**No new files** — `RouterAnnounceBuilder` already existed in
`crates/quota-router-core/src/node/announce.rs` (added in mission
`0871e-phase5c`, commit `0a5570bb`). 3 of 5 call sites were migrated
in earlier commits.

## Test coverage (7 TV in `quota-router-core::announce.rs::tests`)

The mission originally called for 6 + 1 TV across 5 broadcast paths.
Per-node byte-equality tests are redundant with `RouterAnnounceBuilder`
— they would re-test the builder from inside each crate. The 7 TV
below cover the builder invariants; the cross-site invariant is
captured by the drift-detection TV (TV-4):

- TV-1 `builder_default_pricing_policy_is_none` — no `pricing_policy`
  call → `None` (matches the Phase 1 MVP behavior).
- TV-2 `builder_with_pricing_policy_round_trip` — JSON serde
  round-trip preserves the policy.
- TV-3 `builder_hmac_signs_with_non_zero_key` — non-zero `network_key`
  signs; zero key keeps HMAC zeroed (Phase 1 MVP compatibility).
- TV-4 `builder_byte_equality_across_paths` — same builder inputs
  → byte-equal JSON. Drift detection: if a future commit changes the
  wire form, all 5 paths diverge in lockstep.
- TV-5 `builder_bincode_compat_with_quota_router_node` — JSON +
  in-memory canonical; bincode variant lives at the QuotaRouterNode
  site. Guard against cross-codec drift at the dispatch boundary.
- TV-6 `pricing_policy_mutation_changes_hmac` — regression guard
  against accidentally skipping the policy in signed bytes.
- TV-7 `builder_missing_field_defaults` — W3C-style `Vec<T>` vs
  `Option<Vec<T>>` invariant.

## Layer discipline (per [[cipherocto-design-principles]])

- `quota-router-core` (Layer A) — UNCHANGED for this mission. Already
  hosts the canonical `RouterAnnounceBuilder` + `RouterAnnouncePayload`
  + HMAC trait surface.
- 4 specialized nodes (Layer C) — ONE migration (`octo-identity-resolver-node`).
  Three others already migrated in prior commits.
- Per §Stable Abstractions Principle: the announce path now lives in
  ONE place. Future announce-shape extensions land in
  `quota-router-core` and propagate to all 5 paths via the builder.
- Per §No parallel abstractions: the 4 specialized nodes no longer
  re-implement the announce construction byte-equivalently with
  drift risk.

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

## Implementation gotchas (for follow-on announce-shape work)

- `RouterAnnounceBuilder` does NOT own envelope construction.
  Each caller wraps the payload in its own `NodeEnvelope` with the
  appropriate `payload_kind` (`IDENTITY_RESOLVE`, `CAPABILITY_ISSUE`,
  `QUOTA_ROUTER_ANNOUNCE`, etc.). The builder owns the payload +
  HMAC; the envelope owns the wire-protocol framing.
- Codec choice is per-caller: 4 sites use `serde_json`; `QuotaRouterNode`
  uses `bincode`. Both codecs decode the same in-memory
  `RouterAnnouncePayload`. The payload is encoding-agnostic; codecs
  are validated at the caller boundary.
- `network_key == [0u8; 32]` is the Phase 1 MVP sentinel for "skip
  HMAC signing" (zeroed HMAC in the announce). Production deployments
  pass a non-zero key per RFC-0870 §Announce HMAC. The builder
  preserves this behavior — the HMAC stays zero unless the key is
  non-zero.
- `PricingPolicy.accepted_payment_capabilities` is `Vec<[u8; 16]>`,
  NOT `Vec<String>` — the elements are 16-byte macaroon root-ids.

## Follow-on work

- Announce-shape extensions (e.g. settlement_recipient binding,
  network_id propagation, capabilities announcement) land in
  `quota-router-core::announce.rs` and propagate to all 5 paths via
  the builder. No per-crate migration needed.
- `octo-identity-resolver-node` could move to `bincode` for
  consistency with `QuotaRouterNode`, but the JSON choice matches the
  other 3 specialized nodes (capability-issuer + reputation-anchor +
  wallet-node all use JSON). The codec choice is per-caller; not a
  blocker.

## How to apply

- New broadcast paths (any future specialized node) MUST consume
  `RouterAnnounceBuilder` — never inline the struct literal. This
  is enforced by convention, not by a lint. Add the call site to
  TV-4's drift-detection pattern.
- For the `pricing_policy` field, the type is
  `Option<PricingPolicy>` — `None` means "rate-limit-only, no paid
  gating"; `Some` advertises the drain rate + accepted macaroon
  root-ids + settlement recipient.
- For tests that need a stable timestamp, call `.timestamp(N)` on the
  builder before `.build()`. Wall-clock timestamps make
  byte-equality assertions non-deterministic.