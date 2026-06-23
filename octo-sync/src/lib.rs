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
//!                ├── wire primitives
//!                │   ├── envelope (13 envelope types + EnvelopeKind)
//!                │   ├── summary (16-way Merkle tree over segments)
//!                │   ├── keyring (HKDF-BLAKE3 + ChaCha20-Poly1305 AEAD)
//!                │   ├── replay_cache (per-peer BTreeMap, 10K bound)
//!                │   ├── stream (WalTailStreamer with adapter)
//!                │   ├── segment (SegmentIndexer with adapter)
//!                │   ├── dgp_bridge (DGP SnapshotFragment dispatch)
//!                │   ├── carrier (multi-carrier broadcaster)
//!                │   └── raft_overlay (deferred per RFC-0862 §Future Work F1/F8)
//!                ├── state machine
//!                │   ├── state (7-state SyncLifecycle + transition table)
//!                │   ├── lsn (LsnTracker per-peer watermark)
//!                │   ├── identity (SyncNodeId, SyncPeerId)
//!                │   └── config (SyncConfig, SyncRole)
//!                ├── integration
//!                │   ├── adapter (DatabaseSyncAdapter trait — 9 methods)
//!                │   ├── error (SyncError → WireError mapping)
//!                │   ├── types (Lsn, MissionId, NodeId, TableId, SegmentIndex)
//!                │   └── test_util (MockAdapter, gated on test-util feature)
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
//! - [`adapter`] — the [`DatabaseSyncAdapter`] trait (9 methods: 8 RFC-0862 ops + 1 regeneration)
//! - [`config`] — [`SyncConfig`] and [`SyncRole`]
//! - [`envelope`] — the 13 envelope types and [`EnvelopeKind`] discriminator
//! - [`error`] — the internal [`SyncError`] enum and the wire-level [`WireError`] enum
//! - [`identity`] — [`SyncNodeId`] and [`SyncPeerId`] derivation
//! - [`keyring`] — the [`KeyRing`](keyring::KeyRing) trait and [`MissionKeyRing`](keyring::MissionKeyRing) impl
//! - [`lsn`] — the [`LsnTracker`](lsn::LsnTracker) per-peer LSN watermark
//! - [`carrier`] — the multi-carrier broadcaster (mission 0862g)
//! - [`dgp_bridge`] — the DGP sync bridge (mission 0862f)
//! - [`replay_cache`] — the per-peer ReplayCache (mission 0862e; in-memory variant)
//! - [`segment`] — the snapshot segment indexer (mission 0862c)
//! - [`state`] — the 7-state [`SyncLifecycle`] enum and transition table
//! - [`stream`] — the writer-side [`WalTailStreamer`](stream::WalTailStreamer) (mission 0862a)
//! - [`summary`] — the per-table Merkle segment summary builder (mission 0862b)
//! - [`raft_overlay`] — the deferred Raft overlay (mission 0862i; v1 `apply()` only)
//! - [`types`] — type aliases: [`Lsn`], [`MissionId`], [`NodeId`], [`TableId`], [`SegmentIndex`]
//! - [`test_util`] — the [`MockAdapter`](test_util::MockAdapter) test util (gated on `test-util` feature)

#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod adapter;
pub mod carrier;
pub mod config;
pub mod dgp_bridge;
pub mod envelope;
pub mod error;
pub mod identity;
pub mod keyring;
pub mod lsn;
pub mod raft_overlay;
pub mod replay_cache;
pub mod segment;
pub mod session;
pub mod state;
pub mod stream;
pub mod summary;
pub mod types;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

pub use adapter::DatabaseSyncAdapter;
pub use carrier::MultiCarrierSync;
pub use config::{SyncConfig, SyncRole};
pub use dgp_bridge::{DgpSyncBridge, GossipSnapshotFragment};
pub use envelope::{
    AuthChallenge, AuthResponse, EnvelopeKind, Heartbeat, LsnAck, NodeStatus, SegmentNotFound,
    SegmentRequest, SummaryRequest, SummaryResponse, WalTailChunk, WalTailEnd, WalTailRequest,
};
pub use error::{SyncError, WireError};
pub use identity::{SyncNodeId, SyncPeerId};
pub use keyring::MissionKeyRing;
pub use lsn::LsnTracker;
pub use raft_overlay::{RaftEntry, RaftOverlay, RaftRole};
pub use replay_cache::{ReplayCache, ReplayCacheManager};
pub use segment::{SegmentIndexer, SegmentLookupResult, SyncSegment};
pub use session::{PeerSession, SyncSessionManager};
pub use state::{Peer, StateTransition, SyncLifecycle, TransitionTrigger};
pub use stream::{CommitError, RateLimiter, SubscriberChannel, WalTailStreamer};
pub use summary::{MerkleSegmentTree, SegmentMetadata, SyncSummary};
pub use types::{Lsn, MissionId, NodeId, SegmentIndex, TableId};
