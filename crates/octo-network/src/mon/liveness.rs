//! Layer C re-export shim for `octo_coordinator_types::liveness`.
//!
//! Per RFC-0855p-b §Implementation Phase 3, the `mon/liveness.rs` path
//! is the canonical Layer-C consumer entry-point for the heartbeat
//! substrate. We re-export rather than duplicate types per
//! [[cipherocto-design-principles]] §Stable Abstractions.
//!
//! See `octo-coordinator-types::liveness` for the canonical Layer-B
//! definitions.

pub use octo_coordinator_types::liveness::*;
