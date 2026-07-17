# Mission: 0862-base — Stoolap Data Sync Core (single-leader)

## Status

In Review (PR submitted 2026-06-22)

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 0 + Phase 1, §4 Specification (entire), §Key Files to Modify, §DatabaseSyncAdapter Trait (v1.1.0)

## Summary

Implement the v1 single-leader core of the Stoolap Data Sync Protocol: envelope types `0xA0–0xC2`, identity derivation (`SyncNodeId = BLAKE3(public_key || mission_id)`), OCrypt key derivation (`HKDF-BLAKE3(mission_root_key, "sync:v1", mission_id)` → `transport_key` + `execution_key`), WAL-tail streaming on NativeP2P, per-peer LSN watermark + `LsnAck`, heartbeat (5s) + `Suspect` after 10s, rate limit (100/s sustained, 500 burst), mission-binding precondition (`RoleNotSyncCapable` if role ≠ `Replicator`/`Observer`).

This is the **base mission** that all 9 sub-missions build on. Sub-missions 0862a (WAL-tail streamer) and 0862d (OCrypt key ring) are split out of this base mission for parallel execution, but the base mission includes the envelope type definitions, identity derivation, state machine, the `DatabaseSyncAdapter` trait boundary, and integration glue that the sub-missions consume.

**Phase 0 (v1.1.0):** before the v1 core work begins, this mission creates the `octo-sync` leaf workspace and defines the `DatabaseSyncAdapter` trait. This breaks the Cargo workspace cycle (the cipherocto workspace depends on the Stoolap fork; v1.1.0 reverses this without a cycle by extracting `octo-sync` as a leaf workspace, mirroring the `octo-determin` pattern at `/home/mmacedoeu/_w/ai/cipherocto/determin/`). See RFC-0862 §DatabaseSyncAdapter Trait (v1.1.0) for the full design.

## Design

### New leaf workspace: `octo-sync/`

The `octo-sync` crate lives at `cipherocto/octo-sync/`, **not** as a member crate of the main cipherocto workspace. It is a standalone workspace (excluded from the main workspace via `workspace.exclude`), modeled on the existing `octo-determin` pattern. Both the cipherocto workspace and the Stoolap fork depend on it via git.

```
octo-sync/                                  # leaf workspace at cipherocto/octo-sync/
├── Cargo.toml                              # [workspace] section; not part of main cipherocto workspace
├── src/
│   ├── lib.rs                  # public API: Database::open_with_sync, SyncConfig
│   ├── envelope.rs             # EnvelopePayload enum (0xA0-0xC2), DCS encoding
│   ├── identity.rs             # SyncNodeId, SyncPeerId, BLAKE3 derivation
│   ├── keyring.rs              # MissionKeyHierarchy, HKDF-BLAKE3 sync:v1
│   ├── state.rs                # 7-state SyncLifecycle enum + transition logic
│   ├── summary.rs              # SyncSummary, SyncSegment, Merkle root builder
│   ├── stream.rs               # WalTailChunk, WalTailRequest/Response, LsnAck
│   ├── heartbeat.rs            # Heartbeat, AuthChallenge, AuthResponse
│   ├── replay_cache.rs         # Bounded BTreeMap by envelope_id
│   ├── rate_limit.rs           # per-peer token bucket (100/s sustained)
│   ├── error.rs                # SyncError enum (internal); WireError enum (wire-level);
│   │                           #   impl From<SyncError> for WireError
│   ├── lsn.rs                  # LSN monotonicity enforcement
│   ├── config.rs               # SyncConfig, CLI flag parsing
│   ├── keyring_stub.rs         # KeyRing trait (interface only; full impl is in mission 0862d)
│   ├── types.rs                # type aliases: Lsn, MissionId, NodeId, TableId, SegmentIndex
│   ├── adapter.rs              # DatabaseSyncAdapter trait (8 methods, sync, Send + Sync + 'static)
│   └── apply.rs                # apply_wal_entry: feeds bytes to adapter.apply_wal_entry
├── tests/
│   ├── two_node.rs             # end-to-end single-leader sync test (against MockAdapter)
│   ├── heartbeat.rs            # 5s heartbeat, 10s Suspect
│   ├── rate_limit.rs           # 100/s token bucket
│   ├── lsn_monotonicity.rs     # reject regression
│   ├── auth_failure.rs         # reject bad signature
│   ├── role_mismatch.rs        # RoleNotSyncCapable
│   ├── schema_drift.rs         # DDL out-of-order abort
│   └── keyring_stub.rs         # verify 0862-base uses KeyRing trait only (full impl in 0862d)
└── benches/
    └── wal_apply.rs            # benchmark: commits/s (against MockAdapter)
```

