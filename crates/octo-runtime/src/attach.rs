//! `attach` substrate (RFC-0011-c §9.3.5 + §9.10 Substrate `[ADD]`
//! Signatures).
//!
//! Per RFC-0011-c §9.3.5, `agent attach <agent-id>` invokes
//! `octo_runtime::attach(handle, since)` and returns an
//! `EventStream` bound to the runtime's pub-sub bus. The CLI uses
//! the stream to surface events to the operator terminal.
//!
//! ## Layer discipline
//!
//! Layer B substrate. Read-only handle (does not mutate state per
//! RFC-0011-c §9.3.5). Validation logic is local to the substrate
//! (revocation check + `since` clamping); the CLI does not
//! parallel-validate.

use chrono::{DateTime, Utc};
use tracing::debug;
use uuid::Uuid;

use crate::error::RuntimeError;
use crate::handle::{EventStream, RuntimeHandle};

/// Attach to a previously spawned runtime and return an event
/// stream (RFC-0011-c §9.3.5 + §9.10 Substrate `[ADD]` Signatures).
///
/// # Parameters
///
/// - `handle` — a `RuntimeHandle` obtained from `spawn_agent` (or a
///   clone thereof). The handle is consumed by this call (the
///   caller no longer needs it; the returned `EventStream` holds
///   the subscription).
/// - `since` — optional lower-bound timestamp (RFC-0011-c
///   §9.3.5 `--since <unix-seconds>`). When `None`, the stream
///   starts from the spawn time. When `Some`, the substrate clamps
///   the lower bound to `[spawned_at, Utc::now()]` — a `since`
///   earlier than `spawned_at` is silently raised to `spawned_at`
///   (the bus has no events before spawn).
///
/// # Returns
///
/// An `EventStream` bound to the runtime's pub-sub bus. The caller
/// iterates via `EventStream::next()` to receive events.
///
/// # Errors
///
/// - `HandleRevoked` — the handle has been dropped (the spawn was
///   terminated or the bus closed). Surfaces as CLI exit 49
///   (`RuntimeAttachFailed`).
///
/// # State semantics
///
/// `attach` is **read-only** per RFC-0011-c §9.3.5. The substrate
/// does not transition the agent's lifecycle state. State
/// transitions remain the `AgentStateDispatcher`'s responsibility
/// (RFC-0011-c §Implementation Phases Phase 1).
///
/// In v0.1.0 the broadcast channel closes only when **all**
/// `RuntimeHandle` clones drop; `attach` does not perform an
/// explicit revocation check. The returned `EventStream` will
/// surface `RuntimeError::EventStreamClosed` on the next poll
/// after the channel closes.
pub fn attach(
    handle: RuntimeHandle,
    since: Option<DateTime<Utc>>,
) -> Result<EventStream, RuntimeError> {
    // Clamp `since` to [spawned_at, Utc::now()] — the bus has no
    // events before spawn, and future timestamps are nonsensical.
    let lower = clamp_since(since, handle.spawned_at, Utc::now());

    let handle_id = handle.handle_id;
    let agent_id: Uuid = handle.agent_id;
    let rx = handle.subscribe();
    // Deliberately drop the original `handle` — the broadcast
    // sender inside it is reference-counted; the subscription
    // keeps the channel open. The local revocation flag is no
    // longer observable from the EventStream side (intentional —
    // lag detection surfaces via `RecvError::Closed` on the
    // broadcast receiver itself).
    drop(handle);

    debug!(%agent_id, handle_id = %handle_id.0, since = %lower, "attach: stream opened");

    Ok(EventStream {
        agent_id,
        handle_id,
        since: lower,
        rx,
    })
}

