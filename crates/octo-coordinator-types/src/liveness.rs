//! Mission Coordinator liveness substrate (Layer B per CLAUDE.md §Architectural
//! Principles).
//!
//! Canonical home per RFC-0855p-b §Implementation Phase 3 (L819-826) for
//! heartbeat-based coordinator liveness detection.
//!
//! ## Substrate surface
//!
//! - [`CoordinatorHeartbeat`] — canonical 6-field envelope per RFC L819-826;
//!   signed by the active coordinator each `HEARTBEAT_INTERVAL` epochs.
//! - [`LivenessTracker`] — per-coordinator tracker keyed by
//!   `CoordinatorId` carrying `last_heartbeat_epoch + heartbeat_interval +
//!   grace_period`.
//! - [`evaluate_liveness`] — pure fn emitting one of three Phase-3
//!   transitions: `Active → Suspect` (2× interval miss), `Suspect → Active`
//!   (heartbeat recovered), `Suspect → Handover` (grace exceeded).
//!
//! Per RFC-0008 §Class A, `current_epoch` is monotonic-clock-pinned and
//! canonical-bytes derivation uses domain separator
//! `BLAKE3_REPUTATION_COORDINATOR_HEARTBEAT_DOMAIN`.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::state::{CoordinatorError, CoordinatorId, CoordinatorLifecycle, CoordinatorRecord};

// -----------------------------------------------------------------------------
// Constants (RFC §Implementation Phase 3 + §Appendix A Suspect → Active / Handover)
// -----------------------------------------------------------------------------

/// Default heartbeat interval (epochs) per RFC-0855p-b §Implementation
/// Phase 3. Active coordinators MUST emit a signed `CoordinatorHeartbeat`
/// envelope every `HEARTBEAT_INTERVAL_DEFAULT` epochs.
pub const HEARTBEAT_INTERVAL_DEFAULT: u64 = 100;

/// Grace-period multiplier per RFC-0855p-b §Appendix A: `Suspect → Handover`
/// triggers when `current_epoch - last_heartbeat_epoch >
/// GRACE_PERIOD = HEARTBEAT_GRACE_MULTIPLIER × heartbeat_interval`.
pub const HEARTBEAT_GRACE_MULTIPLIER: u64 = 3;

/// Active → Suspect threshold: `2 × heartbeat_interval` (per mission AC).
pub const ACTIVE_SUSPECT_MULTIPLIER: u64 = 2;

/// BLAKE3 domain separator for heartbeat canonical-bytes derivation
/// (RFC-0008 §Class A + RFC-0126 §3.2).
pub const BLAKE3_REPUTATION_COORDINATOR_HEARTBEAT_DOMAIN: &[u8] =
    b"cipherocto/coordinator/heartbeat/v1";

/// Maximum permitted `heartbeat_interval` per RFC-0855p-b L820.
pub const HEARTBEAT_INTERVAL_MAX: u64 = 1_000;

// -----------------------------------------------------------------------------
// CoordinatorHeartbeat (canonical wire form per RFC L819-826)
// -----------------------------------------------------------------------------

/// Heartbeat envelope signed by the active mission coordinator.
///
/// Per RFC-0855p-b §Implementation Phase 3 L819-826:
/// - `coordinator_peer_id`: CoordinatorId of the active sender.
/// - `coordinator_term_id`: `BLAKE3(peer_id || start || source)` from the
///   coordinator's [`CoordinatorRecord`].
/// - `current_epoch`: epoch at which the heartbeat was emitted.
/// - `heartbeat_interval`: configured interval; allows liveness checker to
///   verify sender's interval setting matches its own.
/// - `last_event_digest`: BLAKE3 of the canonical-bytes of the last signed
///   envelope (events chain-back).
/// - `signature`: 64-byte Ed25519 signature over
///   `BLAKE3(HEARTBEAT_DOMAIN || peer_id || term_id || current_epoch ||
///   heartbeat_interval || last_event_digest)`.
///
/// Derives `BorshSerialize + BorshDeserialize` only (no serde) because
/// `[u8; 64]` exceeds serde's const-generic array limit.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CoordinatorHeartbeat {
    /// Sender coordinator id.
    pub coordinator_peer_id: CoordinatorId,
    /// Sender coordinator term id.
    pub coordinator_term_id: [u8; 32],
    /// Epoch the heartbeat was emitted.
    pub current_epoch: u64,
    /// Sender's configured heartbeat interval (epochs).
    pub heartbeat_interval: u64,
    /// BLAKE3 of the last signed envelope's canonical bytes.
    pub last_event_digest: [u8; 32],
    /// 64-byte Ed25519 signature.
    pub signature: [u8; 64],
}

