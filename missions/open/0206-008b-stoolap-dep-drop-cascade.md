---
name: 0206-008b-stoolap-dep-drop-cascade
description: Drop direct `stoolap` Cargo.toml dep from octo-adapter-whatsapp + octo-adapter-telegram-mtproto. Substrate redesign v3.0 (`pub mod stoolap` re-export block) exposes `DataType`, `Error`, `ApiTransaction`, `ResultRow`, `Rows`, `Value` — both crates use only these types (verified zero raw `stoolap::` refs in src/). Restores RFC-0206 §Cargo.toml Templates Layer A invariant for 2 of 3 remaining D1-deviation crates. quota-router-core deferred to 0206-008c (requires `stoolap::pubsub` re-export block extension).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-002-layer-b-type-renames
    - 0206-008-layer-b-type-renames-expansion
    - RFC-0206
---

# Mission `0206-008b-stoolap-dep-drop-cascade` v1.0 — OPEN 2026-08-20

## Context

`0206-008` v1.0 documented a "D1 deviation" — direct `stoolap` Cargo.toml dep
RETAINED in 9 consumer crates because substrate v3.0 did not yet re-export
`stoolap::ResultRow` / `stoolap::ApiTransaction` / `stoolap::Rows` /
`stoolap::Error`. Per `0206-011b` v2.2 amendment (substrate re-export block),
substrate now exposes:

```rust
// crates/octo-storage-core/src/stoolap.rs
pub use stoolap::core::DataType;
pub use stoolap::core::Error;
pub use stoolap::ApiTransaction;
pub use stoolap::ResultRow;
pub use stoolap::Rows;
pub use stoolap::Value;
```

5 of the 9 retaining crates already dropped `stoolap` (octo-ident-storage,
octo-matrix-session-store, octo-reputation, octo-whatsapp, quota-router-storage
via prior 0206-008b commits). 3 remaining:
- `octo-adapter-whatsapp` — IN SCOPE
- `octo-adapter-telegram-mtproto` — IN SCOPE
- `quota-router-core` — DEFERRED to 0206-008c (uses `stoolap::pubsub::*` 34+ sites)

## Scope (revised mid-claim)

### Source edits (zero)

Both `octo-adapter-whatsapp` and `octo-adapter-telegram-mtproto` already route
through `octo_storage_core::stoolap::*` re-export paths. Zero raw `stoolap::`
refs in src/. No source edits required.

### Cargo.toml edits (2 crates)

DROP `stoolap = { git = "https://...", rev = "..." }` direct dep from:

- `crates/octo-adapter-whatsapp/Cargo.toml`
- `crates/octo-adapter-telegram-mtproto/Cargo.toml`

REPLACE D1 deviation comment block with resolution note:
```
# Stoolap direct dep REMOVED 2026-08-20 by mission 0206-008b. Substrate
# `pub mod stoolap` re-export block (DataType, Error, ApiTransaction,
# ResultRow, Rows, Value) covers all consumer types. Substrate remains
# sole owner of direct stoolap dep per RFC-0206 §Cargo.toml Templates
# Layer A invariant.
```

Quota-router-core dep drop DEFERRED to `0206-008c` (requires `stoolap::pubsub`
added to substrate re-export block — substrate v2.4 extension scope).

## Acceptance Criterion

- 2 Cargo.toml direct `stoolap` deps dropped (oaw + oatm)
- AC gate: `rg '^\s*stoolap\s*=' crates/octo-adapter-whatsapp/Cargo.toml crates/octo-adapter-telegram-mtproto/Cargo.toml` → 0 hits
- Workspace direct `stoolap` dep count `rg '^\s*stoolap\s*=' crates/*/Cargo.toml | wc -l` = 2 (substrate + quota-router-core)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green (per `quota-router-core-feature-mutex`)
- `cargo fmt --all -- --check` green
- `rg 'stoolap::[A-Z]' crates/octo-adapter-whatsapp/src crates/octo-adapter-telegram-mtproto/src` → 0 hits (all routed through substrate)

## Files / Artifacts

- Edit: `crates/octo-adapter-whatsapp/Cargo.toml` (drop dep + comment)
- Edit: `crates/octo-adapter-telegram-mtproto/Cargo.toml` (drop dep + comment)
- New: `missions/open/0206-008c-stoolap-pubsub-block-extension.md` (deferred scope)

## Cross-references

- Mission `0206-008-layer-b-type-renames-expansion` (parent; D1 deviation originated)
- Mission `0206-001-substrate-newtype` (substrate `Database` newtype)
- Mission `0206-002-layer-b-type-renames` (sibling pattern)
- Mission `0206-011b` v2.2 amendment (substrate re-export block surface)
- Mission `0206-008c` (deferred; quota-router-core pubsub block)
- RFC-0206 v2.1 §Cargo.toml Templates Layer A (substrate sole owner invariant)

## Out of scope

- Substrate redesign changes (owned by 0206-001)
- Adapter crate creation (owned by 0206-009)
- Phase 2 typed-query expansion (RFC-0206 v3.0)
- Stoolap fork itself (separate repo)
- quota-router-core dep drop (deferred to 0206-008c)

## Dependencies

- `0206-001-substrate-newtype` (substrate `Database` type + `pub mod stoolap` re-export block must exist)
- `0206-002-layer-b-type-renames` (sibling mission; same TYPE rename pattern)
- `0206-008-layer-b-type-renames-expansion` (parent mission; D1 deviation originated)
- RFC-0206 (acceptance precondition)

## Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-20 | Initial filing; 2 Cargo.toml dep drops (oaw + oatm); qrc deferred to 0206-008c after discovery of `stoolap::pubsub` 34+ sites not in v2.3 re-export block |
