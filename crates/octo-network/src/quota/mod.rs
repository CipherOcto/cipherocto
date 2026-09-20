//! Quota Router Network substrate (RFC-0870).
//!
//! Distributed quota router network state for the CipherOcto
//! protocol. Per-extension transport impl crates (Layer D)
//! are OUT OF SCOPE per per-extension crate pattern
//! (RFC-0011-p Phase 8 §Out of Scope); this module exposes
//! only the Layer B substrate projection consumed by
//! `octo network router status` + `octo network router peers`
//! (RFC-0011-p Phase 8 §Subcommand Taxonomy).

pub mod router_node;

pub use router_node::{QuotaRouterNode, RouterStatus};
