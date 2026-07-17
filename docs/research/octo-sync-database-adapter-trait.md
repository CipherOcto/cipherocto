# The `DatabaseSyncAdapter` Trait: Interface-Driven Integration of Stoolap Fork and CipherOcto Network (Phase 2)

**Status:** Draft (awaiting adversarial review)
**Date:** 2026-06-21
**Author:** @cipherocto (research)
**Trigger:** Phase 2 of `docs/research/stoolap-dep-on-cipherocto-circular-avoidance.md` recommends defining a `DatabaseSyncAdapter` trait as the in-process integration boundary between the Stoolap fork and the cipherocto sync engine.

## 1. Problem Statement

Phase 1 of the dep-avoidance research extracts an `octo-sync` leaf workspace that contains the wire-protocol primitives (envelopes, Merkle tree, OCrypt sync context, ReplayCache, SegmentIndexer, etc.). Both `cipherocto` and `stoolap` fork depend on `octo-sync` via git, breaking the Cargo workspace cycle at the workspace-graph level.

**However, `octo-sync` alone is not enough.** The cipherocto sync engine must read WAL entries from the database (writer side) and apply WAL entries to the database (reader side). It must also write and read snapshot segments. The current Phase 1 design has the cipherocto sync engine calling `stoolap` DB functions directly — which re-creates the very Cargo dep cycle that Phase 1 broke.

**The solution: define a Rust trait `DatabaseSyncAdapter` in `octo-sync` that abstracts these operations.** The `stoolap` fork provides a `StoolapAdapter` implementation; the cipherocto sync engine consumes any `T: DatabaseSyncAdapter`. **No Cargo dep cycle.** The dep is replaced by a trait bound.

This research doc designs the trait precisely, informed by:
- The existing `PlatformAdapter` trait (RFC-0850 §8.2) at `crates/octo-network/src/dot/adapters/mod.rs:75`
- The existing `CoordinatorAdmin` trait (RFC-0850 §8 extension) at `crates/octo-network/src/dot/adapters/coordinator_admin.rs:429`
- The existing `Witness` trait (RFC-0854 §6, DPS) at `crates/octo-network/src/dps/witness.rs:9`
- The existing `DeterministicProofSystem` trait (RFC-0854 / DPS) at `crates/octo-network/src/dps/trait_def.rs:15`
- The `octo-network` BINDHook trait (RFC-0850p-c, in the "Cross-platform adapter hook" sub-section) at `crates/octo-network/src/dot/witness.rs:396`
- The RFC-0862 wire protocol and the 0862-base / 0862a–0862i missions
- The `octo-determin` leaf workspace pattern (the existing template for sharing cipherocto crates with the stoolap fork)

## 2. Existing Trait-Pattern Inventory

Cipherocto has **5 existing adapter-style traits**, all using the same `Send + Sync` shape but differing on `async`:

| Trait | File | `async_trait`? | Default impl for optional methods? | Use case |
|---|---|---|---|---|
| `PlatformAdapter` | `crates/octo-network/src/dot/adapters/mod.rs:75` | yes (`#[async_trait]`) | yes (`upload_media`/`download_media` return `Unreachable`) | DOT envelope transport per platform |
| `CoordinatorAdmin` | `crates/octo-network/src/dot/adapters/coordinator_admin.rs:429` | yes (`#[async_trait]`) | yes (every method returns `Unimplemented`) | Group management per platform |
| `Witness` | `crates/octo-network/src/dps/witness.rs:9` | no (sync) | n/a | Witness signing for DPS proof generation |
| `DeterministicProofSystem` | `crates/octo-network/src/dps/trait_def.rs:15` | no (sync) | n/a | DPS proof system (STWO, etc.) |
| `BINDHook` | `crates/octo-network/src/dot/witness.rs:396` | no (sync) | n/a | Hook for BIND/UNBIND events |

**Three observations from this inventory:**

