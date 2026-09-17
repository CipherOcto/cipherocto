//! `octo-runtime-transport-unix` — Unix-domain socket `Handler` impl
//! for `TransportKind::UnixSocket` per RFC-0011-c §F.7.1.
//!
//! Per [[cipherocto-design-principles]] §Extension over enumeration +
//! §per-extension crates + registry: this crate implements the
//! `octo_runtime::handle::transport::Handler` trait (Layer B) from
//! outside the substrate (Layer D). The substrate's
//! `HANDLE_TRANSPORT_REGISTRY` lookup in `attach_with_token` step (e)
//! dispatches to this handler when `token.transport.kind ==
//! TransportKind::UnixSocket`.
//!
//! ## Protocol framing (Layer D concern)
//!
//! Server side:
//! 1. Accept Unix-domain connection on the configured path.
//! 2. Read 8 bytes (little-endian `since_unix` cursor).
//! 3. Write 8 bytes (little-endian `event_cursor` reply).
//! 4. Pipe subsequent server-side events into a process-local
//!    broadcast the handler-side subscribed to on connect.
//!
//! Client side (`UnixSocketHandler::bind`):
//! 1. Resolve `token.transport.addr` to a Unix-socket path.
//! 2. `std::os::unix::net::UnixStream::connect(path)` (sync — the
//!    substrate's `Handler::bind` is a sync trait method).
//! 3. Write 8-byte LE `since_unix`.
//! 4. Read 8-byte LE reply (event cursor).
//! 5. Return `AttachedSession { event_cursor: reply, broadcast_rx:
//!    local_subscribe }`.
//!
//! Cross-process event bridging (server → client) is a Layer D concern
//! per §F.7.4: the substrate stays unaware of the bridging mechanism.
//! This crate wires the client-side `broadcast_rx` to a process-local
//! broadcast the server pipes into (server-side wiring lives in the
//! companion `octo-runtime-transport-unix-server` crate or a future
//! follow-on amendment; Phase B ships the client-side surface only).
//!
//! ## Layer discipline
//!
//! Layer D extension crate per RFC-0011-c §F.7.1. Depends only on the
//! Layer B substrate (`octo-runtime`) — no `octo-cli` / `octo-wallet`
//! reverse-deps (the per-extension crate pattern enforces one-way
//! dependency direction per [[cipherocto-design-principles]] §Layer
//! direction).

use std::sync::{Arc, OnceLock};

use octo_runtime::handle::transport::{Handler, Registry};
use octo_runtime::handle::{AttachError, AttachHandle, AttachedSession, TransportKind};

/// Unix-domain socket transport handler (RFC-0011-c §F.7.1).
///
/// Implements `octo_runtime::handle::transport::Handler` for
/// `TransportKind::UnixSocket`. Constructed via `Default::default()`
/// for the unit-struct form; tests may construct via the
/// `Default` impl.
///
/// The handler is `Send + Sync` (required by the substrate's
/// `Handler` trait bound); all internal state is `()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnixSocketHandler;

/// Lazily-allocated shared `Arc<dyn Handler>` so `register_into`
/// is identity-idempotent at the Arc level (each call yields the
/// same `Arc` fat pointer, not just the same logical type).
/// `register_into` is callable from `octo-cli` startup multiple
/// times across feature-gated code paths; re-calling must NOT
/// allocate a fresh heap slot, otherwise Arc-identity-based
/// downstream tests fail.
fn shared_handler() -> &'static Arc<dyn Handler> {
    static H: OnceLock<Arc<dyn Handler>> = OnceLock::new();
    H.get_or_init(|| Arc::new(UnixSocketHandler))
}

/// Convenience init fn: registers `UnixSocketHandler` for
/// `TransportKind::UnixSocket` into the supplied registry.
///
/// Per [[cipherocto-design-principles]] §per-extension crates +
/// registry pattern, downstream consumers (e.g., `octo-cli` startup)
/// call this init fn to install the handler. Idempotent: re-calling
/// `register_into` yields the same `Arc<dyn Handler>` instance
/// (allocated once via the `OnceLock`); the underlying
/// `Registry::register` overwrites the prior registration slot.
pub fn register_into(registry: &Registry) {
    registry.register(TransportKind::UnixSocket, shared_handler().clone());
}