impl CoordinatorHeartbeat {
    /// Canonical-bytes derivation under
    /// [`BLAKE3_REPUTATION_COORDINATOR_HEARTBEAT_DOMAIN`].
    pub fn canonical_bytes(&self) -> [u8; 32] {
        let bytes = borsh::to_vec(self).expect("CoordinatorHeartbeat borsh serializes");
        let mut hasher = blake3::Hasher::new();
        hasher.update(BLAKE3_REPUTATION_COORDINATOR_HEARTBEAT_DOMAIN);
        hasher.update(&bytes);
        *hasher.finalize().as_bytes()
    }
}

// -----------------------------------------------------------------------------
// LivenessTracker (per-coordinator tracker)
// -----------------------------------------------------------------------------

/// Per-coordinator liveness tracker keyed by `CoordinatorId`.
///
/// Holds the last observed heartbeat epoch + sender's heartbeat_interval.
/// Evaluated by [`evaluate_liveness`] to drive
/// `Active → Suspect / Suspect → Active / Suspect → Handover` transitions.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct LivenessTracker {
    /// Coordinator being tracked.
    pub coordinator: CoordinatorId,
    /// Epoch of the last observed heartbeat.
    pub last_heartbeat_epoch: u64,
    /// Last observed heartbeat interval (epochs).
    pub heartbeat_interval: u64,
    /// Grace period (epochs) before `Suspect → Handover` triggers.
    pub grace_period: u64,
}

impl LivenessTracker {
    /// Construct a tracker initialized at `start_epoch`.
    pub const fn new(
        coordinator: CoordinatorId,
        start_epoch: u64,
        heartbeat_interval: u64,
    ) -> Self {
        Self {
            coordinator,
            last_heartbeat_epoch: start_epoch,
            heartbeat_interval,
            grace_period: HEARTBEAT_GRACE_MULTIPLIER * heartbeat_interval,
        }
    }

    /// Update on observed heartbeat.
    pub fn on_heartbeat(&mut self, current_epoch: u64) {
        self.last_heartbeat_epoch = current_epoch;
    }
}

// -----------------------------------------------------------------------------
// evaluate_liveness (Phase 3 transition selector)
// -----------------------------------------------------------------------------

/// Evaluate the coordinator's lifecycle transition given current epoch +
/// whether a heartbeat was observed this round.
///
/// Returns the [`CoordinatorLifecycle`] the substrate should transition
/// `record.state` TO. Phase 3 transitions:
/// - `Active` + heartbeat within `(0, 2 × heartbeat_interval]` →
///   `Active` (self-loop, no transition).
/// - `Active` + `(current_epoch - last_heartbeat_epoch > 2 ×
///   heartbeat_interval)` AND no heartbeat this round → `Suspect`.
/// - `Suspect` + heartbeat received this round → `Active` (recovery).
/// - `Suspect` + `(current_epoch - last_heartbeat_epoch > grace_period)` →
///   `Handover` (Phase 4 trigger).
/// - `Handover / Demoting / Resigned / Inactive` → unchanged (no Phase 3
///   trigger; downstream phases own transitions).
///
/// Returns the NEW state. Validation against [`CoordinatorLifecycle`]
/// transition table (`phase1::validate_transition`) MUST be applied at the
/// call-site before persisting.
pub fn evaluate_liveness(
    record: &CoordinatorRecord,
    current_epoch: u64,
    has_heartbeat: bool,
) -> Result<CoordinatorLifecycle, CoordinatorError> {
    let interval = record.heartbeat_interval.max(1);
    let last = record.last_heartbeat_epoch;
    let elapsed = current_epoch.saturating_sub(last);
    let grace = HEARTBEAT_GRACE_MULTIPLIER * interval;
    let suspect_threshold = ACTIVE_SUSPECT_MULTIPLIER * interval;

    match record.state {
        CoordinatorLifecycle::Active => {
            if has_heartbeat {
                Ok(CoordinatorLifecycle::Active)
            } else if elapsed > suspect_threshold {
                Ok(CoordinatorLifecycle::Suspect)
            } else {
                Ok(CoordinatorLifecycle::Active)
            }
        }
        CoordinatorLifecycle::Suspect => {
            if has_heartbeat {
                Ok(CoordinatorLifecycle::Active)
            } else if elapsed > grace {
                Ok(CoordinatorLifecycle::Handover)
            } else {
                Ok(CoordinatorLifecycle::Suspect)
            }
        }
        // Other phases — no transition from Phase 3 substrate.
        _ => Ok(record.state),
    }
}