### The `DatabaseSyncAdapter` trait (RFC-0862 v1.1.0 §DatabaseSyncAdapter Trait)

The cipherocto sync engine does **not** call Stoolap DB functions directly. It consumes the `DatabaseSyncAdapter` trait; the Stoolap fork provides a `StoolapAdapter` impl. The trait is the integration boundary that prevents a Cargo workspace cycle.

```rust
// octo-sync/src/adapter.rs
use crate::error::SyncError;
use crate::snapshot::SnapshotSegment;
use crate::types::{Lsn, MissionId, NodeId, SegmentIndex, TableId};

/// Sync (not async) — cipherocto's compute/state trait convention.
/// `Send + Sync + 'static` — cipherocto's trait-object storage pattern.
pub trait DatabaseSyncAdapter: Send + Sync + 'static {
    // ── A. WAL-tail streaming (RFC-0862 §4.3.3) ──────────────────────
    fn read_wal_range(&self, from_lsn: Lsn, to_lsn: Lsn)
        -> Result<Vec<Vec<u8>>, SyncError>;
    fn current_lsn(&self) -> Result<Lsn, SyncError>;
    fn apply_wal_entry(&self, entry: &[u8]) -> Result<(), SyncError>;

    // ── B. Anti-entropy Merkle summary (RFC-0862 §4.3.4) ─────────────
    fn read_snapshot_segment(&self, table_id: TableId, segment_index: SegmentIndex)
        -> Result<Option<SnapshotSegment>, SyncError>;
    fn write_snapshot_segment(&self, table_id: TableId, segment_index: SegmentIndex, payload: &[u8])
        -> Result<(), SyncError>;

    // ── C. LSN model and backpressure (RFC-0862 §4.3.2) ──────────────
    fn set_paused(&self, paused: bool) -> Result<(), SyncError> { Ok(()) }

    // ── D. Identity, key hierarchy, and trust (RFC-0862 §4.3.1) ──────
    fn mission_id(&self) -> Result<MissionId, SyncError>;
    fn node_id(&self) -> Result<NodeId, SyncError>;
}
```

8 methods total: 5 RFC-0862 ops + 1 backpressure + 2 auxiliary. `set_paused` has a default no-op (databases that don't support writer-side pause ignore the call; the cipherocto sync engine falls back to per-peer rate-limiting). The other 7 are required.

### Type aliases

```rust
// octo-sync/src/types.rs
pub type Lsn = u64;                  // WAL Logical Sequence Number (monotonic per writer)
pub type MissionId = [u8; 32];       // per RFC-0853 MissionKeyHierarchy
pub type NodeId = [u8; 32];         // SyncNodeId = BLAKE3(public_key || mission_id)
pub type TableId = u32;              // Database table identifier
pub type SegmentIndex = u32;         // Ordinal position of a snapshot segment
```

### Stoolap fork changes (in `stoolap/Cargo.toml` and `stoolap/src/api/database.rs`)

```toml
# stoolap/Cargo.toml — add optional feature
[features]
sync = ["dep:tokio", "dep:octo-sync"]

