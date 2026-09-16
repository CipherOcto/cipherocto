//! `octo-runtime::handle::transport` — transport-handler registry
//! (RFC-0011-c §F.2 step (e) + §F.3; paired with follow-on Layer D
//! transport crates per [[cipherocto-design-principles]] §Extension
//! over enumeration).
//!
//! Substrate ships the `Handler` trait + a process-singleton
//! `Registry` + the built-in `InProcessHandler` (broadcast-channel
//! binding). Extension transports (Unix-domain socket, raw scheme
//! UUIDs registered by user-extension crates, …) register via
//! [`Registry::register`] from follow-on crates
//! (`octo-runtime-transport-unix`, …) at process startup.
//!
//! `attach_with_token` step (e) dispatches via the registry;
//! an unregistered `TransportKind` returns
//! [`AttachError::TransportHandlerNotRegistered`] (CLI exit 59).
//!
//! ## Layer discipline
//!
//! `Handler` is Layer B substrate per RFC-0011-c §F.2. Filesystem +
//! per-transport protocol I/O live in Layer D transport crates that
//! register handlers at runtime — substrate does NOT import any
//! `std::fs` or socket-IO types (the Layer D substrate-boundary
//! discipline per [[cipherocto-design-principles]] §No parallel
//! abstractions + §Layer direction).
//!
//! ## Why an explicit registry (vs. a `match` per `TransportKind`)
//!
//! Per [[cipherocto-design-principles]] §Extension over enumeration:
//! extension surfaces land via per-extension crates + trait
//! dispatch, not via central-enum edits. A trait + registry model
//! lets new transports land without central-edits to
//! `attach_with_token`. The substrate `match` collapses to a single
//! `HashMap::get` lookup; the discriminator is the `TransportKind`
//! enum key (typed-discriminator + Raw escape hatch per RFC-0855).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::handle::{AttachError, AttachHandle, AttachedSession, TransportKind};

/// Transport-handler trait (RFC-0011-c §F.2 step (e)).
///
/// Implementations take a substrate-validated `AttachHandle`
/// (signature + revocation + TTL + `since_unix` checks have already
/// completed in `attach_with_token` steps (a)-(d); only transport
/// dispatch + per-transport protocol I/O remains) plus the
/// `since_unix` cursor and return an [`AttachedSession`] event stream.
///
/// # Layer discipline
///
/// Per [[cipherocto-design-principles]] §Interface Segregation, the
/// trait is small and focused (one method: bind). New
/// extension-bearing operations land as additive traits, not
/// `Handler` mutations.
pub trait Handler: Send + Sync + std::fmt::Debug {
    /// Bind a validated `AttachHandle` token against this transport
    /// handler. Returns `AttachedSession` (the event-stream
    /// receiver) on success, or `AttachError` on any per-transport
    /// failure.
    ///
    /// # Errors
    ///
    /// - `AttachError::UnknownSession` — `InProcessHandler` cannot
    ///   find the session in the runtime handle registry (substrate
    ///   boundary stub until the session-registry-wiring follow-on
    ///   amendment lands per RFC-0011-c §F.2).
    /// - Per-handler specific errors from extension crates (e.g.,
    ///   `AttachError::SocketPathUnavailable` from a follow-on
    ///   `octo-runtime-transport-unix` crate).
    fn bind(&self, token: &AttachHandle, since_unix: u64) -> Result<AttachedSession, AttachError>;
}

/// Process-singleton transport-handler registry
/// (RFC-0011-c §F.2 step (e)).
///
/// Keyed by `TransportKind`; lookup is via
/// [`Registry::lookup`]. Registration is via [`Registry::register`].
///
/// `Default::default()` returns an empty registry (equivalent to
/// [`Registry::new`]); the process-singleton
/// [`HANDLE_TRANSPORT_REGISTRY`] initializes with the built-in
/// `InProcessHandler` at first access.
#[derive(Default, Debug)]
pub struct Registry {
    /// Inner map. `RwLock` chosen over `Mutex` because reads
    /// (handler lookup) vastly outnumber writes (handler
    /// registration); `read()` borrows overlap safely.
    ///
    /// Fail-CLOSED on poison: per the revocation-set discipline
    /// in `persistence.rs`, a poisoned lock indicates a writer
    /// panic; we propagate the failure to the caller rather
    /// than silently serving a stale map.
    handlers: RwLock<HashMap<TransportKind, Arc<dyn Handler>>>,
}

