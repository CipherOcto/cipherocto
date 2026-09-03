//! Canonical test vectors for §Phase 3 liveness substrate
//! (RFC-0855p-b §Implementation Phase 3 L819-826).

use octo_coordinator_types::liveness::{
    evaluate_liveness, CoordinatorHeartbeat, LivenessTracker, HEARTBEAT_GRACE_MULTIPLIER,
    HEARTBEAT_INTERVAL_DEFAULT,
};
use octo_coordinator_types::state::{CoordinatorLifecycle, CoordinatorRecord, CoordinatorSource};

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

// TV-LV-1: Active + heartbeat within interval → Active (self-loop).
#[test]
fn tv_lv_1_active_within_interval_stays_active() {
    let r = record(CoordinatorLifecycle::Active, 100, 50);
    let next = evaluate_liveness(&r, 140, false).unwrap();
    assert_eq!(next, CoordinatorLifecycle::Active);
}

// TV-LV-2: Active + 2× interval miss → Suspect.
#[test]
fn tv_lv_2_active_two_x_miss_goes_suspect() {
    let r = record(CoordinatorLifecycle::Active, 100, 50);
    let next = evaluate_liveness(&r, 201, false).unwrap();
    assert_eq!(next, CoordinatorLifecycle::Suspect);
}

// TV-LV-3: Suspect + grace exceeded → Handover.
#[test]
fn tv_lv_3_suspect_grace_exceeded_goes_handover() {
    let r = record(CoordinatorLifecycle::Suspect, 100, 50);
    let next = evaluate_liveness(&r, 251, false).unwrap();
    assert_eq!(next, CoordinatorLifecycle::Handover);
}

#[test]
fn tv_lv_aux_heartbeat_constants_pinned() {
    assert_eq!(HEARTBEAT_INTERVAL_DEFAULT, 100);
    assert_eq!(HEARTBEAT_GRACE_MULTIPLIER, 3);
}

#[test]
fn tv_lv_aux_heartbeat_canonical_bytes_deterministic() {
    let hb = CoordinatorHeartbeat {
        coordinator_peer_id: [0xAAu8; 32],
        coordinator_term_id: [0x33u8; 32],
        current_epoch: 100,
        heartbeat_interval: HEARTBEAT_INTERVAL_DEFAULT,
        last_event_digest: [0u8; 32],
        signature: [0u8; 64],
    };
    assert_eq!(hb.canonical_bytes(), hb.canonical_bytes());
}

#[test]
fn tv_lv_aux_tracker_init() {
    let t = LivenessTracker::new([0xAAu8; 32], 100, 50);
    assert_eq!(t.last_heartbeat_epoch, 100);
    assert_eq!(t.grace_period, 150);
}