[dependencies]
tokio = { version = "1", optional = true }
octo-sync = { git = "https://github.com/CipherOcto/cipherocto", branch = "next", optional = true }
```

```rust
// stoolap/src/api/database.rs — new constructor
#[cfg(feature = "sync")]
impl Database {
    pub fn open_with_sync(dsn: &str, sync: SyncConfig) -> Result<Self> {
        // existing open() logic
        // + validate sync.role ∈ {Replicator, Observer} if role is configured
        // + bind to mission_id
        // + construct StoolapAdapter and Arc<dyn DatabaseSyncAdapter>
        // + spawn background task: if role == Replicator, capture WAL commits
        //   via record_commit hook and ship WalTailChunk to subscribed readers
    }
}

// stoolap/crates/sync-adapter/src/lib.rs — StoolapAdapter impl
// This is a new sub-crate in the stoolap fork that implements
// DatabaseSyncAdapter for the Stoolap MVCCEngine. The wrapper satisfies
// the trait's Send + Sync + 'static bounds via parking_lot::Mutex.
use octo_sync::DatabaseSyncAdapter;
use octo_sync::error::SyncError;
use octo_sync::snapshot::SnapshotSegment;
use octo_sync::types::{Lsn, MissionId, NodeId, SegmentIndex, TableId};
use stoolap::storage::mvcc::engine::MVCCEngine;
use std::sync::Arc;

pub struct StoolapAdapter {
    /// The Stoolap MVCC engine, wrapped in parking_lot::Mutex to satisfy
    /// the trait's Send + Sync bounds. Per RFC-0862 §4.3.3, the engine is
    /// read on the writer side (read_wal_range) and written on the reader
    /// side (apply_wal_entry); the Mutex serializes these.
    engine: Arc<parking_lot::Mutex<MVCCEngine>>,
}

impl DatabaseSyncAdapter for StoolapAdapter {
    fn read_wal_range(&self, from_lsn: Lsn, to_lsn: Lsn)
        -> Result<Vec<Vec<u8>>, SyncError>
    {
        if from_lsn > to_lsn {
            return Err(SyncError::InvalidLsnRange { from: from_lsn, to: to_lsn });
        }
        let engine = self.engine.lock();
        let mut entries: Vec<Vec<u8>> = Vec::new();
        engine.wal_manager().replay_two_phase(from_lsn, |entry: &[u8]| {
            entries.push(entry.to_vec());
            Ok(())
        })?;
        Ok(entries)
    }

    fn current_lsn(&self) -> Result<Lsn, SyncError> {
        Ok(self.engine.lock().wal_manager().current_lsn())
    }

    fn apply_wal_entry(&self, entry: &[u8]) -> Result<(), SyncError> {
        // WAL V2 binary format is replay-safe: replaying the same entry
        // twice is a no-op. See PersistenceManager::replay_two_phase at
        // stoolap/src/storage/mvcc/persistence.rs:549.
        let engine = self.engine.lock();
        engine.wal_manager().replay_two_phase(/* parsed LSN */, |_| {
            // Forward the entry to the engine's WAL apply path
            Ok(())
        })?;
        Ok(())
    }

    fn read_snapshot_segment(&self, table_id: TableId, segment_index: SegmentIndex)
        -> Result<Option<SnapshotSegment>, SyncError>
    {
        // Delegate to the existing snapshot file at
        // `snapshot-<ts>.bin` per stoolap/src/storage/mvcc/snapshot.rs:37,98.
        // `segment_index` is the ordinal position; `regenerated` is set
        // by the writer when the segment was regenerated via
        // MVCCEngine::create_snapshot_for_table (mission 0862c).
        todo!("delegated to snapshot.rs; implemented in 0862c")
    }

    fn write_snapshot_segment(&self, table_id: TableId, segment_index: SegmentIndex, payload: &[u8])
        -> Result<(), SyncError>
    {
        // Atomic-rename per stoolap/src/storage/mvcc/engine.rs:2642, :2828.
        todo!("delegated to engine.rs; implemented in 0862c")
    }

    fn mission_id(&self) -> Result<MissionId, SyncError> {
        // Retrieved from the engine's open configuration (DSN/connect-string).
        todo!("read from engine config")
    }

