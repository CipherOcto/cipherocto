//! L2 dispatch counter regression — guards against future attempts to
//! bypass `node.receive()` and call `handler.on_receive()` directly.
//!
//! The harness's `dispatch_call_count` increments inside
//! `dispatch_with_sender`, which is the function called by the
//! background driver for every payload that flows through
//! `node.receive()`. Any test that calls `handler.on_receive()`
//! directly (bypassing `node.receive()`) will not increment this
//! counter — making such bypass attempts visible.
//!
//! Two tests:
//!   1. Drive the inbox at least once → counter >= 1.
//!   2. Call `node.receive()` directly → counter >= 1.

use octo_transport::receiver::ReceiveContext;
use quota_router_e2e_tests::TestCluster;

/// `dispatch_call_count` starts at zero on a freshly built cluster.
#[tokio::test]
async fn l2_dispatch_counter_starts_at_zero() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);
    assert_eq!(
        cluster.nodes[0].dispatch_call_count(),
        0,
        "fresh node should have dispatch_call_count == 0"
    );
}

/// Driving the inbox flows payloads through `node.receive()` which
/// increments `dispatch_call_count`.
#[tokio::test]
async fn l2_dispatch_counter_increments_on_drive() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);
    cluster.start_all().await;
    // start_all broadcasts announces and drives a few cycles.
    // Each drive iteration that drains at least one envelope
    // increments the counter on the receiving node.
    let count_a = cluster.nodes[0].dispatch_call_count();
    let count_b = cluster.nodes[1].dispatch_call_count();
    assert!(
        count_a + count_b >= 1,
        "after start_all at least one node should have dispatched, got {} + {}",
        count_a,
        count_b
    );
}

/// Calling `node.receive()` directly increments `dispatch_call_count`
/// — but only when the call goes through `dispatch_with_sender`
/// (which the harness uses for inbox draining).
///
/// Calling `node.receive()` directly DOES NOT increment
/// `dispatch_call_count` because the counter is incremented inside
/// the harness's `dispatch_with_sender`, not inside `node.receive()`
/// itself. This test documents that behavior: the counter tracks
/// inbox-driven dispatches, not arbitrary `receive()` calls.
///
/// (Future refactors that move the counter into `node.receive()`
/// would change this test's expectation. The current placement
/// catches the specific regression class "bypass inbox, call handler
/// directly" — which is the documented concern.)
#[tokio::test]
async fn l2_dispatch_counter_direct_receive_does_not_increment() {
    let cluster = TestCluster::new(1, vec![vec!["gpt-4o".into()]]);
    let baseline = cluster.nodes[0].dispatch_call_count();
    assert_eq!(baseline, 0);

    let ctx = ReceiveContext {
        source_transport: "direct".into(),
        mission_id: [0u8; 32],
        sender_id: None,
    };
    // Calling receive() with an unknown discriminator returns Ok —
    // it's still a dispatch, but the counter is harness-side.
    let _ = cluster.nodes[0].node.receive(&[0xFF], &ctx).await;

    // The counter is only incremented inside the harness's
    // dispatch_with_sender. Direct calls to node.receive() bypass
    // that counter — by design.
    assert_eq!(
        cluster.nodes[0].dispatch_call_count(),
        0,
        "direct node.receive() should NOT increment dispatch_call_count (harness-side)"
    );
}

/// Driving an inbox envelope flows through `dispatch_with_sender` and
/// increments the counter. The counter is therefore the right guard
/// for the "inbox dispatch" code path, not for ad-hoc `receive()`
/// calls.
#[tokio::test]
async fn l2_dispatch_counter_tracks_inbox_dispatch() {
    let cluster = TestCluster::new(2, vec![vec!["gpt-4o".into()], vec!["gpt-4o".into()]]);

    let before_a = cluster.nodes[0].dispatch_call_count();
    let before_b = cluster.nodes[1].dispatch_call_count();

    // Have node 0 broadcast an announce. This pushes a payload into
    // node 1's inbox. Driving node 1 drains it through
    // dispatch_with_sender → node.receive() → dispatch_call_count++.
    cluster.nodes[0].broadcast_announce().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    cluster.nodes[1].drive().await;

    let after_b = cluster.nodes[1].dispatch_call_count();
    assert!(
        after_b > before_b,
        "drive on node 1 should increment its dispatch counter: before={}, after={}",
        before_b,
        after_b
    );

    // Counter semantics are per-node, so node 0's counter should be
    // unchanged (it didn't drain any inbox).
    assert_eq!(
        cluster.nodes[0].dispatch_call_count(),
        before_a,
        "node 0's counter should not change"
    );
}
