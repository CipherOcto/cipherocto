# Mission: 0862a — WAL-Tail Streamer

## Status

Draft (awaiting adversarial review)

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §4.3.3 WAL-tail streaming, §Implementation Phases Phase 0 + Phase 1, §DatabaseSyncAdapter Trait (v1.1.0)

## Summary

Implement the writer-side WAL-tail streamer: capture LSN ranges on every `TransactionEngineOperations::record_commit(txn_id)`, package them in `WalTailChunk` envelopes, ship to subscribed readers. The reader-side apply path consumes the chunks and applies entries via `adapter.apply_wal_entry(entry)` — the underlying DB write is delegated to the `StoolapAdapter` (mission 0862-base), **not** to `MVCCEngine::replay_two_phase` directly (per RFC-0862 v1.1.0 §Migration path step v1.1.0.d).

This mission is split out of `0862-base` for parallel execution. It depends on `0862-base` for the envelope types, identity derivation, state machine, **and the `DatabaseSyncAdapter` trait**, but ships independently as a focused module.

## Design

### New module: `octo-sync/src/stream.rs` (leaf workspace at `cipherocto/octo-sync/src/stream.rs`)

The streamer has three components:

1. **Commit capture hook** — wraps the writer's `record_commit` to capture `(previous_lsn+1, current_lsn)` ranges
2. **WalTailChunk packaging** — serializes captured entries via `WALEntry::encode()` (raw V2 binary) by calling `adapter.read_wal_range(from, to)`
3. **Subscription manager** — tracks active readers and pushes chunks to each one

### Pseudocode

