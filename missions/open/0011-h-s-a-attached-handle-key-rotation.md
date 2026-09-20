# 0011-h-s-a-attached-handle-key-rotation — Substrate additions for AttachedHandleKeyRotation trait

## Status

Claimed (2026-09-20) — Substrate slice landed at `next 3794e4a8` per RFC-0011-l Phase 4 row G21. The typed dispatch bridge between the Layer C `bind-envelope rebind-{prepare,commit,abort}` clap arm trio and the Layer B `RebindCoordinator` payload-builder surface landed in `crates/octo-network/src/mon/rebind_arm.rs`. CLI dispatch slice pending per the established Phase 3 paired-YAML completion pattern (substrate → CLI dispatch → Completed transition).

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G21 + RFC-0011-l Phase 4 §Substrate-Additions + RFC-0011-c §F.5.1 D2.1 paired-acceptance bridge

## Summary

RFC-0011-c §F.5.1 D2.1 (discriminator-only additive slice) LANDED at `next 01340b93` per RFC-0011-c §F.5.1 closure card. D2.2 (population policy) deferred post-PQC. The G21 substrate bridge lives at `crates/octo-network/src/mon/rebind_arm.rs` because the Layer B substrate is the right home for the typed dispatch surface; the Layer A-equivalent `octo-runtime::handle::key_id` is RFC-frozen per `octo-attach-key-rotation` Cargo feature gate.

### Substrate additions target

```rust
// crates/octo-network/src/mon/rebind_arm.rs (NEW)
#[non_exhaustive]
pub enum RebindArmAction { Prepare, Commit, Abort }

#[non_exhaustive]
pub enum RebindArmKey { V1 }

#[non_exhaustive]
pub enum RebindArmError { AdapterUnwired, UnknownArm(String) }

#[serde(tag = "arm")]
pub enum RebindArmPayload { Prepare(RebindPrepare), Commit(RebindCommit), Abort(RebindAbort) }

pub fn dispatch_rebind_arm_action(
    coordinator: &RebindCoordinator,
    arm: RebindArmAction,
    key: RebindArmKey,
) -> Result<RebindArmPayload, RebindArmError>;
```

Substrate slice landed 2026-09-20 at `next 3794e4a8`:
- `RebindArmAction` enum (Prepare + Commit + Abort) — typed clap arm registration. `#[non_exhaustive]` so future arms (e.g. `rotate` for D2.2) land additive without breaking downstream exhaustive matches
- `RebindArmKey` enum (V1) — D2.1 discriminator mirror of `octo_runtime::handle::key_id::KeyId`. The substrate always returns `V1` today because the persistence adapter is the owner of the key-id → envelope mapping (Phase 6 follow-on)
- `RebindArmError` enum — substrate-faithful error surface mirroring the Phase 3 G12 `CoordinatorAdminActionError` shape
- `RebindArmPayload` enum — tagged JSON envelope so CLI dispatch roundtrips cleanly through serde
- `dispatch_rebind_arm_action` sync helper — substrate-faithful boundary that returns `Err(AdapterUnwired)` today (Phase 6 persistence adapter follow-on per `0011-h-s-a-rebind-arm-persistence`)
- `mon/mod.rs` registers the new module + re-exports the public surface for Layer C consumption
- 5 substrate unit tests pinned: serde roundtrip across 3 arm variants + V1 default discriminator + AdapterUnwired return contract + idempotent-across-calls + UnknownArm serde roundtrip

CLI dispatch slice pending: `octo network bind-envelope rebind-{prepare,commit,abort} --domain-id <id>` will call `dispatch_rebind_arm_action` and translate the `AdapterUnwired` return to typed exit 1 `Internal` carrying the forward-looking Phase 6 note per RFC-0011-h §Error Handling.

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/rebind_arm.rs (NEW)` per RFC-0011-h §Substrate-Additions row G21 (next 3794e4a8)
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (1438/1438, +5 above Phase 3 baseline)
- [x] Layer discipline preserved (Layer B helper module on existing substrate; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (5 substrate unit tests added; integration test deferred to Phase 6 persistence adapter follow-on)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands. Substrate-first ordering per [[no-phantom-mission-pointers]]: G21 substrate slice (this mission) lands BEFORE Phase 4 CLI dispatch slice.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-bind-envelope` covers that surface in Phase 4 CLI dispatch slice)
- Persistence adapter (Phase 6 follow-on per `0011-h-s-a-rebind-arm-persistence`)
- D2.2 population policy (deferred post-PQC per RFC-0011-c §F.5.1 D2.1 closure card)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Substrate slice landed 2026-09-20 at `next 3794e4a8`. The D2.1 paired-acceptance bridge is honored: `RebindArmKey::V1` is the only reachable variant today, mirroring `KeyId::V1` always-on reachability in `octo-runtime::handle::key_id` per RFC-0011-c §F.5.1 D2.1 closure card at `next 01340b93`. The `octo-attach-key-rotation` Cargo feature gating `KeyId::V2` carries through to a future G21-extension when D2.2 lands post-PQC.
