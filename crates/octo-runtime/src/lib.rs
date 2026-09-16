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
//! Layer B substrate (RFC-0011-c §9.1 Architecture). Types are
//! years-stable per [[cipherocto-design-principles]] §Stable
//! Abstractions Principle. The `octo-cli` (Layer C/D) consumes
//! this crate; the dependency direction is one-way (`octo-cli` →
//! `octo-runtime`). New variants on `AgentState` / `RuntimeEvent` /
//! `TransportKind` land additively via `#[non_exhaustive]`
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
    AgentState, AgentStateDispatcher, AttachHandle, AttachPayload, AttachedSession, EventStream,
    RuntimeEvent, RuntimeHandle, RuntimeHandleBinding, RuntimeHandleId, SessionId, Signature,
    Transport, TransportKind, EVENT_CHANNEL_CAPACITY,
};
pub use persistence::{
    is_token_revoked, load_event_cursor, persist_event_cursor, revoke_attach_token,
};
pub use spawn::{spawn_agent, EXPECTED_PRE_SPAWN_STATE};

/// Legacy `execute_agent` shim — kept for callers predating
/// mission `0011-c-octo-runtime-substrate` (the `octo-cli` v0.1.0
/// registry stub still calls it). New code MUST use [`spawn_agent`]
/// + [`attach`] per RFC-0011-c §9.10.
#[deprecated(
    since = "0.2.0",
    note = "use spawn_agent + attach per RFC-0011-c §9.10; execute_agent is a pre-substrate MVP shim"
)]
pub async fn execute_agent(name: &str) -> Result<String, String> {
    println!("🚀 Executing agent: {}", name);
    // MVP behavior preserved verbatim from the 0.1.0 skeleton.
    println!("✓ Agent completed execution");
    println!("✓ Results persisted to registry");
    Ok(format!("Agent {name} executed successfully"))
}

/// Bind to a previously-minted `AttachHandle` token (RFC-0011-c §F.2).
///
/// Validation chain per §F.2:
/// (a) signature verify via [`verify_attach_handle_payload`]
/// (b) revocation-set check via [`is_token_revoked`]
/// (c) `now_unix <= token.ttl_unix`
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
    // (a) Signature verify — canonical helper enforces the same
    // canonical-bytes form used at mint time. Bad signature →
    // substrate-faithful `BadSignature` envelope.
    verify_attach_handle_payload(
        holder_pubkey,
        &token.session_id,
        &token.payload,
        token.mint_timestamp_unix,
        token.ttl_unix,
        &token.signature,
    )?;

    // (b) Revocation-set fast-path check.
    if is_token_revoked(&token.session_id) {
        return Err(AttachError::RevocationError(format!(
            "session 0x{} revoked",
            hex::encode(token.session_id)
        )));
    }

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // (c) TTL check (now_unix <= ttl_unix).
    if now_unix > token.ttl_unix {
        return Err(AttachError::Expired {
            session_id: token.session_id,
            mint_unix: token.mint_timestamp_unix,
            expired_at_unix: token.ttl_unix,
            now_unix,
        });
    }

    // (d) since_unix must be >= mint_timestamp_unix.
    if since_unix < token.mint_timestamp_unix {
        return Err(AttachError::InvalidSinceCursor {
            mint_unix: token.mint_timestamp_unix,
            requested: since_unix,
        });
    }

    // (e) Transport-handler dispatch (RFC-0011-c §F.2 step (e) +
    // [[cipherocto-design-principles]] §per-extension crates +
    // registry). Substrate ships the `Handler` trait + the
    // built-in `InProcessHandler` (built-in broadcast binding);
    // extension transports (`UnixSocket`, …) ship in follow-on
    // Layer D transport crates that register at process startup.
    //
    // If no handler is registered for the token's `TransportKind`,
    // surface `TransportHandlerNotRegistered` (CLI exit 59). The
    // registered handler is responsible for any per-transport
    // resolution (filesystem, I/O, …) — substrate stays
    // filesystem-free.
    let handler = crate::handle::transport::HANDLE_TRANSPORT_REGISTRY
        .get_or_init(crate::handle::transport::build_in_process_registry)
        .lookup(&token.transport.kind)
        .ok_or_else(|| {
            let kind_label = match token.transport.kind {
                crate::handle::TransportKind::InProcess => "InProcess".to_string(),
                crate::handle::TransportKind::UnixSocket => "UnixSocket".to_string(),
                crate::handle::TransportKind::Raw(uuid) => format!("Raw({uuid})"),
            };
            AttachError::TransportHandlerNotRegistered { kind_label }
        })?;
    handler.bind(token, since_unix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn substrate_re_exports_compile() {
        // Smoke test: every re-exported type is constructible in its
        // canonical form. Guards against accidental visibility or
        // name drift during future refactors.
        let agent_id = Uuid::new_v4();
        let handle = spawn_agent(agent_id, None).expect("spawn");
        let handle_id = handle.handle_id;
        let _stream = attach(handle, None).expect("attach");
        let _binding = RuntimeHandleBinding {
            agent_id,
            handle_id,
            session_id: [0u8; 32],
            spawned_at_unix: Utc::now().timestamp(),
        };
        let _token = AttachHandle {
            session_id: [0u8; 32],
            mint_timestamp_unix: 0,
            ttl_unix: 1,
            signature: Signature([0u8; 64]),
            payload: AttachPayload {
                agent_id,
                since_cursor: 0,
            },
            transport: Transport::IN_PROCESS,
        };
        // RuntimeError variants construct.
        let _e = RuntimeError::AgentNotFound(agent_id);
        let _s = AgentState::Active;
        let _ev = RuntimeEvent::Spawned {
            agent_id,
            at: Utc::now(),
        };
        // AttachError variants construct.
        let _ae = AttachError::Expired {
            session_id: [0u8; 32],
            mint_unix: 0,
            expired_at_unix: 0,
            now_unix: 1,
        };
        let _ae2 = AttachError::BadSignature { reason: "x".into() };
    }

    #[test]
    fn event_channel_capacity_is_power_of_two() {
        const { assert!(EVENT_CHANNEL_CAPACITY > 0) };
        assert_eq!(EVENT_CHANNEL_CAPACITY, 1024);
    }
}
