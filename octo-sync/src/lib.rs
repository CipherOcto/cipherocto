//! # octo-sync
//!
//! Wire-protocol primitives, the [`DatabaseSyncAdapter`] trait, and Stoolap sync types
//! for the CipherOcto Stoolap Data Sync Protocol (RFC-0862 v1.1.0).
//!
//! This crate lives at `cipherocto/octo-sync/`, a **leaf workspace** excluded from the
//! main cipherocto workspace via `workspace.exclude`. Both the cipherocto workspace and
//! the Stoolap fork depend on this crate via git. The leaf-workspace pattern mirrors
//! the existing `octo-determin` pattern (see `/home/mmacedoeu/_w/ai/cipherocto/determin/`).
//!
//! # Architecture
//!
//! ```text
//!                octo-sync (this crate, leaf workspace)
//!                ├── wire primitives (envelopes, Merkle tree, OCrypt sync context)
//! ├── DatabaseSyncAdapter trait
//! ├── SyncError enum
//! ├── WireError enum
//! ├── type aliases (Lsn, MissionId, NodeId, TableId, SegmentIndex)
//! └── MockAdapter test util
//!                        ▲                  ▲
//!                        │ trait bound      │ impl
//!                        │                  │
//!              cipherocto workspace        stoolap fork
//!              crates/octo-network/        crates/sync-adapter/
//!              (consumer: bridge)          (provider: StoolapAdapter)
//! ```
//!
//! # Why sync (not async)?
//!
//! The cipherocto convention is `Send + Sync` on the trait itself. Compute/state traits
//! (`Witness`, `DeterministicProofSystem`, `BINDHook`) are sync; transport traits
//! (`PlatformAdapter`, `CoordinatorAdmin`) are async. Database operations are local
//! disk I/O, not network I/O — they sit on the compute/state side. The cipherocto
//! async runtime (`tokio`) wraps every trait call at the boundary via
//! `tokio::task::spawn_blocking`.
//!
//! # Modules
//!
//! - [`adapter`] — the [`DatabaseSyncAdapter`] trait (8 methods)
//! - [`apply`] — the reader-side WAL apply wrapper
//! - [`config`] — [`SyncConfig`] and [`SyncRole`]
//! - [`envelope`] — the 13 envelope types and [`EnvelopeKind`] discriminator
//! - [`error`] — the internal [`SyncError`] enum and the wire-level [`WireError`] enum
//! - [`identity`] — [`SyncNodeId`] and [`SyncPeerId`] derivation
//! - [`keyring_stub`] — the [`KeyRing`](keyring_stub::KeyRing) trait (interface only)
//! - [`lsn`] — the [`LsnTracker`](lsn::LsnTracker) per-peer LSN watermark
//! - [`state`] — the 7-state [`SyncLifecycle`] enum and transition table
//! - [`types`] — type aliases: [`Lsn`], [`MissionId`], [`NodeId`], [`TableId`], [`SegmentIndex`]
//! - [`test_util`] — the [`MockAdapter`](test_util::MockAdapter) test util

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapter;
pub mod apply;
pub mod config;
pub mod envelope;
pub mod error;
pub mod identity;
pub mod keyring_stub;
pub mod lsn;
pub mod state;
pub mod types;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

pub use adapter::DatabaseSyncAdapter;
pub use config::{SyncConfig, SyncRole};
pub use envelope::{
    EnvelopeKind, Heartbeat, LsnAck, SummaryRequest, WalTailChunk,
};
pub use error::{SyncError, WireError};
pub use identity::{SyncNodeId, SyncPeerId};
pub use lsn::LsnTracker;
pub use state::{Peer, StateTransition, SyncLifecycle, TransitionTrigger};
pub use types::{Lsn, MissionId, NodeId, SegmentIndex, TableId};
