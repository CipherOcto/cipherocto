//! Gossip envelope contract for reputation federation (RFC-0968 §12 +
//! amendments 22, 28, 29).
//!
//! ## Authority model
//!
//! The `recorder_signature` is the ONLY authoritative source of a
//! `SignalEvent`. Coordinator and attestor signatures are non-authoritative
//! transport metadata. A peer MUST reject any envelope where:
//!
//! - `recorder_signature` fails verification against
//!   `BLAKE3(BLAKE3_REPUTATION_EVENT_DOMAIN || canonical_ser(event_unsigned))`.
//! - The `recorder_did` does not equal `blake3(pubkey).hash_part` (stale
//!   pubkey mapping per amendment 29; `GossipEnvelopeInvalid`).
//! - The envelope carries a `rotation_receipt` referencing a `new_did`
//!   not bound to a `RotationProvenance` on the embedded event
//!   (`RotationProvenanceMissing`, `0x39`).
//!
//! ## Topic naming
//!
//! Gossip topics are DID-keyed, NOT pubkey-keyed:
//!
//! ```text
//! /dot/reputation/{recorder_did_hex}
//! ```
//!
//! Legacy pubkey-keyed topics (RFC-0855p-b pre-amendment 29) are removed
//! and any ingress bearing a pubkey mapping is rejected with
//! `GossipEnvelopeInvalid("stale_pubkey_mapping")`.

use serde::{Deserialize, Serialize};

use crate::auth::Attestation;
use crate::constants::BLAKE3_REPUTATION_EVENT_DOMAIN;
use crate::error::ReputationError;
use crate::types::{EventId, RecorderDid, RotationProvenance, SignalEvent};

/// Gossip envelope carrying a `SignalEvent` plus cross-mission transport
/// metadata. The transport is dumb: dedup is enforced at the store layer
/// on `event_id` PK; gossipsub does NOT enforce envelope-uniqueness at
/// the transport layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GossipEnvelope {
    /// Inner event. The authoritative source for the signal.
    pub event: SignalEvent,
    /// ed25519 signature from the recorder (authoritative). NOT a
    /// coordinator/attestor signature — those are non-authoritative.
    pub recorder_signature: Vec<u8>,
    /// Source mission identifier (e.g., `mon:whatsapp:phase-1`).
    pub source_mission: String,
    /// Source domain within the mission (e.g., `domain:adapter:whatsapp`).
    pub source_domain: String,
    /// Optional rotation lineage map (amendment 29). When `Some`, the
    /// event was authored AFTER a rotation; `old_did` is the tombstoned
    /// predecessor and `new_did` is the canonical successor.
    pub rotation_provenance: Option<RotationProvenance>,
    /// Attestations piggy-backed on the envelope (transport metadata).
    /// Carrying attestations inline avoids a second round-trip for
    /// quorum formation.
    pub attestations: Vec<Attestation>,
}

impl GossipEnvelope {
    /// Validate envelope shape (no signature verification — that is the
    /// signer's job, called separately). Returns `GossipEnvelopeInvalid`
    /// on any structural defect.
    pub fn validate_shape(&self) -> Result<(), ReputationError> {
        if self.recorder_signature.is_empty() {
            return Err(ReputationError::GossipEnvelopeInvalid(
                "recorder_signature_empty",
            ));
        }
        if self.source_mission.is_empty() {
            return Err(ReputationError::GossipEnvelopeInvalid(
                "source_mission_empty",
            ));
        }
        if self.source_domain.is_empty() {
            return Err(ReputationError::GossipEnvelopeInvalid(
                "source_domain_empty",
            ));
        }
        if let Some(rp) = &self.rotation_provenance {
            if rp.new_did == self.event.recorder_did {
                return Err(ReputationError::GossipEnvelopeInvalid(
                    "rotation_provenance_must_precede_event",
                ));
            }
        }
        Ok(())
    }

    /// Return the event id (shortcut for `self.event.event_id`).
    pub fn event_id(&self) -> EventId {
        self.event.event_id
    }

