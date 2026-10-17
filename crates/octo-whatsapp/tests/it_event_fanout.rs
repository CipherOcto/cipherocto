//! End-to-end test for the event router's per-sink mpsc fan-out.
//!
//! Verifies that:
//! 1. Two sinks both receive every event in order.
//! 2. A slow sink's `lagged` counter increments when its bounded
//!    mpsc fills up.
//! 3. After a sink's `EventsSubscriber` is dropped (closed), the
//!    router's fan-out skips it (no panic; no extra lag).
//!
//! Hermetic — no live WhatsApp session required.

use std::time::Duration;

use octo_whatsapp::events::InboundEvent;
use octo_whatsapp::events_buffer::EventsBuffer;
use octo_whatsapp::events_router::EventsRouter;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

fn dummy_msg(id: &str) -> InboundEvent {
    InboundEvent::Message {
        id: id.to_string(),
        peer: "P".to_string(),
        sender: "S".to_string(),
        ts_unix_ms: 0,
        ts_mono_ns: 0,
        kind: octo_whatsapp::events::MessageKind::Text,
        text: "hi".to_string(),
        media_token: None,
        reply_to: None,
        mentions: Vec::new(),
        mentions_truncated: false,
        from_me: false,
        is_group: false,
        view_once: false,
        ephemeral_expires_at_seconds: None,
    }
}

#[tokio::test]
async fn fanout_delivers_every_event_to_every_sink() {
    let buffer = EventsBuffer::new(100);
    let cancel = CancellationToken::new();
    let router = EventsRouter::new(buffer.clone(), cancel.clone());

    let mut sub_a = router.subscribe(8);
    let mut sub_b = router.subscribe(8);

    let (tx, _rx) = broadcast::channel::<std::sync::Arc<octo_whatsapp::events::InboundEvent>>(16);
    let rx = tx.subscribe();
    let router2 = router.clone();
    let handle = tokio::spawn(async move { router2.run(rx).await });

    for i in 0..5 {
        tx.send(std::sync::Arc::new(dummy_msg(&format!("M{i}"))))
            .unwrap();
    }

    // Drain each sink fully.
    let mut a_ids: Vec<String> = Vec::new();
    let mut b_ids: Vec<String> = Vec::new();
    for _ in 0..5 {
        let (_id, ev) = sub_a.recv().await.unwrap();
        match ev {
            InboundEvent::Message { id, .. } => a_ids.push(id),
            _ => panic!("expected Message"),
        }
        let (_id, ev) = sub_b.recv().await.unwrap();
        match ev {
            InboundEvent::Message { id, .. } => b_ids.push(id),
            _ => panic!("expected Message"),
        }
    }

    assert_eq!(a_ids, vec!["M0", "M1", "M2", "M3", "M4"]);
    assert_eq!(b_ids, vec!["M0", "M1", "M2", "M3", "M4"]);
    assert_eq!(router.sink_count(), 2);
    assert_eq!(router.total_lagged(), 0);

    cancel.cancel();
    drop(sub_a);
    drop(sub_b);
    let _ = handle.await;
}

#[tokio::test]
async fn slow_sink_lagged_counter_increments_under_pressure() {
    let buffer = EventsBuffer::new(100);
    let cancel = CancellationToken::new();
    let router = EventsRouter::new(buffer.clone(), cancel.clone());

    // Capacity 1 — only one event can queue before TrySendError::Full.
    let _slow = router.subscribe(1);
    // Capacity 64 — keeps up easily.
    let _fast = router.subscribe(64);

    let (tx, _rx) = broadcast::channel::<std::sync::Arc<octo_whatsapp::events::InboundEvent>>(64);
    let rx = tx.subscribe();
    let router2 = router.clone();
    let handle = tokio::spawn(async move { router2.run(rx).await });

    // Push 5 events. The slow sink will fill up after the first and
    // drop the rest (lagged counter increments).
    for i in 0..5 {
        tx.send(std::sync::Arc::new(dummy_msg(&format!("E{i}"))))
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    let total_lagged = router.total_lagged();
    assert!(
        total_lagged >= 1,
        "slow sink should have lagged >= 1 event, got {total_lagged}"
    );

    cancel.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn dropped_sink_does_not_panic_router() {
    let buffer = EventsBuffer::new(100);
    let cancel = CancellationToken::new();
    let router = EventsRouter::new(buffer.clone(), cancel.clone());

    let sub = router.subscribe(8);
    // Drop the consumer immediately so the sink is closed.
    drop(sub);

    let (tx, _rx) = broadcast::channel::<std::sync::Arc<octo_whatsapp::events::InboundEvent>>(16);
    let rx = tx.subscribe();
    let router2 = router.clone();
    let handle = tokio::spawn(async move { router2.run(rx).await });

    tx.send(std::sync::Arc::new(dummy_msg("M1"))).unwrap();
    tx.send(std::sync::Arc::new(dummy_msg("M2"))).unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Buffer still has both events; router did not panic.
    assert_eq!(buffer.len(), 2);
    // Sink's lagged counter increments on Closed.
    let total_lagged = router.total_lagged();
    assert!(
        total_lagged >= 2,
        "closed sink should have lagged >= 2 events, got {total_lagged}"
    );

    cancel.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn subscriber_try_recv_returns_none_when_empty() {
    // Smoke test for the non-blocking try_recv API — useful for MCP
    // clients that poll instead of awaiting.
    let buffer = EventsBuffer::new(100);
    let cancel = CancellationToken::new();
    let router = EventsRouter::new(buffer.clone(), cancel.clone());

    let mut sub = router.subscribe(8);
    assert!(sub.try_recv().is_none(), "empty channel must return None");

    let (tx, _rx) = broadcast::channel::<std::sync::Arc<octo_whatsapp::events::InboundEvent>>(16);
    let rx = tx.subscribe();
    let router2 = router.clone();
    let handle = tokio::spawn(async move { router2.run(rx).await });

    tx.send(std::sync::Arc::new(dummy_msg("M1"))).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (_id, ev) = sub.try_recv().expect("event should be available");
    match ev {
        InboundEvent::Message { id, .. } => assert_eq!(id, "M1"),
        _ => panic!("expected Message"),
    }
    assert!(sub.try_recv().is_none(), "drained channel returns None");

    cancel.cancel();
    let _ = handle.await;
}
