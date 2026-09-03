//! Layer C re-export shim for `octo_coordinator_types::election`.
//!
//! Per RFC-0855p-b §Key Files L848, the `mon/election.rs` path is the
//! canonical Layer-C consumer entry-point for the §Phase 2 election
//! algorithm. We re-export rather than duplicate types per
//! [[cipherocto-design-principles]] §Stable Abstractions.
//!
//! See `octo-coordinator-types::election` for the canonical Layer-B
//! definitions.

pub use octo_coordinator_types::election::*;