```rust
// octo-sync/src/stream.rs
//
use octo_sync::DatabaseSyncAdapter;
use octo_sync::error::SyncError;
use octo_sync::types::Lsn;
use std::sync::Arc;

/// The subscribers map is wrapped in a `parking_lot::Mutex` to allow `&self` mutation
/// from both `on_commit` (which iterates over subscribers) and `on_lsn_ack` (which
/// updates the per-peer watermark). parking_lot's Mutex is faster than `std::sync::Mutex`
/// under contention and has no poisoning semantics; `lock()` returns the guard directly
/// without a `Result`.
///
/// The `adapter` field is `Arc<dyn DatabaseSyncAdapter>` (trait object) — the cipherocto
/// sync engine does NOT hold a direct `Arc<MVCCEngine>` reference. The Stoolap fork
/// provides the concrete `StoolapAdapter` impl (per mission 0862-base Phase 0).
pub struct WalTailStreamer {
    /// The database adapter (trait object). Per RFC-0862 v1.1.0 §DatabaseSyncAdapter
    /// Trait, the cipherocto sync engine consumes the trait, not the concrete
    /// `MVCCEngine`. WAL reads go through `adapter.read_wal_range(from, to)`.
    adapter: Arc<dyn DatabaseSyncAdapter>,
    subscribers: parking_lot::Mutex<HashMap<SyncPeerId, SubscriberChannel>>,
    rate_limiter: RateLimiter,
    current_lsn: AtomicU64,            // monotonic, persisted in WAL
    /// Per-txn error queue: drained every 100ms by the Sync engine.
    /// Each entry is a (txn_id, error) pair; the drain maps txn_id → subscribed peers
    /// via `txn_subscribers` and transitions each affected peer to Terminated.
    error_queue: parking_lot::Mutex<VecDeque<(TxnId, SyncError)>>,
    /// Per-peer state (SyncLifecycle 7-state enum per RFC-0862 §Lifecycle Requirements).
    /// `drain_error_queue` transitions a peer to Terminated when its subscribed
    /// txn produces an on_commit error.
    peers: parking_lot::Mutex<HashMap<SyncPeerId, PeerState>>,
    /// Maps each in-flight txn to the set of subscribers that were fanned-out.
    /// Populated by on_commit (when shipping the chunk), consumed by drain_error_queue
    /// (when mapping errors back to peers), cleared on txn acknowledgment.
    txn_subscribers: parking_lot::Mutex<HashMap<TxnId, Vec<SyncPeerId>>>,
    /// Backpressure flag: when the reader sends PAUSE, the writer stops shipping new
    /// chunks until it receives RESUME. Set by the heartbeat handler (not in v1's
    /// `on_commit` path). v1 implements this as a simple AtomicBool checked in `on_commit`
    /// before fan-out; the pause check is a no-op when no PAUSE has been received.
    paused: AtomicBool,
    commit_batch_size: usize,          // default 100 commits per chunk
    commit_batch_timeout: Duration,    // default 50ms
}

pub struct PeerState {
    pub state: SyncLifecycle,
    pub last_ack: u64,
}

impl WalTailStreamer {
    /// Called by the writer's record_commit hook.
    /// Returns Ok(()) on success, Err(SyncError) on a recoverable error
    /// (e.g., rate limit, peer channel closed, LSN regression, missing WAL range).
    /// LSN regression is a peer-Terminated event (per RFC-0862 §4.3.2 / §Lifecycle Requirements),
    /// NOT a process panic — see `E_SYNC_LSN_REGRESSION` in the error.rs map.
    ///
    /// `is_last` semantics: per RFC-0862 §4.3 `WalTailChunk.is_last: bool` is "true if to_lsn == writer.current_lsn".
    /// After the `store` on line 91, `current_lsn == to_lsn`, so this condition is always true.
    /// The flag is therefore unconditionally true; the per-batch "is this the last chunk in the batch"
    /// semantics are NOT the RFC's intent. A separate "batch flusher" is unnecessary in v1.
    pub fn on_commit(&self, txn_id: TxnId, from_lsn: u64, to_lsn: u64) -> Result<()> {
        // 1. Validate LSN monotonicity; on regression, return Err so the caller
        //    can transition the per-peer state machine to Terminated (RFC-0862 §Lifecycle).
        let prev = self.current_lsn.load(Ordering::SeqCst);
        if from_lsn != prev + 1 {
            return Err(SyncError::LsnRegression { expected: prev + 1, actual: from_lsn });
        }
        if to_lsn < from_lsn {
            return Err(SyncError::InvalidLsnRange { from: from_lsn, to: to_lsn });
        }
        // 2. Update current_lsn (advances even when paused, so the next non-paused
        //    commit computes the correct is_last value).
        self.current_lsn.store(to_lsn, Ordering::SeqCst);
        // 3. Check backpressure: if any peer has sent PAUSE, skip the fan-out.
        //    (Resolves R8-2: pause flag is now read before fan-out.)
        if self.paused.load(Ordering::SeqCst) {
            return Ok(());
        }
        // 4. Read WAL entries via the trait. Per RFC-0862 v1.1.0 §Migration path
        //    step v1.1.0.d, the cipherocto sync engine reads WAL through
        //    `adapter.read_wal_range(from, to)` — NOT via direct
        //    `self.engine.wal_manager().replay_two_phase(...)`. The trait is the
        //    integration boundary; the underlying `replay_two_phase` lives inside
        //    the StoolapAdapter impl.
        let entries = self.adapter.read_wal_range(from_lsn, to_lsn)?;
        // 5. Package as WalTailChunk with is_last = true (matches RFC-0862 §4.3).
        let chunk = WalTailChunk { from_lsn, to_lsn, entries, is_last: true };
        // 6. Fan-out to subscribers (rate-limited). Rate-limit and channel errors are
        //    returned to the caller; the caller decides whether to demote the peer.
        let (subscribers, mut txn_subs) = {
            let subs = self.subscribers.lock();
            let mut txn_subs = self.txn_subscribers.lock();
            // Track which peers were fanned out for this txn (so drain_error_queue
            //    can map errors back to affected peers).
            let peer_ids: Vec<SyncPeerId> = subs.keys().copied().collect();
            txn_subs.insert(txn_id, peer_ids);
            (subs, txn_subs)
        };
        // The mutex guards are held for the lifetime of the for-loop below.
        // This is correct: holding the locks during fan-out prevents new peers
        // from being added mid-iteration (which would be a race), at the cost of
        // a brief lock hold. The for-loop is non-blocking (channel.send is sync).
        for (peer_id, channel) in subscribers.iter() {
            self.rate_limiter.check(peer_id)?;
            channel.send(chunk.clone())?;
        }
        // Mutex guards are released here when the for-loop scope ends.
        drop(subscribers);
        drop(txn_subs);
        Ok(())
    }

    /// Set the pause flag (called by the heartbeat handler when a peer sends PAUSE).
    /// When paused, `on_commit` skips fan-out but the LSN counter still advances.
    /// Cleared on RESUME. The heartbeat handler is defined in mission 0862-base
    /// (not 0862a) as part of the unified envelope handler.
    ///
    /// The pause flag is also propagated to the underlying adapter via
    /// `adapter.set_paused(paused)`. This is a default no-op on the trait
    /// (databases that don't support writer-side pause ignore the call); for
    /// StoolapAdapter, the writer-side pause is implemented in the fork and
    /// gates `record_commit`'s WAL emission.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        let _ = self.adapter.set_paused(paused);
    }

    /// Reader's request for WAL entries from a given LSN.
    /// Returns `WalTailChunk` (the wire-level payload for envelope `0xB1 WalTailResponse`,
    /// per RFC-0862 §4.3 and §Envelope Payload Discriminators) containing the entries
    /// in `[from_lsn, current_lsn]` inclusive. The reader is responsible for applying
    /// the entries in LSN order via `adapter.apply_wal_entry(entry)` on the reader side.
    ///
    /// Implementation: inlines the adapter call into `tokio::task::block_in_place` because
    /// `DatabaseSyncAdapter` is a sync trait (per RFC-0862 v1.1.0 §Why sync (not async)?).
    /// The cipherocto async runtime wraps the sync call at the boundary. Returns
    /// `Err(SyncError::InvalidLsnRange)` if `from_lsn > current_lsn`. The `_peer` parameter
    /// is currently unused (prefixed with `_`) — a future per-peer rate-limit check would
    /// use it.
    pub async fn handle_wal_tail_request(
        &self,
        _peer: SyncPeerId,
        from_lsn: u64,
    ) -> Result<WalTailChunk> {
        let prev = self.current_lsn.load(Ordering::SeqCst);
        if from_lsn > prev {
            return Err(SyncError::InvalidLsnRange { from: from_lsn, to: prev });
        }
        if from_lsn == 0 {
            return Err(SyncError::InvalidLsnRange { from: 0, to: prev });
        }
        let adapter = self.adapter.clone();
        let (entries, to_lsn) = tokio::task::block_in_place(|| {
            let entries = adapter.read_wal_range(from_lsn, prev)?;
            Ok((entries, prev))
        })?;
        Ok(WalTailChunk {
            from_lsn,
            to_lsn,
            entries,
            is_last: to_lsn == prev,
        })
    }

    /// Reader sends LsnAck after successful apply.
    /// Takes &self (not &mut self) because `subscribers` is wrapped in a Mutex.
    pub fn on_lsn_ack(&self, peer: SyncPeerId, applied_lsn: u64) -> Result<()> {
        let mut subscribers = self.subscribers.lock();
        let sub = subscribers.get_mut(&peer)
            .ok_or(SyncError::UnknownPeer(peer))?;
        if applied_lsn < sub.last_ack {
            return Err(SyncError::LsnRegression {
                expected: sub.last_ack + 1, actual: applied_lsn
            });
        }
        sub.last_ack = applied_lsn;
        // Clear the per-txn → peers mapping for this peer's acknowledged txns.
        // For each txn that this peer was subscribed to, remove the peer from the
        // peer's vec. If a txn's vec becomes empty (all peers acknowledged), drop
        // the entry entirely. This prevents memory leak and ensures the drain_error_queue
        // mapping is correct.
        let mut txn_subs = self.txn_subscribers.lock();
        for peers in txn_subs.values_mut() {
            peers.retain(|p| *p != peer);
        }
        txn_subs.retain(|_, peers| !peers.is_empty());
        Ok(())
    }
}

// (BatchFlusher was added in a previous review round but has been removed:
// it had no input pipeline, could not legally call on_commit, and its is_last
// computation duplicated what on_commit can compute directly from the LSN.
// Per the RFC-0862 §4.3 definition "is_last = (to_lsn == writer.current_lsn)",
// and the post-store invariant that current_lsn == to_lsn, the flag is always true
// in v1. A future Phase 3 enhancement (per-batch flushing) may reintroduce a
// flusher with a different design, but it is out of scope for v1.)
```

