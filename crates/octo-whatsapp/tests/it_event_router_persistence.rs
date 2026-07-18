//! End-to-end test for the event router's persistence path.
//!
//! Spawns an `EventsRouter` and feeds events through a fresh
//! `tokio::sync::broadcast::Sender<String>` (the same shape the
//! adapter's `subscribe_raw_events` produces). Verifies that:
//! 1. Events land in the `EventsBuffer` with correct order + ids.
//! 2. Eviction kicks in at `max_rows` and the dropped count
//!    accumulates.
//!
//! Hermetic — no live WhatsApp session required.

use std::time::Duration;

use octo_whatsapp::events::{EventEnvelope, InboundEvent};
use octo_whatsapp::events_buffer::EventsBuffer;
use octo_whatsapp::events_router::EventsRouter;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

fn dummy_msg(id: &str) -> String {
    format!(
        "Message(id: \"{id}\", peer: \"P\", sender: \"S\", text: \"hi\", kind: Text, is_group: false)"
    )
}

#[tokio::test]
async fn router_persists_events_in_order_with_sequential_ids() {
    let buffer = EventsBuffer::new(100);
    let cancel = CancellationToken::new();
    let router = EventsRouter::new(buffer.clone(), cancel.clone());

    let (tx, _rx) = broadcast::channel::<String>(16);
    let rx = tx.subscribe();

    let router2 = router.clone();
    let handle = tokio::spawn(async move { router2.run(rx).await });

    tx.send(dummy_msg("M1")).unwrap();
    tx.send(dummy_msg("M2")).unwrap();
    tx.send(dummy_msg("M3")).unwrap();

    // Give the router a beat to drain.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(buffer.len(), 3);
    let recent = buffer.list_recent(10);
    assert_eq!(recent.len(), 3);

    // The ids should be sequential (1, 2, 3).
    for (i, ev) in recent.iter().enumerate() {
        match ev {
            InboundEvent::Message { id, .. } => {
                let expected = format!("M{}", i + 1);
                assert_eq!(id, &expected, "event #{i} should be M{}", i + 1);
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    cancel.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn router_evicts_oldest_at_max_rows() {
    let buffer = EventsBuffer::new(5);
    let cancel = CancellationToken::new();
    let router = EventsRouter::new(buffer.clone(), cancel.clone());

    let (tx, _rx) = broadcast::channel::<String>(64);
    let rx = tx.subscribe();

    let router2 = router.clone();
    let handle = tokio::spawn(async move { router2.run(rx).await });

    // Push 10 events; max_rows=5 means the first 5 get evicted.
    for i in 0..10 {
        tx.send(dummy_msg(&format!("E{i}"))).unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(buffer.len(), 5, "buffer should cap at max_rows");
    assert_eq!(buffer.total_pushed(), 10);
    assert_eq!(
        buffer.total_evicted(),
        5,
        "5 events should have been evicted"
    );

    // The remaining 5 are the most recent: E5, E6, E7, E8, E9.
    let recent = buffer.list_recent(5);
    let ids: Vec<String> = recent
        .iter()
        .map(|ev| match ev {
            InboundEvent::Message { id, .. } => id.clone(),
            _ => panic!("expected Message"),
        })
        .collect();
    assert_eq!(ids, vec!["E5", "E6", "E7", "E8", "E9"]);

    cancel.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn router_handles_unknown_raw_input_as_unknown_variant() {
    // The parser maps unrecognised Debug-formatted strings to
    // `InboundEvent::Unknown`. This test pins that behaviour at the
    // router boundary so a future change to `InboundEvent::parse`
    // doesn't silently drop events.
    let buffer = EventsBuffer::new(100);
    let cancel = CancellationToken::new();
    let router = EventsRouter::new(buffer.clone(), cancel.clone());

    let (tx, _rx) = broadcast::channel::<String>(16);
    let rx = tx.subscribe();

    let router2 = router.clone();
    let handle = tokio::spawn(async move { router2.run(rx).await });

    tx.send("definitely not a wacore Event variant".to_string())
        .unwrap();
    tx.send("Another unmapped payload".to_string()).unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(buffer.len(), 2);
    for ev in buffer.list_recent(10) {
        match ev {
            InboundEvent::Unknown { wacore_event, .. } => {
                let payload = wacore_event.as_str().unwrap_or_default();
                assert!(
                    payload.contains("not a wacore Event variant")
                        || payload.contains("Another unmapped payload")
                );
            }
            other => panic!("expected Unknown variant, got {other:?}"),
        }
    }

    cancel.cancel();
    let _ = handle.await;
}

#[tokio::test]
async fn parse_with_now_propagates_far_future_ts_into_unknown() {
    // Future-timestamped Unknown events carry the envelope's ts
    // straight through into the `Unknown { ts_unix_ms, .. }` field.
    // Direct unit test of `parse_with_now` — the router's broadcast
    // channel only carries Strings, so timestamp injection at the
    // router boundary would require a parallel typed path.
    let env = EventEnvelope {
        raw: "garbage payload".to_string(),
        ts_unix_ms: 4_102_444_800_000, // 2100-01-01
        ts_mono_ns: 999,
    };
    let events = InboundEvent::parse_with_now(env, 0);
    assert_eq!(events.len(), 1);
    match &events[0] {
        InboundEvent::Unknown {
            ts_unix_ms,
            ts_mono_ns,
            ..
        } => {
            assert_eq!(*ts_unix_ms, 4_102_444_800_000);
            assert_eq!(*ts_mono_ns, 999);
        }
        other => panic!("expected Unknown variant, got {other:?}"),
    }
}
