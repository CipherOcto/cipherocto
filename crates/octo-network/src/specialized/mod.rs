//! Specialized Node Record substrate (RFC-0871).
//!
//! Per-node canonical record for the CipherOcto protocol.
//! Per-extension transport impl crates (Layer D) are OUT
//! OF SCOPE per per-extension crate pattern
//! (RFC-0011-q Phase 9 §Out of Scope); this module exposes
//! only the Layer B substrate projection consumed by
//! `octo network node show` (read) + `octo network node
//! bind <node_id_hex> --holder-did <did>` (mutating) per
//! RFC-0011-q Phase 9 §Subcommand Taxonomy.

pub mod node_record;

pub use node_record::{
    HolderDid, NodeClass, SpecializedNodeError, SpecializedNodeRecord, SpecializedNodeRecordAccess,
};
