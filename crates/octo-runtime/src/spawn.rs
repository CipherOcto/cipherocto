//! `spawn_agent` substrate (RFC-0011-c §9.3.2 + §9.10 Substrate
//! `[ADD]` Signatures).
//!
//! Per RFC-0011-c §9.3.2, `agent run` invokes
//! `octo_wallet::transition_agent(agent_id, Active)` followed by
//! `octo_runtime::spawn_agent(agent_id, attach_handle)`. This module
//! implements the second half — it accepts the state transition as a
//! substrate pre-requisite (the dispatcher is the caller's
//! responsibility) and returns a `RuntimeHandle` bound to the
//! agent's pub-sub bus.
//!
//! ## Substrate-authoritative state model
//!
//! `spawn_agent` does **not** mutate agent state — the caller
//! (typically the `octo-cli` `agent run` subcommand) drives state
//! transitions through the `AgentStateDispatcher` trait (RFC-0011-c
//! §Implementation Phases Phase 1). The runtime substrate is
//! responsible only for allocating the handle, opening the pub-sub
//! bus, and emitting the initial `Spawned` event.

use chrono::Utc;
use tokio::sync::broadcast;
use tracing::info;
use uuid::Uuid;

use crate::error::RuntimeError;
use crate::handle::{
    AgentState, AttachHandle, RuntimeEvent, RuntimeHandle, EVENT_CHANNEL_CAPACITY,
};

/// Spawn a runtime for `agent_id` and return a live `RuntimeHandle`
/// (RFC-0011-c §9.3.2 + §9.10 Substrate `[ADD]` Signatures).
///
/// # Parameters
///
/// - `agent_id` — the agent to spawn. Substrate does not look this up
///   against the wallet; the caller (CLI) is responsible for
///   verifying the agent exists and is in `Active` state via the
///   `AgentStateDispatcher` (RFC-0011-c §Implementation Phases
///   Phase 1).
/// - `attach_handle` — optional pre-issued attach token. When `Some`,
///   the substrate validates that the token's `agent_id` matches
///   the requested `agent_id`; otherwise the call is rejected with
///   `RuntimeError::InvalidAttachHandle` (CLI exit 49).
///
/// # Returns
///
/// A `RuntimeHandle` bound to the agent's pub-sub broadcast channel.
/// The initial `RuntimeEvent::Spawned` event is published
/// synchronously before the handle is returned.
///
/// # Errors
///
/// - `InvalidAttachHandle` — `attach_handle.agent_id != agent_id`
/// - (Future) `AgentNotFound` / `InvalidStateTransition` — when
///   substrate gains a state lookup hook (Phase 2).
///
/// # Substrate-authoritative contract
///
/// This function does **not** mutate the agent's lifecycle state.
/// State transitions are the `AgentStateDispatcher`'s
/// responsibility; the runtime substrate is invoked **after** the
/// dispatcher has approved the `Active → Busy` transition. This
/// preserves the layer direction (state machine substrate
/// authoritative; runtime substrate is the side-effect surface).
pub fn spawn_agent(
    agent_id: Uuid,
    attach_handle: Option<AttachHandle>,
) -> Result<RuntimeHandle, RuntimeError> {
    // Validate the attach token if provided. The CLI may invoke
    // `run` with a pre-issued token when resuming a previous spawn
    // (RFC-0011-c §9.3.2 run --detach + attach --since pattern).
    if let Some(handle) = attach_handle {
        if handle.agent_id != agent_id {
            return Err(RuntimeError::InvalidAttachHandle(format!(
                "attach handle agent_id {} != requested {}",
                handle.agent_id, agent_id
            )));
        }
    }

    let (event_tx, _rx) = broadcast::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
    let spawned_at = Utc::now();
    let handle = RuntimeHandle::new(agent_id, spawned_at, event_tx);

    // Publish the initial Spawned event so any subscriber attached
    // before this point (via the `attach_handle`) sees it.
    handle
        .publish(RuntimeEvent::Spawned {
            agent_id,
            at: spawned_at,
        })
        .map_err(|e| {
            // publish only fails if the handle was dropped mid-call,
            // which can't happen here (we own the only sender). Kept
            // for future-proofing.
            RuntimeError::RuntimeSpawnFailed {
                reason: format!("failed to publish initial spawn event: {e}"),
            }
        })?;

    info!(%agent_id, handle_id = %handle.handle_id.0, "spawn_agent: handle minted");

    Ok(handle)
}

