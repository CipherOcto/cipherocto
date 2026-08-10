//! Composite catalog (mission 0959-c4) — combines a storage
//! `CapabilityCatalog` with an async `CapabilityGossip` into a single
//! dispatch handle.

pub mod composite;

pub use composite::{CompositeCapabilityCatalog, CompositeGossip};
