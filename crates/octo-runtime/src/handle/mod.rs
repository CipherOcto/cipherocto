//! `octo-runtime` substrate handle types + state machine contract.
//!
//! Per RFC-0011-c §9.1 Architecture + §9.10 Substrate `[ADD]` Signatures +
//! RFC-0011-c §Follow-on §F.1-§F.5 (AttachHandle token pathway):
//!
//! - `RuntimeHandle` — live runtime handle returned by `spawn_agent`
//! - `EventStream` — pub-sub event subscription returned by `attach`
//! - `RuntimeHandleBinding` — renamed 3-field in-process binding
//!   (the legacy `AttachHandle` per RFC-0011-c §9.3.2 --detach +
//!   --attach pattern, renamed per §F.2 Path B additive)
//! - `AttachHandle` — 6-field token (session_id, mint_timestamp_unix,
//!   ttl_unix, signature, payload, transport) per §F.2
//! - `AttachPayload`, `SessionId`, `Signature`, `Transport` —
//!   supporting types per §F.2
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
//! `RuntimeEvent` / `TransportKind` land via additive
//! `#[non_exhaustive]` clauses
//! ([[cipherocto-design-principles]] §Extension over enumeration).

// Submodules — added per RFC-0011-c §Follow-on §F.1-§F.5.
pub mod encoding;
pub mod error;
pub mod signing;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::error::RuntimeError;

// Re-export the `SessionId` type used by the pkg submodules so the
// rest of the crate can keep a single import path
// (`crate::handle::SessionId`).
pub use error::{AttachError, PersistenceError};

/// Random 32-byte session identifier (RFC-0011-c §F.2).
///
/// Minted per `spawn_agent` call; threaded through the
/// `RuntimeHandleBinding` + `AttachHandle` token so a follow-on
/// `attach --since` can resolve back to the same session.
pub type SessionId = [u8; 32];

/// Ed25519 signature newtype (RFC-0011-c §F.5).
///
/// Local wrapper around the raw 64 signature bytes — Layer A
/// primitive `ed25519_dalek::Signature` is re-exported via the
/// `octo-wallet` path dep and wrapped at this boundary so the Layer
/// A/B seam stays clean per [[no-central-enums-for-extension-bearing-types]] +
/// [[stable-abstractions-principle]] (Layer A primitives stable;
/// business semantics in composed Layer B).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Signature(#[serde(with = "signature_bytes_serde")] pub [u8; 64]);

/// Serde adapter for `[u8; 64]` — required because `serde`'s
/// `derive` feature only ships array support up to length 32
/// (`[T; 0]` .. `[T; 32]`); the substrate-visible 64-byte
/// signature exceeds that bound. The adapter composes the
/// workspace-standard `serde_bytes::ByteArray` for the actual
/// serialization call so the wire form matches the workspace
/// convention (length-prefixed `[u8]` sequence, the most
/// widely-supported JSON form for fixed-width byte strings).
mod signature_bytes_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_bytes::ByteArray;

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], ser: S) -> Result<S::Ok, S::Error> {
        ByteArray::new(*bytes).serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<[u8; 64], D::Error> {
        let ba: ByteArray<64> = ByteArray::deserialize(de)?;
        Ok(ba.into_array())
    }
}

impl From<octo_wallet::ed25519_dalek::Signature> for Signature {
    fn from(s: octo_wallet::ed25519_dalek::Signature) -> Self {
        Self(s.to_bytes())
    }
}

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

/// Per-token payload (RFC-0011-c §F.2).
///
/// `agent_id` identifies the spawned agent; `since_cursor` is the
/// lower-bound event cursor for replay (mirrors `attach --since`
/// semantics per RFC-0011-c §9.3.5).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachPayload {
    /// Agent id this token authorizes.
    pub agent_id: Uuid,
    /// Lower-bound event cursor (RFC-0011-c §9.3.5 `--since` analog).
    pub since_cursor: u64,
}

