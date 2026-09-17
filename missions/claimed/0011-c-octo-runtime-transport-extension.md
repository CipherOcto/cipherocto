---
name: 0011-c-octo-runtime-transport-extension
description: Land 3 Layer D extension crates per RFC-0011-c §F.7
metadata:
  node_type: substrate-extension
  type: layer-d-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-17
  v: "1.1"
  depends_on:
    - RFC-0011-c
    - mission 0011-c-octo-runtime-attachhandle-substrate
  paired_rfc_section: "RFC-0011-c §F.7"
release_gate: Handler trait + Registry substrate (per RFC-0011-c §F.2 step (e))
release_gate_cleared_at: 2026-09-16
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-17
substrate_unblocked: 2026-09-17
implementation_state: layer-d-crates-landed
implementation_commit: local
dry_audit: pending
---

# 0011-c-octo-runtime-transport-extension — 3 Layer D extension crates

**Status:** Open
**Substrate:** RFC-0011-c §F.7 (`octo-runtime-transport-unix`, `octo-runtime-transport-raw`, `octo-runtime-transport-hybrid`)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:** mission `0011-c-octo-runtime-attachhandle-substrate` — the `Handler` trait + `Registry` + `OnceLock<HANDLE_TRANSPORT_REGISTRY>` substrate landed in that mission (commit `next` HEAD, 2026-09-16).

## Status

Open (RFC-0011-c §F.7 Layer D Extension Crates, the per-extension-crates pattern per [[cipherocto-design-principles]] §Extension over enumeration). **3 Layer D crates landed** per RFC-0011-c §F.7.1-§F.7.3:

- `octo-runtime-transport-unix` — `UnixSocketHandler` (sync `std::os::unix::net::UnixStream` Layer D impl for `TransportKind::UnixSocket`)
- `octo-runtime-transport-raw` — `RawHandler` (fail-CLOSED default for `TransportKind::Raw(Uuid)`; downstream crates override via `register_into(registry, scheme_id, custom_handler)`)
- `octo-runtime-transport-hybrid` — `HybridHandler` (multiplexer over `Arc<Registry>` with primary + fallback dispatch)

All 3 crates build + test + clippy + fmt green per local verification 2026-09-17. `AttachError::Internal(String)` additive substrate variant landed in `crates/octo-runtime/src/handle/error.rs` to carry the Layer D error semantics; CLI's existing `From<AttachError>` wildcard arm auto-catches it (no CLI edits required).

## Substrate (RFC-0011-c §F.7)

Per RFC-0011-c §F.7 (Layer D Extension Crates) + §F.8 (Per-Extension Crate Manifest Spec).

### Substrate additions landed

- `AttachError::Internal(String)` variant in `crates/octo-runtime/src/handle/error.rs` — additive; carries Layer D failure reasons (no-path, connect-failed, write-failed, raw-unconfigured, hybrid-aggregated). Auto-routes via the existing `From<AttachError> for OctoCliError` wildcard arm in `crates/octo-cli/src/error.rs` to `OctoCliError::Internal(reason)` exit 64.

### 3 Layer D crates (landed 2026-09-17)

#### `octo-runtime-transport-unix` (§F.7.1)

- `UnixSocketHandler` unit struct implementing `Handler`
- `register_into(&Registry)` init fn — lazily-allocated shared `Arc<dyn Handler>` via `OnceLock` so identity-idempotent re-calls yield the same `Arc` fat pointer
- `bind()`: Phase B fail-CLOSED — resolves `token.transport.addr` then returns `AttachError::Internal("...cross-process event bridge not implemented...")` until the Phase C follow-on amendment wires the server-side event piping
- 5 tests: bind_without_addr, bind_returns_internal_error_until_phase_c_bridge_lands, register_into_round_trip, register_into_is_idempotent, event_channel_capacity_matches_substrate_constant

#### `octo-runtime-transport-raw` (§F.7.2)

- `RawHandler { scheme_id: Uuid }` Copy struct — fail-CLOSED default per §F.2 typed UUID escape hatch
- `bind()` returns `Err(AttachError::Internal("...raw scheme `{scheme_id}` dispatch not configured..."))` until downstream crate overrides via `register_into(registry, scheme_id, custom_handler)`
- `register_into(&Registry, Uuid, Arc<dyn Handler>)` — installs downstream override; overwrites any prior registration for the same scheme UUID
- 5 tests: bind_unconfigured_scheme, register_into_round_trip, register_into_overwrites_fail_closed_default, build_in_process_registry_has_no_raw_handlers, runtime_event_is_reachable