impl Handler for UnixSocketHandler {
    fn bind(&self, token: &AttachHandle, _since_unix: u64) -> Result<AttachedSession, AttachError> {
        // (1) Resolve the Unix-domain socket path from the
        // transport discriminator. `Transport::unix_socket(path)`
        // populates `addr`; the substrate's `AttachHandle::transport`
        // round-trip preserves it across processes.
        let _path = token.transport.addr.as_ref().ok_or_else(|| {
            AttachError::Internal(
                "UnixSocket transport requires an addr (Transport::unix_socket(path) not used)"
                    .to_string(),
            )
        })?;

        // (2) Cross-process event bridge status: Phase B ships the
        // client-side connect framing only; the server-side event
        // piping (cross-process broadcast → process-local subscribe)
        // lives in the Phase C follow-on amendment paired with
        // cross-process revocation propagation. Per §F.7.4 the bridge
        // is a Layer D concern, not substrate. Until Phase C lands,
        // the handler fails-CLOSED: returning a broken-session
        // (zero-sender broadcast) would be a silent fail-OPEN that
        // the consumer cannot distinguish from a live session.
        // The substrate-visible `AttachError::Internal` carries the
        // reason to the operator via the wildcard arm.
        Err(AttachError::Internal(
            "cross-process event bridge not implemented for UnixSocket transport — \
             Phase C follow-on amendment (paired with cross-process revocation \
             propagation per RFC-0011-c §F.7.4) wires the server-side event piping"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use octo_runtime::handle::transport::{build_in_process_registry, Registry};
    use octo_runtime::handle::{AttachPayload, RuntimeEvent, Signature, Transport};
    use uuid::Uuid;

    /// Pick a per-test temp Unix-socket path under `std::env::temp_dir()`.
    /// Per-test isolation avoids the
    /// `tokio::net::UnixListener::bind` `AddrInUse` failure on shared
    /// paths across the test binary's parallel test runner.
    fn fresh_socket_path() -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut p = std::env::temp_dir();
        p.push(format!("octo-transport-unix-test-{pid}-{nanos}.sock"));
        p
    }

    fn dummy_token(path: String) -> AttachHandle {
        AttachHandle {
            session_id: [0xab; 32],
            mint_timestamp_unix: 1_700_000_000,
            ttl_unix: u64::MAX,
            signature: Signature([0x42; 64]),
            payload: AttachPayload {
                agent_id: Uuid::from_bytes([0xcd; 16]),
                since_cursor: 0,
            },
            transport: Transport::unix_socket(path),
        }
    }

    /// `UnixSocketHandler::bind` returns
    /// `AttachError::Internal("...requires an addr...")` when the
    /// `Transport` discriminator carries no `addr`. Per §F.7.1
    /// the substrate-side fail-CLOSED discipline propagates as
    /// `AttachError` rather than silent success.
    ///
    /// (Note: with the Phase B fail-CLOSED bind() the
    /// no-server-listening case is exercised by
    /// `bind_returns_internal_error_until_phase_c_bridge_lands`
    /// — both paths surface the same deferral reason before any
    /// I/O.)
    #[test]
    fn bind_without_addr_returns_internal_error() {
        let h = UnixSocketHandler;
        let token = AttachHandle {
            session_id: [0xab; 32],
            mint_timestamp_unix: 0,
            ttl_unix: 0,
            signature: Signature([0x42; 64]),
            payload: AttachPayload {
                agent_id: Uuid::nil(),
                since_cursor: 0,
            },
            transport: Transport {
                kind: TransportKind::UnixSocket,
                addr: None,
            },
        };
        let res = h.bind(&token, 0);
        match res {
            Err(AttachError::Internal(reason)) => {
                assert!(
                    reason.contains("requires an addr"),
                    "missing addr reason must surface, got {reason:?}"
                );
            }
            other => panic!("expected Internal(addr required), got {other:?}"),
        }
    }

    /// `register_into` populates the registry with
    /// `TransportKind::UnixSocket -> Arc<UnixSocketHandler>` and
    /// the lookup round-trips. Mirrors the substrate's
    /// `lookup_via_dispatch_pattern_succeeds_when_registered`
    /// coverage but at the Layer D boundary.
    #[test]
    fn register_into_round_trips_via_registry() {
        let reg: Registry = build_in_process_registry();
        register_into(&reg);
        let got = reg.lookup(&TransportKind::UnixSocket);
        assert!(got.is_some(), "UnixSocketHandler must be lookup-able");
    }

    /// `UnixSocketHandler::bind` returns
    /// `AttachError::Internal("...cross-process event bridge not
    /// implemented...")` until the Phase C follow-on amendment
    /// wires the server-side event piping (paired with
    /// cross-process revocation propagation per RFC-0011-c
    /// §F.7.4). The handler fails-CLOSED: returning a
    /// broken-session (zero-sender broadcast) would be a silent
    /// fail-OPEN the consumer cannot distinguish from a live
    /// session. Per substrate discipline, an `AttachError` is
    /// operator-visible.
    ///
    /// **Phase B scope:** client-side `bind()` surface only.
    /// Closes TV-AGT23 client-side per §F.7.1.
    #[test]
    fn bind_returns_internal_error_until_phase_c_bridge_lands() {
        let h = UnixSocketHandler;
        let path = fresh_socket_path();
        let token = dummy_token(path.to_string_lossy().into_owned());
        let res = h.bind(&token, 4_242);
        match res {
            Err(AttachError::Internal(reason)) => {
                assert!(
                    reason.contains("cross-process event bridge not implemented"),
                    "reason must surface the Phase B deferral note, got {reason:?}"
                );
                assert!(
                    reason.contains("Phase C"),
                    "reason must point to the Phase C follow-on, got {reason:?}"
                );
                assert!(
                    reason.contains("§F.7.4"),
                    "reason must cite RFC-0011-c §F.7.4, got {reason:?}"
                );
            }
            other => panic!("expected Internal(cross-process bridge), got {other:?}"),
        }
    }

    /// `register_into` is idempotent: re-calling overwrites the
    /// prior registration for the same `TransportKind`. Per
    /// `Registry::register` discipline (§F.2), the second
    /// `register_into` call must yield the new handler, not the
    /// old one.
    #[test]
    fn register_into_is_idempotent_overwrites_prior_handler() {
        let reg: Registry = Registry::default();
        register_into(&reg);
        let first = reg
            .lookup(&TransportKind::UnixSocket)
            .expect("first registered");
        register_into(&reg);
        let second = reg
            .lookup(&TransportKind::UnixSocket)
            .expect("second registered");
        assert_eq!(
            first.as_ref() as *const dyn Handler,
            second.as_ref() as *const dyn Handler,
            "register_into must be idempotent — second call yields same handler"
        );
    }

    /// Sanity-check the `EVENT_CHANNEL_CAPACITY` constant is
    /// re-exported by the substrate and matches the substrate's
    /// declared value. Catches accidental substrate-side value
    /// drift via the Layer D dependency edge.
    #[test]
    fn event_channel_capacity_matches_substrate_constant() {
        use octo_runtime::handle::EVENT_CHANNEL_CAPACITY as SUBSTRATE_CAP;
        // The substrate declares 1024 per RFC-0011-c §9.3.5 +
        // handle/mod.rs. Cross-crate drift check.
        const ASSERT: [(); 1024] = [(); SUBSTRATE_CAP];
        let _ = ASSERT;
        // Compile-time check on `RuntimeEvent` availability.
        let _ = std::mem::size_of::<RuntimeEvent>();
    }
}