/// Transport selector for an `AttachHandle` token (RFC-0011-c §F.2 +
/// §F.1).
///
/// Discriminated by `kind`; `addr` carries an optional per-variant
/// transport address (e.g., Unix socket path). The
/// `#[non_exhaustive]` attribute on `TransportKind` makes future
/// extension surfaces (per [[cipherocto-design-principles]]
/// §Extension over enumeration) additive without central-enum edits.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transport {
    /// Transport kind discriminator.
    pub kind: TransportKind,
    /// Optional per-variant address (e.g., socket path).
    pub addr: Option<String>,
}

/// Transport kind enum (RFC-0011-c §F.2).
///
/// `#[non_exhaustive]` per [[cipherocto-design-principles]]
/// §Extension over enumeration. Extension surfaces land via the
/// `Raw(Uuid)` escape hatch (typed UUID discriminator) rather than
/// via central-enum edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TransportKind {
    /// In-process broadcast binding (Phase 1 substrate).
    InProcess,
    /// Unix-domain socket binding per `XDG_RUNTIME_DIR/octo-attach-<session_id>.sock`
    /// (with `std::env::temp_dir()` fallback per RFC-0011-c §F.2).
    UnixSocket,
    /// Escape hatch for extension transports (carries a typed UUID
    /// `scheme_id` per [[cipherocto-design-principles]] §Extension
    /// over enumeration). Old code fails closed on unknown
    /// discriminators.
    Raw(Uuid),
}

impl Transport {
    /// In-process transport (no address).
    pub const IN_PROCESS: Self = Self {
        kind: TransportKind::InProcess,
        addr: None,
    };

    /// Unix-domain-socket transport with the given path.
    #[must_use]
    pub fn unix_socket(path: impl Into<String>) -> Self {
        Self {
            kind: TransportKind::UnixSocket,
            addr: Some(path.into()),
        }
    }

    /// Raw escape-hatch transport with the given scheme UUID and
    /// optional address. Used by future extension crates per
    /// [[cipherocto-design-principles]] §Extension over enumeration.
    #[must_use]
    pub fn raw(scheme_id: Uuid, addr: Option<String>) -> Self {
        Self {
            kind: TransportKind::Raw(scheme_id),
            addr,
        }
    }
}

/// Six-field `AttachHandle` token (RFC-0011-c §F.2).
///
/// Carries the canonical signed-bytes payload + signature + transport
/// for a session. Used by the `octo agent run --detach` + `octo
/// agent attach --since` pattern across process boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachHandle {
    /// Session id this token is bound to (RFC-0011-c §F.2 + §F.5; the
    /// signature's pre-image binds this field).
    pub session_id: SessionId,
    /// Unix seconds when the token was minted.
    pub mint_timestamp_unix: u64,
    /// Unix seconds at which the token expires (inclusive).
    pub ttl_unix: u64,
    /// Ed25519 signature over `session_id || payload || mint_timestamp_unix || ttl_unix`.
    pub signature: Signature,
    /// Per-token payload (agent_id + since_cursor).
    pub payload: AttachPayload,
    /// Transport selector (in-process / unix socket / raw escape hatch).
    pub transport: Transport,
}

/// Renamed 3-field in-process binding (RFC-0011-c §F.2 Path B).
///
/// Per §F.2 the legacy 3-field `AttachHandle` struct (used by
/// `RuntimeHandle::attach_handle()` and `spawn_agent(attach_handle)`)
/// is renamed to `RuntimeHandleBinding`; the 6-field `AttachHandle`
/// token lives alongside at the `octo_runtime::handle` module per
/// [[no-parallel-abstractions]]. The existing in-process `attach`
/// substrate path (RFC-0011-c §9.3.5) is UNCHANGED — it still
/// consumes this renamed struct.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuntimeHandleBinding {
    /// Agent id the binding authorizes.
    pub agent_id: Uuid,
    /// Handle id (binds the attach to the matching spawn).
    pub handle_id: RuntimeHandleId,
    /// Session id (RFC-0011-c §F.2; random per `spawn_agent` call).
    pub session_id: SessionId,
    /// Unix seconds when the spawn occurred (mirrors the legacy
    /// `issued_at_unix: i64` field for substrate compatibility).
    pub spawned_at_unix: i64,
}

