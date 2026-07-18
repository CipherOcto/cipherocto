//! Polling helpers for the `EventsBuffer`.
//!
//! Live tests assert that a specific inbound event lands in the buffer
//! after a wire action. The buffer is the same store the persister
//! reads from, so when an `InboundEvent` is in the buffer it has also
//! been (or is about to be) written to `events.ndjson`.
//!
//! The helpers in this module are intentionally simple — they poll
//! `EventsBuffer::list_recent` on a fixed cadence and stop as soon as
//! the predicate is satisfied. They are NOT a real-time event stream:
//! live tests that need delivery confirmation use
//! `wait_for(predicate, timeout)` and then assert; tests that need
//! strict receipt-state observation should use the per-id query
//! helpers.
//!
//! ## Polling cadence
//!
//! `WAIT_POLL_MS = 100 ms` is the default. 100 ms is short enough that
//! typical WA server response times (200-500 ms) don't dominate test
//! runtime, and long enough that we don't busy-loop the executor.
//! Live tests that need a finer resolution can pass an explicit
//! `poll_interval` to `wait_for_with`.
//!
//! ## Hermeticity
//!
//! `wait_for` is a pure reader — it never blocks the persister, never
//! mutates the buffer, and never requires a live WA session. The
//! hermetic tests in this module push synthetic `InboundEvent` rows
//! into a test-owned `EventsBuffer` and assert the predicate fires
//! within the timeout.

use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::events::InboundEvent;
use crate::events_buffer::EventsBuffer;

/// Default poll interval for `wait_for`. 100 ms is fast enough to
/// keep live-test runtime bounded against the 2 s WA rate-limit floor,
/// slow enough to avoid burning CPU on idle waits.
pub const WAIT_POLL_MS: u64 = 100;

/// Default overall timeout for `wait_for`. Live tests should pass an
/// explicit timeout tuned to the action being asserted (e.g. 10 s for
/// a self-echo, 30 s for a group change). 30 s is the default for the
/// non-async-friendly helper.
pub const WAIT_DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Error returned by `wait_for` when the predicate does not fire
/// within the timeout.
#[derive(Debug, Error)]
pub enum WaitError {
    #[error("wait_for: predicate not satisfied within {timeout:?} (poll_count={poll_count}, last_id={last_id})")]
    Timeout {
        timeout: Duration,
        poll_count: u64,
        last_id: u64,
    },
    #[error("wait_for: buffer exhausted (largest_id={0}) — predicate never observed")]
    BufferExhausted(u64),
}

/// Poll `buffer.list_recent(limit)` every `poll_interval` until
/// `predicate` returns `true` for some event, or `timeout` elapses.
///
/// Returns the first matching event. On timeout, returns
/// `Err(WaitError::Timeout)` with diagnostics.
///
/// `limit` is the per-poll page size. Live tests should pass
/// `buffer.len()` (i.e. the full buffer) — the buffer is bounded by
/// `max_rows` so this is cheap.
pub fn wait_for(
    buffer: &Arc<EventsBuffer>,
    predicate: impl FnMut(&InboundEvent) -> bool,
    timeout: Duration,
) -> Result<InboundEvent, WaitError> {
    wait_for_with(
        buffer,
        predicate,
        timeout,
        Duration::from_millis(WAIT_POLL_MS),
    )
}

