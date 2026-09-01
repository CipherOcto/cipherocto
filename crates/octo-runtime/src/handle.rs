//! `octo-runtime` substrate handle types + state machine contract.
//!
//! Per RFC-0011-c §9.1 Architecture + §9.10 Substrate `[ADD]` Signatures:
//!
//! - `RuntimeHandle` — live runtime handle returned by `spawn_agent`
//! - `EventStream` — pub-sub event subscription returned by `attach`
//! - `AttachHandle` — token to attach to an existing spawn
//! - `AgentState` — substrate state machine (RFC-0002 §Agent State
//!   Machine, projected onto the runtime-relevant subset)
//! - `AgentStateDispatcher` — shared state-machine trait that
//!   `octo-wallet` consumes (RFC-0011-c §Implementation Phases Phase 1)
//! - `RuntimeEvent` — pub-sub message body (RFC-0011-c §9.3.5)
//!
//! ## Layer discipline
//!
//! Layer B substrate (RFC-0011-c §9.1 Architecture). Types are
//! years-stable per [[cipherocto-design-principles]] §Stable
//! Abstractions Principle. New variants on `AgentState` /
//! `RuntimeEvent` land via additive `#[non_exhaustive]` clauses
//! ([[cipherocto-design-principles]] §Extension over enumeration).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::error::RuntimeError;

/// Default broadcast-channel capacity for runtime events.
///
/// Sized to absorb a burst of state-change + log events from a
/// single agent without back-pressuring the substrate; per the
/// tokio `broadcast` contract, slow consumers see `RecvError::Lagged`
/// and must replay via `attach(handle, since)` per RFC-0011-c
/// §9.3.5.
pub const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Agent lifecycle state (RFC-0002 §Agent State Machine +
/// RFC-0011-c §9.5 Agent State Machine Integration).
///
/// Substrate-authoritative; the CLI never invents new transitions.
/// New states land as additive variants via `#[non_exhaustive]`
/// per [[cipherocto-design-principles]] §Extension over enumeration.
///
/// ```text
/// REGISTERED ──[activate]──→ ACTIVE ──[run]──→ BUSY ──[idle]──→ ACTIVE
///                                │              │                │
///                                │              │                │
///                                ↓              ↓                ↓
///                            TERMINATED     SUSPENDED         RETIRED
/// REGISTERED ──[reject]──→ REJECTED  (terminal; never leaves)
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum AgentState {
    /// Named at init, not yet active. Holder may NOT spawn.
    Registered,
    /// Identity in use; spawning is permitted.
    Active,
    /// Runtime handle attached; agent is executing work.
    Busy,
    /// Paused; spawning is permitted (resume path).
    Suspended,
    /// Rejected at init; never leaves this state.
    Rejected,
    /// Destroyed; never leaves this state.
    Terminated,
    /// Suspended then retired; never leaves this state.
    Retired,
}

impl AgentState {
    /// `true` iff a `spawn_agent` call against this state is valid.
    ///
    /// Per RFC-0011-c §9.5, only `Active` permits spawn. `Busy`
    /// rejects (already running); terminal states reject.
    #[must_use]
    pub const fn can_spawn(self) -> bool {
        matches!(self, Self::Active)
    }

    /// `true` iff an `attach` call against this state is valid.
    ///
    /// Per RFC-0011-c §9.3.5, only `Busy` permits attach (a handle
    /// must exist). `Active` rejects (no runtime attached); terminal
    /// states reject.
    #[must_use]
    pub const fn can_attach(self) -> bool {
        matches!(self, Self::Busy)
    }

    /// `true` iff this state is terminal (no outbound edges).
    ///
    /// `Rejected`, `Terminated`, and `Retired` are terminal per
    /// RFC-0011-c §9.5.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Rejected | Self::Terminated | Self::Retired)
    }

    /// Valid state-machine edges per RFC-0011-c §9.5.
    ///
    /// Used by `AgentStateDispatcher::dispatch` to enforce the
    /// substrate-authoritative transition table.
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        use AgentState::*;
        matches!(
            (self, target),
            (Registered, Active)
                | (Registered, Rejected)
                | (Active, Busy)
                | (Active, Suspended)
                | (Active, Terminated)
                | (Busy, Active)
                | (Busy, Terminated)
                | (Suspended, Active)
                | (Suspended, Retired)
        )
    }

    /// Stable string label (for `RuntimeError::InvalidStateTransition`
    /// payload). Matches the serde `SCREAMING_SNAKE_CASE` rename.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Registered => "REGISTERED",
            Self::Active => "ACTIVE",
            Self::Busy => "BUSY",
            Self::Suspended => "SUSPENDED",
            Self::Rejected => "REJECTED",
            Self::Terminated => "TERMINATED",
            Self::Retired => "RETIRED",
        }
    }
}

