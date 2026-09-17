//! `octo-runtime-transport-raw` — Raw scheme UUID `Handler` impl
//! for `TransportKind::Raw(Uuid)` per RFC-0011-c §F.7.2.
//!
//! Per [[cipherocto-design-principles]] §Extension over enumeration:
//! `TransportKind::Raw(Uuid)` is a typed UUID escape hatch that
//! downstream crates use to register their own protocol handlers
//! keyed by a 128-bit discriminator. The substrate never invents a
//! default — `RawHandler::bind` returns
//! `AttachError::Internal("...not configured...")` until a
//! downstream crate overrides the registration via
//! [`register_into`] with a custom `Arc<dyn Handler>` for the same
//! scheme UUID.
//!
//! ## Fail-CLOSED on unconfigured schemes
//!
//! Per the [[cipherocto-design-principles]] §Discipline at first
//! call site rule: a registered handler that the substrate
//! dispatches to must surface a substrate-visible error, never
//! silently succeed. `RawHandler` is the substrate-side fail-CLOSED
//! default; downstream crates install a real handler via
//! [`register_into`] at process startup, which **overwrites** the
//! fail-CLOSED default. After overwrite, the substrate's
//! `Registry::lookup` returns the custom handler and the
//! fail-CLOSED `RawHandler` is unreachable for that scheme.
//!
//! ## Layer discipline
//!
//! Layer D extension crate per RFC-0011-c §F.7.2. Depends only on
//! the Layer B substrate (`octo-runtime`) — no `octo-cli` /
//! `octo-wallet` reverse-deps.

use std::sync::Arc;

use octo_runtime::handle::transport::{Handler, Registry};
use octo_runtime::handle::{AttachError, AttachHandle, AttachedSession, TransportKind};
use uuid::Uuid;

/// Raw scheme UUID transport handler (RFC-0011-c §F.7.2).
///
/// Carries the `scheme_id` for diagnostics. The fail-CLOSED default
/// `bind` body returns
/// `AttachError::Internal("...raw scheme dispatch not configured...")`
/// until downstream crates override the registration via
/// [`register_into`] with a custom `Arc<dyn Handler>` for the same
/// scheme UUID.
///
/// `Send + Sync` (required by the substrate's `Handler` trait bound).
/// All fields are `Copy + Send + Sync`.
#[derive(Debug, Clone, Copy)]
pub struct RawHandler {
    /// 128-bit scheme UUID discriminator (RFC-0011-c §F.2 typed
    /// escape hatch).
    pub scheme_id: Uuid,
}

impl RawHandler {
    /// Construct a `RawHandler` for the given scheme UUID.
    #[must_use]
    pub const fn new(scheme_id: Uuid) -> Self {
        Self { scheme_id }
    }
}

/// Register a handler for a specific `TransportKind::Raw(scheme_id)`
/// discriminator.
///
/// Downstream crates call this at process startup to install a
/// custom protocol handler keyed by their scheme UUID. The supplied
/// `handler` overrides any prior registration for the same
/// discriminator (per `Registry::register` discipline — last-write
/// wins).
///
/// # Panics
///
/// Panics if the inner registry `RwLock` is poisoned (writer
/// panicked holding the lock). Per fail-CLOSED discipline, the
/// caller cannot recover from a poisoned registry — the panic is
/// the substrate's surface for this condition.
pub fn register_into(registry: &Registry, scheme_id: Uuid, handler: Arc<dyn Handler>) {
    registry.register(TransportKind::Raw(scheme_id), handler);
}

