---
name: 0206-octo-market-storage-adapter
description: Open 2026-08-19; RFC-0206 §Future Work phantom pointer 5/5 — `octo-market-storage/` adapter landing (deferred per RFC-0206 v1.4 §Maintainers `octo-market-storage/` deferred per plan §4.2 B.4). Defines scope: which market-domain traits (`MarketState` / `OrderBook` / `SettlementLedger` / etc.) the adapter implements; cross-RFC dependencies on RFC-0959 (server-side market delivery bundle per `rfc-0969-dual-pipeline-semantics`).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T23:55:00.000Z
---

# Mission `0206-octo-market-storage-adapter` — OPEN 2026-08-19

## Scope

Land the `octo-market-storage/` per-owner adapter crate,
explicitly DEFERRED from RFC-0206 v1.4 (per RFC-0206 §Maintainers
v1.5 statement: "`octo-market-storage/` deferred per plan §4.2
B.4"). Covers:

- **(a) Market-domain traits survey** — enumerate which
  market-domain traits (`MarketState`, `OrderBook`,
  `SettlementLedger`, `BidQueue`, etc.) the adapter will host.
  Requires a separate review RFC per RFC-0206 §Promotion Path
  Condition 4.
- **(b) Adapter crate landing** — `crates/octo-market-storage/`
  workspace member; `Cargo.toml` `[dependencies]` matches the
  per-owner adapter template per RFC-0206 §Cargo.toml Templates
  Per-owner adapter template (owner-trait crate + `octo-storage-core`
  + `octo-determin` conditional).
- **(c) Stoolap migration runner** — `register(Arc<Database>) ->
  Arc<dyn MarketState>` (or whichever traits are in scope per
  step (a)); SQL migrations in `crates/<market-owner>/migrations/*.sql`
  per RFC-0206 §Wiring Pattern.
- **(d) RFC-0959 + RFC-0969 cross-dep** — `octo-market-storage`
  participates in the server-side market delivery bundle per
  RFC-0959 + `rfc-0969-dual-pipeline-semantics` (Dual pipeline =
  server-side market delivery, 100% bearer-only); the adapter
  must satisfy the bearer-only invariant.

## Acceptance Criterion

Mission complete when:

1. `crates/octo-market-storage/` directory exists + workspace
   member registered
2. The traits from step (a) declared in their owner-trait crates
   + implemented in `octo-market-storage/`
3. SQL migrations live under `crates/<market-owner>/migrations/*.sql`
   (NOT in `crates/octo-market-storage/`)
4. `cargo build -p octo-market-storage` green
5. `cargo test -p octo-market-storage --tests` green
6. The bearer-only invariant from RFC-0969 verified via a TV
   added to the adapter's test suite

## Cross-references

- RFC-0206 §Future Work (this mission is the bullet's real pointer)
- RFC-0206 §Maintainers (the v1.4 deferral note)
- RFC-0206 §Wiring Pattern (SQL migration placement)
- RFC-0206 §Promotion Path Condition 4 (per-adapter RFC requirement)
- RFC-0959 + RFC-0969 (the bearer-only market pipeline)
- Mission `0205-stoolap-fork-retirement` (sister substrate retirement)
- Mission `0206-octo-storage-core-deprecation` (sister substrate retirement)

## Out of scope

- Substrate retirement (owned by `0206-octo-storage-core-deprecation`)
- Substrate versioning policy (owned by `0206-octo-storage-facade-versioning`)
- Naming-convention lint (owned by `0206-octo-storage-naming-convention-lint`)
- Policy adapter (owned by `0206-cipherocto-policy-rename-alignment`)
