//! caBLE discovery / PSK / tunnel_id derivation.
//!
//! Mirrors `webauthn-rs/webauthn-authenticator-rs/src/cable/discovery.rs`
//! and Chromium's `device/fido/cable/v2_handshake.cc::Discovery`.
//!
//! For the CLI as initiator (no BLE), the flow is:
//!
//! 1. Phone's `HandshakeV2.secret` (16 bytes) becomes our `qr_secret`.
//! 2. `tunnel_id = HKDF-SHA256(ikm=qr_secret, info="TunnelID")[:16]` —
//!    hex-encoded into the WebSocket URL path.
//! 3. Connect to `wss://{tunnel_domain}/cable/new/{tunnel_id_hex}`.
//! 4. Relay returns `X-caBLE-Routing-ID` header (3 bytes).
//! 5. Generate 10-byte nonce; build `eid = [0x00, nonce, routing_id,
//!    tunnel_server_id(2 LE)]` (16 bytes total).
//! 6. `psk = HKDF-SHA256(ikm=qr_secret, salt=eid, info="Psk")[:32]`.
//!
//! ## Reference
//!
//! - webauthn-rs: `webauthn-authenticator-rs/src/cable/discovery.rs`
//! - Chromium: `device/fido/cable/v2_handshake.cc` — `Discovery::tunnel_id`,
//!   `Discovery::eid_key`, `Discovery::psk`, `Discovery::MakeAuthenticatorEid`

use hkdf::Hkdf;
use sha2::Sha256;

use crate::error::CableError;

/// Hard-coded well-known tunnel server domains per the caBLE v2 spec.
/// Source: `device/fido/cable/v2_handshake.cc` ASSIGNED_DOMAINS.
const ASSIGNED_DOMAINS: &[&str] = &[
    // Google
    "cable.ua5v.com",
    // Apple
    "cable.auth.com",
];

/// Resolve a `tunnel_server_id` (the integer the phone sends in its
/// HandshakeV2 `known_domains_count` / the Eid's `tunnel_server_id`
/// field) to a hostname. IDs ≥ 256 are derived from a SHA-256-based
/// encoding per Chromium; we don't need those for our use case (the
/// phone picks from the assigned list).
pub fn get_domain(tunnel_server_id: u16) -> Option<String> {
    ASSIGNED_DOMAINS
        .get(tunnel_server_id as usize)
        .map(|s| s.to_string())
}

/// 16-byte encrypted identity (eid) layout per `CableEid` in webauthn-rs:
///
/// | Offset | Size | Field |
/// |--------|------|-------|
/// | 0      | 1    | reserved (always 0) |
/// | 1      | 10   | nonce |
/// | 11     | 3    | routing_id (relay-provided) |
/// | 14     | 2    | tunnel_server_id (LE u16) |
pub fn build_eid(nonce: &[u8; 10], routing_id: &[u8; 3], tunnel_server_id: u16) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0] = 0;
    out[1..11].copy_from_slice(nonce);
    out[11..14].copy_from_slice(routing_id);
    out[14..16].copy_from_slice(&tunnel_server_id.to_le_bytes());
    out
}

/// Build the WebSocket URL for the caBLE tunnel init.
///
/// Format: `wss://{domain}/cable/new/{tunnel_id_hex_upper}`
///
/// `tunnel_id_hex_upper` matches Chromium's `hex::encode_upper`.
pub fn build_tunnel_url(qr_secret: &[u8], tunnel_server_id: u16) -> Result<String, CableError> {
    let domain = get_domain(tunnel_server_id)
        .ok_or_else(|| CableError::Cbor(format!("unknown tunnel_server_id {tunnel_server_id}")))?;
    let tunnel_id = derive_tunnel_id(qr_secret);
    let hex = hex::encode_upper(tunnel_id);
    Ok(format!("wss://{domain}/cable/new/{hex}"))
}

/// Derive the 16-byte tunnel ID from `qr_secret` (the phone's
/// `HandshakeV2.secret`). HKDF-SHA256 with `info="TunnelID"` (4-byte
/// little-endian u32 of 2) and empty salt.
pub fn derive_tunnel_id(qr_secret: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    derive(qr_secret, &[], DerivedValueType::TunnelID, &mut out);
    out
}