    fn node_id(&self) -> Result<NodeId, SyncError> {
        // SyncNodeId = BLAKE3(public_key || mission_id); public_key is the
        // local node's OverlayIdentity.public_key per RFC-0853:163.
        todo!("derive from local OverlayIdentity")
    }
}
```

### Cargo dep graph (v1.1.0)

```toml
# cipherocto/Cargo.toml — exclude octo-sync from main workspace
[workspace]
exclude = ["determin", "octo-sync"]

# cipherocto/crates/octo-network/Cargo.toml — git dep on octo-sync
[dependencies]
octo-sync = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }

# stoolap/Cargo.toml — git dep on both octo-determin (existing) and octo-sync (new)
[dependencies]
octo-determin = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }
octo-sync     = { git = "https://github.com/CipherOcto/cipherocto", branch = "next" }   # new (v1.1.0)
```

The trait is not a Cargo dep — it is a trait bound. The workspace graph becomes:
- `octo-sync` (leaf workspace, excluded from both projects)
- `cipherocto` (workspace) → `octo-network` (member) → `octo-sync` (git dep) → (no further cipherocto deps)
- `stoolap` fork (single-package) → `octo-sync` (git dep) → (no further cipherocto deps)

**No cycle.** The trait is the boundary.

### Critical: build profile

All Sync code MUST inherit Stoolap's release profile per `stoolap/Cargo.toml:215-228`:
```toml
[profile.release]
codegen-units = 1
lto = true
overflow-checks = false
panic = "abort"
# RUSTFLAGS="-C target-feature=-fma" required for DFP determinism
```

## Acceptance Criteria

### Phase 0 (v1.1.0) — Trait boundary

- [ ] `octo-sync/` leaf workspace exists at `cipherocto/octo-sync/` (modeled on `octo-determin/`), with its own `Cargo.toml` `[workspace]` section
- [ ] `octo-sync` is added to `workspace.exclude` in the main cipherocto `Cargo.toml`
- [ ] `octo-sync/src/adapter.rs` defines the `DatabaseSyncAdapter` trait (8 methods, sync, `Send + Sync + 'static`)
- [ ] `octo-sync/src/types.rs` defines type aliases: `Lsn = u64`, `MissionId = [u8; 32]`, `NodeId = [u8; 32]`, `TableId = u32`, `SegmentIndex = u32`
- [ ] `octo-sync/src/error.rs` defines the 9-variant `SyncError` enum + the 9-variant `WireError` enum + `impl From<SyncError> for WireError` (many-to-one mapping; see RFC-0862 §DatabaseSyncAdapter Trait §Error Model)
- [ ] `octo-sync/src/test_util.rs` defines a `MockAdapter` (in-memory, parking_lot::Mutex-protected) implementing `DatabaseSyncAdapter` for unit tests
- [ ] `stoolap/Cargo.toml` adds `octo-sync` as a **git dep** (`branch = "next"`), not a `path` dep
- [ ] `stoolap/crates/sync-adapter/src/lib.rs` defines `StoolapAdapter` (wraps `Arc<parking_lot::Mutex<MVCCEngine>>`) implementing `DatabaseSyncAdapter`
- [ ] `cipherocto/crates/octo-network/Cargo.toml` adds `octo-sync` as a **git dep** (same as stoolap fork)
- [ ] `cargo metadata --no-deps` on both projects shows no cycle
- [ ] Unit test: `MockAdapter` round-trip — `apply_wal_entry(entry)` followed by `read_wal_range(...)` includes `entry`

### Phase 1 — Core (MVE)