1. **4 of 5 traits have `Send + Sync` on the trait itself** (`PlatformAdapter`, `CoordinatorAdmin`, `Witness`, `BINDHook`); the 5th (`DeterministicProofSystem`) places `Send + Sync` on its associated types (`type Proof: Clone + Send + Sync;` etc.) rather than on the trait itself. The cipherocto convention is `Send + Sync`; DatabaseSyncAdapter MUST follow it on the trait itself.
2. **2 of 5 are async (`PlatformAdapter`, `CoordinatorAdmin`); 3 of 5 are sync.** The async ones are the "transport" / "I/O-bound" traits; the sync ones are the "compute" / "state" traits. DatabaseSyncAdapter falls on the I/O-bound side (DB reads/writes), so it could go either way.
3. **All traits that have optional methods use a default `Unimplemented` (or equivalent) return.** This is the cipherocto convention for "the trait surface is wider than any one implementer needs to support". DatabaseSyncAdapter does NOT need optional methods (every implementer must support all of them — you can't have a database that can't read WAL), so the `Unimplemented` default is unnecessary here.

## 3. What the Sync Protocol Actually Needs

The RFC-0862 wire protocol involves 5 operations on the underlying database. The trait must expose all 5:

| RFC-0862 op | Direction | Required trait method | Caller |
|---|---|---|---|
| `read_wal_range(from_lsn, to_lsn)` | writer-side | `read_wal_range(from: Lsn, to: Lsn) -> Result<Vec<Vec<u8>>>` | 0862a `WalTailStreamer::on_commit` (writer fans out the chunk to subscribers) |
| `current_lsn()` | both sides | `current_lsn() -> Result<Lsn>` | 0862a `WalTailStreamer::current_lsn` (for monotonicity checks and `is_last` computation) |
| `apply_wal_entry(entry)` | reader-side | `apply_wal_entry(entry: &[u8]) -> Result<()>` | 0862a `WalTailStreamer::on_lsn_ack` (reader applies the entry after receiving it) |
| `read_snapshot_segment(table_id, segment_index)` | reader-side | `read_snapshot_segment(table_id: u32, segment_index: u32) -> Result<Option<SnapshotSegment>>` | 0862c `SegmentIndexer::find_segment_file` (reader descends the Merkle tree) |
| `write_snapshot_segment(table_id, segment_index, payload)` | writer-side | `write_snapshot_segment(table_id: u32, segment_index: u32, payload: &[u8]) -> Result<()>` | 0862c `SegmentIndexer::regenerate_snapshot` (writer regenerates a missing segment) |

**One additional operation** is needed for backpressure (per RFC-0862 §4.3.2, R12-N12 resolution):

| Operation | Direction | Required trait method | Caller |
|---|---|---|---|
| `set_paused(paused: bool)` | reader → writer | `set_paused(paused: bool)` | 0862a `WalTailStreamer::set_paused` (reader sends PAUSE when apply queue > 10K) |

**Two auxiliary methods** are needed for the cipherocto sync engine to integrate cleanly:

| Method | Purpose | Returns |
|---|---|---|
| `mission_id()` | The `MissionKeyHierarchy` (RFC-0853) keys are derived per-mission; the sync engine needs the mission ID to derive the `transport_key` and `execution_key` (per RFC-0862 §4.3.1 and mission 0862d) | `Result<MissionId>` (type alias for `[u8; 32]`) |
| `node_id()` | The `SyncNodeId = BLAKE3(public_key || mission_id)` is per-node; the sync engine needs the local node's `OverlayIdentity.public_key` (or a stable equivalent) | `Result<NodeId>` (type alias for `[u8; 32]`) |

**Total: 8 trait methods** (5 RFC-0862 operations + 1 backpressure + 2 auxiliary).

## 4. Trait Design

### 4.1 The full trait

