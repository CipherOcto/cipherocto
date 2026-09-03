//! Layer C re-export shim for `octo_coordinator_types::slashing`.
//!
//! Per RFC-0855p-b §Key Files L849, the `mon/slashing.rs` path is the
//! canonical Layer-C consumer entry-point for the §Phase 5 slashing
//! verification. We re-export rather than duplicate types per
//! [[cipherocto-design-principles]] §Stable Abstractions.
//!
//! See `octo-coordinator-types::slashing` for the canonical Layer-B
//! definitions.

pub use octo_coordinator_types::slashing::*;