impl Handler for RawHandler {
    fn bind(
        &self,
        _token: &AttachHandle,
        _since_unix: u64,
    ) -> Result<AttachedSession, AttachError> {
        // Fail-CLOSED default per RFC-0011-c §F.7.2. Downstream
        // crates wire a real handler via `register_into` before
        // the substrate's `attach_with_token` step (e) reaches
        // this code path.
        Err(AttachError::Internal(format!(
            "raw scheme `{}` dispatch not configured — downstream crate must register a Handler via octo_runtime_transport_raw::register_into before attach",
            self.scheme_id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use octo_runtime::handle::transport::{build_in_process_registry, Registry};
    use octo_runtime::handle::{AttachPayload, RuntimeEvent, Signature, Transport};
    use uuid::Uuid;

    /// `RawHandler::bind` returns the substrate-side fail-CLOSED
    /// `AttachError::Internal("...not configured...")` for an
    /// unconfigured scheme UUID. Closes TV-AGT25 substrate-side
    /// per §F.7.2.
    #[test]
    fn bind_unconfigured_scheme_returns_internal_error() {
        let scheme_id = Uuid::from_bytes([0x42; 16]);
        let h = RawHandler::new(scheme_id);
        let token = AttachHandle {
            session_id: [0xab; 32],
            mint_timestamp_unix: 0,
            ttl_unix: 0,
            signature: Signature([0x42; 64]),
            payload: AttachPayload {
                agent_id: Uuid::nil(),
                since_cursor: 0,
            },
            transport: Transport::raw(scheme_id, None),
        };
        let res = h.bind(&token, 0);
        match res {
            Err(AttachError::Internal(reason)) => {
                assert!(
                    reason.contains("not configured"),
                    "fail-CLOSED message must mention not configured, got {reason:?}"
                );
                assert!(
                    reason.contains(&scheme_id.to_string()),
                    "fail-CLOSED message must include the scheme_id, got {reason:?}"
                );
            }
            other => panic!("expected Internal(not configured), got {other:?}"),
        }
    }

    /// `register_into` populates the registry with the custom
    /// `Arc<dyn Handler>` under `TransportKind::Raw(scheme_id)`
    /// and the lookup round-trips. Mirrors the substrate's
    /// `lookup_via_dispatch_pattern_succeeds_when_registered`
    /// coverage but at the Layer D boundary.
    #[test]
    fn register_into_round_trips_via_registry() {
        let scheme_id = Uuid::from_bytes([0xab; 16]);
        let reg: Registry = Registry::default();

        #[derive(Debug)]
        struct CustomHandler;
        impl Handler for CustomHandler {
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

        register_into(&reg, scheme_id, Arc::new(CustomHandler));
        let got = reg.lookup(&TransportKind::Raw(scheme_id));
        assert!(got.is_some(), "registered handler must be lookup-able");
    }

    /// `register_into` overwrites the prior registration for the
    /// same `scheme_id` (last-write wins per `Registry::register`
    /// discipline). After overwrite, the substrate's lookup
    /// yields the custom handler, not the fail-CLOSED
    /// `RawHandler`.
    #[test]
    fn register_into_overwrites_fail_closed_default() {
        let scheme_id = Uuid::from_bytes([0xcd; 16]);
        let reg: Registry = Registry::default();

        // Pre-register the fail-CLOSED default (the same path the
        // substrate would take if no downstream crate overrode).
        reg.register(
            TransportKind::Raw(scheme_id),
            Arc::new(RawHandler::new(scheme_id)),
        );
        let pre = reg
            .lookup(&TransportKind::Raw(scheme_id))
            .expect("pre-registered");
        let res = pre.bind(
            &AttachHandle {
                session_id: [0; 32],
                mint_timestamp_unix: 0,
                ttl_unix: 0,
                signature: Signature([0; 64]),
                payload: AttachPayload {
                    agent_id: Uuid::nil(),
                    since_cursor: 0,
                },
                transport: Transport::raw(scheme_id, None),
            },
            0,
        );
        assert!(
            matches!(res, Err(AttachError::Internal(_))),
            "pre-registered handler must be fail-CLOSED, got {res:?}"
        );

        // Overwrite with a custom handler.
        #[derive(Debug)]
        struct CustomHandler;
        impl Handler for CustomHandler {
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
        register_into(&reg, scheme_id, Arc::new(CustomHandler));
        let post = reg
            .lookup(&TransportKind::Raw(scheme_id))
            .expect("post-registered");
        let res = post.bind(
            &AttachHandle {
                session_id: [0; 32],
                mint_timestamp_unix: 0,
                ttl_unix: 0,
                signature: Signature([0; 64]),
                payload: AttachPayload {
                    agent_id: Uuid::nil(),
                    since_cursor: 0,
                },
                transport: Transport::raw(scheme_id, None),
            },
            0,
        );
        match res {
            Ok(attached) => {
                assert_eq!(
                    attached.event_cursor, 999,
                    "post-registered handler must be the custom one, not the fail-CLOSED default"
                );
            }
            other => panic!("expected Ok(999), got {other:?}"),
        }
    }

    /// Sanity-check the build_in_process_registry returns an empty
    /// registry (no handlers installed); downstream crates register
    /// their handlers on top.
    #[test]
    fn build_in_process_registry_has_no_raw_handlers() {
        let reg: Registry = build_in_process_registry();
        let any_uuid = Uuid::from_bytes([0xee; 16]);
        assert!(
            reg.lookup(&TransportKind::Raw(any_uuid)).is_none(),
            "build_in_process_registry must not pre-register any Raw scheme UUIDs"
        );
    }

    /// Sanity-check the substrate-visible `RuntimeEvent` type is
    /// reachable from this Layer D crate (catches substrate
    /// re-export drift).
    #[test]
    fn runtime_event_is_reachable_from_layer_d_crate() {
        let _ = std::mem::size_of::<RuntimeEvent>();
    }

    /// Helper: builds a dummy `tokio::sync::broadcast::Receiver`
    /// of capacity 1 so test `Handler` impls can construct
    /// `AttachedSession` values without depending on a live
    /// `RuntimeHandle`.
    fn dummy_broadcast_rx() -> tokio::sync::broadcast::Receiver<RuntimeEvent> {
        let (tx, _) = tokio::sync::broadcast::channel::<RuntimeEvent>(1);
        tx.subscribe()
    }
}