```rust
// octo-sync/src/adapter.rs

use crate::error::SyncError;
use crate::snapshot::SnapshotSegment;

/// Type aliases used in the trait signatures. Defined in `octo-sync/src/types.rs`:
/// - `type Lsn = u64;` — WAL Logical Sequence Number (monotonic per writer)
/// - `type MissionId = [u8; 32];` — Mission identifier (per RFC-0853 MissionKeyHierarchy)
/// - `type NodeId = [u8; 32];` — `SyncNodeId = BLAKE3(public_key || mission_id)`
/// - `type TableId = u32;` — Database table identifier (assigned by the underlying engine)
/// - `type SegmentIndex = u32;` — Ordinal position of a snapshot segment
use crate::types::{Lsn, MissionId, NodeId, TableId, SegmentIndex};

/// Adapter trait that the cipherocto sync engine uses to read and write
/// the underlying database. The trait is **sync** (not async) so that
/// implementations on synchronous database engines (e.g. the Stoolap fork,
/// which is built on a synchronous `std` core) don't need a `tokio` runtime.
///
/// The cipherocto async runtime (`octo-network` via `tokio::task::spawn_blocking`)
/// wraps every trait call when called from an async context. Implementations
/// may return `SyncError::BackendNotReady` if the database is in a state
/// that cannot service the request (e.g. the DB is shutting down); the cipherocto
/// sync engine treats this as a transient error and retries with backoff.
///
/// # Send + Sync
///
/// The trait requires `Send + Sync` (the cipherocto convention; see e.g.
/// `PlatformAdapter: Send + Sync` at `crates/octo-network/src/dot/adapters/mod.rs:75`).
/// This means the underlying database must be safe to access from multiple
/// threads. For the Stoolap fork, this requires the adapter to wrap the
/// `MVCCEngine` in a `parking_lot::Mutex` or similar (per the
/// 0862-base `Arc<...>` patterns).
///
/// # Error model
///
/// Every method returns `Result<T, SyncError>`. The cipherocto sync engine
/// maps `SyncError` variants to the 9 wire-level error codes
/// (RFC-0862 §Error Handling) at the transport boundary
/// (see `octo-sync/src/error.rs` for the `impl From<SyncError> for WireError`
/// mapping table).
///
/// # `Send + Sync + 'static` bounds
///
/// The trait requires `Send + Sync` (the cipherocto convention; see e.g.
/// `PlatformAdapter: Send + Sync` at `crates/octo-network/src/dot/adapters/mod.rs:75`).
/// The `+ 'static` bound is added to allow the trait object to be stored in
/// `Box<dyn DatabaseSyncAdapter + 'static>` and to satisfy `'static` requirements
/// of the cipherocto async runtime (e.g. `tokio::task::spawn_blocking`).
/// None of the 5 existing cipherocto adapter traits have this bound; it is
/// a new addition justified by the trait-object storage pattern, not a
/// pre-existing convention. The Stoolap implementer wraps `MVCCEngine` in
/// `Arc<parking_lot::Mutex<MVCCEngine>>` to satisfy the bounds.
pub trait DatabaseSyncAdapter: Send + Sync + 'static {
    // ── A. WAL operations (RFC-0862 §4.3.3) ──────────────────────────────

    /// Read WAL entries in the range `[from_lsn, to_lsn]` (inclusive on
    /// both ends). Returns the raw `WALEntry::encode()` bytes (not parsed)
    /// so the sync engine can ship them verbatim per RFC-0862 §4.2.
    ///
    /// MUST be monotonic: if `from_lsn < current_lsn()`, the call returns
    /// only the entries with LSN ≥ `from_lsn`; entries with LSN < `from_lsn`
    /// are silently dropped (they've already been shipped). The cipherocto
    /// sync engine relies on this to handle restart-after-crash correctly.
    ///
    /// MUST return `Err(SyncError::InvalidLsnRange)` if `from_lsn > to_lsn`.
    fn read_wal_range(
        &self,
        from_lsn: Lsn,
        to_lsn: Lsn,
    ) -> Result<Vec<Vec<u8>>, SyncError>;

    /// Return the current LSN of the database (highest LSN that has been
    /// committed). MUST be monotonic across calls (LSN counters are
    /// append-only per the WAL V2 binary format at
    /// `stoolap/src/storage/mvcc/wal_manager.rs:69`).
    fn current_lsn(&self) -> Result<Lsn, SyncError>;

    /// Apply a single WAL entry to the database. The entry is the raw
    /// `WALEntry::encode()` output (not parsed). The cipherocto sync engine
    /// calls this on the reader side after a successful `WalTailChunk`
    /// reception and a verified `LsnAck`.
    ///
    /// MUST be idempotent: replaying the same entry twice is a no-op (the
    /// WAL V2 binary format is designed for this; see
    /// `stoolap/src/storage/mvcc/persistence.rs:549`,
    /// `PersistenceManager::replay_two_phase`).
    fn apply_wal_entry(&self, entry: &[u8]) -> Result<(), SyncError>;

    // ── B. Snapshot operations (RFC-0862 §4.3.4) ──────────────────────────

    /// Read the snapshot segment at ordinal position `segment_index` in the
    /// snapshot directory for `table_id`. Returns `Ok(Some(segment))` if the
    /// file exists, `Ok(None)` if no file at that position (the reader
    /// interprets `None` as a signal to descend the Merkle tree or request
    /// a different ordinal).
    ///
    /// MUST return the segment with the **uncompressed** payload (the cipherocto
    /// sync engine applies its own LZ4 compression per RFC-0862 §4.3.4). The
    /// `STSVSHD` magic and atomic-rename semantics (per
    /// `stoolap/src/storage/mvcc/snapshot.rs:37,98`) are the underlying
    /// database's responsibility.
    fn read_snapshot_segment(
        &self,
        table_id: TableId,
        segment_index: SegmentIndex,
    ) -> Result<Option<SnapshotSegment>, SyncError>;

    /// Write a snapshot segment at ordinal position `segment_index` in the
    /// snapshot directory for `table_id`. The `payload` is the uncompressed
    /// segment bytes (typically the full `snapshot-<ts>.bin` file). Returns
    /// once the segment is durably written (atomic-rename completed).
    ///
    /// MUST be atomic: either the segment is fully visible to subsequent
    /// `read_snapshot_segment` calls, or it is not visible at all. The
    /// atomic-rename pattern at
    /// `stoolap/src/storage/mvcc/engine.rs:2642` / `:2828` is the
    /// canonical implementation.
    fn write_snapshot_segment(
        &self,
        table_id: TableId,
        segment_index: SegmentIndex,
        payload: &[u8],
    ) -> Result<(), SyncError>;

    // ── C. Backpressure (RFC-0862 §4.3.2, R12-N12) ────────────────────────

    /// Set or clear the writer's pause flag. The cipherocto sync engine
    /// calls this when the reader's apply queue exceeds 10K entries (per
    /// RFC-0862 §Implicit Assumptions Audit row 6). When `paused = true`,
    /// the writer skips fan-out in `WalTailStreamer::on_commit` (per
    /// 0862a lines 92-96); the LSN counter still advances. When
    /// `paused = false`, normal fan-out resumes.
    ///
    /// Default: implementers may treat this as a no-op (return `Ok(())`)
    /// if they do not support backpressure. The cipherocto sync engine
    /// will fall back to per-peer rate-limiting in that case.
    fn set_paused(&self, paused: bool) -> Result<(), SyncError> {
        let _ = paused;
        Ok(())
    }

    // ── D. Identity (RFC-0862 §4.3.1) ────────────────────────────────────

    /// Return the mission ID that this database instance is bound to.
    /// The cipherocto sync engine uses this to derive the per-mission
    /// `transport_key` and `execution_key` via `HKDF-BLAKE3(mission_root_key,
    /// "sync:v1", mission_id)` (per RFC-0862 §4.3.1 and mission 0862d).
    /// Returns the `MissionId` (a type alias for `[u8; 32]`, defined in
    /// `octo-sync/src/types.rs`).
    fn mission_id(&self) -> Result<MissionId, SyncError>;

    /// Return the local node's `SyncNodeId = BLAKE3(public_key || mission_id)`.
    /// MUST be stable for the lifetime of the sync session (per RFC-0862
    /// §Implicit Assumptions Audit row 5: "Node identity is stable for
    /// the duration of a sync session"). The cipherocto sync engine caches
    /// this value at session start. Returns the `NodeId` (a type alias for
    /// `[u8; 32]`, defined in `octo-sync/src/types.rs`).
    fn node_id(&self) -> Result<NodeId, SyncError>;
}
```

### 4.2 Why sync (not async)?

`octo-network` uses `#[async_trait]` for the transport-side traits (`PlatformAdapter`, `CoordinatorAdmin`) because every transport operation is a network round-trip. The cipherocto async runtime (`tokio`) drives those.

