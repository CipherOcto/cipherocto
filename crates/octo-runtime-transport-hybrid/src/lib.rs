//! `octo-runtime-transport-hybrid` — Hybrid multiplexer `Handler`
//! impl per RFC-0011-c §F.7.3.
//!
//! The hybrid multiplexer dispatches across two handlers already
//! registered in the supplied `Registry`: primary dispatch via
//! `self.primary_kind`, fallback dispatch via `self.fallback_kind`
//! on primary failure. The multiplexer owns the dispatch logic only;
//! the per-protocol I/O is delegated to the registered handlers
//! (`UnixSocketHandler`, `InProcessHandler`, downstream crates'
//! custom Raw handlers, …).
//!
//! ## Why a multiplexer (vs. a `match` per `TransportKind`)
//!
//! Per [[cipherocto-design-principles]] §Composition over
//! inheritance: `HybridHandler` composes two existing handlers
//! rather than inheriting from a central enum dispatch. New
//! transport combinations land by registering a hybrid against two
//! existing `TransportKind` discriminators — no central edit to the
//! substrate's `attach_with_token` step (e).
//!
//! ## Failure semantics
//!
//! Primary failure triggers fallback dispatch. If both primary and
//! fallback dispatch fail, the aggregated error wraps both substrings
//! (the substrate-visible `AttachError::Internal(reason)` form) so
//! the operator sees both attempts in the diagnostic surface.
//!
//! ## Layer discipline
//!
//! Layer D extension crate per RFC-0011-c §F.7.3. Depends only on
//! the Layer B substrate (`octo-runtime`) — no `octo-cli` /
//! `octo-wallet` reverse-deps.

use std::sync::Arc;

use octo_runtime::handle::transport::{Handler, Registry};
use octo_runtime::handle::{AttachError, AttachHandle, AttachedSession, TransportKind};

/// Hybrid multiplexer transport handler (RFC-0011-c §F.7.3).
///
/// Owns a reference to the supplied `Registry` (cloned `Arc`) so it
/// can look up the primary + fallback handlers at `bind()` time.
/// The multiplexer does **not** own the per-protocol handlers
/// themselves — those live in the registry, registered by their
/// respective crates (`octo-runtime-transport-unix`,
/// `octo-runtime-transport-raw`, the built-in `InProcessHandler`,
/// …).
///
/// `Send + Sync` (required by the substrate's `Handler` trait
/// bound). `Arc<Registry>` is the only non-`Copy` field; `Clone`
/// is implemented for the unit-struct ergonomics of
/// `register_into`.
pub struct HybridHandler {
    /// Primary `TransportKind` discriminator (looked up in
    /// `self.registry` at `bind()` time).
    pub primary_kind: TransportKind,
    /// Fallback `TransportKind` discriminator (looked up on
    /// primary failure).
    pub fallback_kind: TransportKind,
    /// Reference to the registry holding the primary +
    /// fallback handlers. `Arc<Registry>` so the handler can
    /// be shared across `Arc<dyn Handler>` clones per the
    /// substrate's `Handler: Send + Sync` bound.
    pub registry: Arc<Registry>,
}

impl std::fmt::Debug for HybridHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridHandler")
            .field("primary_kind", &self.primary_kind)
            .field("fallback_kind", &self.fallback_kind)
            .field("registry", &"<Arc<Registry>>")
            .finish()
    }
}

impl HybridHandler {
    /// Construct a `HybridHandler` over the supplied registry.
    /// The caller is responsible for ensuring the primary +
    /// fallback handlers are registered in `registry` before this
    /// handler is invoked.
    #[must_use]
    pub fn new(
        primary_kind: TransportKind,
        fallback_kind: TransportKind,
        registry: Arc<Registry>,
    ) -> Self {
        Self {
            primary_kind,
            fallback_kind,
            registry,
        }
    }
}

