---
rfc: 0206-v40
title: Substrate Re-export Block v2.3 → v2.4 (pubsub module extension)
status: Draft
version: 0.1.0
date: 2026-08-24
authors:
  - cipherocto-claim-and-implement-plan
maintainers:
  - cipherocto-core
depends_on:
  - RFC-0206
  - 0206-008b-stoolap-dep-drop-cascade
  - 0206-008c-stoolap-pubsub-block-extension
---

# RFC-0206 v2.4 — Substrate Re-export Block pubsub extension

## Status

**Draft v0.1.0** — initial amendment filing per claim-and-implement plan v1.0 Session 5 deferred-work unblocking. Mission `0206-008c-stoolap-pubsub-block-extension` v1.0 OPEN.

## Summary

RFC-0206 v2.4 amends the Substrate Re-export Block (§Substrate Re-export Block) to add the `pubsub` module re-export. The v2.3 block re-exports 6 stoolap types (`DataType`, `Error`, `ApiTransaction`, `ResultRow`, `Rows`, `Value`); v2.4 adds the `pubsub` module with 6 type re-exports (`EventBus`, `WalPubSub`, `DatabaseEvent`, `InvalidationReason`, `SchemaChangeType`, `OperationType`) + 1 fn re-export (`generate_event_id`) + nested `pub mod wal_pubsub` (`parse_event`).

**Total surface:**
- v2.3: 6 top-level re-exports
- v2.4: 6 top-level re-exports + 1 nested module (`pubsub`) + 6 pubsub types + 1 pubsub fn + 1 nested module (`wal_pubsub`) + 1 wal_pubsub fn = **14 names total** under `octo_storage_core::stoolap::*`

The 8 top-level `pub use` cap (Layer B stability) is UNCHANGED — v2.4 grows the re-export surface via nested `pub mod` (allowed under §Substrate Re-export Block v2.1 §Re-export Block Cap).

## §1 Diff — `crates/octo-storage-core/src/lib.rs` §pub mod stoolap

**v2.3** (current state):

```rust
pub mod stoolap;
```

(Single-line wholesale re-export of the entire stoolap crate. Functional but over-broad.)

**v2.4** (this amendment):

```rust
pub mod stoolap {
    //! Substrate Re-export Block v2.4 (RFC-0206 v2.4).
    //!
    //! 6 top-level types (DataType/Error/ApiTransaction/ResultRow/Rows/Value)
    //! + nested `pub mod pubsub` (6 types + 1 fn + nested `wal_pubsub` (1 fn))
    //! = 14 names total under `octo_storage_core::stoolap::*`.

    pub use ::stoolap::ApiTransaction;
    pub use ::stoolap::DataType;
    pub use ::stoolap::Error;
    pub use ::stoolap::ResultRow;
    pub use ::stoolap::Rows;
    pub use ::stoolap::Value;

    /// Pub-sub event bus for substrate change notifications (RFC-0206 v2.4).
    pub mod pubsub {
        pub use ::stoolap::pubsub::DatabaseEvent;
        pub use ::stoolap::pubsub::EventBus;
        pub use ::stoolap::pubsub::InvalidationReason;
        pub use ::stoolap::pubsub::OperationType;
        pub use ::stoolap::pubsub::SchemaChangeType;
        pub use ::stoolap::pubsub::WalPubSub;
        pub use ::stoolap::pubsub::generate_event_id;

        /// WalPubSub event parser (RFC-0206 v2.4).
        pub mod wal_pubsub {
            pub use ::stoolap::pubsub::wal_pubsub::parse_event;
        }
    }
}
```

## §2 Migration Path

### Step 1: Substrate re-export block v2.3 → v2.4

Edit `crates/octo-storage-core/src/lib.rs` per §1 diff. The wholesale `pub mod stoolap;` (current) is replaced by the explicit narrowed block (proposed).

### Step 2: Consumer path replacement

Edit `crates/quota-router-core/src/cache.rs` — replace remaining raw `stoolap::pubsub::*` paths (if any) with `octo_storage_core::stoolap::pubsub::*`. (37 sites already use the substrate prefix; this step is largely complete per pre-condition.)

### Step 3: Cargo.toml dep drop (qrc)

Drop `stoolap = { git = "..." }` direct dep from `crates/quota-router-core/Cargo.toml`. (Already dropped per pre-condition.)

### Step 4: Workspace dep count gate

`rg '^\s*stoolap\s*=' crates/*/Cargo.toml | wc -l` = 1 (substrate only).

## §3 Acceptance Criteria

- `pub mod stoolap` block contains 6 top-level types + nested `pubsub` + nested `wal_pubsub`
- 34+ raw `stoolap::pubsub::*` paths in quota-router-core replaced (pre-condition: 37/37 already done)
- 1 Cargo.toml direct `stoolap` dep dropped (qrc) (pre-condition: done)
- AC gate: `rg '^\s*stoolap\s*=' crates/quota-router-core/Cargo.toml` → 0 hits
- Workspace direct `stoolap` dep count = 1 (substrate only)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green
- `cargo fmt --all -- --check` green

## §4 Cross-References

- RFC-0206 v2.3 (current re-export block — 6 types only)
- RFC-0206 v2.1 §Substrate Re-export Block (8 top-level cap)
- Mission `0206-008b-stoolap-dep-drop-cascade` (sibling; landed 2 of 3 crates)
- Mission `0206-008c-stoolap-pubsub-block-extension` (this amendment's mission)
- Research doc §14 (re-export block surface enumeration)

## §5 Lifecycle Requirements

- On Accept: substrate code lands per §1 diff (mandatory substrate re-export block narrowing).
- On Accept: v2.3 → v2.4 state bump; YYYY-MM-DD amendment date row appended to RFC-0206 VH.
- On Accept: `crates/octo-storage-core/src/lib.rs` line 84-98 comment block updated to reflect v2.4 surface.

## §6 Version History

| Version | Date | Change |
|---|---|---|
| 0.1.0 | 2026-08-24 | Initial draft. v2.3 → v2.4 amendment adds `pub mod pubsub` extension (6 types + 1 fn + nested `wal_pubsub::parse_event`). 14 names total under substrate. 8 top-level `pub use` cap UNCHANGED. |