`octo-sync`'s database operations are fundamentally **local disk I/O** — `read`/`write`/`fsync` of WAL and snapshot files. These are `std::fs` operations on the Stoolap fork; they do not benefit from an async runtime. Making the trait `async` would force every implementer to either:
- Hold a `tokio` runtime (which the Stoolap fork does not have), OR
- Wrap sync operations in `tokio::task::spawn_blocking` (the cipherocto side already does this for other sync operations, e.g. the WAL read at `0862a:101`)

By keeping the trait **sync**, the cipherocto sync engine's `octo-network-bridge` (the cipherocto-side wrapper) can call `tokio::task::spawn_blocking(move || adapter.read_wal_range(...))` once, at the boundary. Implementations stay simple and `std`-only.

This matches the existing convention: `Witness`, `DeterministicProofSystem`, and `BINDHook` (all compute/state traits, not transport) are sync.

### 4.3 Error model

The trait returns `Result<T, SyncError>`. The `SyncError` enum is defined in `octo-sync/src/error.rs` (per the RFC-0862 §Error Handling + R8-N9 mapping table). The full enum is:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("LSN regression: expected {expected}, got {actual}")]
    LsnRegression { expected: u64, actual: u64 },

    #[error("invalid LSN range: from {from} > to {to}")]
    InvalidLsnRange { from: u64, to: u64 },

    #[error("unknown peer: {0}")]
    UnknownPeer([u8; 32]),

    #[error("all carriers failed")]
    AllCarriersFailed,

    #[error("unknown envelope subtype: {0}")]
    UnknownEnvelopeSubtype(u8),

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("segment not found: table_id={table_id}, segment_index={segment_index}, regenerated={regenerated}")]
    SegmentNotFound { table_id: u32, segment_index: u32, regenerated: bool },

    #[error("unknown carrier: {0}")]
    UnknownCarrier(String),

    #[error("backend not ready: {0}")]
    BackendNotReady(String),
}
```

**Mapping to wire codes** (per RFC-0862 §Error Handling + R8-N9):

| Internal `SyncError` variant | Wire code | Used in |
|---|---|---|
| `LsnRegression { expected, actual }` | `E_SYNC_LSN_REGRESSION` | 0862a |
| `InvalidLsnRange { from, to }` | `E_SYNC_LSN_REGRESSION` (with extended detail) | 0862a |
| `UnknownPeer(SyncPeerId)` | `E_SYNC_AUTH_FAIL` (no such peer = auth fail) | 0862a |
| `AllCarriersFailed` | `E_SYNC_RATE_LIMIT` (all carriers failed = rate-limited) | 0862g |
| `UnknownEnvelopeSubtype(u8)` | `E_SYNC_AUTH_FAIL` (unknown subtype = corrupt/forged envelope) | 0862f |
| `DecryptionFailed` | `E_SYNC_AUTH_FAIL` (AEAD failure = auth fail) | 0862d |
| `SegmentNotFound { table_id, segment_index, regenerated }` | `E_SYNC_SEGMENT_NOT_FOUND` | 0862c |
| `UnknownCarrier(String)` | `E_SYNC_AUTH_FAIL` (no such carrier = bad config) | 0862g |
| `BackendNotReady(String)` | `E_SYNC_RATE_LIMIT` (backpressure signal; the reader's apply queue is full) | 0862a |

The mapping is implemented as `impl From<SyncError> for WireError` in `octo-sync/src/error.rs` (per the R8-N9 acceptance criterion on 0862-base:135-144).

### 4.4 What about async backpressure?

The `set_paused(paused: bool)` method is sync. The cipherocto sync engine's heartbeat handler calls it via `spawn_blocking` (one syscall, no async benefit). The default no-op implementation lets databases that don't support writer-side pause simply ignore the call; the cipherocto sync engine falls back to per-peer rate-limiting in that case.

A future Phase 3 could add a richer backpressure API (e.g., `register_backpressure_handler(Fn)`) — but for v1 (per RFC-0862 §Implementation Phases), the simple boolean is sufficient.

## 5. Phase 1 + Phase 2: the Combined Architecture

The hybrid A+D approach from the Phase 1 research translates to the following architecture:

```
                                    ┌──────────────────┐
                                    │   octo-sync      │
                                    │  (leaf workspace) │
                                    │                  │
                                    │  - envelope types │
                                    │  - Merkle tree   │
                                    │  - OCrypt keys   │
                                    │  - ReplayCache   │
                                    │  - SegmentIndex  │
                                    │  - SyncError     │
                                    │                  │
                                    │  DatabaseSync    │
                                    │  Adapter trait   │  ← Phase 2
                                    │  (sync, 8 methods)│
                                    └────────┬─────────┘
                                             │ git dep
                            ┌────────────────┼────────────────┐
                            │                                  │
                  ┌─────────▼─────────┐              ┌──────────▼──────────┐
                  │  cipherocto       │              │  stoolap fork       │
                  │  workspace         │              │  (separate repo)    │
                  │                    │              │                     │
                  │  crates/octo-       │              │  crates/sync-       │
                  │  network/src/      │              │  adapter/src/       │
                  │  sync_bridge/      │              │                     │
                  │  (cipherocto-side  │              │  impl Database-     │
                  │   bridge: spawn_   │              │  SyncAdapter for    │
                  │   blocking wrap)   │              │  the Stoolap MVCC   │
                  └────────────────────┘              │  engine             │
                                                     └─────────────────────┘
