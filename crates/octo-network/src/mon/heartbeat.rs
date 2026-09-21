//! Heartbeat probe substrate (RFC-0855 §Wire Format heartbeat probing).
//!
//! Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Operates
//! on the in-memory snapshot; real transport-level probe OUT OF
//! SCOPE for Phase 14. Per-extension impl crates (Layer D) provide
//! real transport-level probes in follow-on missions. BTreeMap-based
//! deterministic iteration ordering preserved per RFC-0011-h
//! §Output Envelope determinism. `mon/liveness.rs` exists for a
//! different concern (election-window liveness) and is NOT touched.

/// Heartbeat probe (RFC-0855 §Wire Format heartbeat probing).
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Operates
/// on the in-memory snapshot; real transport-level probe OUT OF
/// SCOPE for Phase 14. Per-extension impl crates (Layer D) provide
/// real transport-level probes in follow-on missions.
#[derive(Clone, Debug, Default)]
pub struct Heartbeat;

/// Heartbeat probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Additive
/// enum on the NEW module per Phase 7 RFC-0011-o precedent; no
/// central edit.
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
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Additive
/// enum on the NEW module per Phase 7 RFC-0011-o precedent; no
/// central edit.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// indicating reachability + RTT or unreachable reason or
    /// `Timeout`. Phase 14 returns `Timeout` unconditionally
    /// (real transport-level probe OUT OF SCOPE).
    pub fn probe(&self, peer_did: &str, timeout_ms: u16) -> HeartbeatProbeResult {
        let _ = (peer_did, timeout_ms);
        HeartbeatProbeResult::Timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // tv_phase14_substrate_1: Heartbeat default construction.
    #[test]
    fn tv_phase14_substrate_1_heartbeat_default_construction() {
        let hb = Heartbeat;
        // Default construction must succeed without panic.
        let _ = format!("{:?}", hb);
    }

    // tv_phase14_substrate_2: probe returns Timeout unconditionally.
    #[test]
    fn tv_phase14_substrate_2_probe_returns_timeout() {
        let hb = Heartbeat;
        let result = hb.probe("did:octo:example-peer", 5000);
        assert_eq!(result, HeartbeatProbeResult::Timeout);
    }

    // tv_phase14_substrate_3: probe with timeout_ms=0 returns Timeout.
    #[test]
    fn tv_phase14_substrate_3_probe_zero_timeout_returns_timeout() {
        let hb = Heartbeat;
        let result = hb.probe("did:octo:example-peer", 0);
        assert_eq!(result, HeartbeatProbeResult::Timeout);
    }

    // tv_phase14_substrate_4: HeartbeatProbeResult variants PartialEq.
    #[test]
    fn tv_phase14_substrate_4_probe_result_variants_partial_eq() {
        let r1 = HeartbeatProbeResult::Reachable { rtt_ms: 42 };
        let r2 = HeartbeatProbeResult::Reachable { rtt_ms: 42 };
        let r3 = HeartbeatProbeResult::Reachable { rtt_ms: 100 };
        assert_eq!(r1, r2);
        assert_ne!(r1, r3);

        let u1 = HeartbeatProbeResult::Unreachable {
            reason: UnreachableReason::InvalidPeerDid,
        };
        let u2 = HeartbeatProbeResult::Unreachable {
            reason: UnreachableReason::NoTransportAdapter,
        };
        assert_ne!(u1, u2);

        assert_eq!(HeartbeatProbeResult::Timeout, HeartbeatProbeResult::Timeout);
    }

    // tv_phase14_substrate_5: UnreachableReason::Other carries string payload.
    #[test]
    fn tv_phase14_substrate_5_unreachable_reason_other_payload() {
        let reason = UnreachableReason::Other("connection-reset".to_string());
        let result = HeartbeatProbeResult::Unreachable { reason };
        if let HeartbeatProbeResult::Unreachable {
            reason: UnreachableReason::Other(payload),
        } = result
        {
            assert_eq!(payload, "connection-reset");
        } else {
            panic!("expected Unreachable Other");
        }
    }
}
