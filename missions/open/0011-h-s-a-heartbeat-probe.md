# 0011-h-s-a-heartbeat-probe — Substrate additions for Heartbeat::probe impl (RFC-0855 §Wire Format heartbeat probing)

## Status

Claimed (2026-09-20) — Substrate slice landed at `next ffd3b9bf` (Heartbeat + HeartbeatProbeResult + UnreachableReason additive types + probe method on NEW module mon/heartbeat.rs per Phase 7 RFC-0011-o SlashBridge NEW module precedent). RFC draft at `next 2bb4bfb2`. Stub fill-in at `next d7be389d`. Paired-YAML Completed transition pending CLI dispatch slice per no-phantom-mission-pointers pairing invariant.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G17 + RFC-0011-v Phase 14 G17 heartbeat-probe amendment Draft at `next 2bb4bfb2`.

## Summary

Substrate-side heartbeat probe for peer reachability diagnostic. Required by `octo network heartbeat probe <peer_did>` per RFC-0011-v Phase 14 §Subcommand Taxonomy.

### Substrate additions target

NEW module `crates/octo-network/src/mon/heartbeat.rs` (per Phase 7 RFC-0011-o NEW module precedent for SlashBridge) with NEW additive types:

```rust
// crates/octo-network/src/mon/heartbeat.rs (NEW module)
/// Heartbeat probe (RFC-0855 §Wire Format heartbeat probing).
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Operates
/// on the in-memory snapshot; real transport-level probe OUT OF
/// SCOPE. Per-extension impl crates (Layer D) provide real
/// transport-level probes in follow-on missions. BTreeMap-based
/// deterministic iteration ordering preserved per RFC-0011-h
/// §Output Envelope determinism.
#[derive(Clone, Debug, Default)]
pub struct Heartbeat;

/// Heartbeat probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeartbeatProbeResult {
    /// Peer is reachable; `rtt_ms` is round-trip-time in milliseconds.
    Reachable { rtt_ms: u32 },
    /// Peer is unreachable; `reason` describes why.
    Unreachable { reason: UnreachableReason },
    /// Probe timed out.
    Timeout,
}

/// Reason for an unreachable probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnreachableReason {
    /// Peer DID is malformed.
    InvalidPeerDid,
    /// No transport adapter registered for this peer.
    NoTransportAdapter,
    /// Transport adapter refused the probe (e.g. protocol mismatch).
    AdapterRefused,
    /// Reserved for follow-on Layer D adapter detail.
    Other(String),
}

impl Heartbeat {
    /// Probe a peer by DID; returns `HeartbeatProbeResult`
    /// indicating reachability + RTT or unreachable reason.
    /// Phase 14 returns `Timeout` unconditionally (real probe
    /// OUT OF SCOPE).
    pub fn probe(&self, peer_did: &str, timeout_ms: u16) -> HeartbeatProbeResult {
        // Phase 14 additive-type-only: real transport-level probe
        // wiring OUT OF SCOPE; this stub returns Timeout per
        // RFC-0011-v §Heartbeat Probe semantics.
        let _ = (peer_did, timeout_ms);
        HeartbeatProbeResult::Timeout
    }
}
```

NEW module `crates/octo-network/src/mon/heartbeat.rs`. No existing modules touched. Zero regression on existing modules. `mon/liveness.rs` exists for a different concern (election-window liveness) and is NOT touched.

## Acceptance Criteria

- [ ] NEW module `crates/octo-network/src/mon/heartbeat.rs` lands per RFC-0011-h §Substrate-Additions row G17 + RFC-0011-v Phase 14 §Substrate Mapping Table
- [ ] `Heartbeat` struct lands (Clone + Debug + Default)
- [ ] `HeartbeatProbeResult` enum lands with 3 variants (Reachable { rtt_ms: u32 } + Unreachable { reason: UnreachableReason } + Timeout)
- [ ] `UnreachableReason` enum lands `#[non_exhaustive]` with 4 variants (InvalidPeerDid + NoTransportAdapter + AdapterRefused + Other(String))
- [ ] `Heartbeat::probe(&self, peer_did: &str, timeout_ms: u16) -> HeartbeatProbeResult` method lands (returns Timeout for Phase 14 stub)
- [ ] `pub mod heartbeat;` added to `crates/octo-network/src/mon/mod.rs`
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green (≥3 unit tests added; zero regression)
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥1 integration test (substrate-faithful boundary tests pin Timeout stub semantics + UnreachableReason enum variants + probe method signature)

## Dependencies

- RFC-0011-h Accepted (RFC-0011-h must be Accepted before this mission lands per RFC-0011-h §Substrate-Additions Companion Missions)
- RFC-0011-v Phase 14 heartbeat-probe amendment Draft
- Phase 14 G17 RFC draft at `next 2bb4bfb2`

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-heartbeat` covers that surface; CREATED at CLI dispatch slice time per user decision)
- Wire format versioning (deferred to RFC-0011-h §Future Work items F8 + F9)
- Per-extension transport impl (substrate-ext-heartbeat-transport-* Layer D follow-on missions, OUT OF SCOPE for this additive-type-only phase)
- Live transport-level probe (Phase 14 substrate operates on the in-memory snapshot only; real probe in follow-on Layer D adapter mission)

## Notes

Stub originally filed 2026-09-18 per [[no-phantom-mission-pointers]]. Stub note pinned path `crates/octo-network/src/mon/heartbeat.rs (NEW)` — substrate-faithful per Phase 7 RFC-0011-o SlashBridge NEW module precedent. Full AC + scope land in Phase 14 stub fill-in commit per the Phase 5 RFC-0011-m 5-commit pattern. Phase 14 follows the Phase 5 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`. Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent (0 NEW OctoCliError variants). BTreeMap-based deterministic iteration ordering preserved per RFC-0011-h §Output Envelope determinism. `Heartbeat::probe()` operates on in-memory snapshot only; real transport-level probe in follow-on Layer D adapter mission per per-extension crate pattern.