```

**The trait is the integration boundary.** Cargo deps are:

```toml
# cipherocto/crates/octo-network/Cargo.toml
[dependencies]
octo-sync = { path = "../../octo-sync" }   # path = "../../octo-sync" (Phase 1)

# stoolap fork/Cargo.toml
[dependencies]
octo-determin = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }
octo-sync     = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }   # Phase 1
```

The trait is **not** a Cargo dep — it's a trait bound. cipherocto's `octo-sync-bridge` is generic over `A: DatabaseSyncAdapter`, and at the integration point (a binary crate or feature-gated adapter that wires the two together), the Stoolap adapter is plugged in:

```rust
// hypothetical: cipherocto/crates/octo-sync-stoolap-bridge/src/lib.rs
// (only compiled when the cipherocto workspace enables the `stoolap` feature;
//  in the stoolap fork build, this bridge is not needed — the fork implements
//  the trait directly against its own MVCC engine)

use octo_sync::DatabaseSyncAdapter;
use octo_sync::snapshot::SnapshotSegment;
// ... other imports

pub struct StoolapAdapter {
    /// The Stoolap MVCC engine, wrapped in `parking_lot::Mutex` to satisfy
    /// the trait's `Send + Sync` bounds. Per RFC-0862 §4.3.3, the engine
    /// is read on the writer side (read_wal_range) and written on the
    /// reader side (apply_wal_entry); the Mutex serializes these.
    engine: Arc<parking_lot::Mutex<MVCCEngine>>,
}