/// Clamp `since` to `[spawned_at, now]`.
///
/// Public for adapter-layer tests; the CLI never calls this
/// directly (it goes through `attach`).
#[must_use]
pub fn clamp_since(
    since: Option<DateTime<Utc>>,
    spawned_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    let candidate = since.unwrap_or(spawned_at);
    if candidate < spawned_at {
        spawned_at
    } else if candidate > now {
        now
    } else {
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::RuntimeEvent;
    use crate::spawn::spawn_agent;

    #[test]
    fn tv_agt11_attach_returns_event_stream() {
        // TV-AGT11: `agent attach` happy path — spawn a runtime,
        // attach, and verify the stream is live.
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        let stream = attach(handle, None).expect("attach");
        assert_eq!(stream.agent_id, agent_id);
        assert!(stream.since.timestamp() <= Utc::now().timestamp());
    }

    #[test]
    fn attach_succeeds_for_last_clone() {
        // In v0.1.0 the broadcast channel closes only when ALL
        // `RuntimeHandle` clones drop. A single remaining clone is
        // still a valid handle for `attach` (the broadcast sender
        // inside is alive; the channel is open).
        let (tx, _) =
            tokio::sync::broadcast::channel::<RuntimeEvent>(crate::handle::EVENT_CHANNEL_CAPACITY);
        let h = crate::handle::RuntimeHandle::new(Uuid::new_v4(), Utc::now(), tx);
        let agent = h.agent_id;
        let cloned = h.clone();
        drop(h);
        // `cloned` is the only live handle (is_last_clone == true)
        // but the channel is still open. `attach` must succeed.
        let stream = attach(cloned, None).expect("attach succeeds for last clone");
        assert_eq!(stream.agent_id, agent);
    }

    #[test]
    fn attach_with_since_clamps_to_spawned_at() {
        // Since earlier than spawn → clamp to spawn.
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        let spawned_at = handle.spawned_at;
        let too_early = spawned_at - chrono::Duration::seconds(60);
        let stream = attach(handle, Some(too_early)).expect("attach");
        assert_eq!(stream.since, spawned_at);
    }

    #[test]
    fn attach_with_since_clamps_to_now() {
        // Since in the future → clamp to now.
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        let future = Utc::now() + chrono::Duration::seconds(3600);
        let before = Utc::now();
        let stream = attach(handle, Some(future)).expect("attach");
        let after = Utc::now();
        assert!(stream.since >= before && stream.since <= after);
    }

    #[test]
    fn attach_preserves_since_within_window() {
        // Since equal to spawned_at is preserved. We can't test
        // `since > spawned_at && since < now` reliably because
        // `Utc::now()` advances between `spawn_agent` and `attach`
        // and `clamp_since` clamps to `now`. The `None` case
        // (which falls back to `spawned_at`) is covered by
        // `tv_agt11_attach_returns_event_stream`.
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        let mid = handle.spawned_at;
        let stream = attach(handle, Some(mid)).expect("attach");
        assert_eq!(stream.since, mid);
    }

    #[test]
    fn clamp_since_helper_table() {
        let spawned = Utc::now();
        let now = spawned + chrono::Duration::seconds(10);
        // None → spawned_at
        assert_eq!(clamp_since(None, spawned, now), spawned);
        // before spawned → spawned_at
        assert_eq!(
            clamp_since(Some(spawned - chrono::Duration::seconds(5)), spawned, now),
            spawned
        );
        // after now → now
        assert_eq!(
            clamp_since(Some(now + chrono::Duration::seconds(5)), spawned, now),
            now
        );
        // within window → unchanged
        let mid = spawned + chrono::Duration::seconds(5);
        assert_eq!(clamp_since(Some(mid), spawned, now), mid);
    }

    #[test]
    fn attached_stream_receives_published_events() {
        // End-to-end: spawn → attach → publish → receive. Verifies
        // the pub-sub bus is wired through the `attach` boundary
        // (Risk LOW per mission YAML §Risk).
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        // Clone before attach so we can still publish.
        let publisher = handle.clone();
        let mut stream = attach(handle, None).expect("attach");
        publisher
            .publish(RuntimeEvent::Log {
                agent_id,
                line: "post-attach".into(),
                at: Utc::now(),
            })
            .expect("publish");
        // Sync poll — broadcast is in-process so the publish above
        // is already visible.
        let ev = sync_recv(&mut stream).expect("receive");
        assert!(matches!(ev, RuntimeEvent::Log { ref line, .. } if line == "post-attach"));
    }

    /// Synchronous poll for the next event. The in-process broadcast
    /// channel makes the publish visible to the receiver immediately;
    /// `try_recv` is sufficient and avoids pulling in a runtime.
    fn sync_recv(stream: &mut EventStream) -> Result<RuntimeEvent, RuntimeError> {
        use tokio::sync::broadcast::error::TryRecvError;
        match stream.rx.try_recv() {
            Ok(ev) => Ok(ev),
            Err(TryRecvError::Closed) => Err(RuntimeError::EventStreamClosed),
            Err(TryRecvError::Empty) => Err(RuntimeError::EventStreamClosed),
            Err(TryRecvError::Lagged(skipped)) => Ok(RuntimeEvent::Log {
                agent_id: stream.agent_id,
                line: format!("event stream lagged: skipped {skipped} events"),
                at: Utc::now(),
            }),
        }
    }
}