/// Like [`wait_for`] but with a configurable poll interval. The
/// `poll_count` in the [`WaitError::Timeout`] variant is the number
/// of times the buffer was scanned before giving up.
pub fn wait_for_with(
    buffer: &Arc<EventsBuffer>,
    mut predicate: impl FnMut(&InboundEvent) -> bool,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<InboundEvent, WaitError> {
    let start = Instant::now();
    let mut poll_count: u64 = 0;
    loop {
        // Page through the entire buffer — `list_recent` returns the
        // most recent N; for tests we want a global scan. The buffer
        // is bounded (default 10k rows) so this is cheap.
        let page = buffer.list_recent(buffer.len().max(1));
        for ev in &page {
            if predicate(ev) {
                return Ok(ev.clone());
            }
        }
        poll_count += 1;
        if start.elapsed() >= timeout {
            return Err(WaitError::Timeout {
                timeout,
                poll_count,
                last_id: buffer.largest_id(),
            });
        }
        std::thread::sleep(poll_interval);
    }
}

/// One-shot helper: wait for an event whose `id` field (or
/// `msg_id`/`group_jid` for non-Message variants) equals the given
/// key. Returns the event. Used by live tests that have a known
/// `message_id` from a prior `daemon.send.*` response.
///
/// Note: only `Message` and `Receipt` variants carry an `id`/`msg_id`
/// field directly. For other variants use [`wait_for`] with a
/// structural predicate.
pub fn wait_for_id(
    buffer: &Arc<EventsBuffer>,
    target_id: &str,
    timeout: Duration,
) -> Result<InboundEvent, WaitError> {
    let target = target_id.to_string();
    wait_for(
        buffer,
        move |ev| match ev {
            InboundEvent::Message { id, .. } => id == &target,
            InboundEvent::Receipt { msg_id, .. } => msg_id == &target,
            InboundEvent::Reaction { id, .. } => id == &target,
            InboundEvent::IncomingCall { .. } => false,
            InboundEvent::MissedCall { .. } => false,
            InboundEvent::CallEndedElsewhere { .. } => false,
            InboundEvent::Story { id, .. } => id == &target,
            _ => false,
        },
        timeout,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ConnectionKind, MessageKind};
    use std::sync::Arc;
    use std::time::Duration;

    fn synth_message(id: &str, peer: &str, text: &str, ts_ms: i64) -> InboundEvent {
        InboundEvent::Message {
            id: id.to_string(),
            peer: peer.to_string(),
            sender: peer.to_string(),
            ts_unix_ms: ts_ms,
            ts_mono_ns: 0,
            kind: MessageKind::Text,
            text: text.to_string(),
            media_token: None,
            reply_to: None,
            mentions: vec![],
            mentions_truncated: false,
            from_me: false,
            is_group: false,
            view_once: false,
            ephemeral_expires_at_seconds: None,
        }
    }

    fn synth_connection_open(ts_ms: i64) -> InboundEvent {
        InboundEvent::Connection {
            kind: ConnectionKind::Connected,
            cause: None,
            ts_unix_ms: ts_ms,
            ts_mono_ns: 0,
        }
    }

    #[test]
    fn wait_for_returns_first_matching_event() {
        let buf = Arc::new(EventsBuffer::new(100));
        buf.push(synth_message("m1", "+1", "first", 1_700_000_000));
        buf.push(synth_message("m2", "+1", "second", 1_700_000_001));
        let ev = wait_for(
            &buf,
            |e| matches!(e, InboundEvent::Message { text, .. } if text == "second"),
            Duration::from_secs(1),
        )
        .expect("predicate should match");
        if let InboundEvent::Message { text, .. } = ev {
            assert_eq!(text, "second");
        } else {
            panic!("expected Message variant");
        }
    }

    #[test]
    fn wait_for_times_out_when_no_match() {
        let buf = Arc::new(EventsBuffer::new(100));
        buf.push(synth_message("m1", "+1", "hi", 1_700_000_000));
        let err = wait_for(
            &buf,
            |e| matches!(e, InboundEvent::Message { text, .. } if text == "missing"),
            Duration::from_millis(200),
        )
        .unwrap_err();
        match err {
            WaitError::Timeout {
                poll_count,
                last_id,
                ..
            } => {
                assert!(poll_count >= 1);
                assert_eq!(last_id, 1);
            }
            other => panic!("expected Timeout, got {other:?}"),
        }
    }

    #[test]
    fn wait_for_handles_event_arriving_after_poll_began() {
        let buf = Arc::new(EventsBuffer::new(100));
        // Pre-existing event that does NOT match the predicate.
        buf.push(synth_message("m1", "+1", "old", 1_700_000_000));
        let buf_clone = buf.clone();
        // Spawn a thread that pushes the matching event after 150 ms.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            buf_clone.push(synth_message("m2", "+1", "new", 1_700_000_001));
        });
        let ev = wait_for(
            &buf,
            |e| matches!(e, InboundEvent::Message { text, .. } if text == "new"),
            Duration::from_secs(2),
        )
        .expect("predicate should match after async push");
        if let InboundEvent::Message { text, .. } = ev {
            assert_eq!(text, "new");
        } else {
            panic!("expected Message variant");
        }
    }

    #[test]
    fn wait_for_id_resolves_message() {
        let buf = Arc::new(EventsBuffer::new(100));
        buf.push(synth_message("target-msg-id", "+1", "x", 1_700_000_000));
        buf.push(synth_connection_open(1_700_000_001));
        let ev = wait_for_id(&buf, "target-msg-id", Duration::from_secs(1)).expect("id match");
        assert!(matches!(ev, InboundEvent::Message { .. }));
    }

    #[test]
    fn wait_for_id_resolves_receipt() {
        let buf = Arc::new(EventsBuffer::new(100));
        buf.push(InboundEvent::Receipt {
            msg_id: "rcpt-1".into(),
            peer: "+1".into(),
            kind: crate::events::ReceiptKind::Delivered,
            ts_unix_ms: 1_700_000_000,
            ts_mono_ns: 0,
        });
        let ev = wait_for_id(&buf, "rcpt-1", Duration::from_secs(1)).expect("id match");
        assert!(matches!(ev, InboundEvent::Receipt { .. }));
    }

    #[test]
    fn wait_for_id_times_out_for_unknown_id() {
        let buf = Arc::new(EventsBuffer::new(100));
        let err = wait_for_id(&buf, "missing", Duration::from_millis(150)).unwrap_err();
        assert!(matches!(err, WaitError::Timeout { .. }));
    }

    #[test]
    fn wait_for_with_fast_poll_still_resolves() {
        let buf = Arc::new(EventsBuffer::new(100));
        let buf_clone = buf.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            buf_clone.push(synth_message("fast", "+1", "x", 1_700_000_000));
        });
        let ev = wait_for_with(
            &buf,
            |e| matches!(e, InboundEvent::Message { id, .. } if id == "fast"),
            Duration::from_secs(1),
            Duration::from_millis(5),
        )
        .expect("fast poll should still find it");
        if let InboundEvent::Message { id, .. } = ev {
            assert_eq!(id, "fast");
        } else {
            panic!("expected Message");
        }
    }
}