    /// Return the recorder DID (shortcut).
    pub fn recorder_did(&self) -> RecorderDid {
        self.event.recorder_did
    }
}

/// Catch-up request — an attestor that joined late asks the mesh for
/// envelopes newer than `since_event_id` for the given `attestor_did`'s
/// subscription set. Per amendment 22.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GossipCatchUp {
    /// The attestor asking for catch-up.
    pub attestor_did: AttestorId,
    /// Event id below which the catch-up asker has already seen.
    pub since_event_id: EventId,
}

// Re-export here so the gossip module owns the canonical attestor-did
// type for envelope fields. The canonical `AttestorId` lives in
// `crate::auth`; this alias keeps the gossip module self-contained.
pub use crate::auth::AttestorId;

/// Compute the libp2p gossipsub topic for a recorder DID.
///
/// Format: `/dot/reputation/{hex(recorder_did)}`. The topic is
/// DID-keyed per amendment 29; pubkey-keyed topics are removed.
pub fn topic_for_recorder(did: &RecorderDid) -> String {
    format!("/dot/reputation/{}", hex::encode(did.as_bytes()))
}

/// Compute the libp2p gossipsub topic for a DomainCoordinator
/// reputation stream (mission 0855p-c). Format:
/// `/dot/reputation/dc/{hex(dc_did)}`. The DC DID is the canonical
/// DC's recorder DID (or stable lineage identifier) — NOT a
/// `dc_pubkey`, per RFC-0968-A1 amendment 29. Cross-domain slash
/// events ride this topic; the gossip substrate's record_signal +
/// record_attestation path persists them with `ReputationLayer::Coordinator`.
pub fn topic_for_dc_recorder(did: &RecorderDid) -> String {
    format!("/dot/reputation/dc/{}", hex::encode(did.as_bytes()))
}

/// Compute the gossipsub message id for an envelope. The transport
/// uses this for **deduplication only** — store-level idempotency on
/// `event_id` PK is the authoritative dedup path.
pub fn message_id_for_envelope(env: &GossipEnvelope) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(BLAKE3_REPUTATION_EVENT_DOMAIN);
    h.update(&env.event.canonical_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    arr
}

/// Default attestor rate limit: `DEFAULT_ATTESTOR_RATE_LIMIT` attestations
/// per `ATTESTOR_RATE_WINDOW_SECS` rolling window per attestor DID. Per
/// RFC-0968 §12 + mission 0968 Phase 4.
pub const DEFAULT_ATTESTOR_RATE_LIMIT: u32 = 10;
pub const ATTESTOR_RATE_WINDOW_SECS: u64 = 1;

/// Outcome of a rate-limit check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// Attestor under the limit — allow the attestation.
    Allow,
    /// Attestor over the limit for the current window — drop.
    Reject,
}

/// Sliding-window rate limiter, keyed by `AttestorId`. Records the
/// timestamp of every accepted attestation; on each `check(attestor,
/// now_unix)` call, evicts entries older than the window before
/// comparing the count to the cap.
///
/// Used by the gossip substrate (`octo-network::gossip::reputation`)
/// to drop over-budget attestations before they reach the store. The
/// store-level idempotency guard on `(attestor_did, event_id)` still
/// runs for accepted attestations; this is purely a write-side
/// defence against a flood.
#[derive(Debug)]
pub struct RateLimitedAttestor {
    cap: u32,
    window_secs: u64,
    inner: std::sync::Mutex<std::collections::HashMap<AttestorId, Vec<u64>>>,
}

