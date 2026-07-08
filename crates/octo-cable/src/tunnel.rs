//! caBLE WebSocket tunnel — initiator side.
//!
//! Connects to `wss://{tunnel_domain}/cable/new/{tunnel_id_hex}` with
//! `Sec-WebSocket-Protocol: fido.cable`, drives the Noise
//! `NKpsk0_P256_AESGCM_SHA256` handshake, and yields a
//! [`CableTunnel`] that wraps CTAP2 commands in encrypted
//! [`CableFrame`]s.
//!
//! ## Flow
//!
//! ```text
//! 1. Build tunnel URL from HandshakeV2.secret (qr_secret)
//! 2. Connect WebSocket → receive X-caBLE-Routing-ID header (3 bytes)
//! 3. Generate 10-byte nonce; build Eid = [0, nonce, routing_id, server_id=0]
//! 4. Derive PSK = HKDF(ikm=qr_secret, salt=eid, info="Psk")
//! 5. Send Noise initial message (65 bytes ephemeral P-256 pubkey)
//! 6. Receive Noise responder message (65 bytes re + encrypted payload)
//! 7. Decrypt → Crypter
//! 8. Decrypt post-handshake CablePostHandshake (GetInfoResponse bytes)
//! 9. Tunnel ready: send/receive encrypted CableFrames
//! ```
//!
//! ## Reference
//!
//! - webauthn-rs: `webauthn-authenticator-rs/src/cable/tunnel.rs::connect_initiator`
//! - Chromium: `device/fido/cable/fido_tunnel_device.cc`

use futures::{SinkExt, StreamExt};
use rand::RngCore;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http::{HeaderValue, Uri},
    Message,
};

use crate::discovery::{build_eid, build_tunnel_url, derive_psk};
use crate::error::CableError;
use crate::framing::{CableFrame, MessageType, SHUTDOWN_COMMAND_BYTES};
use crate::handshake::HandshakeV2;
use crate::noise::{build_initiator_message, Crypter, InitiatorResult};

/// Subprotocol required by `cable.ua5v.com` and Chromium clients.
/// Source: `webauthn-rs::tunnel::Self::connect` — `fido.cable` literal.
const SUBPROTOCOL: &str = "fido.cable";

/// HTTP header the relay uses to send us our routing id.
/// 3 bytes of hex per `webauthn-rs::tunnel::connect`.
const ROUTING_ID_HEADER: &str = "X-caBLE-Routing-ID";

/// HTTP header we send to identify our origin.
const ORIGIN_HEADER: &str = "Origin";

/// tunnel_server_id for `cable.ua5v.com` (index 0 in
/// `discovery::ASSIGNED_DOMAINS`). Empirically the only domain WA's
/// gms FIDO module uses today.
const TUNNEL_SERVER_ID_GOOGLE: u16 = 0;