/// Typed UUID identifier for a `RuntimeHandle`
/// ([[cipherocto-design-principles]] §Extension over enumeration
/// — typed 128-bit UUID discriminator per agent-id namespace).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuntimeHandleId(pub Uuid);

impl RuntimeHandleId {
    /// Mint a fresh handle id.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RuntimeHandleId {
    fn default() -> Self {
        Self::new()
    }
}

/// Token that authorizes attaching to a previously spawned runtime.
///
/// Returned by `spawn_agent` alongside `RuntimeHandle`; consumed by
/// `attach` to verify the operator holds the matching spawn. Per
/// RFC-0011-c §9.3.2 (`--detach` + `--attach`) the CLI forwards
/// this token between invocations so an operator can `run --detach`
/// then `attach --since` from a separate process.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachHandle {
    /// Agent id the handle authorizes.
    pub agent_id: Uuid,
    /// Handle id (binds the attach to the matching spawn).
    pub handle_id: RuntimeHandleId,
    /// Unix-seconds when the spawn occurred.
    pub issued_at_unix: i64,
}

impl AttachHandle {
    /// Build an attach handle from a runtime handle.
    #[must_use]
    pub fn from_runtime_handle(handle: &RuntimeHandle) -> Self {
        Self {
            agent_id: handle.agent_id,
            handle_id: handle.handle_id,
            issued_at_unix: handle.spawned_at.timestamp(),
        }
    }
}

/// Inner state shared across all `RuntimeHandle` clones.
///
/// `Arc<HandleInner>` is held by every clone. When the last
/// `Arc<HandleInner>` drops, the inner drops, and the broadcast
/// channel closes (no more senders). `is_revoked()` observes the
/// remaining `Arc::strong_count` directly — when it reaches 1
/// (only the current handle), the channel is about to close.
///
/// The `keepalive_rx` field holds an internal broadcast receiver
/// for the handle's lifetime. Per the tokio `broadcast::Sender`
/// contract, `Sender::send` returns `Err(SendError(_))` only when
/// no active receivers exist — by holding one receiver internally,
/// the publish path ALWAYS succeeds even when no external
/// subscriber has attached yet.
///
/// The ONLY invariant this guarantees is `Sender::send` cannot
/// return `Err(SendError(_))` for the lifetime of any handle clone
/// (because the keep-alive receiver holds an active subscription on
/// the broadcast channel). It does NOT guarantee that any event
/// reaches any external consumer: the `keepalive_rx` is never
/// `recv()`d, so new external `subscribe()` calls still join at
/// the current tail position and miss events that were sent before
/// they subscribed (standard tokio `broadcast` semantics). The
/// initial `Spawned` event from `spawn_agent` is therefore NOT
/// guaranteed to reach the next `attach` — an `attach` issued
/// after `spawn_agent` returns will start from the channel tail.
struct HandleInner {
    /// Pub-sub broadcast sender (Layer D transport substrate).
    event_tx: broadcast::Sender<RuntimeEvent>,
    /// Keep-alive receiver held for the handle's lifetime.
    /// Guarantees `Sender::send` cannot return `Err(SendError)`
    /// during the handle's lifetime even when no external
    /// subscriber has attached yet. The receiver is intentionally
    /// never `recv()`d from — its sole purpose is to register as a
    /// live receiver with the broadcast channel. The
    /// `#[allow(dead_code)]` suppresses the false-positive
    /// dead-code warning (the field's value matters as a side
    /// effect of being held, not via any access).
    #[allow(dead_code)]
    keepalive_rx: broadcast::Receiver<RuntimeEvent>,
}