### Stoolap fork changes (resolves N2, N3)

The Stoolap fork's `record_commit` hook now feeds the cipherocto sync engine via the `DatabaseSyncAdapter` trait, not by direct engine calls. The `WalTailStreamer` already holds an `Arc<dyn DatabaseSyncAdapter>` — the fork's job is to:

1. Build a `StoolapAdapter` (from mission 0862-base) and wrap it in `Arc<dyn DatabaseSyncAdapter>`
2. Construct the `WalTailStreamer` with that adapter
3. In `record_commit`, call `streamer.on_commit(txn_id, from_lsn, to_lsn)` — same as before; the trait boundary is internal to the streamer

```rust
// stoolap/src/storage/mvcc/transaction.rs
// record_commit is a trait method on TransactionEngineOperations (defined at
// transaction.rs:113 and implemented for EngineOperations at engine.rs:3479, 3681).
// The wal_manager provides `current_lsn()` (no txn_id arg) and `previous_lsn()`.
//
// On_commit errors (LSN regression, rate limit) cannot be returned via the
// trait method's return type (which is `()` in the current fork). The errors
// are logged via `tracing::error!` and routed to the Sync engine's per-txn
// error queue via `sync.streamer.record_commit_error(txn_id, e)`. The Sync
// engine polls this queue every 100ms; entries are mapped txn_id → subscribed
// peers and each affected peer is transitioned to Terminated.
impl TransactionEngineOperations for EngineOperations {
    fn record_commit(&self, txn_id: TxnId) {
        // existing logic
        let to_lsn = self.wal_manager.current_lsn();
        let from_lsn = self.wal_manager.previous_lsn();
        // existing logic
        // + invoke Sync engine if attached
        #[cfg(feature = "sync")]
        if let Some(sync) = &self.sync_engine {
            if let Err(e) = sync.streamer.on_commit(txn_id, from_lsn, to_lsn) {
                tracing::error!(error = ?e, "Sync on_commit error (peer demoted via error queue)");
                sync.streamer.record_commit_error(txn_id, e);
            }
        }
    }
}
```