impl RateLimitedAttestor {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_ATTESTOR_RATE_LIMIT, ATTESTOR_RATE_WINDOW_SECS)
    }

    pub fn with_capacity(cap: u32, window_secs: u64) -> Self {
        Self {
            cap,
            window_secs,
            inner: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Decide whether to accept a new attestation from `attestor` at
    /// `now_unix`. If accepted, the timestamp is recorded; if rejected,
    /// no state change. The caller must `.await` the returned decision
    /// (this method is sync; it is cheap — `O(n)` over the window).
    pub fn check(&self, attestor: &AttestorId, now_unix: u64) -> RateLimitDecision {
        let mut g = self.inner.lock().expect("rate_limit mutex poisoned");
        let entries = g.entry(*attestor).or_default();
        // Evict entries older than the window.
        let cutoff = now_unix.saturating_sub(self.window_secs);
        entries.retain(|t| *t > cutoff);
        if entries.len() as u32 >= self.cap {
            return RateLimitDecision::Reject;
        }
        entries.push(now_unix);
        RateLimitDecision::Allow
    }

    /// Number of attestors currently tracked (for ops diagnostics).
    pub fn tracked_attestors(&self) -> usize {
        let g = self.inner.lock().expect("rate_limit mutex poisoned");
        g.len()
    }
}

impl Default for RateLimitedAttestor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ControllerId, EventId, ReputationLayer, SignalEvent, SignalKind};
    use octo_determin::Dfp;

    fn dummy_event(seed: u64, did: RecorderDid) -> SignalEvent {
        SignalEvent {
            event_id: EventId::from_u64(seed),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        }
    }

    fn dummy_envelope(seed: u64, did: RecorderDid) -> GossipEnvelope {
        GossipEnvelope {
            event: dummy_event(seed, did),
            recorder_signature: vec![1u8; 64],
            source_mission: "mon:test".into(),
            source_domain: "domain:adapter:test".into(),
            rotation_provenance: None,
            attestations: vec![],
        }
    }

    #[test]
    fn topic_format_matches_amendment_29() {
        let did = RecorderDid::from_array([0xAB; 52]);
        let topic = topic_for_recorder(&did);
        assert_eq!(topic.len(), "/dot/reputation/".len() + 52 * 2);
        assert!(topic.starts_with("/dot/reputation/"));
        // The hex form must be 104 chars (52 bytes).
        assert_eq!(
            topic.len(),
            "/dot/reputation/".len() + 52 * 2,
            "topic must encode the 52-byte DID as 104 hex chars"
        );
        assert!(topic.ends_with(&"ab".repeat(52) as &str));
    }

    #[test]
    fn validate_shape_rejects_empty_signature() {
        let did = RecorderDid::from_array([0u8; 52]);
        let mut env = dummy_envelope(1, did);
        env.recorder_signature.clear();
        let err = env.validate_shape().unwrap_err();
        assert_eq!(err.discriminant(), 0x3A);
    }

    #[test]
    fn validate_shape_rejects_empty_source_mission() {
        let did = RecorderDid::from_array([0u8; 52]);
        let mut env = dummy_envelope(1, did);
        env.source_mission.clear();
        let err = env.validate_shape().unwrap_err();
        assert_eq!(err.discriminant(), 0x3A);
    }

    #[test]
    fn validate_shape_rejects_empty_source_domain() {
        let did = RecorderDid::from_array([0u8; 52]);
        let mut env = dummy_envelope(1, did);
        env.source_domain.clear();
        let err = env.validate_shape().unwrap_err();
        assert_eq!(err.discriminant(), 0x3A);
    }

    #[test]
    fn validate_shape_rejects_rotation_provenance_matching_event_did() {
        // amendment 29: the rotation_provenance's new_did must be a
        // PREDECESSOR of the event's recorder_did (i.e., the event is
        // authored AFTER rotation). If they match, the rotation
        // didn't actually rotate anything.
        let old_did = RecorderDid::from_array([0u8; 52]);
        let mut env = dummy_envelope(1, old_did);
        env.rotation_provenance = Some(RotationProvenance {
            new_did: old_did,
            consumed_at_unix: 1_000,
            rotation_id: 1,
        });
        let err = env.validate_shape().unwrap_err();
        assert_eq!(err.discriminant(), 0x3A);
    }

    #[test]
    fn validate_shape_accepts_well_formed_envelope() {
        let did = RecorderDid::from_array([0u8; 52]);
        let env = dummy_envelope(1, did);
        assert!(env.validate_shape().is_ok());
    }

    #[test]
    fn message_id_is_deterministic_per_event() {
        let did = RecorderDid::from_array([0u8; 52]);
        let a = dummy_envelope(7, did);
        let b = dummy_envelope(7, did);
        assert_eq!(message_id_for_envelope(&a), message_id_for_envelope(&b));
    }

    #[test]
    fn message_id_differs_for_different_events() {
        let did = RecorderDid::from_array([0u8; 52]);
        let a = dummy_envelope(7, did);
        let b = dummy_envelope(8, did);
        assert_ne!(message_id_for_envelope(&a), message_id_for_envelope(&b));
    }

    #[test]
    fn event_id_and_recorder_did_shortcuts() {
        let did = RecorderDid::from_array([0xCDu8; 52]);
        let env = dummy_envelope(42, did);
        assert_eq!(env.event_id(), EventId::from_u64(42));
        assert_eq!(env.recorder_did(), did);
    }

    // -- Session 8 / mission 0968 Phase 4: RateLimitedAttestor --

    #[test]
    fn rate_limit_allows_under_cap() {
        let rl = RateLimitedAttestor::with_capacity(3, 60);
        let a = AttestorId::from_array([0xAA; 52]);
        // First three within the window: allow.
        assert_eq!(rl.check(&a, 1_000), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_010), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_020), RateLimitDecision::Allow);
        // Fourth within the same window: reject.
        assert_eq!(rl.check(&a, 1_030), RateLimitDecision::Reject);
    }

    #[test]
    fn rate_limit_window_evicts_old_entries() {
        let rl = RateLimitedAttestor::with_capacity(2, 60);
        let a = AttestorId::from_array([0xBB; 52]);
        assert_eq!(rl.check(&a, 1_000), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_010), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_011), RateLimitDecision::Reject);
        // Move past the window (60s). The first two entries evict,
        // the third (1_011) was rejected so not stored.
        assert_eq!(rl.check(&a, 1_080), RateLimitDecision::Allow);
    }

    #[test]
    fn rate_limit_per_attestor_isolation() {
        let rl = RateLimitedAttestor::with_capacity(1, 60);
        let a = AttestorId::from_array([0x01; 52]);
        let b = AttestorId::from_array([0x02; 52]);
        assert_eq!(rl.check(&a, 1_000), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_001), RateLimitDecision::Reject);
        // `b` is unaffected.
        assert_eq!(rl.check(&b, 1_001), RateLimitDecision::Allow);
    }

    #[test]
    fn rate_limit_tracked_attestors_count() {
        let rl = RateLimitedAttestor::new();
        let a = AttestorId::from_array([0x01; 52]);
        let b = AttestorId::from_array([0x02; 52]);
        rl.check(&a, 1_000);
        rl.check(&b, 1_000);
        assert_eq!(rl.tracked_attestors(), 2);
    }

    #[test]
    fn rate_limit_default_constants() {
        // The plan calls for N=10/sec/attestor as the default; pin it
        // here so future changes are deliberate.
        assert_eq!(DEFAULT_ATTESTOR_RATE_LIMIT, 10);
        assert_eq!(ATTESTOR_RATE_WINDOW_SECS, 1);
    }

    // -- Session 8 / mission 0855p-c: DC reputation topic --

    #[test]
    fn topic_for_dc_recorder_format() {
        let did = RecorderDid::from_array([0xAB; 52]);
        let topic = topic_for_dc_recorder(&did);
        assert!(topic.starts_with("/dot/reputation/dc/"));
        assert_eq!(topic.len(), "/dot/reputation/dc/".len() + 52 * 2);
        assert!(topic.ends_with(&"ab".repeat(52) as &str));
        // Distinct from the recorder-keyed topic.
        assert_ne!(topic, topic_for_recorder(&did));
    }
}
