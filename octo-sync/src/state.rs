//! 7-state `SyncLifecycle` enum and transition table (per RFC-0862 §Lifecycle Requirements).
//!
//! The cipherocto sync engine has 7 states per peer (vs. the 8-state `CoordinatorLifecycle`
//! in RFC-0855p-b). Sync does not exercise the `Handover` state (a coordinator-only
//! state per RFC-0855p-b); v1 has no auto-failover. The 7-state machine is the
//! minimal state set that satisfies v1 requirements without introducing
//! coordinator-only states that v1 doesn't use.

/// The 7-state per-peer sync lifecycle (per RFC-0862 §Lifecycle Requirements).
///
/// # State machine
///
/// ```text
///        ┌────────┐
///   [*]─►│  Init  │
///        └───┬────┘
///            │ local config matches
///            ▼
///        ┌────────────┐  3 × connect_timeout   ┌─────────────┐
///        │ Connecting ├───────────────────────►│ Terminated  │
///        └─────┬──────┘                        └─────────────┘
///              │ TCP/TLS handshake                    ▲
///              ▼                                       │
///        ┌────────────────┐ sig invalid / pk mismatch  │
///        │ Authenticating ├───────────────────────────┤
///        └────────┬───────┘                           │
///                 │ signature valid, pk matches      │
///                 ▼                                   │
///        ┌────────────┐ no heartbeat > 2×interval    │
///        │ Streaming  ├──────────────────┐           │
///        └────┬───────┘                  │           │
///             │                          ▼           │
///             │                  ┌────────────┐      │
///             │ LSN regression / │  Suspect   │      │
///             │ epoch rollback  └─────┬──────┘      │
///             │                        │ reconnect  │
///             │                        ▼            │
///             │                  ┌─────────────┐    │
///             │                  │Reconnecting │    │
///             │                  └──────┬──────┘    │
///             │                         │ backoff   │
///             │                         ▼           │
///             │                  5 × reconnect      │
///             └────────────────►  attempts ─────────┘
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SyncLifecycle {
    /// Initial state. The local config is being validated against the mission.
    Init,
    /// TCP/TLS handshake in progress.
    Connecting,
    /// Signature verification in progress.
    Authenticating,
    /// Active WAL streaming. The default steady state.
    Streaming,
    /// No heartbeat for `2 × heartbeat_interval` (10s). Investigation pending.
    Suspect,
    /// Attempting to reconnect after a network blip.
    Reconnecting,
    /// Terminal state. The peer has been disconnected; the cipherocto sync
    /// engine will not attempt to reconnect. Triggered by LSN regression,
    /// identity_epoch rollback, signature failure, or 5 failed reconnect
    /// attempts (~5 min).
    Terminated,
}

impl SyncLifecycle {
    /// Return `true` if this state is a terminal state (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(self, SyncLifecycle::Terminated)
    }

    /// Return `true` if this state is an active state (i.e., the peer is
    /// receiving WAL chunks).
    pub fn is_active(self) -> bool {
        matches!(self, SyncLifecycle::Streaming)
    }

    /// Return `true` if this state is a transient fault (suspect / reconnecting).
    pub fn is_transient_fault(self) -> bool {
        matches!(self, SyncLifecycle::Suspect | SyncLifecycle::Reconnecting)
    }

    /// Return the human-readable name of this state.
    pub fn name(self) -> &'static str {
        match self {
            SyncLifecycle::Init => "Init",
            SyncLifecycle::Connecting => "Connecting",
            SyncLifecycle::Authenticating => "Authenticating",
            SyncLifecycle::Streaming => "Streaming",
            SyncLifecycle::Suspect => "Suspect",
            SyncLifecycle::Reconnecting => "Reconnecting",
            SyncLifecycle::Terminated => "Terminated",
        }
    }
}

/// A transition between two `SyncLifecycle` states.
///
/// The transition table is the canonical source of truth for the per-peer state
/// machine; any code that wants to transition a peer MUST go through [`Peer::transition`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateTransition {
    /// The state being transitioned from.
    pub from: SyncLifecycle,
    /// The state being transitioned to.
    pub to: SyncLifecycle,
    /// The trigger that causes the transition.
    pub trigger: TransitionTrigger,
}

