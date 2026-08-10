# RFC-0862 v1.2.0 — Writer Election + Bootstrap Integration

**Status:** Draft (2026-08-10)
**Author:** @cipherocto + @mmacedoeu
**Substrate:** RFC-0862 (Accepted v1.2.0 2026-06-25) + RFC-0855p-c (handover) + RFC-0863 (bootstrap)
**Parent:** missions `0871e-phase5c-1-cross-instance-drain` + `0871e-f7-cross-instance-did-coordination`

> **Promotion note:** In-place additive amendment to RFC-0862 (third
> update). Promotes §Future Work F8 (writer election / auto-failover)
>
> - F11 (bootstrap-orchestrated peer discovery for sync) to
>   §Specification. Adds `WriterElection` protocol that serves as the
>   substrate for both `DrainCoordinator` (spend) + `DidWriteCoordinator`
>   (DID). Adds CRDT-extension hooks (NEW F9 + F10 in §Future Work)
>   so future Option C migration = substrate extension not rewrite.

## Summary

Extend RFC-0862 §Roles (writer/reader split) with:

1. **`WriterElection` protocol** — server-elected writer per
   `(shard_key, partition_id)` shard. Election uses `DomainCoordinator`
   handover (RFC-0855p-c); campaigns on heartbeat timeout (~3s). Two
   substrate roles gain a writer identity: `StoolapSpendLedger` (mission
   `0871e-phase5b-stoolap-ledger`) and `StoolapDidRegistry` (mission
   `0871b-storage-backend`).
2. **`BootstrapOrchestrator`-driven peer discovery for sync** —
   wire `BootstrapOrchestrator` (RFC-0863) into the sync startup path
   so production deployments acquire peers via the RFC-0851p-a Mode A
   bootstrap protocol instead of `--peer` CLI args. F11 promoted from
   §Future Work.
3. **CRDT-extension hooks** — add NEW §Future Work F9 (HLC + LWW
   per-instance counter) + F10 (CRDT-style reconciliation between
   read-replicas during failover window). Both stay deferred but the
   `WriterElection` protocol exposes extension points (HLC stamps on
   drain records, per-instance drain log) so future Option C migration
   is additive.

## Why Now

Two missions BLOCKED on this RFC per `mission-gap-closure-priorities-2026-08-10`:

- `0871e-phase5c-1-cross-instance-drain` — needs `DrainCoordinator`
  trait + writer election
- `0871e-f7-cross-instance-did-coordination` — needs `DidWriteCoordinator`
  trait + same writer election

User direction (2026-08-10): Option B (centralized aggregator) primary,
Option C (CRDT LWW) extension room. This RFC ships Option B substrate
with C-migration hooks.

## Specification

### WriterElection Protocol (NEW §Specification §WriterElection)

```rust
/// Per-shard writer identity. Elected via `DomainCoordinator` handover
/// (RFC-0855p-c); campaigns on heartbeat timeout.
pub struct WriterIdentity {
    /// Mission ID of the elected writer node.
    pub writer_mission_id: MissionId,
    /// Election term (incremented on each handover).
    pub term: u64,
    /// HLC timestamp at election (for HLC-extension hooks per F9).
    pub elected_at_hlc: HlcTimestamp,
    /// Shard key for the elected shard (e.g., `(holder_did,
    /// macaroon_id)` hash, or `(canonical_did, chain_id)` hash).
    pub shard_key: ShardKey,
}

/// Writer election service. One instance per node; manages election
/// campaigns for shards this node holds candidacy for.
pub trait WriterElection: Send + Sync {
    /// Acquire writer status for `shard_key`. Blocks until elected or
    /// timeout. Returns the elected `WriterIdentity`.
    /// # Errors
    /// Returns `WriterElectionError::CampaignTimeout` if no quorum
    /// reached within `election_timeout_ms`. Returns
    /// `WriterElectionError::ShardAlreadyHeld` if another node holds
    /// the writer role and refuses handover.
    async fn acquire_writer(
        &self,
        shard_key: &ShardKey,
        election_timeout_ms: u64,
    ) -> Result<WriterIdentity, WriterElectionError>;