/// Live runtime handle for a spawned agent (RFC-0011-c §9.3.2 +
/// §9.10 Substrate `[ADD]` Signatures).
///
/// `Clone` shares the underlying pub-sub broadcast sender so
/// multiple `RuntimeHandle` instances observe the same channel
/// (the CLI's `run --detach` + `attach --since` pattern per
/// RFC-0011-c §9.3.2). The handle is "revoked" when **all**
/// clones have been dropped — at that point the broadcast channel
/// closes and live `EventStream`s see `RecvError::Closed` on the
/// next poll.
#[derive(Clone)]
pub struct RuntimeHandle {
    /// Unique handle id (preserved across clones).
    pub handle_id: RuntimeHandleId,
    /// Agent id this handle is bound to.
    pub agent_id: Uuid,
    /// When the spawn occurred (RFC 3339 UTC).
    pub spawned_at: DateTime<Utc>,
    /// Shared inner state (broadcast sender + self-weak).
    inner: Arc<HandleInner>,
}

impl RuntimeHandle {
    /// Construct a handle from raw substrate parts. Used by
    /// `spawn_agent` and by tests; not part of the public API.
    pub(crate) fn new(
        agent_id: Uuid,
        spawned_at: DateTime<Utc>,
        event_tx: broadcast::Sender<RuntimeEvent>,
        keepalive_rx: broadcast::Receiver<RuntimeEvent>,
    ) -> Self {
        let inner = Arc::new(HandleInner {
            event_tx,
            keepalive_rx,
        });
        Self {
            handle_id: RuntimeHandleId::new(),
            agent_id,
            spawned_at,
            inner,
        }
    }

    /// Subscribe a new `EventStream` to this handle's broadcast.
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.inner.event_tx.subscribe()
    }

    /// Publish an event to all live subscribers.
    ///
    /// Used by substrate internals (`spawn_agent` initial event,
    /// state-machine dispatch); also exposed for adapter-layer code
    /// that wishes to feed the pub-sub bus (per Layer D transport
    /// contract).
    ///
    /// The handle retains an internal keep-alive receiver
    /// ([`HandleInner::keepalive_rx`]) so `Sender::send` cannot
    /// return `Err(SendError)` for the lifetime of any handle
    /// clone — the channel always has at least one receiver.
    ///
    /// Note: the keep-alive receiver is never `recv()`d. It only
    /// guarantees that `send` does not return `Err(SendError(_))`;
    /// it does NOT cause events to be retained for any future
    /// external subscriber. New `subscribe()` calls join at the
    /// current tail position per tokio `broadcast` semantics, so
    /// events sent before subscription are not replayed.
    ///
    /// In v0.1.0 the broadcast channel closes only when **all**
    /// `RuntimeHandle` clones are dropped (natural tokio
    /// `broadcast` behavior — no explicit revocation). Live
    /// `EventStream`s see `RuntimeError::EventStreamClosed` on the
    /// next poll. Future missions may add an explicit
    /// `revoke()` entry point; the `HandleRevoked` error variant
    /// is reserved for that path.
    pub fn publish(&self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        // `Sender::send` returns `Err(SendError(value))` only when no
        // receivers are alive. The keep-alive receiver inside
        // `HandleInner` is held by every clone of the handle, so the
        // channel always has ≥1 receiver while the handle is live —
        // `send` cannot fail here. The `let _ = ...` is defensive
        // (silently ignoring the Ok(usize) receiver count).
        let _ = self.inner.event_tx.send(event);
        Ok(())
    }

    /// `true` iff all peer `RuntimeHandle` clones have been dropped.
    ///
    /// In v0.1.0 the only way to reach the revoked state is for every
    /// clone (including the one calling this method) to be dropped,
    /// at which point the broadcast channel has already closed and
    /// `EventStream` subscribers see `EventStreamClosed`. The check
    /// here is a forward-compat hook for future missions that add
    /// explicit revocation (`revoke()` API on the substrate). For
    /// now, the only call site is the test suite.
    #[must_use]
    pub fn is_revoked(&self) -> bool {
        // When the inner Arc's strong count drops to 0, the
        // broadcast sender is dropped and the channel closes. We
        // approximate "this handle is the only one alive" as a
        // proxy for "about to close" — see `is_last_clone` for
        // the precise semantic.
        self.is_last_clone()
    }

    /// `true` iff this is the only live `RuntimeHandle` for the
    /// underlying broadcast channel (i.e., all peer clones have
    /// been dropped). The next `Drop` on this handle will close
    /// the channel.
    ///
    /// Reserved for adapter-layer telemetry; future missions may
    /// use it to drive graceful-shutdown signals.
    #[must_use]
    pub fn is_last_clone(&self) -> bool {
        Arc::strong_count(&self.inner) <= 1
    }

    /// Build an attach token for this handle (RFC-0011-c §9.3.2).
    #[must_use]
    pub fn attach_handle(&self) -> AttachHandle {
        AttachHandle::from_runtime_handle(self)
    }
}