/// Triggers for state transitions (per RFC-0862 §Lifecycle Requirements).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionTrigger {
    /// Local config matches the mission.
    LocalConfigMatched,
    /// TCP/TLS handshake completed.
    TlsHandshakeComplete,
    /// 3 × connect_timeout exceeded.
    ConnectTimeoutExceeded,
    /// Signature is valid and the public key matches.
    SignatureValid,
    /// Signature is invalid, OR the public key does not match.
    SignatureInvalid,
    /// No heartbeat for `2 × heartbeat_interval` (10s).
    HeartbeatTimeout,
    /// An LSN regression was detected (`entry.lsn < previous_lsn + 1`).
    LsnRegression,
    /// The peer's `identity_epoch` rolled back.
    IdentityEpochRollback,
    /// The reconnect interval elapsed.
    ReconnectIntervalElapsed,
    /// 5 × reconnect attempts failed.
    ReconnectAttemptsExhausted,
    /// The mission has been Terminated.
    MissionTerminated,
}

impl StateTransition {
    /// Return `true` if this transition is allowed by the per-peer state machine.
    ///
    /// This is the canonical transition table from RFC-0862 §Lifecycle
    /// Requirements. Any transition not in this table MUST be rejected.
    pub fn is_allowed(&self) -> bool {
        use SyncLifecycle::*;
        use TransitionTrigger::*;
        matches!(
            (self.from, self.to, self.trigger),
            // Init → Connecting
            (Init, Connecting, LocalConfigMatched)
            // Connecting → Authenticating
            | (Connecting, Authenticating, TlsHandshakeComplete)
            // Connecting → Terminated (3 × connect_timeout)
            | (Connecting, Terminated, ConnectTimeoutExceeded)
            // Authenticating → Streaming
            | (Authenticating, Streaming, SignatureValid)
            // Authenticating → Terminated
            | (Authenticating, Terminated, SignatureInvalid)
            // Streaming → Suspect
            | (Streaming, Suspect, HeartbeatTimeout)
            // Streaming → Terminated
            | (Streaming, Terminated, LsnRegression)
            | (Streaming, Terminated, IdentityEpochRollback)
            | (Streaming, Terminated, MissionTerminated)
            // Suspect → Reconnecting
            | (Suspect, Reconnecting, ReconnectIntervalElapsed)
            // Reconnecting → Connecting
            | (Reconnecting, Connecting, ReconnectIntervalElapsed)
            // Reconnecting → Terminated (5 × reconnect_attempts)
            | (Reconnecting, Terminated, ReconnectAttemptsExhausted)
        )
    }
}

/// The per-peer state record held by the cipherocto sync engine.
///
/// # LSN tracking
///
/// The per-peer LSN watermark (highest LSN that has been acknowledged) is
/// held in `WalTailStreamer::peers: HashMap<SyncPeerId, LsnTracker>` (the
/// single source of truth). This struct does NOT duplicate the watermark;
/// it only tracks the lifecycle state and the last heartbeat timestamp.
#[derive(Clone, Debug)]
pub struct Peer {
    /// The peer's `SyncPeerId`.
    pub peer_id: crate::identity::SyncPeerId,
    /// The peer's current lifecycle state.
    pub state: SyncLifecycle,
    /// The peer's last heartbeat timestamp (Unix seconds).
    pub last_heartbeat_unix: u64,
}

impl Peer {
    /// Create a new `Peer` in the `Init` state.
    pub fn new(peer_id: crate::identity::SyncPeerId) -> Self {
        Self {
            peer_id,
            state: SyncLifecycle::Init,
            last_heartbeat_unix: 0,
        }
    }

