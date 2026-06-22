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
//! - [`error`] — the internal [`SyncError`] enum and the wire-level [`WireError`] enum
//! - [`types`] — type aliases: [`Lsn`], [`MissionId`], [`NodeId`], [`TableId`], [`SegmentIndex`]
//! - [`test_util`] — the [`MockAdapter`](test_util::MockAdapter) test util

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapter;
pub mod error;
pub mod types;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

pub use adapter::DatabaseSyncAdapter;
pub use error::{SyncError, WireError};
pub use types::{Lsn, MissionId, NodeId, SegmentIndex, TableId};