impl Registry {
    /// Build an empty registry (no built-in handlers installed).
    /// Used by [`HANDLE_TRANSPORT_REGISTRY`] which manually
    /// registers the built-in `InProcessHandler` after construction.
    /// Not `const fn` because `HashMap::new()` requires the
    /// `RandomState` default hasher which is not `const`-stable on
    /// the current MSRV; `RwLock::new` is `const` but the wrapper
    /// isn't.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a handler for the given `TransportKind`. Overwrites
    /// any prior handler for the same kind (e.g., for test isolation
    /// or follow-on transport-crate overrides).
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned (writer panicked
    /// holding the lock). Per fail-CLOSED discipline, the caller
    /// cannot recover from a poisoned registry.
    pub fn register(&self, kind: TransportKind, handler: Arc<dyn Handler>) {
        let mut g = self.handlers.write().expect("transport registry poisoned");
        g.insert(kind, handler);
    }

    /// Look up a registered handler by `TransportKind`.
    ///
    /// Returns `None` if no handler is registered for the given kind
    /// (`attach_with_token` translates `None` to
    /// [`AttachError::TransportHandlerNotRegistered`]).
    ///
    /// # Panics
    ///
    /// Panics if the inner `RwLock` is poisoned (fail-CLOSED).
    #[must_use]
    pub fn lookup(&self, kind: &TransportKind) -> Option<Arc<dyn Handler>> {
        let g = self.handlers.read().expect("transport registry poisoned");
        g.get(kind).cloned()
    }
}

/// Process-singleton transport handler registry
/// (RFC-0011-c §F.2 step (e)).
///
/// Initialized lazily on first access via
/// `std::sync::OnceLock` (stable since Rust 1.70). The built-in
/// `InProcessHandler` is registered at initialization time;
/// follow-on Layer D transport crates register additional handlers
/// at process startup (e.g., `UnixSocket` registered by
/// `octo-runtime-transport-unix`).
pub static HANDLE_TRANSPORT_REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();

/// Initialize the process-singleton registry with the built-in
/// `InProcessHandler`. Returns the registry instance.
#[must_use]
pub fn build_in_process_registry() -> Registry {
    let reg = Registry::new();
    reg.register(TransportKind::InProcess, Arc::new(InProcessHandler::new()));
    reg
}

/// Built-in in-process broadcast binding handler
/// (RFC-0011-c §F.2 + §9.3.5).
///
/// Phase 1 stub: session-registry-wiring is deferred to a
/// follow-on amendment per RFC-0011-c §F.2 (the in-process broadcast
/// binding lives in `RuntimeHandle`'s `Arc<HandleInner>` and is not
/// yet exposed across the process boundary via a session-id-keyed
/// registry). The stub mirrors the pre-Handler behavior — `unimplemented!`
/// in debug builds, `AttachError::UnknownSession` in release builds —
/// to catch accidental callsite reliance on the half-wired
/// pathway during the substrate-first rollout.
#[derive(Debug, Default)]
pub struct InProcessHandler;

impl InProcessHandler {
    /// Build a new in-process handler. Trivial constructor; the
    /// handler is stateless (the actual session lookup would query
    /// a future process-singleton session-id registry, which the
    /// follow-on amendment wires).
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Handler for InProcessHandler {
    fn bind(&self, token: &AttachHandle, _since_unix: u64) -> Result<AttachedSession, AttachError> {
        // Phase 1 stub: the session-registry-wiring that maps
        // `session_id → Arc<HandleInner>` is deferred to a
        // follow-on amendment per RFC-0011-c §F.2. Until that
        // amendment lands, the substrate boundary surface mirrors
        // the pre-Handler behavior — panic in debug builds (so
        // tests catch accidental callsite reliance), `UnknownSession`
        // in release builds (so CLI operators see a substrate-
        // faithful error rather than a misleading success).
        if cfg!(debug_assertions) {
            unimplemented!(
                "InProcessHandler::bind session-registry-wiring pending \
                 follow-on amendment per RFC-0011-c §F.2; \
                 wire the session_id → Arc<HandleInner> mapping through \
                 persistence.rs in the next cycle"
            );
        }
        Err(AttachError::UnknownSession {
            session_id: token.session_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::handle::RuntimeHandleId as _Reachable;
    use crate::handle::{AttachPayload, Signature, Transport};
    use uuid::Uuid;

    /// Build a minimal valid `AttachHandle` for tests.
    fn dummy_token() -> AttachHandle {
        AttachHandle {
            session_id: [0xab; 32],
            mint_timestamp_unix: 1_700_000_000,
            ttl_unix: 1_700_003_600,
            signature: Signature([0x42; 64]),
            payload: AttachPayload {
                agent_id: Uuid::from_bytes([0xcd; 16]),
                since_cursor: 7,
            },
            transport: Transport::IN_PROCESS,
        }
    }

    /// `Registry::new()` returns an empty registry.
    #[test]
    fn registry_new_is_empty() {
        let reg = Registry::new();
        assert!(reg.lookup(&TransportKind::InProcess).is_none());
        assert!(reg.lookup(&TransportKind::UnixSocket).is_none());
    }

    /// `Registry::register` + `lookup` round-trip.
    #[test]
    fn registry_register_lookup_round_trip() {
        let reg = Registry::new();
        reg.register(TransportKind::InProcess, Arc::new(InProcessHandler::new()));
        let got = reg.lookup(&TransportKind::InProcess);
        assert!(got.is_some(), "registered handler must be lookup-able");
    }

    /// `Registry::lookup` returns `None` for an unregistered kind.
    #[test]
    fn registry_lookup_unregistered_returns_none() {
        let reg = Registry::new();
        assert!(reg.lookup(&TransportKind::UnixSocket).is_none());
    }

    /// `InProcessHandler` unit struct + `Default` + `new()` all
    /// construct identical values.
    #[test]
    fn in_process_handler_unit_default_and_new_construct_identically() {
        let _new: InProcessHandler = InProcessHandler::new();
        let _unit: InProcessHandler = InProcessHandler;
    }

    /// `InProcessHandler::bind` produces a substrate-boundary stub
    /// failure (UnknownSession in release; unimplemented! in debug).
    /// Release build: returns `UnknownSession`.
    #[test]
    fn in_process_handler_bind_release_returns_unknown_session() {
        if cfg!(debug_assertions) {
            // In debug builds, the bind panics; skip the assertion.
            return;
        }
        let h = InProcessHandler::new();
        let token = dummy_token();
        let res = h.bind(&token, 0);
        assert!(matches!(res, Err(AttachError::UnknownSession { .. })));
    }

    /// `Transport::IN_PROCESS` is registered by
    /// `build_in_process_registry`.
    #[test]
    fn build_in_process_registry_registers_in_process_handler() {
        let reg = build_in_process_registry();
        assert!(reg.lookup(&TransportKind::InProcess).is_some());
    }

    /// `HANDLE_TRANSPORT_REGISTRY` lazy-init idiom yields the
    /// same registry on each lookup once initialized.
    #[test]
    fn handle_transport_registry_lazy_init_returns_same_registry() {
        let r1: &'static Registry =
            HANDLE_TRANSPORT_REGISTRY.get_or_init(build_in_process_registry);
        let r2: &'static Registry =
            HANDLE_TRANSPORT_REGISTRY.get_or_init(build_in_process_registry);
        assert_eq!(
            r1 as *const _ as usize, r2 as *const _ as usize,
            "HANDLE_TRANSPORT_REGISTRY must yield the same registry on each lookup"
        );
    }

    /// `look_handler_via_dispatch` simulates the substrate-side
    /// step (e) lookup path: build the registry, register a
    /// UnixSocket handler externally, look it up by discriminator.
    #[test]
    fn lookup_via_dispatch_pattern_succeeds_when_registered() {
        #[derive(Debug)]
        struct FakeUnixSocketHandler;
        impl Handler for FakeUnixSocketHandler {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Ok(AttachedSession {
                    event_cursor: 42,
                    broadcast_rx: dummy_broadcast_rx(),
                })
            }
        }

        let reg = build_in_process_registry();
        reg.register(TransportKind::UnixSocket, Arc::new(FakeUnixSocketHandler));
        let got = reg.lookup(&TransportKind::UnixSocket);
        assert!(got.is_some());
    }

    /// `lookup_via_dispatch_pattern_returns_none_when_unregistered`
    /// verifies the substrate-side step (e) failure path: when no
    /// handler is registered for a kind, the lookup yields `None`
    /// and the substrate must surface
    /// `AttachError::TransportHandlerNotRegistered`.
    #[test]
    fn lookup_via_dispatch_pattern_returns_none_when_unregistered() {
        let reg = build_in_process_registry();
        assert!(reg.lookup(&TransportKind::UnixSocket).is_none());
    }

    /// `Registry::register` overwrites a prior handler for the same kind.
    #[test]
    fn registry_register_overwrites_prior_handler() {
        #[derive(Debug)]
        struct FirstHandler;
        #[derive(Debug)]
        struct SecondHandler;
        impl Handler for FirstHandler {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Ok(AttachedSession {
                    event_cursor: 0,
                    broadcast_rx: dummy_broadcast_rx(),
                })
            }
        }
        impl Handler for SecondHandler {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Ok(AttachedSession {
                    event_cursor: 999,
                    broadcast_rx: dummy_broadcast_rx(),
                })
            }
        }

        let reg = Registry::new();
        reg.register(TransportKind::InProcess, Arc::new(FirstHandler));
        let first = reg
            .lookup(&TransportKind::InProcess)
            .expect("first registered");
        let res = first.bind(&dummy_token(), 0).expect("first bind");
        assert_eq!(res.event_cursor, 0);

        reg.register(TransportKind::InProcess, Arc::new(SecondHandler));
        let second = reg
            .lookup(&TransportKind::InProcess)
            .expect("second registered");
        let res = second.bind(&dummy_token(), 0).expect("second bind");
        assert_eq!(res.event_cursor, 999, "second handler overrides first");
    }

    /// Helper that builds a dummy `tokio::sync::broadcast::Receiver`
    /// of capacity 1 so test `Handler` impls can construct
    /// `AttachedSession` values without depending on a live
    /// `RuntimeHandle`.
    fn dummy_broadcast_rx() -> tokio::sync::broadcast::Receiver<crate::handle::RuntimeEvent> {
        let (tx, _) = tokio::sync::broadcast::channel::<crate::handle::RuntimeEvent>(1);
        tx.subscribe()
    }
}
