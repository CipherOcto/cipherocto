//! High-level `assert_via_cable` helper.
//!
//! Composes the pieces of the SHORTCAKE_PASSKEY companion-link flow:
//!
//! 1. Connect to `wss://cable.ua5v.com` via the phone's QR handshake
//! 2. Receive + drop the post-handshake `GetInfoResponse` info
//! 3. Convert wacore's WebAuthn `request_options_json` to CTAP2 CBOR
//! 4. Send the CTAP2 GetAssertion command over the encrypted tunnel
//! 5. Receive the encrypted CTAP2 response
//! 6. Decode it into a WebAuthn `PublicKeyCredential` JSON
//! 7. Shutdown the tunnel
//!
//! Returns the WebAuthn JSON object ready for wacore's
//! `webauthn_assertion` field (or `build_webauthn_assertion_json`).
//!
//! ## Failure modes
//!
//! - Tunnel never gets a peer response (phone disconnected or never
//!   scanned) — bubbles up as a `CableError::Cbor` after the timeout
//!   set by the caller (we don't impose one).
//! - Phone returns a CTAP error status — surfaced as
//!   `CableError::Cbor("CTAP error status 0xNN")`.
//! - Phone returns a malformed response — surfaced as `CableError::Cbor`.

use std::time::Duration;

use serde_json::Value as JsonValue;
use tokio::time::timeout;

use crate::ctap2::{build_get_assertion, decode_assertion_response};
use crate::error::CableError;
use crate::handshake::HandshakeV2;
use crate::tunnel::{connect_initiator, CableTunnel};

/// Default timeout for the full connect + handshake. Generous because
/// the user needs time to point their phone at the QR.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// One-shot helper: connect to the caBLE relay using the phone's
/// HandshakeV2, run a CTAP2 GetAssertion over the tunnel, and return
/// the WebAuthn `PublicKeyCredential` JSON.
///
/// `request_options_json` is wacore's `Event::PairPasskeyRequest.
/// request_options_json` verbatim (WebAuthn JSON shape).
pub async fn assert_via_cable(
    handshake: &HandshakeV2,
    request_options_json: &str,
) -> Result<JsonValue, CableError> {
    assert_via_cable_with_timeout(handshake, request_options_json, DEFAULT_TIMEOUT).await
}

/// Same as [`assert_via_cable`] but with a caller-controlled timeout
/// for the full connect + handshake + assertion round-trip.
pub async fn assert_via_cable_with_timeout(
    handshake: &HandshakeV2,
    request_options_json: &str,
    limit: Duration,
) -> Result<JsonValue, CableError> {
    // 1. Build CTAP2 request bytes from wacore's WebAuthn JSON.
    let ctap_request = build_get_assertion(request_options_json)?;

    // 2. Run connect + handshake + assert under one timeout so the
    //    caller can bound user-perceived latency.
    timeout(limit, async {
        let mut tunnel = connect_initiator(handshake).await?;
        // Drop the post-handshake GetInfoResponse; we don't need it
        // for a single assertion. We MUST read it though — caBLE is
        // single-shot and this is the only header message.
        let _info = tunnel.recv_post_handshake().await?;
        // Send the assertion request.
        tunnel.send_ctap(&ctap_request).await?;
        // Receive the assertion response.
        let ctap_response = tunnel.recv_ctap().await?;
        // Politely shut down (phone hangs up after one command anyway).
        let _ = tunnel.shutdown().await;
        // Decode CTAP2 response → WebAuthn JSON.
        let credential = decode_assertion_response(&ctap_response)?;
        Ok(credential)
    })
    .await
    .map_err(|_| CableError::Cbor("assert_via_cable timeout".into()))?
}

/// Lower-level: caller already has the [`CableTunnel`] ready and just
/// needs to send the assertion and decode the response. Useful for
/// tests and for callers that want to inspect the post-handshake
/// info before issuing the command.
pub async fn run_assertion(
    tunnel: &mut CableTunnel,
    request_options_json: &str,
) -> Result<JsonValue, CableError> {
    let ctap_request = build_get_assertion(request_options_json)?;
    tunnel.send_ctap(&ctap_request).await?;
    let ctap_response = tunnel.recv_ctap().await?;
    let _ = tunnel.shutdown().await;
    decode_assertion_response(&ctap_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_timeout_is_generous() {
        // Sanity: 120s gives the user time to scan with the phone.
        assert!(DEFAULT_TIMEOUT >= Duration::from_secs(60));
    }

    // Live integration test (the real assertion) lives in
    // `examples/live_assert.rs`. Unit tests here stay pure.
}