impl DatabaseSyncAdapter for StoolapAdapter {
    fn read_wal_range(
        &self,
        from_lsn: Lsn,
        to_lsn: Lsn,
    ) -> Result<Vec<Vec<u8>>, SyncError> {
        // ... implementation per mission 0862a
    }
    // ... other 7 methods
}
```

For the cipherocto workspace, the `stoolap` feature pulls in `stoolap` as a dep (the `stoolap` package is declared in `Cargo.lock:8044` as `name = "stoolap"`) and the bridge connects it to `octo-sync`.

For the stoolap fork standalone build, the fork implements `DatabaseSyncAdapter` for its own `MVCCEngine` directly (no cipherocto runtime dep at all). The cipherocto workspace has nothing to do with the fork's standalone build.

**The Cargo workspace graph is now:**
- `octo-sync` (leaf workspace, excluded from both projects' workspaces)
- `cipherocto` (workspace) → `octo-network` (member) → `octo-sync` (git dep, internal path) → (no further cipherocto deps)
- `stoolap` fork (separate repo, single-package Cargo manifest) → `octo-sync` (git dep) → (no further cipherocto deps)

**No cycle. The trait is the boundary.**

## 6. Testability

The trait is designed for testability. A mock implementation looks like:

```rust
// octo-sync/src/test_util.rs (provided by the library)
pub struct MockAdapter {
    pub wal: parking_lot::Mutex<Vec<(u64, Vec<u8>)>>,
    pub snapshots: parking_lot::Mutex<HashMap<(u32, u32), Vec<u8>>>,
    pub lsn: AtomicU64,
    pub paused: AtomicBool,
}