impl std::fmt::Debug for RuntimeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeHandle")
            .field("handle_id", &self.handle_id)
            .field("agent_id", &self.agent_id)
            .field("spawned_at", &self.spawned_at)
            .field("revoked", &self.is_revoked())
            .finish_non_exhaustive()
    }
}

/// Pub-sub event subscription (RFC-0011-c §9.3.5 + §9.10).
///
/// Returned by `attach`; consumed by the CLI to stream events to the
/// operator terminal. Read-only; does not mutate agent state.
#[derive(Debug)]
pub struct EventStream {
    /// Agent id the stream is bound to.
    pub agent_id: Uuid,
    /// Handle id the stream is bound to.
    pub handle_id: RuntimeHandleId,
    /// Lower-bound timestamp (RFC-0011-c §9.3.5 `--since`).
    pub since: DateTime<Utc>,
    /// Broadcast receiver for the runtime events.
    pub rx: broadcast::Receiver<RuntimeEvent>,
}

impl EventStream {
    /// Receive the next event; returns `RuntimeError::EventStreamClosed`
    /// when the broadcast sender has been dropped.
    pub async fn next(&mut self) -> Result<RuntimeEvent, RuntimeError> {
        match self.rx.recv().await {
            Ok(ev) => Ok(ev),
            Err(broadcast::error::RecvError::Closed) => Err(RuntimeError::EventStreamClosed),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                // Lagged consumers re-subscribe via `attach(handle, since)`
                // (RFC-0011-c §9.3.5). Surface the lag as a synthetic
                // log event so the CLI can warn the operator.
                Ok(RuntimeEvent::Log {
                    agent_id: self.agent_id,
                    line: format!("event stream lagged: skipped {skipped} events"),
                    at: Utc::now(),
                })
            }
        }
    }
}

/// Pub-sub message body (RFC-0011-c §9.3.5 event stream).
///
/// New variants land additively per
/// [[cipherocto-design-principles]] §Extension over enumeration.
/// CLI surfaces events as JSON envelopes per RFC-0011 §Output Envelope
/// (schema_version = 4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuntimeEvent {
    /// Initial spawn event — emitted by `spawn_agent`.
    Spawned {
        /// Agent id.
        agent_id: Uuid,
        /// Spawn timestamp.
        at: DateTime<Utc>,
    },
    /// State-machine transition.
    StateChanged {
        /// Agent id.
        agent_id: Uuid,
        /// Previous state.
        from: AgentState,
        /// New state.
        to: AgentState,
        /// Transition timestamp.
        at: DateTime<Utc>,
    },
    /// Free-form log line from the runtime container.
    Log {
        /// Agent id.
        agent_id: Uuid,
        /// Log line.
        line: String,
        /// Timestamp.
        at: DateTime<Utc>,
    },
    /// Termination event — last event emitted before handle drop.
    Terminated {
        /// Agent id.
        agent_id: Uuid,
        /// Termination timestamp.
        at: DateTime<Utc>,
    },
}