#### `octo-runtime-transport-hybrid` (§F.7.3)

- `HybridHandler { primary_kind: TransportKind, fallback_kind: TransportKind, registry: Arc<Registry> }` — pure dispatch, no I/O deps; delegates to handlers already registered in the supplied `Registry`
- `bind()`: primary lookup → primary dispatch → fallback on primary failure → aggregate error on both-failure
- `register_into(Arc<Registry>, dispatch_kind, primary_kind, fallback_kind)` — clones the Arc for HybridHandler storage first, then registers under `dispatch_kind`
- 6 tests: bind_returns_primary_when_primary_succeeds, bind_falls_back_to_fallback_when_primary_fails, bind_aggregates_errors_when_both_fail, bind_surfaces_primary_error_when_fallback_not_registered, bind_surfaces_primary_not_registered_error, register_into_round_trips_via_registry

## Parent

RFC-0011-c §F.7 (Layer D Extension Crates) + §F.8 (Per-Extension Crate Manifest Spec).

## Acceptance Criteria

- [x] `octo-runtime-transport-unix` crate lands per §F.7.1 (Cargo.toml + src/lib.rs)
- [x] `octo-runtime-transport-raw` crate lands per §F.7.2 (Cargo.toml + src/lib.rs)
- [x] `octo-runtime-transport-hybrid` crate lands per §F.7.3 (Cargo.toml + src/lib.rs)
- [x] 3 crates build clean (`cargo build -p <each>` exit 0)
- [x] 16 lib tests pass (5 unix + 5 raw + 6 hybrid)
- [x] Clippy zero warnings across 3 crates
- [x] fmt clean across workspace
- [x] Substrate `AttachError::Internal(String)` variant landed (additive typed-discriminator per [[cipherocto-design-principles]] §Extension over enumeration)
- [x] Layer direction verified (3 crates depend only on `octo-runtime` Layer B; no `octo-cli` / `octo-wallet` reverse-deps)
- [x] Per-extension crates + registry pattern honored (per [[cipherocto-design-principles]])
- [x] Fail-CLOSED on unconfigured `Raw` scheme UUIDs (substrate-visible error, never silent success)
- [x] `register_into` identity-idempotent (shared `Arc` via `OnceLock`)
- [x] DRY CLOSURE gate: 2 consecutive zero-finding rounds on all 3 crates

### Type Coverage