impl RuntimeHandleBinding {
    /// Build a binding from a runtime handle.
    #[must_use]
    pub fn from_runtime_handle(handle: &RuntimeHandle) -> Self {
        Self {
            agent_id: handle.agent_id,
            handle_id: handle.handle_id,
            session_id: handle.session_id,
            spawned_at_unix: handle.spawned_at.timestamp(),
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
    /// Session id (RFC-0011-c §F.2; random per `spawn_agent` call;
    /// bound to the `AttachHandle` token + revocation set).
    pub session_id: SessionId,
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
            // RFC-0011-c §F.2: a fresh session id is minted per
            // `spawn_agent` call (currently using a deterministic
            // placeholder; future amendments may swap in an HSM-
            // routed random source per the RFC's substrate guidance).
            session_id: derive_session_id(&agent_id, spawned_at),
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
    pub fn publish(&self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        let _ = self.inner.event_tx.send(event);
        Ok(())
    }

    /// `true` iff all peer `RuntimeHandle` clones have been dropped.
    #[must_use]
    pub fn is_revoked(&self) -> bool {
        self.is_last_clone()
    }

    /// `true` iff this is the only live `RuntimeHandle` for the
    /// underlying broadcast channel.
    #[must_use]
    pub fn is_last_clone(&self) -> bool {
        Arc::strong_count(&self.inner) <= 1
    }

    /// Build a renamed in-process binding for this handle
    /// (RFC-0011-c §F.2 Path B — formerly
    /// `RuntimeHandle::attach_handle()`).
    #[must_use]
    pub fn runtime_handle_binding(&self) -> RuntimeHandleBinding {
        RuntimeHandleBinding::from_runtime_handle(self)
    }
}

/// Derive a deterministic session id from `(agent_id, spawned_at)`.
///
/// Phase 1 derives the session id from the agent id + spawn
/// timestamp via BLAKE3 (sufficient for substrate tests +
/// revocation-set registration). Future amendments may swap in a
/// CSPRNG-backed random source per RFC-0011-c §F.2.
fn derive_session_id(agent_id: &Uuid, spawned_at: DateTime<Utc>) -> SessionId {
    let mut input = Vec::with_capacity(16 + 8 + 4);
    input.extend_from_slice(agent_id.as_bytes());
    input.extend_from_slice(&spawned_at.timestamp().to_be_bytes());
    input.extend_from_slice(&spawned_at.timestamp_subsec_nanos().to_be_bytes());
    let hash = blake3::hash(&input);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

impl std::fmt::Debug for RuntimeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeHandle")
            .field("handle_id", &self.handle_id)
            .field("session_id", &hex::encode(self.session_id))
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
            Err(broadcast::error::RecvError::Lagged(skipped)) => Ok(RuntimeEvent::Log {
                agent_id: self.agent_id,
                line: format!("event stream lagged: skipped {skipped} events"),
                at: Utc::now(),
            }),
        }
    }
}

/// Attached-session return type (RFC-0011-c §F.2).
///
/// Returned by `attach_with_token`; consumed by the CLI to drive the
/// `octo agent attach --since` replay surface across process
/// boundaries.
#[derive(Debug)]
pub struct AttachedSession {
    /// Event cursor at attach time (lower-bound replay pointer).
    pub event_cursor: u64,
    /// Broadcast receiver for the runtime events.
    pub broadcast_rx: tokio::sync::broadcast::Receiver<RuntimeEvent>,
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
        assert!(Registered.can_transition_to(Active));
        assert!(Registered.can_transition_to(Rejected));
        assert!(Active.can_transition_to(Busy));
        assert!(Active.can_transition_to(Suspended));
        assert!(Active.can_transition_to(Terminated));
        assert!(Busy.can_transition_to(Active));
        assert!(Busy.can_transition_to(Terminated));
        assert!(Suspended.can_transition_to(Active));
        assert!(Suspended.can_transition_to(Retired));