/// Shared state-machine trait (RFC-0011-c §Implementation Phases
/// Phase 1; Layer B contract between `octo-wallet` and `octo-runtime`).
///
/// `octo-wallet` implements this trait and exposes it via the
/// `octo_wallet::state::dispatcher()` accessor. `octo-runtime`
/// consumes the trait to enforce state-machine transitions without
/// re-implementing the table (Risk HIGH mitigation per mission YAML
/// §Risk).
///
/// # Layer discipline
///
/// Per [[cipherocto-design-principles]] §Stable Abstractions
/// Principle, this trait is Layer B — years-stable; new methods land
/// as `#[non_exhaustive]` additions with default impls.
#[async_trait]
pub trait AgentStateDispatcher: Send + Sync {
    /// Dispatch a state transition for `agent_id`.
    ///
    /// Returns the **previous** state on success so the caller can
    /// emit a `RuntimeEvent::StateChanged`. Returns
    /// `RuntimeError::AgentNotFound` if `agent_id` is unknown to the
    /// wallet and `RuntimeError::InvalidStateTransition` if the
    /// requested target is not reachable from the current state.
    async fn dispatch(
        &self,
        agent_id: Uuid,
        target: AgentState,
        reason: Option<&str>,
    ) -> Result<AgentState, RuntimeError>;

    /// Look up the current state of `agent_id`.
    ///
    /// Returns `None` if `agent_id` is unknown.
    async fn current_state(&self, agent_id: Uuid) -> Option<AgentState>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_state_can_spawn_matches_rfc_0011_c() {
        // Only Active permits spawn per RFC-0011-c §9.5.
        assert!(AgentState::Active.can_spawn());
        assert!(!AgentState::Registered.can_spawn());
        assert!(!AgentState::Busy.can_spawn());
        assert!(!AgentState::Suspended.can_spawn());
        assert!(!AgentState::Rejected.can_spawn());
        assert!(!AgentState::Terminated.can_spawn());
        assert!(!AgentState::Retired.can_spawn());
    }

    #[test]
    fn agent_state_can_attach_matches_rfc_0011_c() {
        // Only Busy permits attach per RFC-0011-c §9.3.5.
        assert!(AgentState::Busy.can_attach());
        assert!(!AgentState::Registered.can_attach());
        assert!(!AgentState::Active.can_attach());
        assert!(!AgentState::Suspended.can_attach());
        assert!(!AgentState::Rejected.can_attach());
        assert!(!AgentState::Terminated.can_attach());
        assert!(!AgentState::Retired.can_attach());
    }

    #[test]
    fn agent_state_is_terminal_matches_rfc_0011_c() {
        assert!(!AgentState::Registered.is_terminal());
        assert!(!AgentState::Active.is_terminal());
        assert!(!AgentState::Busy.is_terminal());
        assert!(!AgentState::Suspended.is_terminal());
        assert!(AgentState::Rejected.is_terminal());
        assert!(AgentState::Terminated.is_terminal());
        assert!(AgentState::Retired.is_terminal());
    }

    #[test]
    fn agent_state_transitions_match_rfc_0011_c_section_9_5() {
        use AgentState::*;
        // Valid edges per RFC-0011-c §9.5 state-machine table.
        assert!(Registered.can_transition_to(Active));
        assert!(Registered.can_transition_to(Rejected));
        assert!(Active.can_transition_to(Busy));
        assert!(Active.can_transition_to(Suspended));
        assert!(Active.can_transition_to(Terminated));
        assert!(Busy.can_transition_to(Active));
        assert!(Busy.can_transition_to(Terminated));
        assert!(Suspended.can_transition_to(Active));
        assert!(Suspended.can_transition_to(Retired));

        // Invalid edges (must be rejected by the substrate).
        assert!(!Registered.can_transition_to(Busy));
        assert!(!Registered.can_transition_to(Terminated));
        assert!(!Active.can_transition_to(Active));
        assert!(!Busy.can_transition_to(Busy));
        assert!(!Busy.can_transition_to(Suspended));
        // Terminal states have no outbound edges.
        assert!(!Rejected.can_transition_to(Active));
        assert!(!Terminated.can_transition_to(Active));
        assert!(!Retired.can_transition_to(Active));
    }