// -----------------------------------------------------------------------------
// Inline unit tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CoordinatorSource, GenesisState};

    fn record(state: CoordinatorLifecycle, last_hb: u64, interval: u64) -> CoordinatorRecord {
        CoordinatorRecord {
            coordinator_peer_id: [0xAAu8; 32],
            state,
            term_start_epoch: 100,
            term_end_epoch: 200,
            source: CoordinatorSource::Election,
            coordinator_term_id: [0u8; 32],
            slash_count: 0,
            octo_o_stake_locked: 1_000_000,
            last_heartbeat_epoch: last_hb,
            heartbeat_interval: interval,
        }
    }

    #[test]
    fn t_liveness_heartbeat_default_constant_pinned() {
        assert_eq!(HEARTBEAT_INTERVAL_DEFAULT, 100);
        assert_eq!(HEARTBEAT_GRACE_MULTIPLIER, 3);
        assert_eq!(ACTIVE_SUSPECT_MULTIPLIER, 2);
        assert_eq!(HEARTBEAT_INTERVAL_MAX, 1_000);
    }

    #[test]
    fn t_liveness_heartbeat_canonical_bytes_deterministic() {
        let hb = CoordinatorHeartbeat {
            coordinator_peer_id: [0xAAu8; 32],
            coordinator_term_id: [0xBBu8; 32],
            current_epoch: 100,
            heartbeat_interval: HEARTBEAT_INTERVAL_DEFAULT,
            last_event_digest: [0xCCu8; 32],
            signature: [0u8; 64],
        };
        let a = hb.canonical_bytes();
        let b = hb.canonical_bytes();
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn t_liveness_heartbeat_canonical_bytes_changes_with_epoch() {
        let mut a = CoordinatorHeartbeat {
            coordinator_peer_id: [0xAAu8; 32],
            coordinator_term_id: [0u8; 32],
            current_epoch: 100,
            heartbeat_interval: HEARTBEAT_INTERVAL_DEFAULT,
            last_event_digest: [0u8; 32],
            signature: [0u8; 64],
        };
        let mut b = a.clone();
        a.current_epoch = 100;
        b.current_epoch = 101;
        assert_ne!(a.canonical_bytes(), b.canonical_bytes());
    }

    #[test]
    fn t_liveness_tracker_init_and_update() {
        let mut tracker = LivenessTracker::new([0xAAu8; 32], 100, 50);
        assert_eq!(tracker.last_heartbeat_epoch, 100);
        assert_eq!(tracker.heartbeat_interval, 50);
        assert_eq!(tracker.grace_period, 150);
        tracker.on_heartbeat(150);
        assert_eq!(tracker.last_heartbeat_epoch, 150);
    }

    #[test]
    fn t_liveness_active_within_interval_stays_active() {
        let r = record(CoordinatorLifecycle::Active, 100, 50);
        let next = evaluate_liveness(&r, 140, false).unwrap();
        assert_eq!(next, CoordinatorLifecycle::Active);
    }

    #[test]
    fn t_liveness_active_two_x_miss_goes_suspect() {
        // 2x interval = 100; last_hb=100; current_epoch=201 → elapsed=101 > 100.
        let r = record(CoordinatorLifecycle::Active, 100, 50);
        let next = evaluate_liveness(&r, 201, false).unwrap();
        assert_eq!(next, CoordinatorLifecycle::Suspect);
    }

    #[test]
    fn t_liveness_active_heartbeat_recovers_to_active() {
        let r = record(CoordinatorLifecycle::Suspect, 100, 50);
        let next = evaluate_liveness(&r, 150, true).unwrap();
        assert_eq!(next, CoordinatorLifecycle::Active);
    }

    #[test]
    fn t_liveness_suspect_grace_exceeded_goes_handover() {
        // grace = 3*50 = 150; last_hb=100; current_epoch=251 → elapsed=151 > 150.
        let r = record(CoordinatorLifecycle::Suspect, 100, 50);
        let next = evaluate_liveness(&r, 251, false).unwrap();
        assert_eq!(next, CoordinatorLifecycle::Handover);
    }

    #[test]
    fn t_liveness_suspect_within_grace_stays_suspect() {
        let r = record(CoordinatorLifecycle::Suspect, 100, 50);
        let next = evaluate_liveness(&r, 200, false).unwrap();
        assert_eq!(next, CoordinatorLifecycle::Suspect);
    }

    #[test]
    fn t_liveness_demoting_state_unchanged() {
        // Demoting is owned by Phase 5 substrate; Phase 3 doesn't transition.
        let r = record(CoordinatorLifecycle::Demoting, 100, 50);
        let next = evaluate_liveness(&r, 999, false).unwrap();
        assert_eq!(next, CoordinatorLifecycle::Demoting);
    }

    #[test]
    fn t_liveness_genesis_state_link() {
        // Sanity check that GenesisState v1.1 values exist.
        assert_eq!(GenesisState::GenesisDesignated.discriminant(), 0x00);
    }
}