    /// Relinquish writer status (voluntary handover). Flushes any
    /// pending drain records to the WAL before stepping down.
    /// # Errors
    /// Returns `WriterElectionError::NotWriter` if this node is not
    /// the current writer for `shard_key`.
    async fn relinquish_writer(
        &self,
        shard_key: &ShardKey,
    ) -> Result<(), WriterElectionError>;

    /// Heartbeat: writer node refreshes its lease. Called every
    /// `heartbeat_interval_ms` (~500ms) by the current writer.
    /// # Errors
    /// Returns `WriterElectionError::NotWriter` if the lease has
    /// expired (another node won the campaign in the meantime).
    async fn heartbeat(&self, shard_key: &ShardKey)
        -> Result<(), WriterElectionError>;
}

/// Shard key derivation. Specialized for spend + DID; pluggable for
/// future shards.
pub trait ShardKeyDerivation: Send + Sync {
    /// Derive the shard key for a given record. Spend: hash
    /// `(holder_did, macaroon_id)`. DID: hash `(canonical_did,
    /// chain_id)`.
    fn derive(&self, record_key: &[u8]) -> ShardKey;
}
```

### Bootstrap-orchestrated peer discovery (F11 promoted)

Wire `BootstrapOrchestrator` (RFC-0863) into the sync startup path
via a new `BootstrapSyncAdapter` wrapper around `DatabaseSyncAdapter`:

```rust
/// Wraps `DatabaseSyncAdapter` with `BootstrapOrchestrator`-driven
/// peer acquisition. Production deployments use this wrapper; the
/// `--peer` CLI path remains as a development/testing shortcut.
pub struct BootstrapSyncAdapter {
    inner: Arc<dyn DatabaseSyncAdapter>,
    bootstrap: Arc<BootstrapOrchestrator>,
}

impl BootstrapSyncAdapter {
    /// Acquire peers via RFC-0851p-a Mode A bootstrap, then start
    /// sync against the acquired peer set. Replaces the `--peer`
    /// CLI shortcut.
    pub async fn start(&self) -> Result<(), SyncError>;
}
```

### DrainCoordinator trait (NEW §Specification §DrainCoordinator)

Lives in `crates/octo-paid-query/src/drain_coordinator.rs`. Mirrors
the `WriterElection` protocol; routes drains through the elected
writer.

```rust
/// Cross-instance spend drain coordinator. The elected writer for
/// `(holder_did, macaroon_id)` shard owns the drain authority;
/// read-replicas forward drain requests to the writer.
pub trait DrainCoordinator: Send + Sync {
    /// Submit a drain request to the elected writer for
    /// `(holder_did, macaroon_id)`. The writer applies the drain
    /// atomically + emits a WAL entry that `DatabaseSyncAdapter`
    /// fans out to read-replicas.
    /// # Errors
    /// Returns `DrainCoordinatorError::WriterUnavailable` if the
    /// writer is mid-failover (no local fallback per fail-closed).
    /// Returns `DrainCoordinatorError::UnknownHolder` /
    /// `InsufficientBalance` propagated from the writer.
    async fn submit_drain(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        cost: u128,
    ) -> Result<MicroOctoW, DrainCoordinatorError>;

    /// CRDT-extension hook (Option C migration): emit a local
    /// drain record when the writer is unavailable, queued for
    /// reconciliation when the writer returns. Default impl:
    /// `Err(DrainCoordinatorError::WriterUnavailable)`. Future
    /// amendment: per-instance LWW counter + HLC.
    async fn submit_drain_local_fallback(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        cost: u128,
    ) -> Result<(), DrainCoordinatorError>;
}
```

### DidWriteCoordinator trait (NEW §Specification §DidWriteCoordinator)

Lives in `crates/octo-ident/src/write_coordinator.rs`. Same substrate
pattern as `DrainCoordinator`.

```rust
/// Cross-instance DID write coordinator. Elected writer per
/// `(canonical_did, chain_id)` shard.
pub trait DidWriteCoordinator: Send + Sync {
    async fn submit_register(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
        document: &DidDocument,
    ) -> Result<(), DidWriteCoordinatorError>;

    async fn submit_revoke(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
    ) -> Result<(), DidWriteCoordinatorError>;