/// Convenience init fn: registers a `HybridHandler` against the
/// supplied registry under a caller-chosen discriminator. The
/// caller must pre-register the primary + fallback handlers in the
/// same registry (otherwise the hybrid's `bind()` will fail with
/// `AttachError::Internal("...primary not registered...")`).
///
/// The hybrid's own registration `TransportKind` is operator-chosen
/// — the multiplexer dispatches via the primary + fallback
/// `TransportKind`s, not via its own registration kind. Operators
/// commonly reuse one of the primary/fallback kinds or pick a
/// distinct Raw scheme UUID for the hybrid's registration.
///
/// `registry` is `Arc<Registry>` (not `&Registry`) because
/// `Registry` does not implement `Clone` (the inner `RwLock`
/// prevents that) — the hybrid needs an owned `Arc<Registry>` to
/// look up primary + fallback handlers at `bind()` time.
///
/// # Panics
///
/// Panics if the inner registry `RwLock` is poisoned (writer
/// panicked holding the lock). Per fail-CLOSED discipline, the
/// caller cannot recover from a poisoned registry.
///
/// Panics if `primary_kind == fallback_kind` (degenerate hybrid:
/// `primary_kind` and `fallback_kind` are the same `TransportKind`
/// discriminator, so the multiplexer would dispatch to one and
/// fall back to the identical target). The init-time panic
/// surfaces misuse loud and early rather than papering over it
/// at `bind()` time.
pub fn register_into(
    registry: Arc<Registry>,
    dispatch_kind: TransportKind,
    primary_kind: TransportKind,
    fallback_kind: TransportKind,
) {
    // Degenerate-hybrid check: primary == fallback means the
    // handler is identical to a single-dispatch handler. Fail
    // closed at init time (operator-visible panic) rather than
    // papering over the misuse at `bind()` time.
    if primary_kind == fallback_kind {
        panic!(
            "hybrid register_into: primary_kind ({primary_kind}) equals fallback_kind — degenerate hybrid rejected at init time"
        );
    }
    // Clone the Arc for the HybridHandler storage first; the
    // `register` call below borrows the original Arc.
    let handler = Arc::new(HybridHandler::new(
        primary_kind,
        fallback_kind,
        Arc::clone(&registry),
    )) as Arc<dyn Handler>;
    registry.register(dispatch_kind, handler);
}