```rust
// crates/octo-sync/src/stream.rs (continuation of the WalTailStreamer impl)
impl WalTailStreamer {
    /// Record an on_commit error for later per-peer demotion.
    /// The Sync engine polls this queue every 100ms; entries are mapped
    /// txn_id → set of subscribed peers and each affected peer is transitioned
    /// to Terminated (per RFC-0862 §Lifecycle Requirements).
    pub fn record_commit_error(&self, txn_id: TxnId, error: SyncError) {
        let mut queue = self.error_queue.lock();
        queue.push_back((txn_id, error));
    }

    /// Periodic poll (every 100ms) that demotes peers affected by recorded errors.
    pub fn drain_error_queue(&self) -> Vec<(SyncPeerId, SyncError)> {
        let mut queue = self.error_queue.lock();
        let mut affected = Vec::new();
        for (txn_id, error) in queue.drain(..) {
            // Look up the set of subscribers that were fanned-out for this txn
            let peer_ids: Vec<SyncPeerId> = self.txn_subscribers.lock()
                .get(&txn_id).cloned().unwrap_or_default();
            for peer_id in peer_ids {
                affected.push((peer_id, error.clone()));
                // Transition the peer to Terminated
                if let Some(peer) = self.peers.lock().get_mut(&peer_id) {
                    peer.state = SyncLifecycle::Terminated;
                }
            }
            // Clean up the txn → peers mapping
            self.txn_subscribers.lock().remove(&txn_id);
        }
        affected
    }

    /// Look up the set of subscribers that were fanned-out for a given txn.
    /// Used by `drain_error_queue`. Public for testability.
    pub fn subscribers_for_txn(&self, txn_id: TxnId) -> Vec<SyncPeerId> {
        self.txn_subscribers.lock()
            .get(&txn_id).cloned().unwrap_or_default()
    }
}
```

Note: `record_commit` is a trait method, not an inherent method on `MVCCEngine`. The pseudocode above shows the trait impl, not an `impl MVCCEngine` block. The actual method signatures of `WALManager::current_lsn` and `WALManager::previous_lsn` take no `txn_id` argument (see `wal_manager.rs:1282` and `wal_manager.rs:1353` respectively).

## Acceptance Criteria

