//! caBLE (Cloud Assisted BLE) hybrid authenticator transport.
//!
//! Used for the WebAuthn cross-device auth flow that lets a new device
//! prove possession of a passkey registered on a phone. The transport
//! sits between the QR (which contains a `FIDO:/<digits>` URI encoding
//! a [`HandshakeV2`] bootstrap) and the relay server (which brokers the
//! encrypted tunnel between the new device and the phone).
//!
//! The pieces:
//!
//! 1. [`base10`] — Chromium's `BytesToDigits` encoder. Maps binary to
//!    zero-padded decimal so the QR's numeric mode stays dense. Used
//!    to encode the HandshakeV2 CBOR bytes into the QR's digit stream.
//! 2. [`handshake`] — the `HandshakeV2` CBOR struct (keys 0-6). Defines
//!    the peer identity, tunnel secret, and the request type that the
//!    phone will service over the established tunnel.
//! 3. (future) `tunnel` — the QR-only relay transport. Connects to a
//!    relay (typically `cable.ua5v.com` for Chromium, or a WA-specific
//!    relay), negotiates the Noise handshake, and carries the
//!    `request_options_json` GetAssertion payload + response through
//!    the encrypted tunnel.
//!
//! ## Reference
//!
//! - Chromium spec: `cable/v2_handshake.cc` and `cable/handshake.h`
//! - WebAuthn-rs port: `webauthn-authenticator-rs/src/cable/`
//! - Captured live URI from official WA phone (2026-07-08): see
//!   `docs/plans/.../phase5-passkey.md` for the full analysis.

pub mod assert;
pub mod base10;
pub mod ctap2;
pub mod discovery;
pub mod error;
pub mod framing;
pub mod handshake;
pub mod noise;
pub mod tunnel;

pub use assert::{assert_via_cable, assert_via_cable_with_timeout, run_assertion, DEFAULT_TIMEOUT};
pub use ctap2::{build_get_assertion, decode_assertion_response};
pub use discovery::{build_eid, build_tunnel_url, derive_psk, derive_tunnel_id, get_domain};
pub use error::CableError;
pub use framing::{CableFrame, MessageType, SHUTDOWN_COMMAND_BYTES};
pub use handshake::{HandshakeV2, RequestType};
pub use noise::{
    build_initiator_message, responder_process_initial, CableNoiseInitiator, Crypter,
    InitiatorResult,
};
pub use tunnel::{connect_initiator, connect_responder, CablePostHandshake, CableTunnel};
// Re-export the base10 codec so callers don't have to know the module path.
// `URL_PREFIX` is the FIDO URI scheme per caBLE spec.
pub use base10::{decode as decode_base10, encode as encode_base10, URL_PREFIX as FIDO_PREFIX};
