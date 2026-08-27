# 0871e-f7-coordinator-impl — Concrete `WriterElection` + `octo-sync` crate landing

**Status:** claimed (2026-08-11); RFC substrate accepted same session.
**Substrate RFCs (all Accepted 2026-08-11):**
- RFC-0862 v1.3 §Specification §WriterElection + §DidWriteCoordinator + §Substrate types
- RFC-0862 v1.4 §Concrete Impl Extension (NEW)
- RFC-0010 v1.4 §ChainId Namespace Extension (depended on by `ChainId`)
- RFC-0010 v1.5 §Rich DidDocument Extension (depended on by `DidDocument` chain_depth / chain_parent / verification_method)
**Parent:** Mission `0871e-f7-cross-instance-did-coordination` (substrate) + Mission `0871e-f7-impl-resolver-mediation` (mediation; LANDED 2026-08-11)
**DAG predecessors:** `0871e-f7-impl-resolver-mediation` (LANDED); `0871b-storage-backend` (LANDED 2026-08-11, commit `71f8d745`)

## Scope

Land the v1.3 `WriterElection` / `DidWriteCoordinator` / `NonceTracker` /
WAL substrate types in `octo-sync/` (currently RFC-0862 v1.1.0 substrate;
v1.3 types have NOT landed yet — this mission is the substrate-landing +
concrete-impl mission) AND add the concrete `RaftLikeWriterElection` +
`RaftLikeDidWriteCoordinator` impls from RFC-0862 v1.4 §Concrete Impl
Extension. Lift `octo-sync` workspace membership per RFC-0862 v1.4
AC#17. Land 4 cross-instance TV.

### Mission scope

1. **Substrate types** — port v1.3 §Specification §Substrate types to
   `crates/octo-sync/src/types.rs` (or `octo-sync/src/types.rs` if
   moving the crate under `crates/`). Newtypes + impls:
   - `HlcTimestamp`, `HlcClock`, `WriterNodeId`, `ShardMissionId`,
     `ShardKey`, `ChainId`, `OperatorId`, `OperatorSignature`,
     `OperatorSet`, `WriterIdentity`, `WriterContext`, `ReplayState`
     (7-state `WriterLifecycle`), `NonceRecord`, `PeerIdentity`
   - `WalEntry` + WAL magic constants + entry layout per v1.3
     §Specification §V2 WAL header_size extension
2. **Sealed traits** — `WriterElection` + `WriterElectionForceRelinquish`
   + `BootstrapOrchestrator` + `DrainCoordinator` + `DidWriteCoordinator`
   + `WalWriter` + `WalReader` + `WalNonceScanner` per v1.3 §Specification
   §traits sections.
3. **`octo_sync::did::canonical_hash` free fn** (per v1.3 R11 H2) +
   `EncodedDidDocument` impl for `DidDocument`.
4. **`verify_governance_attestation`** + `governance_signature_message`
   per v1.3 §Specification §Governance.
5. **`replay_wal`** + `apply_entry` per v1.3 §Specification §WAL Replay.
6. **Concrete `RaftLikeWriterElection`** per v1.4 §Concrete Impl Extension
   §Data Structures. Production impl; sealed trait pattern preserved.
7. **Concrete `RaftLikeDidWriteCoordinator`** per v1.4 §Concrete Impl
   Extension §Data Structures. Backed by `RaftLikeWriterElection`.
8. **Workspace membership lift** — root `Cargo.toml` `exclude = [...]`
   drops `"octo-sync"` (per v1.4 AC#17). The existing
   `/home/mmacedoeu/_w/ai/cipherocto/octo-sync/Cargo.toml` stays at
   the same path; only the workspace exclusion changes.
9. **4 cross-instance TV** — `crates/octo-sync/tests/cross_instance_tv.rs`
   (or `octo-sync/tests/cross_instance_tv.rs` if workspace-stays at
   root):
   - TV-1 atomic_register — 3 instances concurrent register
   - TV-2 leader_failover — kill elected leader, new leader wins
   - TV-3 wal_replay — A commits 3 entries, crash A, replay on restart
   - TV-4 fail_closed — inject WriterUnavailable mock
10. **Optional `crdt` feature flag** (per v1.4 §Motivation 4) — opt-in,
    gated on `cargo test --features crdt`. Default builds stay
    linearizable.

### Layer discipline

Per [[cipherocto-design-principles]] §Layer direction:
- `octo-sync` (Layer B-substrate) — concrete `WriterElection` +
  `DidWriteCoordinator` impls + substrate types
- `octo-ident` (Layer B) — `DidWriteCoordinator` TRAIT only (existing;
  substrate spec unchanged from v1.3)
- `octo-identity-resolver-node` (Layer C) — mediator (LANDED 2026-08-11
  via `0871e-f7-impl-resolver-mediation`)
- `quota-router-storage` (Layer B) — `StoolapDidRegistry` UNCHANGED
  (consumes the trait; no `octo-sync` dep — same DI shape as
  `IdentityResolverNodeConfig`)

`octo-ident` does NOT depend on `octo-sync` directly (per R12 M19).
`DidWriteCoordinator` trait in `octo-ident` is the only surface;
`octo-sync` provides the concrete impl.

### Approach

Per [[drain-coordinator-approach-2026-08-10]] Option B (centralized
aggregator) — picked for `DrainCoordinator` work; same substrate,
separate amendment applies here for `DidWriteCoordinator`. The
`crdt` feature flag enables Option C LWW for deployments that need
partition-tolerance over the linearizability guarantee.

### Test Vectors

- 4 cross-instance TV (TV-1 through TV-4 from v1.4 §Test Vectors).
- 8 v1.3 §Performance Targets TV (Phase 1 + Phase 3 split; the 4
  unassigned TV `election_acquire_returns_within_3s`,
  `drain_throughput_1k_per_sec`, `failover_pause_under_3s`,
  `wal_fanout_lag_under_100ms` land in Phase 3 with the concrete
  impl).
- Optional `crdt`-feature TV: `lww_resolves_conflict_during_partition`.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-sync --all-targets --all-features -- -D warnings
cargo clippy -p octo-ident --all-targets -- -D warnings
cargo clippy -p octo-protocol --all-targets -- -D warnings
cargo test --lib -p octo-sync
cargo test --lib -p octo-ident
cargo test --lib -p octo-protocol
cargo test --lib -p octo-identity-resolver-node
# Phase 3 perf TV (8 vectors) require `cargo bench` integration;
# covered when the concrete impl lands Phase 3 deliverables.
```

## Cross-references

- [[rfc-0862-writer-election-bootstrap]] — v1.3 substrate (Accepted 2026-08-11)
- [[drain-coordinator-approach-2026-08-10]] — Option B pick (centralized aggregator)
- [[mission-0871e-f7-cross-instance-did-coordination]] — substrate mission
- [[mission-0871e-f7-impl-resolver-mediation]] — mediation layer (LANDED)
- [[mission-0871b-storage-backend]] — sibling substrate mission
- [[cipherocto-design-principles]] — Layer direction (A→B→C→D/E)

## Follow-on

- Mission `0871e-force-relinquish-governance` v0.2 snapshot+replay AC
  remains pending (RFC-0862 v1.3 §Future Work).
- Mission `0871e-phase5c-1-cross-instance-drain` BLOCKED on
  `DrainCoordinator` impl (sibling mission; separate work item).
- RFC-0862 v2.0 amendment planned for `octo-coordinator-bft` Layer A
  crate per v1.4 §Out-of-scope + RFC-0862 v1.3 §Future Work.

## Claimant

@mmacedoeu

## Pull Request

#