- [ ] `octo-sync/src/stream.rs` (in the `octo-sync/` leaf workspace) exists with `WalTailStreamer` struct
- [ ] `WalTailStreamer` holds `adapter: Arc<dyn DatabaseSyncAdapter>` — NOT `engine: Arc<MVCCEngine>` (per RFC-0862 v1.1.0)
- [ ] `on_commit` reads WAL via `adapter.read_wal_range(from_lsn, to_lsn)` — NOT via direct `self.engine.wal_manager().replay_two_phase(...)` (per RFC-0862 v1.1.0 §Migration path step v1.1.0.d)
- [ ] `on_commit` captures LSN range `(from_lsn, to_lsn)` and packages as `WalTailChunk`
- [ ] `WalTailChunk` contains `from_lsn`, `to_lsn`, `entries: Vec<Vec<u8>>`, `is_last: bool`
- [ ] `is_last` is always `true` (post-store invariant: `to_lsn == current_lsn` is unconditionally true after `current_lsn.store(to_lsn, …)`)
- [ ] `handle_wal_tail_request` returns `WalTailChunk` with entries in `[from_lsn, current_lsn]` inclusive (also via `adapter.read_wal_range`)
- [ ] `on_lsn_ack` updates per-peer watermark
- [ ] Batch-by-count (default 100 commits per chunk) AND batch-by-time (default 50ms timeout) — whichever comes first
- [ ] Per-peer rate limit: 100 envelopes/s sustained, 500 burst (delegated to `rate_limit.rs`)
- [ ] LSN monotonicity: reject any chunk where `from_lsn != previous_chunk.to_lsn + 1`
- [ ] `set_paused` propagates to the adapter via `adapter.set_paused(paused)` (default no-op on the trait; the underlying StoolapAdapter may or may not implement it)
- [ ] `record_commit` hook invokes `WalTailStreamer::on_commit` when `sync` feature is enabled
- [ ] Unit tests for all 3 components (capture, package, subscribe) using `MockAdapter` (per mission 0862-base)
- [ ] Integration test: writer commits 10K rows in one transaction → reader receives one `WalTailChunk` with all 10K entries → reader applies via `adapter.apply_wal_entry` in LSN order → `BLAKE3-256(SELECT * FROM table)` matches

## Tests

- **Unit:**
  - `on_commit` packages LSN range correctly
  - `is_last` is **always `true`** (post-store invariant: `to_lsn == current_lsn`)
  - LSN monotonicity check rejects gaps
  - LSN monotonicity check rejects duplicates
  - `handle_wal_tail_request` returns empty response when `from_lsn > current_lsn`
  - `handle_wal_tail_request` returns full range when `from_lsn == 1`
  - `on_lsn_ack` updates per-peer watermark
  - `on_lsn_ack` rejects LSN regression
  - `on_lsn_ack` removes the peer from `txn_subscribers` for the acknowledged txn
  - `record_commit_error` pushes to `error_queue`
  - `drain_error_queue` maps txn_id → set of subscribed peers and transitions each to Terminated
  - `pause` flag is honored: when reader sends `PAUSE`, writer stops shipping new chunks; on `RESUME`, shipping resumes

- **Integration:**
  - Writer commits 1 row → reader receives `WalTailChunk { from_lsn: 1, to_lsn: 1, is_last: true }` → applies → sends `LsnAck { applied_lsn: 1 }`
  - Writer commits 10K rows in one transaction → reader receives one chunk → applies all → state matches
  - Writer commits 1000 rows in 10 transactions (100 rows each) → reader receives 10 chunks (one per batch) → applies all → state matches
  - Writer and reader restart → reader sends `WalTailRequest { from_lsn: persisted_watermark + 1 }` → writer responds with missing entries
  - LSN regression: forge a chunk with `from_lsn: 1` after reader has already applied LSN 1000 → `E_SYNC_LSN_REGRESSION`
  - Backpressure: reader artificially fills its apply queue to 11K → reader sends `PAUSE` → writer stops → reader drains queue to 5K → reader sends `RESUME` → writer resumes

## Dependencies

- **Requires:**
  - `0862-base` — envelope types, identity derivation, state machine, **`DatabaseSyncAdapter` trait**, `MockAdapter`
  - `octo_sync::DatabaseSyncAdapter` trait (8 methods, sync, `Send + Sync + 'static`; per RFC-0862 v1.1.0)
  - The `adapter.read_wal_range(from, to)` method (per RFC-0862 §4.3.3 via the trait boundary)
  - RFC-0862 §4.3.3 (WAL-tail streaming algorithm)

- **Required by:**
  - `0862-base` (integration glue)
  - `0862h` (property tests for LSN monotonicity)

- **No longer requires direct access to:**
  - `stoolap/src/storage/mvcc/wal_manager.rs:1282` (`current_lsn()` no-arg) — accessed via `adapter.current_lsn()`
  - `stoolap/src/storage/mvcc/wal_manager.rs:1353` (`previous_lsn()` no-arg) — not directly used; the sync engine tracks its own LSN watermark
  - `stoolap/src/storage/mvcc/wal_manager.rs:1595` (`WALManager::replay_two_phase` for reader apply) — accessed via `adapter.apply_wal_entry(entry)` on the reader side, which internally delegates to `replay_two_phase` inside the StoolapAdapter impl
  - `stoolap/src/storage/mvcc/persistence.rs:549` (`PersistenceManager::replay_two_phase` — thin wrapper around `WALManager::replay_two_phase`) — not directly used; the adapter owns the choice of which apply method to call

