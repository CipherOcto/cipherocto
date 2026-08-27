//! `octo-ident-storage` — persistent DID registry adapter (mission 0206-005).
//!
//! Sole home of the Stoolap-backed `StoolapDidRegistry` impl per
//! RFC-0206 §Adapter Crate List row 3 (formerly in
//! `crates/quota-router-storage/src/stoolap_did_registry.rs`; move owned by
//! mission 0206-003).
//!
//! ## Layer model
//!
//! - Layer A substrate (`octo_storage_core::Database`) — RFC-frozen
//! - Layer B trait declarer (`octo_ident::DidRegistry`) — RFC-0010
//! - This crate (adapter) — Layer C, per RFC-0206 §Three-Tier
//!   Architecture
//!
//! The crate declares NO direct `stoolap` dep (per RFC-0205 §Cargo.toml
//! Pinning Layer A); the substrate is the sole fork consumer.
//!
//! ## Migration runner dep
//!
//! `StoolapDidRegistry::open_in_memory` / `open_path` call
//! `quota_router_storage::migrations::apply_pending` to apply the
//! `did_registry` schema (v008..v011). Per mission 0206-003 R1 review,
//! the substrate's `apply_pending` skips migrations with version <=
//! current DB version, so the schema migrations stay in quota-router-storage
//! (sole source of truth for the `cipherocto_schema_version` tracker
//! table). This crate is a Layer C adapter; its dep on quota-router-storage
//! (Layer B) is the canonical pattern.

#![forbid(unsafe_code)]

pub mod did_registry;
pub mod holder_registry;

pub use did_registry::{DidRegistryStorageError, StoolapDidRegistry, MAINNET_CHAIN_ID_BYTES};
pub use holder_registry::StoolapHolderRegistry;