/// Open caBLE tunnel as the initiator, scanning the phone's
/// `HandshakeV2` from its `FIDO:/<digits>` QR.
///
/// Performs the full Noise NKpsk0 handshake, then returns a ready
/// [`CableTunnel`]. Caller owns the tunnel and is responsible for
/// issuing one CTAP2 command and shutting down (caBLE single-shot).
///
/// ## Errors
///
/// - DNS / TLS / WebSocket connect failures propagate from
///   `tokio-tungstenite`.
/// - Missing or malformed `X-caBLE-Routing-ID` header.
/// - Noise handshake failures (phone disconnect, protocol drift).
pub async fn connect_initiator(handshake: &HandshakeV2) -> Result<CableTunnel, CableError> {
    let url = build_tunnel_url(&handshake.secret, TUNNEL_SERVER_ID_GOOGLE)?;
    let uri: Uri = url
        .parse()
        .map_err(|e| CableError::Cbor(format!("bad tunnel URI: {e}")))?;

    // Build the WebSocket request with the required subprotocol +
    // origin headers. The fido.cable subprotocol is mandatory per
    // Chromium's fido_cable_discovery.cc.
    let mut request = IntoClientRequest::into_client_request(&uri)
        .map_err(|e| CableError::Cbor(format!("ws request build: {e}")))?;
    let headers = request.headers_mut();
    headers.insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static(SUBPROTOCOL),
    );
    let origin = format!("wss://{}", uri.host().unwrap_or_default());
    headers.insert(
        ORIGIN_HEADER,
        HeaderValue::from_str(&origin).map_err(|e| CableError::Cbor(format!("origin: {e}")))?,
    );

    let (mut ws, response) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| CableError::Cbor(format!("websocket connect: {e}")))?;

    // Read the routing id from the response header (3 bytes of hex).
    let routing_id = response
        .headers()
        .get(ROUTING_ID_HEADER)
        .ok_or_else(|| CableError::Cbor("missing X-caBLE-Routing-ID header".into()))?
        .to_str()
        .map_err(|e| CableError::Cbor(format!("routing-id header: {e}")))?;
    let routing_bytes = hex::decode(routing_id.trim())
        .map_err(|e| CableError::Cbor(format!("routing-id hex: {e}")))?;
    if routing_bytes.len() != 3 {
        return Err(CableError::Cbor(format!(
            "routing-id wrong length: {} bytes",
            routing_bytes.len()
        )));
    }
    let routing_id_arr: [u8; 3] = routing_bytes
        .as_slice()
        .try_into()
        .expect("checked length == 3");

    // Generate a random 10-byte nonce for the Eid.
    let mut nonce = [0u8; 10];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    // Build the Eid per cable/v2_handshake.cc MakeAuthenticatorEid.
    // For the CLI as initiator, we ARE the initiator (new device), so
    // our Eid uses our own nonce + the relay-provided routing_id + the
    // tunnel_server_id we connected to.
    let eid = build_eid(&nonce, &routing_id_arr, TUNNEL_SERVER_ID_GOOGLE);

    // Derive the PSK from qr_secret (= handshake.secret) + eid.
    let psk = derive_psk(&handshake.secret, &eid);

    // Build the Noise initial message (65 bytes ephemeral P-256 pub).
    let InitiatorResult {
        initial_message,
        state,
    } = build_initiator_message(&psk)?;

    // Send it.
    ws.send(Message::Binary(initial_message))
        .await
        .map_err(|e| CableError::Cbor(format!("ws send initial: {e}")))?;

    // Wait for the responder's reply. caBLE single-shot: phone
    // disconnects after one command, so this is the only handshake
    // message we ever read.
    let reply = ws
        .next()
        .await
        .ok_or_else(|| CableError::Cbor("ws closed before response".into()))?
        .map_err(|e| CableError::Cbor(format!("ws recv: {e}")))?;
    let reply_bytes = match reply {
        Message::Binary(b) => b,
        Message::Close(_) => return Err(CableError::Cbor("ws closed by peer".into())),
        other => {
            return Err(CableError::Cbor(format!(
                "unexpected ws message type: {other:?}"
            )))
        }
    };

    // Process the Noise response → Crypter.
    let crypter = state.process_response(&reply_bytes)?;

    Ok(CableTunnel { ws, crypter })
}

/// Decoded `CablePostHandshake` message — the first encrypted payload
/// the authenticator sends after the Noise handshake completes. Per
/// Chromium's `fido_tunnel_device.cc::OnTunnelReady`, the post-handshake
/// is a CBOR map with one mandatory entry: `0x01 → GetInfoResponse bytes`.
/// `0x02` (`linking_info`) is optional and currently ignored.
#[derive(Debug, Clone)]
pub struct CablePostHandshake {
    /// Raw CBOR bytes of the authenticator's `GetInfoResponse`. Parse
    /// with the CTAP2 client if you need structured fields.
    pub info: Vec<u8>,
}

impl CableTunnel {
    /// Receive the next encrypted frame and parse it as the
    /// post-handshake info message. Should be called once
    /// immediately after [`connect_initiator`] returns.
    pub async fn recv_post_handshake(&mut self) -> Result<CablePostHandshake, CableError> {
        let raw = self
            .ws
            .next()
            .await
            .ok_or_else(|| CableError::Cbor("ws closed before post-handshake".into()))?
            .map_err(|e| CableError::Cbor(format!("ws recv phm: {e}")))?;
        let bytes = match raw {
            Message::Binary(b) => b,
            other => {
                return Err(CableError::Cbor(format!(
                    "unexpected ws message type for phm: {other:?}"
                )))
            }
        };
        let plaintext = self.crypter.decrypt(&bytes)?;
        let map: std::collections::BTreeMap<u32, ciborium::value::Value> =
            ciborium::de::from_reader(plaintext.as_slice())
                .map_err(|e| CableError::Cbor(format!("phm cbor: {e}")))?;
        let info = match map.get(&0x01) {
            Some(ciborium::value::Value::Bytes(b)) => b.clone(),
            Some(other) => return Err(CableError::Cbor(format!("phm 0x01 wrong type: {other:?}"))),
            None => return Err(CableError::Cbor("phm missing 0x01".into())),
        };
        Ok(CablePostHandshake { info })
    }
}

/// An established caBLE tunnel. Single-shot: the phone hangs up after
/// one CTAP2 command. Caller must either [`send_ctap`] + [`shutdown`]
/// or drop without sending (which silently fails the auth).
pub struct CableTunnel {
    /// Encrypted WebSocket stream.
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    /// AES-GCM crypt state.
    crypter: Crypter,
}