/// Derive the 32-byte pre-shared key for the Noise handshake from
/// `qr_secret` and `eid`. HKDF-SHA256 with `info="Psk"` (4-byte LE u32
/// of 3) and `salt=eid`.
pub fn derive_psk(qr_secret: &[u8], eid: &[u8; 16]) -> [u8; 32] {
    let mut out = [0u8; 32];
    derive(qr_secret, eid, DerivedValueType::Psk, &mut out);
    out
}

/// 4-byte little-endian info tags for HKDF per Chromium
/// `Discovery::DerivedValueType`. See `discovery.rs` line 32-41.
#[derive(Copy, Clone, Debug)]
enum DerivedValueType {
    /// EidKey (32+32 bytes): derived from qr_secret only. Used by the
    /// BLE encryption path for the service-data advert. The CLI's
    /// WebSocket-only initiator path does NOT need this; we keep the
    /// variant for spec completeness / future BLE use.
    #[allow(dead_code)]
    EidKey = 1,
    TunnelID = 2,
    Psk = 3,
}

fn derive(ikm: &[u8], salt: &[u8], typ: DerivedValueType, out: &mut [u8]) {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let info = (typ as u32).to_le_bytes();
    hk.expand(&info, out)
        .expect("HKDF expand never fails for <=255 byte outputs on 32+ byte ikm");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_domains_resolve() {
        assert_eq!(get_domain(0), Some("cable.ua5v.com".to_string()));
        assert_eq!(get_domain(1), Some("cable.auth.com".to_string()));
        assert_eq!(get_domain(2), None);
        assert_eq!(get_domain(255), None);
    }

    #[test]
    fn eid_layout_matches_spec() {
        let nonce = [1u8; 10];
        let routing_id = [0xAA, 0xBB, 0xCC];
        let eid = build_eid(&nonce, &routing_id, 0);
        assert_eq!(eid[0], 0);
        assert_eq!(&eid[1..11], &nonce);
        assert_eq!(&eid[11..14], &routing_id);
        assert_eq!(u16::from_le_bytes([eid[14], eid[15]]), 0);
    }

    #[test]
    fn tunnel_id_is_deterministic_per_secret() {
        let s1 = [7u8; 16];
        let id1 = derive_tunnel_id(&s1);
        let id2 = derive_tunnel_id(&s1);
        assert_eq!(id1, id2);
        // Different secret → different tunnel_id (with overwhelming prob)
        let s2 = [8u8; 16];
        let id3 = derive_tunnel_id(&s2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn psk_is_deterministic_per_secret_and_eid() {
        let s = [9u8; 16];
        let eid_a = build_eid(&[1u8; 10], &[0, 1, 2], 0);
        let eid_b = build_eid(&[2u8; 10], &[0, 1, 2], 0);
        let psk_a1 = derive_psk(&s, &eid_a);
        let psk_a2 = derive_psk(&s, &eid_a);
        let psk_b = derive_psk(&s, &eid_b);
        assert_eq!(psk_a1, psk_a2);
        assert_ne!(psk_a1, psk_b); // different eid → different psk
        assert_eq!(psk_a1.len(), 32);
    }

    #[test]
    fn build_tunnel_url_for_captured_wa_secret() {
        // The captured WA HandshakeV2 had a 16-byte secret starting
        // 0xde 0x26 0x7a 0xb1... Use the actual bytes to confirm the
        // URL we will connect to is well-formed.
        let secret = [
            0xde, 0x26, 0x7a, 0xb1, 0xde, 0x13, 0xde, 0x1b, 0x9b, 0x5e, 0x51, 0x4b, 0xb2, 0x39,
            0x4d, 0x74,
        ];
        let url = build_tunnel_url(&secret, 0).unwrap();
        assert!(url.starts_with("wss://cable.ua5v.com/cable/new/"));
        // tunnel_id is 32 hex chars (16 bytes upper-hex)
        assert_eq!(url.len(), "wss://cable.ua5v.com/cable/new/".len() + 32);
    }
}