- [ ] `octo-sync/` exists with the 14 source modules listed above (including `keyring_stub.rs`, plus `adapter.rs`, `types.rs`, `test_util.rs` from Phase 0)
- [ ] `stoolap/Cargo.toml` adds optional `sync` feature with `tokio` and `octo-sync` deps (git, not path)
- [ ] `stoolap/src/api/database.rs` exposes `Database::open_with_sync(dsn, SyncConfig)` when `sync` feature enabled
- [ ] Envelope types `0xA0–0xC2` (13 types) defined in `envelope.rs` (SummaryRequest/Response, SegmentRequest/Response/NotFound, NodeStatus, WalTailRequest/Response/End, LsnAck, Heartbeat, AuthChallenge/Response)
- [ ] `SyncNodeId = BLAKE3(public_key || mission_id)` matches RFC-0862 §4.3.1
- [ ] `KeyRing` trait defined in `keyring_stub.rs`; the trait has methods `transport_key()`, `execution_key()`, `summary_hmac()`, `encrypt()`, `decrypt()`. The full `MissionKeyRing` implementation is in mission 0862d, NOT in 0862-base.
- [ ] `SyncLifecycle` 7-state enum (Init/Connecting/Authenticating/Streaming/Suspect/Reconnecting/Terminated) with full transition table from RFC-0862 §Lifecycle Requirements
- [ ] Per-peer LSN watermark rejects LSN regression
- [ ] Heartbeat 5s interval, `Suspect` after 10s, `Terminated` after 5 reconnect attempts (~5 min)
- [ ] Per-peer rate limit: 100 envelopes/s sustained, 500 burst
- [ ] Mission-binding precondition: `RoleNotSyncCapable` returned if mission role ≠ `Replicator`/`Observer`
- [ ] All 9 wire-level error codes (E_SYNC_AUTH_FAIL, E_SYNC_LSN_REGRESSION, E_SYNC_SEGMENT_CORRUPTION, E_SYNC_SEGMENT_NOT_FOUND, E_SYNC_RATE_LIMIT, E_SYNC_WAL_APPEND_FAIL, E_SYNC_SCHEMA_DRIFT, E_SYNC_HEARTBEAT_TIMEOUT, E_SYNC_ROLE_NOT_SYNC_CAPABLE) defined in `error.rs`

> **Error code completeness (resolves N5, R8-9):** The above 9 codes are the WIRE-LEVEL stable codes (RFC-0862 §Error Handling). The full internal `SyncError` enum in `error.rs` includes additional variants for implementation-internal error mapping. The explicit mapping from internal variants to wire codes is:

| Internal variant | Wire code | Used in |
|------------------|-----------|---------|
| `LsnRegression { expected, actual }` | `E_SYNC_LSN_REGRESSION` | 0862a |
| `InvalidLsnRange { from, to }` | `E_SYNC_LSN_REGRESSION` (with extended detail) | 0862a |
| `UnknownPeer(SyncPeerId)` | `E_SYNC_AUTH_FAIL` (no such peer → auth fail) | 0862a |
| `AllCarriersFailed` | `E_SYNC_RATE_LIMIT` (all carriers failed = rate-limited) | 0862g |
| `UnknownEnvelopeSubtype(u8)` | `E_SYNC_AUTH_FAIL` (unknown subtype = corrupt/forged envelope) | 0862f |
| `DecryptionFailed` | `E_SYNC_AUTH_FAIL` (AEAD failure = auth fail) | 0862d |
| `SegmentNotFound { table_id, segment_index, regenerated }` | `E_SYNC_SEGMENT_NOT_FOUND` | 0862c |
| `UnknownCarrier(String)` | `E_SYNC_AUTH_FAIL` (no such carrier = bad config) | 0862g |

The `impl From<SyncError> for WireError` (defined in `error.rs`) implements this mapping. The mission tests must cover BOTH the 9 wire codes AND the internal variant mapping.
- [ ] Two-node integration test passes: 1h of writes on writer, reader state matches (`BLAKE3-256(SELECT * FROM table)` per table)
- [ ] `cargo test -p octo-sync` passes (unit + integration)
- [ ] `cargo test -p stoolap --features sync` passes (fork integration)
- [ ] DFP determinism: same input on Linux x86_64 and macOS arm64 with `RUSTFLAGS="-C target-feature=-fma"` produces byte-exact wire output
- [ ] `cargo bench -p octo-sync` shows ≥ 5,000 commits/s throughput (matches RFC-0862 G3)
- [ ] `cargo doc -p octo-sync` builds with no warnings
- [ ] No `unwrap()` in production code paths (test code is fine)

## Tests