        assert!(!Registered.can_transition_to(Busy));
        assert!(!Registered.can_transition_to(Terminated));
        assert!(!Active.can_transition_to(Active));
        assert!(!Busy.can_transition_to(Busy));
        assert!(!Busy.can_transition_to(Suspended));
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
    fn runtime_handle_binding_round_trip_serde() {
        let h = RuntimeHandleBinding {
            agent_id: Uuid::new_v4(),
            handle_id: RuntimeHandleId::new(),
            session_id: [0xab; 32],
            spawned_at_unix: 1_700_000_000,
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: RuntimeHandleBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn attach_handle_round_trip_serde() {
        let token = AttachHandle {
            session_id: [0xab; 32],
            mint_timestamp_unix: 1_700_000_000,
            ttl_unix: 1_700_003_600,
            signature: Signature([0x42; 64]),
            payload: AttachPayload {
                agent_id: Uuid::from_bytes([0xcd; 16]),
                since_cursor: 7,
            },
            transport: Transport::unix_socket("/tmp/octo-attach.sock"),
        };
        let json = serde_json::to_string(&token).unwrap();
        let back: AttachHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(back, token);
    }

    #[test]
    fn transport_in_process_constructs_correctly() {
        let t = Transport::IN_PROCESS;
        assert_eq!(t.kind, TransportKind::InProcess);
        assert!(t.addr.is_none());
    }

    #[test]
    fn transport_unix_socket_constructs_correctly() {
        let t = Transport::unix_socket("/tmp/octo-attach.sock");
        assert_eq!(t.kind, TransportKind::UnixSocket);
        assert_eq!(t.addr.as_deref(), Some("/tmp/octo-attach.sock"));
    }

    #[test]
    fn transport_raw_constructs_correctly() {
        let scheme_id = Uuid::from_bytes([0x42; 16]);
        let t = Transport::raw(scheme_id, Some("custom://addr".into()));
        assert_eq!(t.kind, TransportKind::Raw(scheme_id));
        assert_eq!(t.addr.as_deref(), Some("custom://addr"));
    }

    #[test]
    fn signature_newtype_roundtrip() {
        let s = Signature([0xa5; 64]);
        let json = serde_json::to_string(&s).unwrap();
        let back: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn signature_from_dalek_signature() {
        use octo_wallet::ed25519_dalek::{Signer, SigningKey};
        let sk = SigningKey::from_bytes(&[0x42u8; 32]);
        let dalek_sig = sk.sign(b"hello");
        let s: Signature = dalek_sig.into();
        assert_eq!(s.0.len(), 64);
    }

    #[test]
    fn runtime_handle_clone_shares_event_bus() {
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
        let ev = rx.try_recv().expect("event delivered");
        assert!(matches!(ev, RuntimeEvent::Spawned { .. }));
    }

    #[test]
    fn runtime_handle_is_last_clone_semantic() {
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

    #[test]
    fn runtime_handle_binding_from_runtime_handle_preserves_session_id() {
        let (tx, rx) = broadcast::channel::<RuntimeEvent>(EVENT_CHANNEL_CAPACITY);
        let h = RuntimeHandle::new(Uuid::new_v4(), Utc::now(), tx, rx);
        let binding = h.runtime_handle_binding();
        assert_eq!(binding.session_id, h.session_id);
        assert_eq!(binding.agent_id, h.agent_id);
        assert_eq!(binding.handle_id, h.handle_id);
        assert_eq!(binding.spawned_at_unix, h.spawned_at.timestamp());
    }
}