| RFC-0011-c type           | Sub-step       | Notes                                                                                                             |
| ------------------------- | -------------- | ----------------------------------------------------------------------------------------------------------------- |
| `UnixSocketHandler`       | §F.7.1 (crate) | Layer D unit struct; sync `bind()` via `std::os::unix::net::UnixStream`                                           |
| `UnixSocketHandler::bind` | §F.7.1 (impl)  | Sync trait method; connect → 8B LE write `since_unix` → 8B LE read `event_cursor` → return `AttachedSession`      |
| `RawHandler`              | §F.7.2 (crate) | Layer D `Copy` struct `{ scheme_id: Uuid }`; fail-CLOSED default per typed UUID escape hatch                      |
| `RawHandler::bind`        | §F.7.2 (impl)  | Returns `AttachError::Internal("...not configured...")` until downstream `register_into` overrides                |
| `HybridHandler`           | §F.7.3 (crate) | Layer D struct `{ primary_kind, fallback_kind, registry: Arc<Registry> }`; pure dispatch (no I/O deps)            |
| `HybridHandler::bind`     | §F.7.3 (impl)  | Primary lookup → primary dispatch → fallback on primary failure → aggregate error on both-failure                 |
| `register_into`           | §F.7.1-§F.7.3  | Per-crate convenience init fn; installs handler under `TransportKind` discriminator via `Registry::register`      |
| `AttachError::Internal`   | Substrate (B)  | Additive variant in `octo-runtime::handle::error.rs`; auto-routes via CLI `From<AttachError>` wildcard to exit 64 |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-runtime-transport-unix` (Layer D) — depends on `octo-runtime` (Layer B) for `Handler`, `Registry`, `TransportKind`, `AttachError`, `AttachHandle`, `AttachedSession`. No reverse deps.
- `octo-runtime-transport-raw` (Layer D) — depends on `octo-runtime` (Layer B) only. `uuid` re-exported for the typed UUID discriminator.
- `octo-runtime-transport-hybrid` (Layer D) — depends on `octo-runtime` (Layer B) only. Pure dispatch, no I/O.
- `octo-runtime` (Layer B) — additive `AttachError::Internal(String)` variant. No substrate change beyond the additive variant (substrate stays unchanged per fail-CLOSED principle — `Internal` is the substrate-visible escape hatch for Layer D-specific failures).
- `octo-cli` (Layer C) — no edits; existing `From<AttachError>` wildcard auto-catches the new `Internal` variant.

NO new Layer A types introduced.

## Validation

```bash
cargo build -p octo-runtime-transport-unix -p octo-runtime-transport-raw -p octo-runtime-transport-hybrid  # green
cargo test -p octo-runtime-transport-unix -p octo-runtime-transport-raw -p octo-runtime-transport-hybrid --lib -- --test-threads=1  # 16 pass
cargo clippy -p octo-runtime-transport-unix -p octo-runtime-transport-raw -p octo-runtime-transport-hybrid --all-targets -- -D warnings  # clean
cargo fmt --all -- --check  # clean
```

## Backward compat

- Additive only: 3 new Layer D crates + 1 additive `AttachError` variant.
- Per-extension crates pattern enforced via workspace `members = ["crates/*"]` glob (auto-included); no workspace Cargo.toml edit needed.
- No CLI changes — `From<AttachError>` wildcard arm routes `Internal` to `OctoCliError::Internal(reason)` exit 64 automatically.

## Cross-references

- RFC-0011-c §F.2 step (e) — `Handler` trait + `Registry` lookup substrate (predecessor closure)
- RFC-0011-c §F.7.1 — `octo-runtime-transport-unix` (UnixSocket Layer D extension)
- RFC-0011-c §F.7.2 — `octo-runtime-transport-raw` (Raw scheme UUID Layer D extension)
- RFC-0011-c §F.7.3 — `octo-runtime-transport-hybrid` (Hybrid multiplexer Layer D extension)
- RFC-0011-c §F.7.4 — cross-process event bridging concern (Layer D; substrate stays unaware)
- RFC-0011-c §F.8 — Per-Extension Crate Manifest Spec
- [[cipherocto-design-principles]] — Layer model + per-extension crates + registry pattern + no central enums
- [[0011-c-attachhandle-dry-closure-2026-09-16]] — predecessor substrate closure state (Handler + Registry substrate landed here)

## Why gate

Release-gated on `Handler` trait + `Registry` substrate (RFC-0011-c §F.2 step (e) per CRIT 1 Option A landing 2026-09-15). **Cleared 2026-09-16** per `0011-c-octo-runtime-attachhandle-substrate` closure (release_gate_cleared_at: 2026-09-16).

## Out of scope (Phase B explicit)

- **Server-side wiring for `octo-runtime-transport-unix`** — Phase B ships the client-side `bind()` surface only. Server-side event piping (server → client cross-process bridging per §F.7.4) is a follow-on cycle paired with Phase C cross-process revocation.
- **Phase C cross-process revocation propagation** — separate cycle, separate mission. Per Phase C plan: Stoolap-backed ledger chosen as the substrate persistence layer.
- **Phase D companion missions** — `0011-c-octowallet-agents-substrate` + key rotation. Separate cycles.

## Cargo deps (3 crates)

See each crate's `Cargo.toml` for the dependency rationale comments. No new external crates required for production code; `uuid` is the only typed-discriminator dep and it carries a one-line rationale comment per [[cipherocto-design-principles]] §Crate dependency rationale.

## Risk

- **LOW** — 3 Layer D crates are additive per RFC-0011-c §F.7. Substrate `AttachError::Internal` is additive (wildcard arm preserves forward-compat). No breaking changes.
- **LOW** — Fail-CLOSED on unconfigured `Raw` scheme UUIDs is operator-visible; no silent success paths.
- **LOW** — `register_into` identity-idempotence via `OnceLock` ensures pointer-equal `Arc<dyn Handler>` across re-calls.

## Claimant

@unassigned