- **Unit tests (in each module):**
  - `envelope.rs`: DCS round-trip for all 13 envelope types
  - `identity.rs`: `SyncNodeId` determinism (same input → same output)
  - `keyring.rs`: `transport_key` and `execution_key` derivation match Appendix B
  - `state.rs`: every transition in the RFC's transition table, including failure cases
  - `replay_cache.rs`: bounded size eviction, deterministic LRU tie-break
  - `rate_limit.rs`: 100/s sustained allows up to 100, denies the 101st; 500 burst allows up to 500, then denies
  - `lsn.rs`: reject `entry.lsn != previous_lsn + 1`
  - `error.rs`: every error code maps to a stable u8

- **Integration tests (in `tests/`):**
  - `two_node.rs`: two `Database::open_with_sync` instances in the same process; writer commits 1000 rows, reader applies all, verify state
  - `heartbeat.rs`: kill writer, observe `Suspect` after 10s
  - `rate_limit.rs`: synthetic peer floods at 200/s, observe `Suspect`
  - `lsn_monotonicity.rs`: forge a chunk with LSN-1, observe `E_SYNC_LSN_REGRESSION`
  - `auth_failure.rs`: forge AuthResponse with bad signature, observe `E_SYNC_AUTH_FAIL`
  - `role_mismatch.rs`: open with `sync=on` and role=`Validator`, observe `E_SYNC_ROLE_NOT_SYNC_CAPABLE`
  - `schema_drift.rs`: writer adds a column, reader has no migration, observe `E_SYNC_SCHEMA_DRIFT`

- **Cross-implementation determinism (CI gate):**
  - Same input on Linux x86_64 and macOS arm64 with `RUSTFLAGS="-C target-feature=-fma"` produces byte-exact wire output. Enforced in CI as a `test_determinism_x86_vs_arm64` integration test.

## Dependencies

- **Requires (all accepted):**
  - RFC-0850 (Networking): Deterministic Overlay Transport — envelope wire format, replay cache, fragmentation
  - RFC-0852 (Networking): Deterministic Gossip Protocol — anti-entropy pattern (consumed by 0862b)
  - RFC-0853 (Networking): Overlay Cryptography — `OverlayIdentity`, `MissionKeyHierarchy` (per-mission `transport_keys_root`, `execution_keys_root`), HKDF-BLAKE3 derivation, ChaCha20-Poly1305 AEAD, Ed25519 signatures, AAD binding, replay protection (1h or 10K entries per `§7`), key rotation (24h grace per `§12`).
  - RFC-0126 (Numeric): Deterministic Serialization — DCS encoding
  - RFC-0104 (Numeric): Deterministic Floating-Point — Stoolap's `octo_determin` dep

- **Requires (sub-missions, not yet started):**
  - 0862a (WAL-tail streamer) — split out for parallel execution
  - 0862d (OCrypt key ring) — split out for parallel execution; 0862-base provides only a `KeyRingStub` interface that 0862d implements

- **Optional:**
  - RFC-0855p-c (Domain Coordinator Role) — writer is a `DomainCoordinator` per RFC-0855p-c; not required for v1 but the role may be configured

## Blockers / Dependencies

- **Blocked by:** RFC-0862 acceptance (✅ 2026-06-20)
- **Blocks:** 0862a (WAL-tail streamer), 0862b (Merkle summary), 0862c (snapshot segment), 0862e (ReplayCache persistence), 0862f (multi-peer), 0862g (cross-carrier), 0862h (property tests), 0862i (Raft overlay). **0862d does NOT block 0862-base**: 0862-base provides a `KeyRingStub` interface (an `Arc<dyn KeyRing>` trait object) that 0862d fills in. This breaks the apparent cycle.

## Description

Build the v1 single-leader core of the Stoolap Data Sync Protocol. The base mission covers everything needed for a two-node deployment (one writer, one reader) to sync over NativeP2P, with deterministic LSN ordering, replay protection, and mission-binding preconditions. Sub-missions extend this core with snapshot catch-up (0862b/0862c), persistence (0862e), multi-peer (0862f), and cross-carrier (0862g).

## Technical Details