impl DatabaseSyncAdapter for MockAdapter {
    fn read_wal_range(&self, from: Lsn, to: Lsn) -> Result<Vec<Vec<u8>>, SyncError> {
        Ok(self.wal.lock().iter()
            .filter(|(lsn, _)| *lsn >= from && *lsn <= to)
            .map(|(_, entry)| entry.clone())
            .collect())
    }
    // ... 7 more methods with trivial test implementations
}
```

This allows the cipherocto sync engine to be unit-tested in isolation, without a real Stoolap database. The 9 cipherocto missions that depend on the sync engine (0862a–0862i, per the Phase 1 doc) can use `MockAdapter` in their integration tests.

## 7. Forward Compatibility

The trait uses `Result<T, SyncError>` (not panics) for all fallible operations, so new error variants can be added without breaking implementers (they would just `match` the new variant when added).

For **new operations** (e.g., a Phase 3 `set_rate_limit(bps: u32)`), the cipherocto convention is:
- Add a new method with a default no-op implementation.
- Existing implementers are not broken (they get the default no-op).
- The cipherocto sync engine detects the missing override via the `admin_capabilities`-style `BackendCapabilities` report (to be added in Phase 3).

For **new error variants**, the convention is:
- Add to the `SyncError` enum.
- Update the `From<SyncError> for WireError` impl with a new mapping (or to a `WireError::Other(SyncError)` fallback).
- Implementers are not broken (Rust's `#[non_exhaustive]` attribute on the enum, if added in Phase 3, lets us add variants without breaking downstream `match` exhaustiveness).

## 8. Migration Path

| Phase | Artifact | Owner |
|---|---|---|
| **1** | New `octo-sync` leaf workspace (Phase 1 of dep-avoidance research) | cipherocto `octo-network` team |
| **2a** | Add `DatabaseSyncAdapter` trait to `octo-sync/src/adapter.rs` (this research) | cipherocto `octo-network` team |
| **2b** | Update `octo-network`'s sync engine to consume `dyn DatabaseSyncAdapter` (replaces direct `stoolap` DB calls) | cipherocto `octo-network` team |
| **2c** | Add `crates/sync-adapter/` to the stoolap fork with `StoolapAdapter` impl | stoolap fork maintainers |
| **2d** | Update missions 0862a–0862i to use the trait (replace direct DB calls with `adapter.read_wal_range(...)` etc.) | cipherocto `octo-network` team |
| **3** | (Optional) Add `BackendCapabilities` report + richer backpressure API | future RFC |

Each step is a separate PR. The trait itself (step 2a) is the smallest possible change — a single ~80-line file in `octo-sync/src/adapter.rs` — and can be merged independently of the consuming-side changes (step 2b).

## 9. Risks and Mitigations