    #[test]
    fn agent_state_label_roundtrips_serde_screaming_snake_case() {
        let states = [
            AgentState::Registered,
            AgentState::Active,
            AgentState::Busy,
            AgentState::Suspended,
            AgentState::Rejected,
            AgentState::Terminated,
            AgentState::Retired,
        ];
        for s in states {
            assert_eq!(s.as_label(), format!("{s:?}").to_uppercase());
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(json, format!("\"{}\"", s.as_label()));
            let back: AgentState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn runtime_handle_id_default_is_fresh() {
        let a = RuntimeHandleId::default();
        let b = RuntimeHandleId::default();
        assert_ne!(a, b, "default RuntimeHandleId must mint fresh UUIDs");
    }

    #[test]
    fn attach_handle_round_trip_serde() {
        let h = AttachHandle {
            agent_id: Uuid::new_v4(),
            handle_id: RuntimeHandleId::new(),
            issued_at_unix: 1_700_000_000,
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: AttachHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn runtime_handle_clone_shares_event_bus() {
        // Build a handle, clone it, publish from the clone, observe on a
        // subscription owned by the original. Verifies that clone()
        // shares the broadcast sender (RFC-0011-c §9.3.2 run --detach
        // + attach pattern).
        let (tx, rx) = broadcast::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
        let h = RuntimeHandle::new(Uuid::new_v4(), Utc::now(), tx, rx);
        let cloned = h.clone();
        let mut rx = h.subscribe();
        cloned
            .publish(RuntimeEvent::Spawned {
                agent_id: cloned.agent_id,
                at: Utc::now(),
            })
            .unwrap();
        // Spawned event surfaces on the receiver.
        let ev = rx.try_recv().expect("event delivered");
        assert!(matches!(ev, RuntimeEvent::Spawned { .. }));
    }

    #[test]
    fn runtime_handle_is_last_clone_semantic() {
        // The handle is "last clone" when only one Arc<HandleInner>
        // remains. Dropping this handle will close the broadcast
        // channel.
        let (tx, rx) = broadcast::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
        let h = RuntimeHandle::new(Uuid::new_v4(), Utc::now(), tx, rx);
        assert!(h.is_last_clone());
        let cloned = h.clone();
        assert!(!h.is_last_clone());
        assert!(!cloned.is_last_clone());
        drop(h);
        assert!(cloned.is_last_clone());
    }

    #[test]
    fn runtime_handle_publish_does_not_require_revocable_semantic() {
        // In v0.1.0 there is no explicit revocation; the broadcast
        // channel closes when ALL clones drop. A single live handle
        // can always publish. Future missions may add an explicit
        // `revoke()` API that returns `HandleRevoked` from publish;
        // until then this test pins the v0.1.0 contract.
        let (tx, rx) = broadcast::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
        let h = RuntimeHandle::new(Uuid::new_v4(), Utc::now(), tx, rx);
        let res = h.publish(RuntimeEvent::Spawned {
            agent_id: h.agent_id,
            at: Utc::now(),
        });
        assert!(res.is_ok(), "v0.1.0 publish does not fail: {res:?}");
    }

    #[test]
    fn runtime_event_serde_tagged_round_trip() {
        let events = vec![
            RuntimeEvent::Spawned {
                agent_id: Uuid::new_v4(),
                at: Utc::now(),
            },
            RuntimeEvent::StateChanged {
                agent_id: Uuid::new_v4(),
                from: AgentState::Active,
                to: AgentState::Busy,
                at: Utc::now(),
            },
            RuntimeEvent::Log {
                agent_id: Uuid::new_v4(),
                line: "hello".into(),
                at: Utc::now(),
            },
            RuntimeEvent::Terminated {
                agent_id: Uuid::new_v4(),
                at: Utc::now(),
            },
        ];
        for ev in events {
            let json = serde_json::to_string(&ev).unwrap();
            let back: RuntimeEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ev);
        }
    }
}