### Performance targets (from RFC-0862 §Performance Targets)

- End-to-end replication latency: < 50 ms p50, < 200 ms p99 (LAN, 1 KB write)
- Throughput: > 5,000 commits/s (single writer, 200-byte avg entry)
- Memory overhead: ≤ 50 MB per peer (50 MB ReplayCache + 160 KB dedup cache)
- Wire overhead: 256 bytes per envelope baseline

### Implementation order

#### Phase 0 (v1.1.0) — Trait boundary (must precede Phase 1)

1. Create `cipherocto/octo-sync/` leaf workspace with its own `Cargo.toml` `[workspace]` section (modeled on `octo-determin/`)
2. Add `octo-sync` to `workspace.exclude` in main `Cargo.toml`
3. Define type aliases in `octo-sync/src/types.rs` (`Lsn`, `MissionId`, `NodeId`, `TableId`, `SegmentIndex`)
4. Define `DatabaseSyncAdapter` trait in `octo-sync/src/adapter.rs` (8 methods, sync, `Send + Sync + 'static`)
5. Define `SyncError` (9 variants) and `WireError` (9 variants) enums + `From<SyncError> for WireError` in `octo-sync/src/error.rs`
6. Define `MockAdapter` in `octo-sync/src/test_util.rs` (in-memory, parking_lot::Mutex-protected)
7. Add `octo-sync` as a git dep in `cipherocto/crates/octo-network/Cargo.toml` and `stoolap/Cargo.toml`
8. Add `stoolap/crates/sync-adapter/src/lib.rs` with `StoolapAdapter` impl (wraps `Arc<parking_lot::Mutex<MVCCEngine>>`)
9. Verify `cargo metadata --no-deps` shows no cycle on both projects

#### Phase 1 — Core (MVE)

1. Define the 13 envelope types in `envelope.rs` with DCS round-trip tests
2. Implement `SyncNodeId` and `SyncPeerId` derivation in `identity.rs`
3. Define the `KeyRing` trait in `keyring_stub.rs` (interface only; full `MissionKeyRing` impl is in 0862d)
4. Define `SyncLifecycle` 7-state enum and transition table in `state.rs`
5. Implement WAL-tail streaming in `stream.rs` (uses the `KeyRing` trait, not the concrete impl; consumes `Arc<dyn DatabaseSyncAdapter>`)
6. Implement heartbeat in `heartbeat.rs`
7. Implement replay cache in `replay_cache.rs`
8. Implement rate limiter in `rate_limit.rs`
9. Implement `apply.rs` — the reader-side apply path that calls `adapter.apply_wal_entry(entry)` (the underlying DB write is delegated to the `StoolapAdapter` impl, not to `MVCCEngine` directly)
10. Wire everything together in `lib.rs` and the `Database::open_with_sync` constructor (the constructor builds a `StoolapAdapter` and wraps it in `Arc<dyn DatabaseSyncAdapter>`)
11. Add `tokio` as an optional dep to `stoolap/Cargo.toml` with the `sync` feature
12. Wrap `record_commit` in `stoolap/src/storage/mvcc/transaction.rs` to feed LSN ranges to the Sync engine (the Sync engine now consumes an `Arc<dyn DatabaseSyncAdapter>`; the engine no longer holds a direct `Arc<MVCCEngine>` reference)

### Pitfalls