    /// Attempt to transition this peer to `to` with the given `trigger`.
    ///
    /// Returns `Ok(new_state)` on success, or `Err(SyncError::InvalidStateTransition)`
    /// if the transition is not in the canonical transition table (RFC-0862
    /// §Lifecycle Requirements). The peer's state is unchanged on error.
    ///
    /// Invalid transitions are a sign of a bug in the cipherocto sync engine
    /// (e.g., trying to transition from `Init` to `Streaming` without going
    /// through `Connecting` / `Authenticating`). They MUST be surfaced to the
    /// caller so the engine can emit a tracing event and (in production)
    /// transition the peer to `Terminated` per the RFC.
    pub fn transition(
        &mut self,
        to: SyncLifecycle,
        trigger: TransitionTrigger,
    ) -> Result<SyncLifecycle, crate::error::SyncError> {
        let t = StateTransition {
            from: self.state,
            to,
            trigger,
        };
        if t.is_allowed() {
            self.state = to;
            Ok(self.state)
        } else {
            // Invalid transition: surface to the caller. The caller decides
            // what to do (typically: log via tracing, transition to
            // Terminated, increment a metrics counter).
            Err(crate::error::SyncError::InvalidStateTransition {
                from: self.state,
                to,
                trigger,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SyncPeerId;

    #[test]
    fn happy_path_init_to_streaming() {
        let mut p = Peer::new(SyncPeerId([0u8; 32]));
        assert_eq!(p.state, SyncLifecycle::Init);
        p.transition(
            SyncLifecycle::Connecting,
            TransitionTrigger::LocalConfigMatched,
        )
        .unwrap();
        p.transition(
            SyncLifecycle::Authenticating,
            TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
        p.transition(SyncLifecycle::Streaming, TransitionTrigger::SignatureValid)
            .unwrap();
        assert_eq!(p.state, SyncLifecycle::Streaming);
    }

    #[test]
    fn streaming_to_terminated_on_lsn_regression() {
        let mut p = Peer::new(SyncPeerId([0u8; 32]));
        p.transition(
            SyncLifecycle::Connecting,
            TransitionTrigger::LocalConfigMatched,
        )
        .unwrap();
        p.transition(
            SyncLifecycle::Authenticating,
            TransitionTrigger::TlsHandshakeComplete,
        )
        .unwrap();
        p.transition(SyncLifecycle::Streaming, TransitionTrigger::SignatureValid)
            .unwrap();
        p.transition(SyncLifecycle::Terminated, TransitionTrigger::LsnRegression)
            .unwrap();
        assert_eq!(p.state, SyncLifecycle::Terminated);
        assert!(p.state.is_terminal());
    }

    #[test]
    fn connecting_terminates_on_timeout() {
        let mut p = Peer::new(SyncPeerId([0u8; 32]));
        p.transition(
            SyncLifecycle::Connecting,
            TransitionTrigger::LocalConfigMatched,
        )
        .unwrap();
        p.transition(
            SyncLifecycle::Terminated,
            TransitionTrigger::ConnectTimeoutExceeded,
        )
        .unwrap();
        assert!(p.state.is_terminal());
    }

    #[test]
    fn invalid_transition_is_rejected() {
        let mut p = Peer::new(SyncPeerId([0u8; 32]));
        // Cannot go Init → Streaming directly (must go through Connecting)
        let err = p
            .transition(SyncLifecycle::Streaming, TransitionTrigger::SignatureValid)
            .unwrap_err();
        // State should be unchanged
        assert_eq!(p.state, SyncLifecycle::Init);
        // The error carries the from/to/trigger for diagnostics
        match err {
            crate::error::SyncError::InvalidStateTransition { from, to, .. } => {
                assert_eq!(from, SyncLifecycle::Init);
                assert_eq!(to, SyncLifecycle::Streaming);
            }
            _ => panic!("expected InvalidStateTransition, got {:?}", err),
        }
    }

    #[test]
    fn state_predicates() {
        assert!(SyncLifecycle::Terminated.is_terminal());
        assert!(!SyncLifecycle::Streaming.is_terminal());
        assert!(SyncLifecycle::Streaming.is_active());
        assert!(SyncLifecycle::Suspect.is_transient_fault());
        assert!(SyncLifecycle::Reconnecting.is_transient_fault());
        assert!(!SyncLifecycle::Streaming.is_transient_fault());
    }
}