impl Handler for HybridHandler {
    fn bind(&self, token: &AttachHandle, since_unix: u64) -> Result<AttachedSession, AttachError> {
        // (1) Primary lookup.
        let primary = self.registry.lookup(&self.primary_kind).ok_or_else(|| {
            AttachError::Internal(format!(
                "hybrid primary `{}` not registered",
                self.primary_kind
            ))
        })?;

        // (2) Primary dispatch.
        match primary.bind(token, since_unix) {
            Ok(attached) => Ok(attached),
            Err(primary_err) => {
                // (3) Fallback lookup on primary failure.
                let fallback = self.registry.lookup(&self.fallback_kind).ok_or_else(|| {
                    // Both primary + fallback un-registered:
                    // surface the primary error (the actual
                    // failure) with the fallback-unavailable
                    // context appended.
                    AttachError::Internal(format!(
                        "hybrid primary `{}` failed ({}); fallback `{}` not registered",
                        self.primary_kind, primary_err, self.fallback_kind,
                    ))
                })?;

                // (4) Fallback dispatch; aggregate error on failure.
                fallback.bind(token, since_unix).map_err(|fallback_err| {
                    AttachError::Internal(format!(
                        "hybrid primary `{}` failed ({}); fallback `{}` failed ({})",
                        self.primary_kind, primary_err, self.fallback_kind, fallback_err,
                    ))
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use octo_runtime::handle::transport::Registry;
    use octo_runtime::handle::{AttachPayload, RuntimeEvent, Signature, Transport};
    use uuid::Uuid;

    /// Build a dummy `AttachHandle` for tests.
    fn dummy_token(kind: TransportKind) -> AttachHandle {
        AttachHandle {
            session_id: [0xab; 32],
            mint_timestamp_unix: 1_700_000_000,
            ttl_unix: u64::MAX,
            signature: Signature([0x42; 64]),
            payload: AttachPayload {
                agent_id: Uuid::from_bytes([0xcd; 16]),
                since_cursor: 0,
            },
            transport: Transport { kind, addr: None },
        }
    }

    /// Build a dummy `tokio::sync::broadcast::Receiver` of
    /// capacity 1 so test `Handler` impls can construct
    /// `AttachedSession` values without depending on a live
    /// `RuntimeHandle`.
    fn dummy_broadcast_rx() -> tokio::sync::broadcast::Receiver<RuntimeEvent> {
        let (tx, _) = tokio::sync::broadcast::channel::<RuntimeEvent>(1);
        tx.subscribe()
    }

    /// `HybridHandler::bind` returns the primary's `AttachedSession`
    /// when the primary dispatch succeeds. Closes TV-AGT26
    /// substrate-side primary path per §F.7.3.
    #[test]
    fn bind_returns_primary_when_primary_succeeds() {
        let reg = Arc::new(Registry::default());

        #[derive(Debug)]
        struct PrimaryOk;
        impl Handler for PrimaryOk {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Ok(AttachedSession {
                    event_cursor: 1,
                    broadcast_rx: dummy_broadcast_rx(),
                })
            }
        }
        #[derive(Debug)]
        struct FallbackOk;
        impl Handler for FallbackOk {
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
        reg.register(TransportKind::InProcess, Arc::new(PrimaryOk));
        reg.register(TransportKind::UnixSocket, Arc::new(FallbackOk));

        let h = HybridHandler::new(
            TransportKind::InProcess,
            TransportKind::UnixSocket,
            reg.clone(),
        );
        let token = dummy_token(TransportKind::InProcess);
        let res = h.bind(&token, 0);
        match res {
            Ok(attached) => {
                assert_eq!(
                    attached.event_cursor, 1,
                    "primary dispatch must succeed; fallback must NOT be invoked"
                );
            }
            other => panic!("expected Ok(1) from primary, got {other:?}"),
        }
    }

    /// `HybridHandler::bind` falls back to the fallback handler
    /// when the primary dispatch fails. Closes TV-AGT26
    /// substrate-side fallback path per §F.7.3.
    #[test]
    fn bind_falls_back_to_fallback_when_primary_fails() {
        let reg = Arc::new(Registry::default());

        #[derive(Debug)]
        struct PrimaryFail;
        impl Handler for PrimaryFail {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Err(AttachError::Internal("primary down".to_string()))
            }
        }
        #[derive(Debug)]
        struct FallbackOk;
        impl Handler for FallbackOk {
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
        reg.register(TransportKind::InProcess, Arc::new(PrimaryFail));
        reg.register(TransportKind::UnixSocket, Arc::new(FallbackOk));

        let h = HybridHandler::new(
            TransportKind::InProcess,
            TransportKind::UnixSocket,
            reg.clone(),
        );
        let token = dummy_token(TransportKind::InProcess);
        let res = h.bind(&token, 0);
        match res {
            Ok(attached) => {
                assert_eq!(
                    attached.event_cursor, 999,
                    "fallback dispatch must succeed after primary failure"
                );
            }
            other => panic!("expected Ok(999) from fallback, got {other:?}"),
        }
    }

    /// `HybridHandler::bind` aggregates the primary + fallback
    /// errors when both fail. The aggregated `Internal` reason
    /// contains both substrings so the operator sees both
    /// attempts. Per §F.7.3 the wrapping preserves operator
    /// observability of both attempts.
    #[test]
    fn bind_aggregates_errors_when_both_primary_and_fallback_fail() {
        let reg = Arc::new(Registry::default());

        #[derive(Debug)]
        struct PrimaryFail;
        impl Handler for PrimaryFail {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Err(AttachError::Internal("primary-down".to_string()))
            }
        }
        #[derive(Debug)]
        struct FallbackFail;
        impl Handler for FallbackFail {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Err(AttachError::Internal("fallback-down".to_string()))
            }
        }
        reg.register(TransportKind::InProcess, Arc::new(PrimaryFail));
        reg.register(TransportKind::UnixSocket, Arc::new(FallbackFail));

        let h = HybridHandler::new(
            TransportKind::InProcess,
            TransportKind::UnixSocket,
            reg.clone(),
        );
        let token = dummy_token(TransportKind::InProcess);
        let res = h.bind(&token, 0);
        match res {
            Err(AttachError::Internal(reason)) => {
                assert!(
                    reason.contains("primary-down"),
                    "aggregated reason must mention primary failure, got {reason:?}"
                );
                assert!(
                    reason.contains("fallback-down"),
                    "aggregated reason must mention fallback failure, got {reason:?}"
                );
            }
            other => panic!("expected Internal(aggregated), got {other:?}"),
        }
    }

    /// `HybridHandler::bind` surfaces the primary error and a
    /// "fallback not registered" hint when the primary fails and
    /// no fallback is registered. Per §F.7.3 the primary error is
    /// the actual failure and must remain operator-visible.
    #[test]
    fn bind_surfaces_primary_error_when_fallback_not_registered() {
        let reg = Arc::new(Registry::default());

        #[derive(Debug)]
        struct PrimaryFail;
        impl Handler for PrimaryFail {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Err(AttachError::Internal("primary-down".to_string()))
            }
        }
        reg.register(TransportKind::InProcess, Arc::new(PrimaryFail));
        // Fallback NOT registered.

        let h = HybridHandler::new(
            TransportKind::InProcess,
            TransportKind::UnixSocket,
            reg.clone(),
        );
        let token = dummy_token(TransportKind::InProcess);
        let res = h.bind(&token, 0);
        match res {
            Err(AttachError::Internal(reason)) => {
                assert!(
                    reason.contains("primary-down"),
                    "reason must mention primary failure, got {reason:?}"
                );
                assert!(
                    reason.contains("not registered"),
                    "reason must mention fallback not registered, got {reason:?}"
                );
            }
            other => panic!("expected Internal(primary + not registered), got {other:?}"),
        }
    }

    /// `HybridHandler::bind` surfaces a "primary not registered"
    /// error when the primary kind has no registered handler.
    /// The fallback is never consulted in this path.
    #[test]
    fn bind_surfaces_primary_not_registered_error() {
        let reg = Arc::new(Registry::default());
        // Neither primary nor fallback registered.

        let h = HybridHandler::new(
            TransportKind::InProcess,
            TransportKind::UnixSocket,
            reg.clone(),
        );
        let token = dummy_token(TransportKind::InProcess);
        let res = h.bind(&token, 0);
        match res {
            Err(AttachError::Internal(reason)) => {
                assert!(
                    reason.contains("primary"),
                    "reason must mention primary, got {reason:?}"
                );
                assert!(
                    reason.contains("not registered"),
                    "reason must mention not registered, got {reason:?}"
                );
            }
            other => panic!("expected Internal(primary not registered), got {other:?}"),
        }
    }

    /// `register_into` populates the registry with the hybrid
    /// multiplexer under the caller-chosen `dispatch_kind`. The
    /// hybrid's stored `Arc<Registry>` is the same `Arc` the
    /// caller passed in (the primary + fallback handlers
    /// registered before `register_into` remain visible to the
    /// hybrid).
    #[test]
    fn register_into_round_trips_via_registry() {
        let scheme_id = uuid::Uuid::from_bytes([0xef; 16]);
        let reg = Arc::new(Registry::default());

        #[derive(Debug)]
        struct OkHandler;
        impl Handler for OkHandler {
            fn bind(
                &self,
                _token: &AttachHandle,
                _since_unix: u64,
            ) -> Result<AttachedSession, AttachError> {
                Ok(AttachedSession {
                    event_cursor: 7,
                    broadcast_rx: dummy_broadcast_rx(),
                })
            }
        }
        reg.register(TransportKind::InProcess, Arc::new(OkHandler));

        register_into(
            reg.clone(),
            TransportKind::Raw(scheme_id),
            TransportKind::InProcess,
            TransportKind::Raw(scheme_id),
        );
        let got = reg.lookup(&TransportKind::Raw(scheme_id));
        assert!(got.is_some(), "hybrid handler must be lookup-able");
    }

    /// `register_into` panics when `primary_kind == fallback_kind`
    /// (degenerate hybrid rejected at init time). The panic
    /// surfaces misuse loud and early rather than papering over it
    /// at `bind()` time — see the `# Panics` block on
    /// `register_into`.
    #[test]
    #[should_panic(expected = "primary_kind")]
    fn register_into_panics_on_degenerate_hybrid() {
        let reg = Arc::new(Registry::default());
        register_into(
            reg,
            TransportKind::Raw(uuid::Uuid::from_bytes([0x77; 16])),
            TransportKind::InProcess,
            TransportKind::InProcess,
        );
    }
}
