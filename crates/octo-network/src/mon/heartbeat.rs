//! Heartbeat probe substrate (RFC-0855 §Wire Format heartbeat probing).
//!
//! Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Real
//! transport-level probe is OUT OF SCOPE for Phase 14; per-extension
//! impl crates (Layer D) provide real transport-level probes in
//! follow-on missions. `mon/liveness.rs` exists for a different
//! concern (election-window liveness) and is NOT touched.

/// Heartbeat probe (RFC-0855 §Wire Format heartbeat probing).
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Real
/// transport-level probe OUT OF SCOPE for Phase 14; per-extension
/// impl crates (Layer D) provide real transport-level probes in
/// follow-on missions.
#[derive(Clone, Debug, Default)]
pub struct Heartbeat;

/// Heartbeat probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Additive
/// enum on the NEW module per Phase 7 RFC-0011-o precedent; no
/// central edit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Per-adapter detail payload; opaque to the substrate
    /// (Layer D concrete adapter crates define the detail
    /// payload semantics in follow-on missions).
    Other(String),
}

/// Minimal DID syntactic validator (RFC-0855 §Identifiers).
///
/// Phase 14 stub: a peer DID is structurally valid iff it starts
/// with `did:octo:` and has a non-empty method-specific identifier
/// segment. Real DID validation (signature checks, DID-document
/// lookup, schema validation) is OUT OF SCOPE for Phase 14; the
/// stub-envelope extension defers this to per-extension Layer D
/// adapter missions.
fn is_structurally_valid_did(peer_did: &str) -> bool {
    let Some(method_specific) = peer_did.strip_prefix("did:octo:") else {
        return false;
    };
    !method_specific.is_empty()
}

impl Heartbeat {
    /// Probe a peer by DID; returns `HeartbeatProbeResult`
    /// indicating reachability + RTT or unreachable reason or
    /// `Timeout`. Phase 14 stub contract:
    ///
    /// 1. A structurally malformed `peer_did` (does not start
    ///    with `did:octo:` or has empty method-specific
    ///    identifier) returns
    ///    `Unreachable { reason: InvalidPeerDid }`. This is the
    ///    ONLY information-bearing branch in Phase 14 (it is
    ///    derived from pure substring analysis).
    /// 2. A structurally well-formed `peer_did` returns `Timeout`
    ///    unconditionally. Real transport-level probe (reachability
    ///    detection + RTT measurement) is OUT OF SCOPE for
    ///    Phase 14; per-extension Layer D adapter crates provide
    ///    real transport-level probes in follow-on missions.
    ///
    /// `timeout_ms` is part of the public API for forward
    /// compatibility with the Layer D adapter missions but is
    /// NOT consumed by the Phase 14 stub.
    pub fn probe(&self, peer_did: &str, timeout_ms: u16) -> HeartbeatProbeResult {
        if !is_structurally_valid_did(peer_did) {
            return HeartbeatProbeResult::Unreachable {
                reason: UnreachableReason::InvalidPeerDid,
            };
        }
        let _timeout_ms = timeout_ms;
        HeartbeatProbeResult::Timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // tv_phase14_substrate_1: Heartbeat default construction +
    // structural presence probe (R1.5 fix: strengthened assertion).
    #[test]
    fn tv_phase14_substrate_1_heartbeat_default_construction() {
        let hb = Heartbeat;
        // Default construction must succeed without panic.
        let dbg = format!("{:?}", hb);
        assert_eq!(dbg, "Heartbeat");
    }

    // tv_phase14_substrate_2: probe returns Timeout for structurally
    // well-formed peer_did (R1.5 fix: narrows the Phase 14 stub
    // contract — malformed DIDs now return InvalidPeerDid per
    // `is_structurally_valid_did`).
    #[test]
    fn tv_phase14_substrate_2_probe_returns_timeout() {
        let hb = Heartbeat;
        let result = hb.probe("did:octo:example-peer", 5000);
        assert_eq!(result, HeartbeatProbeResult::Timeout);
    }

    // tv_phase14_substrate_3: probe with timeout_ms=0 returns Timeout
    // for a structurally well-formed peer_did (R1.5 fix:
    // `timeout_ms` is plumbed through the stub API but is
    // deliberately NOT consumed by Phase 14 — preserved for
    // forward compatibility with Phase 4-style Layer D
    // adapter missions).
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

    // tv_phase14_substrate_6: probe returns Unreachable InvalidPeerDid
    // for structurally malformed peer_did (R1.5 fix: explicit
    // distinction between well-formed → Timeout and malformed →
    // Unreachable{InvalidPeerDid}, per substrate-faithfulness
    // contract).
    #[test]
    fn tv_phase14_substrate_6_probe_malformed_peer_did_returns_unreachable() {
        let hb = Heartbeat;
        // Missing `did:octo:` prefix.
        let r1 = hb.probe("not-a-did", 5000);
        assert_eq!(
            r1,
            HeartbeatProbeResult::Unreachable {
                reason: UnreachableReason::InvalidPeerDid
            }
        );
        // Empty DID.
        let r2 = hb.probe("", 5000);
        assert_eq!(
            r2,
            HeartbeatProbeResult::Unreachable {
                reason: UnreachableReason::InvalidPeerDid
            }
        );
        // Empty method-specific identifier.
        let r3 = hb.probe("did:octo:", 5000);
        assert_eq!(
            r3,
            HeartbeatProbeResult::Unreachable {
                reason: UnreachableReason::InvalidPeerDid
            }
        );
        // Wrong method.
        let r4 = hb.probe("did:key:abc", 5000);
        assert_eq!(
            r4,
            HeartbeatProbeResult::Unreachable {
                reason: UnreachableReason::InvalidPeerDid
            }
        );
    }

    // tv_phase14_substrate_7: UnreachableReason all variants
    // distinct + Equality (R1.5 fix: `#[non_exhaustive]`
    // requires a catch-all in user code; `Other(String)`
    // payload semantics validated via Debug round-trip;
    // Copy derive deliberately OMITTED because
    // `Other(String)` payload must remain heap-backed).
    #[test]
    fn tv_phase14_substrate_7_unreachable_reason_all_variants_eq() {
        let reasons = [
            UnreachableReason::InvalidPeerDid,
            UnreachableReason::NoTransportAdapter,
            UnreachableReason::AdapterRefused,
            UnreachableReason::Other("connection-reset".to_string()),
        ];
        let mut seen = std::collections::BTreeSet::new();
        for r in reasons.iter() {
            // Clone derive: use after move via explicit clone.
            let cloned = r.clone();
            assert_eq!(*r, cloned);
            // Insert via Debug string for canonical deterministic
            // representation (BTreeSet needs Ord; UnreachableReason
            // has PartialEq + Eq; canonicalize via Debug string
            // for set storage).
            seen.insert(format!("{r:?}"));
        }
        // 4 distinct reasons expected (3 unit variants + 1 Other
        // with "connection-reset" payload).
        assert_eq!(seen.len(), 4);
    }
}
