//! `octo-ident-storage` — persistent DID registry adapter (mission 0206-005).
//!
//! Sole home of the Stoolap-backed `StoolapDidRegistry` impl per
//! RFC-0206 v2.1 §Adapter Crate List row 3 (formerly in
//! `crates/quota-router-storage/src/stoolap_did_registry.rs`; move owned by
//! mission 0206-003).
//!
//! ## Layer model
//!
//! - Layer A substrate (`octo_storage_core::Database`) — RFC-frozen
//! - Layer B trait declarer (`octo_ident::DidRegistry`) — RFC-0010 v1.3
//! - This crate (adapter) — Layer C, per RFC-0206 v2.1 §Three-Tier
//!   Architecture
//!
//! The crate declares NO direct `stoolap` dep (per RFC-0205 §Cargo.toml
//! Pinning Layer A); the substrate is the sole fork consumer.

#![forbid(unsafe_code)]

pub mod did_registry;

pub use did_registry::{DidRegistryStorageError, StoolapDidRegistry, MAINNET_CHAIN_ID_BYTES};