## Blockers / Dependencies

- **Blocked by:** `0862-base` (for envelope types, state machine, **and the `DatabaseSyncAdapter` trait**)
- **Blocks:** `0862b` (Merkle summary for catch-up), `0862c` (snapshot segment), `0862f` (multi-peer)

## Description

The WAL-tail streamer is the heart of v1 single-leader sync. The writer captures LSN ranges on every commit and ships them as `WalTailChunk` envelopes. The reader applies them in LSN order via `adapter.apply_wal_entry(entry)`, which (for the StoolapAdapter impl) delegates to `WALManager::replay_two_phase` — the Stoolap fork's built-in recovery path. This is the same pattern as PostgreSQL logical replication, MySQL binlog replication, and SQLite session extension. **The trait is the integration boundary; the cipherocto sync engine never calls Stoolap DB functions directly.**

## Technical Details

### Performance

- **Throughput target:** > 5,000 commits/s (matches RFC-0862 G3)
- **Latency target:** < 50 ms p50, < 200 ms p99 (LAN, 1 KB write)
- **Backpressure:** when the reader's apply queue exceeds 10K entries, the **reader** sends `PAUSE` to the writer (per RFC-0862 §Implicit Assumptions Audit row 6: "Reader's per-peer backpressure: reader sends `PAUSE` if its apply queue > 10K entries"). The writer stops shipping new chunks until it receives a `RESUME`.

### Cargo dependencies

- `tokio` 1.x (async runtime; **optional** behind `sync` feature)
- `blake3` (already in `stoolap/Cargo.toml:111`)
- `tracing` (for the `tracing::error!` call in `record_commit`; for structured error logging)
- `parking_lot` (for the `Mutex<HashMap>` wrappers; the code uses `parking_lot::Mutex` not `std::sync::Mutex`)
- `octo-sync` (git dep, `branch = "next"`; the `DatabaseSyncAdapter` trait and `MockAdapter` are consumed from this leaf workspace)

### Pitfalls

- **Don't read entries from the WAL after they have been truncated.** The reader's LSN watermark must always be > the writer's truncated LSN, otherwise the reader must re-snapshot. Per the v1.1.0 trait boundary, this check happens inside `StoolapAdapter::read_wal_range` (it returns `Err(SyncError::InvalidLsnRange)` if `from_lsn > current_lsn`), NOT in the cipherocto sync engine.
- **Don't ship `Rollback` entries.** Only ship `Commit` markers; `Rollback` markers trigger entry discard on the reader (matches `WALManager::replay_two_phase` semantics). The StoolapAdapter impl must filter out `Rollback` entries when responding to `read_wal_range`.
- **Don't conflate "current LSN" with "highest shipped LSN".** The writer's current LSN is the highest committed (returned by `adapter.current_lsn()`); the writer's highest shipped LSN is what the reader has acknowledged.
- **Don't use `is_last` to mean "no more chunks in this session".** It means "this chunk's `to_lsn` equals the writer's `current_lsn` at packaging time". The reader should treat `is_last || WalTailEnd` as the stop signal (defense-in-depth).
- **Don't ship chunks out of order across multiple writers.** v1 is single-leader, so this can't happen, but the design must reject chunks with `from_lsn != previous_chunk.to_lsn + 1` (LSN monotonicity).
- **Don't store `Arc<MVCCEngine>` in the streamer.** Per RFC-0862 v1.1.0, the streamer stores `Arc<dyn DatabaseSyncAdapter>`. Direct engine access is forbidden — it would bypass the trait boundary and re-create the Cargo workspace cycle.

---

**Mission Type:** Implementation
**Priority:** Critical
**Phase:** 1 (Core / MVE)
**RFC Section Coverage:** §4.3.3 WAL-tail streaming

## Type Coverage

This mission implements the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `WalTailChunk` | The envelope payload produced by the writer's `WalTailStreamer::on_commit`; consumed by the reader's `apply_wal_entry` |
| `WalTailStreamer` | The writer-side struct that captures LSN ranges and fans out `WalTailChunk` envelopes to subscribers |
| `apply_wal_entry` | The reader-side function that applies a single WAL entry via `WALManager::replay_two_phase` |

The mission does NOT implement `SyncSummary`, `SyncSegment`, `SyncNodeId`, or `KeyRing` — those are handled by missions 0862b, 0862c, 0862-base, and 0862d respectively. See the Type Coverage table in 0862-base for the full mapping.