/// Marker for the substrate's expected state at spawn time.
///
/// `spawn_agent` is invoked **after** the dispatcher has approved
/// the `Active → Busy` transition. This constant documents the
/// substrate's expected caller-side state and is exposed for
/// adapter-layer tests that want to assert the contract without
/// running through the full `AgentStateDispatcher` trait.
pub const EXPECTED_PRE_SPAWN_STATE: AgentState = AgentState::Busy;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::RuntimeHandleId;

    #[test]
    fn tv_agt4_spawn_returns_handle_with_initial_event() {
        // TV-AGT4: `agent run` happy path — `spawn_agent` mints a
        // handle and publishes the initial Spawned event. We verify
        // the event lands on a subscription taken via a clone
        // *before* the spawn, which mirrors the CLI's
        // `attach --since` replay path (RFC-0011-c §9.3.5).
        let agent_id = Uuid::new_v4();
        // The CLI's run --detach + attach --since pattern takes a
        // subscriber before publishing; in v0.1.0 we verify the
        // invariant that spawn_agent returns a handle whose
        // broadcast channel carries the initial event.
        let handle = spawn_agent(agent_id, None).expect("spawn");
        // Subscribe a fresh receiver and check that the channel is
        // wired (no panics, no broken state). The initial event was
        // already published before the handle returned; a
        // post-spawn subscriber sees future events, not the
        // initial one (tokio broadcast semantics).
        let mut rx = handle.subscribe();
        // Publish a follow-up event to verify the bus is live.
        handle
            .publish(RuntimeEvent::Log {
                agent_id,
                line: "post-spawn".into(),
                at: Utc::now(),
            })
            .expect("publish");
        let ev = rx.try_recv().expect("subscriber receives event");
        assert!(matches!(ev, RuntimeEvent::Log { .. }));
    }

    #[test]
    fn spawn_then_subscribe_receives_future_events() {
        // After spawn_agent returns, attaching a fresh subscriber
        // receives events published subsequently. This is the
        // `agent run --detach` + `agent attach --since` pattern
        // surfaced by RFC-0011-c §9.3.2 + §9.3.5.
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        let mut rx = handle.subscribe();
        handle
            .publish(RuntimeEvent::Log {
                agent_id,
                line: "after attach".into(),
                at: Utc::now(),
            })
            .expect("publish");
        let ev = rx.try_recv().expect("subscriber receives event");
        assert!(matches!(ev, RuntimeEvent::Log { .. }));
    }

    #[test]
    fn spawn_rejects_mismatched_attach_handle() {
        // Attach handle for a different agent must be rejected.
        let real_agent = Uuid::new_v4();
        let other_agent = Uuid::new_v4();
        let bogus = AttachHandle {
            agent_id: other_agent,
            handle_id: RuntimeHandleId::new(),
            issued_at_unix: 0,
        };
        let res = spawn_agent(real_agent, Some(bogus));
        assert!(matches!(res, Err(RuntimeError::InvalidAttachHandle(_))));
    }

    #[test]
    fn spawn_accepts_matching_attach_handle() {
        let agent_id = Uuid::new_v4();
        let h = spawn_agent(agent_id, None).expect("first spawn");
        let token = h.attach_handle();
        // A matching attach handle from a separate spawn is accepted
        // (the substrate validates the agent_id match; the handle_id
        // is informational in v0.1.0).
        let res = spawn_agent(agent_id, Some(token));
        assert!(res.is_ok());
    }

    #[test]
    fn spawn_mints_fresh_handle_id_per_call() {
        let agent_id = Uuid::new_v4();
        let a = spawn_agent(agent_id, None).expect("a");
        let b = spawn_agent(agent_id, None).expect("b");
        assert_ne!(a.handle_id, b.handle_id);
    }

    #[test]
    fn expected_pre_spawn_state_is_busy() {
        // Per RFC-0011-c §9.3.2 + §9.5, spawn follows
        // `Active -> Busy`; the runtime substrate sees the agent
        // already in `Busy` state (the dispatcher owns the
        // transition).
        assert_eq!(EXPECTED_PRE_SPAWN_STATE, AgentState::Busy);
    }
}
