---
name: 0206-008c-stoolap-pubsub-block-extension
description: Extend substrate `pub mod stoolap` re-export block from v2.3 (6 types: DataType, Error, ApiTransaction, ResultRow, Rows, Value) to v2.4 (add `pubsub` module re-export: EventBus, WalPubSub, DatabaseEvent, InvalidationReason, SchemaChangeType, OperationType, generate_event_id, parse_event). Then drop direct `stoolap` Cargo.toml dep from quota-router-core. Closes remaining D1 deviation carrier.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-008b-stoolap-dep-drop-cascade
    - RFC-0206
status: OPEN
---

# Mission `0206-008c-stoolap-pubsub-block-extension` v1.0 — OPEN 2026-08-20

## Context

`0206-008b` v1.0 attempted to drop direct `stoolap` Cargo.toml dep from
quota-router-core, but discovered 34+ sites in `src/cache.rs` reference
`stoolap::pubsub::*` (EventBus, WalPubSub, DatabaseEvent, InvalidationReason,
SchemaChangeType, OperationType, generate_event_id, parse_event). The
substrate v2.3 re-export block does NOT include `pubsub`. Mission 0206-008b
scope reduced to oaw + oatm (2 of 3 crates); qrc deferred to this mission.

## Scope

### Step 1: Substrate re-export block v2.3 → v2.4

Add `pub mod pubsub` to `crates/octo-storage-core/src/lib.rs`:

```rust
// Extend the existing pub mod stoolap block
pub mod pubsub {
    pub use stoolap::pubsub::DatabaseEvent;
    pub use stoolap::pubsub::EventBus;
    pub use stoolap::pubsub::InvalidationReason;
    pub use stoolap::pubsub::OperationType;
    pub use stoolap::pubsub::SchemaChangeType;
    pub use stoolap::pubsub::WalPubSub;
    pub use stoolap::pubsub::generate_event_id;
    pub mod wal_pubsub {
        pub use stoolap::pubsub::wal_pubsub::parse_event;
    }
}
```

(RFC-0206 v2.4 amendment scope — must be filed before consumer edits.)

### Step 2: Source edits (34+ sites, 1 crate)

`crates/quota-router-core/src/cache.rs` — replace `stoolap::pubsub::X` with
`octo_storage_core::stoolap::pubsub::X` at all 34+ sites.

### Step 3: Cargo.toml edit

DROP `stoolap = { git = "..." }` direct dep from `crates/quota-router-core/Cargo.toml`.

## Acceptance Criterion

- 34+ raw `stoolap::pubsub::*` paths in quota-router-core replaced
- 1 Cargo.toml direct `stoolap` dep dropped (qrc)
- AC gate: `rg '^\s*stoolap\s*=' crates/quota-router-core/Cargo.toml` → 0 hits
- Workspace direct `stoolap` dep count `rg '^\s*stoolap\s*=' crates/*/Cargo.toml | wc -l` = 1 (substrate only)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green
- `cargo fmt --all -- --check` green
- RFC-0206 v2.4 amendment filed

## Files / Artifacts

- RFC-0206 v2.4 amendment (RFC only, no code)
- Edit: `crates/octo-storage-core/src/lib.rs` (add `pub mod pubsub` block)
- Edit: `crates/quota-router-core/src/cache.rs` (34+ sites)
- Edit: `crates/quota-router-core/Cargo.toml` (drop dep)

## Cross-references

- Mission `0206-008b-stoolap-dep-drop-cascade` (sibling; closed 2 of 3 crates)
- Mission `0206-011b` v2.2 amendment (predecessor re-export block)
- RFC-0206 v2.3 (current re-export block surface)
- RFC-0206 v2.4 (this mission's amendment target)

## Out of scope

- Substrate redesign changes (owned by 0206-001)
- Adapter crate creation (owned by 0206-009)
- Phase 2 typed-query expansion (RFC-0206 v3.0)
- New types beyond `pubsub` module (defer to future RFC amendments)

## Dependencies

- `0206-001-substrate-newtype` (substrate `Database` type + re-export block)
- `0206-008b-stoolap-dep-drop-cascade` (sibling; closes 2 of 3 crates)
- RFC-0206 v2.4 (amendment filed)

## Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-20 | Initial filing; substrate re-export block v2.3 → v2.4 extension (pubsub module); qrc dep drop after 34+ path replacements |