| Risk | Severity | Mitigation |
|---|---|---|
| Implementers' `Send + Sync` requirement adds complexity (e.g. `Mutex` wrapping) | Medium | Provide a `MockAdapter` example in `octo-sync/src/test_util.rs` showing the `parking_lot::Mutex` pattern. |
| Sync trait vs. async trait debate | Low | Document the rationale (§4.2). Future Phase 3 can introduce `async-trait` if the use case changes. |
| Trait evolution (new methods) breaking implementers | Low | Use the cipherocto `default = Unimplemented` pattern for new optional methods; use `#[non_exhaustive]` on `SyncError` for new error variants. |
| Stoolap fork's `Send + Sync` requirement conflicts with its sync core | Low | The StoolapAdapter wraps `Arc<MVCCEngine>` in `parking_lot::Mutex<...>`; standard pattern, matches the `subscribers: parking_lot::Mutex<...>` field used elsewhere in mission 0862a (line 39). The underlying `MVCCEngine` is `Send + Sync` per the Stoolap fork's architecture (no `Rc`/`RefCell` in the engine's hot path). |

## 10. Open Questions

1. **Should the trait be `pub trait` or `pub sealed`?** The current 5 cipherocto traits are all `pub trait`. A sealed trait would prevent downstream extensions but improve compile times. For now: `pub trait` (matches the convention); revisit if extension becomes an issue.

2. **Should `set_paused` have a default no-op implementation, or should all implementers be required to support it?** Current proposal: default no-op (allows simpler databases). The cipherocto sync engine reads the per-peer pause state from the `octo-network` side, not from the database.

3. **Should the trait be `unsafe`-free?** Yes. All methods are safe; the cipherocto sync engine handles any unsafe operations (FFI, raw pointers) internally. The `Send + Sync` bound is sufficient.

## 11. Decision

**Define `DatabaseSyncAdapter` as a sync trait in `octo-sync/src/adapter.rs` with the 8 methods listed in §4.1.** Proceed with Phase 2 of the dep-avoidance research (steps 2a–2d in §8).

**Next BLUEPRINT artifacts:**
- A new Use Case `docs/use-cases/octo-sync-database-adapter.md` (formalizing the integration between `stoolap` and `octo-network` via the trait)
- A new RFC `rfcs/draft/networking/0863-database-sync-adapter-trait.md` (the trait specification)
- Missions: one per file in `octo-sync/src/`, plus a `stoolap-adapter` mission in the fork

## 12. Cross-References

- RFC-0850 §8.2 — DOT `PlatformAdapter` trait (the closest existing pattern). **(Accepted)**
- RFC-0850 §8 ext — `CoordinatorAdmin` trait (the `default = Unimplemented` pattern). **(Accepted)**
- RFC-0852 — DGP envelope types. **(Draft)**
- RFC-0853 — OCrypt `MissionKeyHierarchy` (where `mission_id()` comes from). **(Draft)**
- RFC-0854 — DPS `Witness` + `DeterministicProofSystem` traits (sync-trait pattern). **(Draft)**
- RFC-0862 — the wire protocol that motivates the trait. **(Accepted)**
- RFC-0863 (proposed) — the future RFC that will formalize this trait.
- `docs/research/stoolap-dep-on-cipherocto-circular-avoidance.md` — the Phase 1 research that this Phase 2 builds on. **(Draft)**
- `crates/octo-network/src/dot/adapters/mod.rs:75` — `PlatformAdapter` trait (the sync/async convention).
- `crates/octo-network/src/dot/adapters/coordinator_admin.rs:429` — `CoordinatorAdmin` trait (the `default = Unimplemented` convention).
- `crates/octo-network/src/dps/trait_def.rs:15` — `DeterministicProofSystem` trait (the sync-trait-without-default-implementations convention).
- `missions/open/networking/0862-base-stoolap-data-sync-core.md` — the base mission that this trait enables. **(Accepted)**
- `missions/open/networking/0862a-wal-tail-streamer.md` — the writer-side engine that consumes the trait. **(Accepted)**
- `docs/BLUEPRINT.md` — the canonical workflow that this research feeds.

---

**Review note:** This document is Draft. It must pass the BLUEPRINT Research Review Gate (minimum 2 maintainer reviewers) before promoting to Use Case → RFC → Missions.