impl CableTunnel {
    /// Encrypt + send one CTAP2 CBOR payload. Wraps in
    /// `CableFrame{1, Ctap, data}` and AEAD-seals with the send key.
    pub async fn send_ctap(&mut self, cbor: &[u8]) -> Result<(), CableError> {
        let frame = CableFrame {
            protocol_version: 1,
            message_type: MessageType::Ctap,
            data: cbor.to_vec(),
        };
        let plaintext = frame.to_bytes();
        let ciphertext = self.crypter.encrypt(&plaintext)?;
        self.ws
            .send(Message::Binary(ciphertext))
            .await
            .map_err(|e| CableError::Cbor(format!("ws send ctap: {e}")))
    }

    /// Receive one encrypted CTAP2 response frame and return the
    /// decrypted CTAP data (no 4-byte header).
    ///
    /// Skips `KeepAlive` frames by looping.
    pub async fn recv_ctap(&mut self) -> Result<Vec<u8>, CableError> {
        loop {
            let raw = self
                .ws
                .next()
                .await
                .ok_or_else(|| CableError::Cbor("ws closed before response".into()))?
                .map_err(|e| CableError::Cbor(format!("ws recv: {e}")))?;
            let bytes = match raw {
                Message::Binary(b) => b,
                Message::Close(_) => return Err(CableError::Cbor("ws closed by peer".into())),
                other => {
                    return Err(CableError::Cbor(format!(
                        "unexpected ws message type: {other:?}"
                    )))
                }
            };
            let plaintext = self.crypter.decrypt(&bytes)?;
            let frame = CableFrame::from_bytes(&plaintext)?;
            match frame.message_type {
                MessageType::Ctap => return Ok(frame.data),
                MessageType::KeepAlive => continue,
                MessageType::Shutdown => {
                    return Err(CableError::Cbor("peer sent shutdown mid-flow".into()))
                }
            }
        }
    }

    /// Politely terminate the tunnel by sending a SHUTDOWN frame.
    pub async fn shutdown(&mut self) -> Result<(), CableError> {
        let _ = self
            .ws
            .send(Message::Binary(SHUTDOWN_COMMAND_BYTES.to_vec()))
            .await;
        let _ = self.ws.close(None).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RequestType;

    /// The exact URI captured live from official WA Android's
    /// "Link a Device" flow. We use the parsed HandshakeV2 to
    /// verify our tunnel URL builder reproduces what we expect.
    const CAPTURED_URI: &str = "FIDO:/450667960436000384212746765638726635029113873858466150978817481746737139187585179964034382683425543718266291918030680810069082498271112126385317319279362107096654083076";

    #[test]
    fn captured_uri_yields_expected_tunnel_url_for_cable_ua5v() {
        let h = HandshakeV2::from_fido_uri(CAPTURED_URI).expect("decode");
        let url = build_tunnel_url(&h.secret, TUNNEL_SERVER_ID_GOOGLE).expect("url");
        // Must point at cable.ua5v.com (Google's relay) with a
        // 32-char hex tunnel_id derived from the phone's secret.
        assert!(url.starts_with("wss://cable.ua5v.com/cable/new/"));
        assert_eq!(url.len(), "wss://cable.ua5v.com/cable/new/".len() + 32);
        // tunnel_id must be deterministic for the same secret.
        let url2 = build_tunnel_url(&h.secret, TUNNEL_SERVER_ID_GOOGLE).expect("url");
        assert_eq!(url, url2);
    }

    /// Smoke-test that RequestType doesn't affect tunnel setup
    /// (request_type lives in the HandshakeV2 CBOR but isn't used
    /// in tunnel URL / PSK derivation).
    #[test]
    fn request_type_does_not_affect_tunnel_url() {
        let mut h = HandshakeV2::from_fido_uri(CAPTURED_URI).expect("decode");
        h.request_type = RequestType::GetAssertion;
        let url_ga = build_tunnel_url(&h.secret, TUNNEL_SERVER_ID_GOOGLE).expect("url");
        h.request_type = RequestType::MakeCredential;
        let url_mc = build_tunnel_url(&h.secret, TUNNEL_SERVER_ID_GOOGLE).expect("url");
        assert_eq!(url_ga, url_mc);
    }

    /// base10.rs lives next door in this crate; confirm the round-
    /// trip URI path decodes to the same secret we use to build the
    /// tunnel URL. This pins the bridge between the QR codec and
    /// the tunnel URL builder.
    #[test]
    fn captured_uri_secret_round_trips_through_base10() {
        let h = HandshakeV2::from_fido_uri(CAPTURED_URI).expect("decode");
        // Re-encode the HandshakeV2 bytes via base10 and re-parse —
        // they must come back identical.
        let cbor = h.to_cbor_bytes().expect("encode");
        let digits = crate::base10::encode(&cbor);
        let uri2 = format!("{}{}", crate::base10::URL_PREFIX, digits);
        let h2 = HandshakeV2::from_fido_uri(&uri2).expect("re-decode");
        assert_eq!(h2.secret, h.secret, "secret must round-trip");
    }
}
