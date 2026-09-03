//! Canonical test vectors for §Phase 4 handover substrate
//! (RFC-0855p-b §Implementation Phase 4 + RFC-0855p-e).

use octo_coordinator_types::HandoverReasonTypeId;
use octo_network::dot::handover::{
    resolve_emergency_handover, resolve_forced_handover, resolve_voluntary_handover,
    EmergencyHandover, ForcedHandoverTrigger, HandoverOutcome, MessagePreservationQueue,
    VoluntaryHandoverRequest,
};

// TV-HO-1: Voluntary Active → Handover → Inactive path.
#[test]
fn tv_ho_1_voluntary_handover_pinned() {
    let req = VoluntaryHandoverRequest {
        predecessor: [0xAAu8; 32],
        successor: [0xBBu8; 32],
        mission_id: [0x01u8; 32],
        handover_epoch: 200,
        reason: HandoverReasonTypeId::SUBDC_VOLUNTARY_RESIGNATION,
    };
    let pending = vec![(1u64, [0xC1u8; 32]), (2u64, [0xC2u8; 32])];
    let outcome = resolve_voluntary_handover(&req, pending);
    match outcome {
        HandoverOutcome::Initiated {
            predecessor,
            successor,
            queue,
            ..
        } => {
            assert_eq!(predecessor, [0xAAu8; 32]);
            assert_eq!(successor, [0xBBu8; 32]);
            assert_eq!(queue.len(), 2);
        }
        _ => panic!("expected Initiated"),
    }
}

// TV-HO-2: Forced Suspect → Handover (Phase 3 trigger).
#[test]
fn tv_ho_2_forced_handover_pinned() {
    let trigger = ForcedHandoverTrigger {
        predecessor: [0xAAu8; 32],
        successor: [0xBBu8; 32],
        mission_id: [0x01u8; 32],
        trigger_epoch: 200,
        reason: HandoverReasonTypeId::SUBDC_MISCONDUCT,
    };
    let outcome = resolve_forced_handover(&trigger);
    assert!(matches!(outcome, HandoverOutcome::Triggered { .. }));
}

// TV-HO-3: Emergency governance override.
#[test]
fn tv_ho_3_emergency_handover_pinned() {
    let emergency = EmergencyHandover {
        predecessor: [0xAAu8; 32],
        successor: [0xCCu8; 32],
        mission_id: [0x01u8; 32],
        override_epoch: 200,
        governance_id: [0x99u8; 32],
        reason: HandoverReasonTypeId::TERM_EXPIRED,
    };
    let outcome = resolve_emergency_handover(&emergency);
    assert!(matches!(outcome, HandoverOutcome::Overridden { .. }));
}

#[test]
fn tv_ho_aux_preservation_queue_flush() {
    let mut q = MessagePreservationQueue::new([0xAAu8; 32], [0xBBu8; 32]);
    q.enqueue(1, [0xC1u8; 32]);
    q.enqueue(2, [0xC2u8; 32]);
    assert_eq!(q.len(), 2);
    let drained = q.flush();
    assert_eq!(drained.len(), 2);
    assert!(q.is_empty());
}