- **Don't pull `tokio` into the default `stoolap` build.** The fork's existing user base does not use `tokio`. The `sync` feature MUST be opt-in.
- **Don't break the WAL V2 binary format.** The Sync protocol ships raw `WALEntry::encode()` output; changing the WAL format would break downstream ZK proofs.
- **Don't use `std::time::SystemTime` for LSN ordering.** The WAL counter is the source of truth; the system clock is for diagnostics only.
- **Don't use `tokio::spawn` from a sync `Database::open_with_sync` call.** The constructor must return a `Database` synchronously; the background tasks must be spawned via `tokio::runtime::Handle::current()` or similar.
- **Don't forget to bump WAL header version if Sync is enabled.** The reader needs to know that WAL V2 entries are part of a sync-enabled DB; this is handled by the WAL_FORMAT_VERSION constant in `wal_manager.rs:69`.
- **Don't store `Arc<MVCCEngine>` in the cipherocto sync engine.** Per RFC-0862 v1.1.0, the sync engine stores `Arc<dyn DatabaseSyncAdapter>`. The trait boundary is the integration point; the cipherocto workspace does not depend on the Stoolap fork (Cargo-wise).
- **Don't use `path = "../octo-sync"` for the `octo-sync` dep.** The dep MUST be `git = "https://github.com/CipherOcto/cipherocto", branch = "next"`. A `path` dep would re-create the Cargo workspace cycle that v1.1.0 explicitly broke.
- **Don't call `self.engine.wal_manager().replay_two_phase(...)` from the cipherocto sync engine.** All WAL reads go through `adapter.read_wal_range(from, to)` (per RFC-0862 v1.1.0 §Migration path step v1.1.0.d). Direct calls are forbidden — they bypass the trait and the abstraction layer.

## Claimant

@cipherocto (agent)

## Completion Criteria

When complete:
1. `cargo build -p octo-sync` succeeds with no warnings
2. `cargo test -p octo-sync` passes 100% (unit + integration)
3. `cargo test -p stoolap --features sync` passes 100%
4. `cargo bench -p octo-sync` shows ≥ 5,000 commits/s
5. CI gate `test_determinism_x86_vs_arm64` passes
6. Two-node smoke test (1h, 100K writes) shows no data drift (`BLAKE3-256(SELECT * FROM table)` matches per table)

---

**Mission Type:** Implementation
**Priority:** Critical
**Phase:** 1 (Core / MVE)
**RFC Section Coverage:** §4 Specification (entire), §Implementation Phases Phase 1, §Key Files to Modify, §Error Handling

## Type Coverage

Per BLUEPRINT mission template, this section maps each RFC type to the mission that implements it.

| RFC-0862 Type | Defined In | Implemented By | Status |
|---|---|---|---|
| `SyncSummary` | §4.3 | mission 0862b | Draft |
| `SyncSegment` | §4.3 | mission 0862c | Draft |
| `WalTailChunk` | §4.3 | mission 0862a | Draft |
| `NodeStatus` | §4.3 | mission 0862-base | Draft |
| `SyncNodeId` | §4.3 | mission 0862-base | Draft |
| `SyncPeerId` | §4.3 | mission 0862-base | Draft |
| `SyncLifecycle` (7-state enum) | §Lifecycle Requirements | mission 0862-base | Draft |
| `SyncConfig` | §Error Handling (implicit) | mission 0862-base | Draft |
| `SyncError` (9 error codes) | §Error Handling | mission 0862-base | Draft |
| `KeyRing` (trait) | §4.3.1 | mission 0862-base (stub); 0862d (impl) | Draft |
| `MissionKeyRing` (concrete impl) | §4.3.1 + Appendix B | mission 0862d | Draft |
| `ReplayCache` (in-memory) | §Performance Targets (inherited from RFC-0850) | mission 0862-base | Draft |
| `ReplayCache` (persistent) | §Performance Targets | mission 0862e | Draft |
| `WalTailStreamer` | §4.3.3 | mission 0862a | Draft |
| `MerkleSegmentTree` | §4.3.4 | mission 0862b | Draft |
| `SegmentIndexer` | §4.3.4 | mission 0862c | Draft |
| `DgpSyncBridge` | §Implementation Phases Phase 3 | mission 0862f | Draft |
| `MultiCarrierSync` | §Implementation Phases Phase 4 | mission 0862g | Draft |
| `RateLimiter` (per-peer token bucket) | §4.3.1 | mission 0862-base | Draft |
| `apply_wal_entry` | §Algorithms §4.3.3 | mission 0862a (writer) and 0862-base (reader) | Draft |
| `MissionBinding` precondition | §4.1 G8 | mission 0862-base | Draft |
| `RaftOverlay` (deferred) | §Future Work F1, F8 | mission 0862i (deferred) | Draft |
