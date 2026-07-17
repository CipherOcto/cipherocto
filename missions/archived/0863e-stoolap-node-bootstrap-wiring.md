# Mission: 0863e — Wire BootstrapOrchestrator into stoolap-node

## Status

Open (2026-06-25) — pre-public-launch

## RFC

- RFC-0863 (Networking): General-Purpose Network Integration — Phase 4 (last item)
- RFC-0862 (Networking): Stoolap Data Sync — F11 (bootstrap-orchestrated peer discovery)

## Summary

Wire the `BootstrapOrchestrator` (from mission `0851p-a-base-bootstrap-orchestrator`) into `stoolap-node` as the default peer discovery path. When no `--peer` CLI args are provided and adapters are loaded, the node runs the RFC-0851p-a Mode A bootstrap protocol to acquire peers instead of requiring manual peer configuration.

This mission closes the gap between "the bootstrap protocol exists as code in `octo-transport`" and "a real node actually uses it on startup." Without this mission, operators must manually specify `--peer` addresses — which is the E2E testing path, not the production path.

## Design

### Current behavior

```text
stoolap-node --dsn test.db --listen 9000 \
  --adapter quic --adapter webhook \
  --peer 192.168.1.10:9000 --peer 192.168.1.11:9000
```

The `--peer` args are passed to `TcpStream::connect()` for direct TCP sync. No bootstrap, no seed list, no Sybil defense.

### Target behavior

```text
# Production: bootstrap from seed list (no --peer needed)
stoolap-node --dsn test.db --listen 9000 \
  --adapter quic --adapter webhook \
  --seed-list /etc/cipherocto/seed_list_v1.json

# Development: manual peers (existing behavior, unchanged)
stoolap-node --dsn test.db --listen 9000 \
  --peer 192.168.1.10:9000 --peer 192.168.1.11:9000
```

### Changes

#### 1. New CLI args in `Args` struct

```rust
/// Path to seed list JSON file (RFC-0851p-a §1).
/// When provided, runs BootstrapOrchestrator instead of --peer TCP path.
#[arg(long)]
seed_list: Option<String>,

/// Seed list authority type: "foundation" or "dao" (default: "foundation").
#[arg(long, default_value = "foundation")]
seed_authority: String,
```

#### 2. Bootstrap path in `main()`

After adapters are loaded (step 2) and before the transport receive loop (step 4), insert:

```rust
// Bootstrap path: when --seed-list is provided and no --peer args
if let Some(seed_list_path) = &args.seed_list {
    if args.peers.is_empty() {
        let authority = match args.seed_authority.as_str() {
            "dao" => SeedListAuthority::Dao,
            _ => SeedListAuthority::Foundation,
        };
        let mut orchestrator = BootstrapOrchestrator::new(
            &seed_list_path,
            BootstrapConfig {
                authority,
                ..BootstrapConfig::default()
            },
        )?;
        let discovery_arc = discovery.clone();
        let mut disc_state = DiscoveryState::new(BootstrapMethod::Static);
        let count = orchestrator.run(
            &transport,
            &discovery_arc.lock().unwrap(),
            &mut disc_state,
        ).await?;
        tracing::info!(peers = count, "bootstrap complete");
    }
}
```

#### 3. `--peer` path unchanged

The existing `--peer` TCP path remains as-is for development and testing. When both `--seed-list` and `--peer` are provided, `--peer` takes precedence (backward compatible).

#### 4. Dependency update

`sync-e2e-tests/stoolap-node/Cargo.toml` needs `octo-transport` as a dependency (already present for the `--adapter` path). No new deps needed — `BootstrapOrchestrator` lives in `octo-transport`.

## Acceptance Criteria

- [ ] `--seed-list` CLI arg added to `Args` struct
- [ ] `--seed-authority` CLI arg added (foundation/dao, default: foundation)
- [ ] When `--seed-list` provided and `--peer` empty, `BootstrapOrchestrator::run()` executes before transport receive loop
- [ ] `--peer` path unchanged (backward compatible)
- [ ] When both `--seed-list` and `--peer` provided, `--peer` takes precedence
- [ ] Bootstrap success logs peer count at INFO level
- [ ] Bootstrap failure logs error and exits with clear message
- [ ] L4 E2E test: two nodes discover each other via seed list (no `--peer`)
- [ ] L4 E2E test: seed list + `--peer` fallback (mixed mode)

### Type Coverage

| RFC-0863 Phase 4 Task | Implemented By |
|-----------------------|----------------|
| "Wire into stoolap-node as default bootstrap path" | This mission |
| All other Phase 4 tasks | Mission `0851p-a-base-bootstrap-orchestrator` |

## Dependencies

Depends on:
- **Mission `0851p-a-base-bootstrap-orchestrator`** — must be completed first (creates `BootstrapOrchestrator`)
- RFC-0863 Phase 4: `BootstrapOrchestrator` must exist in `octo-transport`
- RFC-0862: `stoolap-node` must have `--adapter` path (done)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

| File | Action |
|------|--------|
| `sync-e2e-tests/stoolap-node/src/main.rs` | Add `--seed-list`/`--seed-authority` args; add bootstrap path in `main()` |
| `sync-e2e-tests/stoolap-node/Cargo.toml` | No change needed (`octo-transport` already a dep) |

## Complexity

Low (~60-80 lines; 2 new CLI args, one bootstrap invocation block, one E2E test).

## Prerequisites

- Mission `0851p-a-base-bootstrap-orchestrator`: Open (must complete first)
- `octo-transport::BootstrapOrchestrator`: Does not exist yet (created by prerequisite mission)

## Notes

### Why --peer takes precedence

Backward compatibility. Existing E2E tests and development workflows use `--peer` exclusively. The bootstrap path is additive — it only activates when `--seed-list` is provided and `--peer` is empty.

### Why not require --adapter for bootstrap

The `BootstrapOrchestrator` sends `BOOTSTRAP_REQ` via `NodeTransport`, which requires adapters. But `--adapter` is also used for the non-bootstrap transport path. Requiring `--adapter` when `--seed-list` is present is natural and already implied. No additional constraint needed.

### Relationship to mission 0851p-a-base

Mission `0851p-a-base-bootstrap-orchestrator` creates the `BootstrapOrchestrator` in `octo-transport`. This mission is the **consumer** — it wires the orchestrator into the only real node binary. Without this mission, the orchestrator exists but nothing calls it.

## Mitigates

- RFC-0863 Phase 4 last item: "Wire into stoolap-node as default bootstrap path"
- RFC-0862 F11: "Bootstrap-orchestrated peer discovery for sync"

## Deadline

Pre-public-launch

## Related Missions

- `0851p-a-base-bootstrap-orchestrator.md` — prerequisite (creates `BootstrapOrchestrator`)
- `0862j-network-layer-integration.md` — related (transport sync wiring, already complete)