    /// CRDT-extension hook (Option C migration). Default impl:
    /// fail-closed.
    async fn submit_register_local_fallback(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
        document: &DidDocument,
    ) -> Result<(), DidWriteCoordinatorError>;
}
```

### §Future Work reconciliation

**Promoted to §Specification:**

- F8 (writer election / auto-failover) — promoted; `WriterElection`
  protocol defined above
- F11 (bootstrap-orchestrated peer discovery for sync) — promoted;
  `BootstrapSyncAdapter` defined above

**NEW §Future Work items (CRDT-extension hooks):**

- **F12 — HLC + LWW per-instance counter.** Add `HlcTimestamp` to
  drain records + DID write records. Per-instance drain log
  (`Vec<DrainRecord>` keyed by HLC) maintained on read-replicas.
  Failure mode: writer unavailable → drain refused (current) vs.
  drain logged locally + reconciled on writer return (future).
  Migration path: `submit_drain_local_fallback` is the extension
  point; default fail-closed impl ships now, LWW impl lands when
  correctness analysis converges.
- **F13 — CRDT-style reconciliation during failover window.** When
  the writer steps down, the next writer merges any local drain
  logs from the previous term. Reconciliation order = HLC. Conflicts
  resolved by `HlcTimestamp::max()` (last-write-wins). Requires
  F12 substrate.

**Stays in §Future Work (unchanged):**

- F1-F7, F9-F10 (see RFC-0862 §Future Work v1.2.0)

### Backward compatibility

- `DatabaseSyncAdapter` trait unchanged. New wrappers (`BootstrapSyncAdapter`)
  are additive; existing call sites unchanged.
- `WriterElection` is a NEW protocol; no existing call sites affected.
- `DrainCoordinator` + `DidWriteCoordinator` are NEW traits; existing
  `StoolapSpendLedger` + `StoolapDidRegistry` continue to work via
  per-instance mutex (mission `0871e-phase5b-stoolap-ledger` +
  `0871b-storage-backend`). The coordinator traits are wired via a
  follow-on mission AFTER this RFC ships.
- WAL format gains `HlcTimestamp` field (additive); old readers
  ignore the new field (RFC-0126 forward-compat invariant).

## Test Vectors (preview)

- 8 new TV: `writer_election_acquire_relinquish`; `heartbeat_lease_renewal`;
  `campaign_timeout_no_quorum`; `failover_writer_unavailable_drain_refused`;
  `bootstrap_orchestrator_drives_peer_acquisition`; `crdt_extension_hook_default_fail_closed`;
  `crdt_local_drain_log_queued_for_reconciliation` (F12 substrate);
  `crdt_reconciliation_hlc_ordering` (F13 substrate).

## Layer direction (per [[cipherocto-design-principles]])

- `octo-sync` (Layer B-substrate) — `WriterElection` protocol +
  `BootstrapSyncAdapter` wrapper
- `octo-paid-query` (Layer E) — `DrainCoordinator` trait
- `octo-ident` (Layer B) — `DidWriteCoordinator` trait
- `quota-router-storage` (Layer B-adjacent) — `StoolapSpendLedger` +
  `StoolapDidRegistry` consume the coordinators (replaces per-instance
  mutex with coordinator-mediated atomicity)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

## Cross-references

- [[rfc-0010-v13-storage-extension]] — `StoolapDidRegistry` substrate
- [[mission-0871e-phase5c-1-cross-instance-drain]] — substrate mission
- [[mission-0871e-f7-cross-instance-did-coordination]] — sister mission
- [[mission-0871b-storage-backend]] — `StoolapDidRegistry` predecessor
- [[cipherocto-design-principles]] — Layer B additive-only rule

## Version History

| Version | Date       | Status   | Changes                                                                                                                      |
| ------- | ---------- | -------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 1.0.0   | 2026-06-20 | Accepted | Initial specification                                                                                                        |
| 1.1.0   | 2026-06-21 | Accepted | `DatabaseSyncAdapter` trait + `octo-sync` leaf-workspace                                                                     |
| 1.2.0   | 2026-06-25 | Accepted | Bootstrap integration path clarified                                                                                         |
| 1.3.0   | 2026-08-10 | Draft    | `WriterElection` + bootstrap-orchestrated sync + `DrainCoordinator` + `DidWriteCoordinator` + CRDT-extension hooks (F12/F13) |

## Review Process

Multi-round adversarial review per BLUEPRINT §RFC Process. R1
expected 2026-08-11+. Convergence target: R3.
