//! CipherOcto Runtime — agent spawn / attach substrate.
//!
//! Mission `0011-c-octo-runtime-substrate` (RFC-0011-c §9.10
//! Substrate `[ADD]` Signatures; Layer B substrate-authoritative) +
//! follow-on `0011-c-octo-runtime-attachhandle-substrate` (RFC-0011-c
//! §Follow-on §F.1-§F.5).
//!
//! ## Architecture
//!
//! ```text
//! CLI → octo-runtime → RuntimeHandle → broadcast::Sender<RuntimeEvent>
//!                          ↓
//!                  AgentStateDispatcher (octo-wallet substrate)
//! ```
//!
//! ## Substrate surface
//!
//! - [`spawn_agent`] — mint a `RuntimeHandle` for an `agent_id`
//!   (RFC-0011-c §9.3.2 `agent run`).
//! - [`attach`] — open an `EventStream` against a live `RuntimeHandle`
//!   (RFC-0011-c §9.3.5 `agent attach`).
//! - [`AgentStateDispatcher`] — shared state-machine trait consumed
//!   from `octo-wallet` (RFC-0011-c §Implementation Phases Phase 1).
//! - [`AttachHandle`] — 6-field signed token (RFC-0011-c §F.2).
//! - [`mint_attach_handle`] + [`attach_with_token`] — token mint +
//!   cross-process attach substrate (RFC-0011-c §F.2 + §F.5).
//!
//! ## Layer discipline
//!
//! Layer B substrate (RFC-0011-c §9.1 Architecture); see
//! `octo_runtime::handle` for the canonical §Layer discipline
//! prose. Types are years-stable per
//! [[cipherocto-design-principles]] §Stable Abstractions Principle.
//! New variants on `AgentState` / `RuntimeEvent` / `TransportKind`
//! land additively via `#[non_exhaustive]`
//! ([[cipherocto-design-principles]] §Extension over enumeration).
//!
//! ## State semantics
//!
//! `octo-runtime` does **not** mutate agent lifecycle state. State
//! transitions are the `AgentStateDispatcher`'s responsibility
//! (RFC-0011-c §Implementation Phases Phase 1). The runtime
//! substrate allocates the handle, opens the pub-sub bus, and
//! surfaces events — it does not own the state machine.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]

pub mod attach;
pub mod error;
pub mod handle;
pub mod persistence;
pub mod spawn;

// Public re-exports — substrate surface per RFC-0011-c §9.10 + §F.1-§F.5.
pub use attach::{attach, clamp_since};
pub use error::RuntimeError;
pub use handle::{
    encoding::{canonical_payload_bytes, decode_token, encode_token},
    error::{AttachError, PersistenceError},
    signing::{mint_attach_handle, sign_attach_handle_payload, verify_attach_handle_payload},
    transport::{
        build_in_process_registry, Handler, InProcessHandler, Registry, HANDLE_TRANSPORT_REGISTRY,
    },
    AgentState, AgentStateDispatcher, AttachHandle, AttachPayload, AttachedSession, EventStream,
    RuntimeEvent, RuntimeHandle, RuntimeHandleBinding, RuntimeHandleId, SessionId, Signature,
    Transport, TransportKind, EVENT_CHANNEL_CAPACITY,
};
pub use persistence::{
    is_token_revoked, load_event_cursor, persist_event_cursor, revoke_attach_token,
};
pub use spawn::{spawn_agent, EXPECTED_PRE_SPAWN_STATE};

/// Bind to a previously-minted `AttachHandle` token (RFC-0011-c §F.2).
///
/// Validation chain per §F.2:
/// (a) signature verify via [`verify_attach_handle_payload`]
/// (b) revocation-set check via [`is_token_revoked`]
/// (c) `now_unix <= token.ttl_unix` (rejects reserved sentinel
///     `ttl_unix == u64::MAX` as fail-CLOSED)
/// (d) `since_unix >= token.mint_timestamp_unix`
/// (e) transport-handler dispatch via
///     `octo_runtime::handle::transport::HANDLE_TRANSPORT_REGISTRY`
///     (built-in `InProcessHandler` registered at init; extension
///     transports register via follow-on Layer D crates)
///
/// # Errors
/// - `AttachError::BadSignature` — step (a) fail
/// - `AttachError::RevocationError` — step (b) fail (session revoked)
/// - `AttachError::Expired` — step (c) fail (TTL elapsed)
/// - `AttachError::InvalidSinceCursor` — step (d) fail (`since_unix` below mint)
/// - `AttachError::TransportHandlerNotRegistered` — step (e) fail
///   (no handler registered for the token's `TransportKind`)
/// - Per-handler errors from the registered handler
///   (`AttachError::UnknownSession` from the built-in
///   `InProcessHandler`; per-Layer-D errors from extension
///   crates)
pub async fn attach_with_token(
    holder_pubkey: &[u8; 32],
    token: &AttachHandle,
    since_unix: u64,
) -> Result<AttachedSession, AttachError> {
    verify_attach_handle_payload(
        holder_pubkey,
        &token.session_id,
        &token.payload,
        token.mint_timestamp_unix,
        token.ttl_unix,
        &token.signature,
    )?;

    if is_token_revoked(&token.session_id) {
        return Err(AttachError::RevocationError(format!(
            "session 0x{} revoked",
            hex::encode(token.session_id)
        )));
    }

    // Fail-CLOSED discipline on broken clock: any `duration_since`
    // error saturates `now_unix` to `u64::MAX`. The same value is
    // the reserved TTL sentinel — a token whose `ttl_unix == u64::MAX`
    // is always rejected as `Expired` (fail-CLOSED on broken-clock
    // ambiguity: the substrate cannot distinguish a sentinel TTL
    // from a saturated clock and rejects uniformly).
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX);

    if token.ttl_unix == u64::MAX || now_unix > token.ttl_unix {
        return Err(AttachError::Expired {
            session_id: token.session_id,
            mint_unix: token.mint_timestamp_unix,
            expired_at_unix: token.ttl_unix,
            now_unix,
        });
    }

    if since_unix < token.mint_timestamp_unix {
        return Err(AttachError::InvalidSinceCursor {
            mint_unix: token.mint_timestamp_unix,
            requested: since_unix,
        });
    }

    let handler = crate::handle::transport::HANDLE_TRANSPORT_REGISTRY
        .get_or_init(crate::handle::transport::build_in_process_registry)
        .lookup(&token.transport.kind)
        .ok_or_else(|| AttachError::TransportHandlerNotRegistered {
            kind_label: token.transport.kind.to_string(),
        })?;
    handler.bind(token, since_unix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_channel_capacity_is_power_of_two() {
        const { assert!(EVENT_CHANNEL_CAPACITY > 0) };
        assert_eq!(EVENT_CHANNEL_CAPACITY, 1024);
    }
}
