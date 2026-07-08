//! caBLE HandshakeV2 — the bootstrap payload that gets base10-encoded
//! into the QR's `FIDO:/<digits>` URI.
//!
//! The HandshakeV2 is a CTAP2-style canonical CBOR map with integer
//! keys 0..=6. The phone's WA app constructs it when a user triggers
//! "Link a Device"; the new device (our CLI) scans the QR and parses
//! it to bootstrap the encrypted tunnel.
//!
//! ## Field layout
//!
//! | Key | Field                                  | Type      | Required |
//! |-----|----------------------------------------|-----------|----------|
//! | 0   | `peer_identity`                        | bytes     | yes      |
//! | 1   | `secret`                               | bytes     | yes      |
//! | 2   | `known_domains_count`                  | uint      | optional |
//! | 3   | `timestamp`                            | uint      | yes      |
//! | 4   | `supports_linking_info`                | bool      | optional |
//! | 5   | `request_type`                         | text      | yes      |
//! | 6   | `supports_non_discoverable_make_credential` | bool | optional |
//!
//! ## CTAP2 canonical ordering
//!
//! The encoded map keys MUST be sorted by `(length_in_decimal_digits,
//! lexicographic)` per CTAP2 §6.5.1. For keys 0-6 (single digit each)
//! the natural order 0..6 satisfies the rule. We sort anyway in case
//! the scheme ever adds a 10+ key.
//!
//! ## Wire format (empirically verified)
//!
//! Captured live from the official WA Android app's "Link a Device"
//! flow on 2026-07-08, decoded with this module:
//!
//! ```text
//! URI:  FIDO:/450667960436000384212746765638726635029113873858466150978817481746737139187585179964034382683425543718266291918030680810069082498271112126385317319279362107096654083076
//! Bytes: 69
//! Map:  { 0: bytes(33), 1: bytes(16), 2: 2, 3: 1783545181, 4: false, 5: "ga" }
//! ```
//!
//! Note: `peer_identity` is 33 bytes (not the 32 of a curve25519
//! public key); `secret` is 16 bytes (matching webauthn-rs exactly).
//! The single extra byte on `peer_identity` is likely a scheme /
//! routing tag. We store these as `Vec<u8>` to absorb the delta until
//! we know what the trailing byte means.
//!
//! ## Reference
//!
//! - Chromium: `device/fido/cable/v2_handshake.cc`
//! - WebAuthn-rs: `webauthn-authenticator-rs/src/cable/handshake.rs`
//! - WA capture: `/tmp/wa-fido-uri-decode.md`

use crate::base10;
use crate::error::CableError;
use ciborium::value::Value;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::SecretKey as StaticSecret;
use rand::rngs::OsRng;
use rand::RngCore;
use std::time::{SystemTime, UNIX_EPOCH};

/// The kind of WebAuthn operation the phone will perform over the
/// established tunnel. Encoded as a string ("ga" / "mc") in CBOR per
/// the caBLE v2 spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestType {
    /// `navigator.credentials.get()` — assertion against a registered
    /// passkey. Used for the SHORTCAKE_PASSKEY companion-link flow
    /// (and the WA Web bot-verification flow we observed).
    GetAssertion,
    /// `navigator.credentials.create()` — register a new passkey.
    MakeCredential,
}

impl RequestType {
    /// The string code used in the CBOR `text` value at key 5.
    pub fn as_str(&self) -> &'static str {
        match self {
            RequestType::GetAssertion => "ga",
            RequestType::MakeCredential => "mc",
        }
    }

    fn from_str(s: &str) -> Result<Self, CableError> {
        match s {
            "ga" => Ok(RequestType::GetAssertion),
            "mc" => Ok(RequestType::MakeCredential),
            other => Err(CableError::UnknownRequestType(other.to_string())),
        }
    }
}

/// The parsed HandshakeV2 bootstrap. Constructed by [`HandshakeV2::from_fido_uri`]
/// or [`HandshakeV2::from_cbor_bytes`]. Encoded to the QR via
/// [`HandshakeV2::to_fido_uri`] or [`HandshakeV2::to_cbor_bytes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeV2 {
    /// Key 0 — peer identity. Empirically 33 bytes from the WA capture;
    /// webauthn-rs's curve25519 pubkey is 32 bytes. We use `Vec<u8>`
    /// to absorb the +1 trailing byte (likely a scheme / routing tag).
    pub peer_identity: Vec<u8>,
    /// Key 1 — tunnel secret. Empirically 16 bytes from the WA capture;
    /// matches webauthn-rs exactly.
    pub secret: Vec<u8>,
    /// Key 2 — count of relying-party domains known to the companion.
    /// Defaults to 0 when absent.
    pub known_domains_count: u64,
    /// Key 3 — handshake timestamp (epoch seconds). Lets the phone
    /// reject stale QRs.
    pub timestamp: u32,
    /// Key 4 — whether the phone will send an extra `linking_info`
    /// payload after the tunnel is up. We observed `false` from WA.
    pub supports_linking_info: bool,
    /// Key 5 — what the phone will do over the tunnel.
    pub request_type: RequestType,
    /// Key 6 — whether the phone supports non-discoverable
    /// MakeCredential. NOT observed in the WA capture (key omitted);
    /// `None` here means "field absent in the wire format".
    pub supports_non_discoverable_make_credential: Option<bool>,
}

impl HandshakeV2 {
    /// Generate a fresh HandshakeV2 + the corresponding P-256 static
    /// private key for the **QR publisher** side of caBLE v2.
    ///
    /// This is what WA Web Browser does: it generates its own
    /// keypair + random 16-byte secret, encodes the public key +
    /// secret into a HandshakeV2, and renders that as the FIDO
    /// QR for the phone (with Google Lens) to scan. The static
    /// key is needed later for the Noise NKpsk0 responder side
    /// of the tunnel.
    ///
    /// `peer_identity` is the **compressed SEC1** form of the
    /// static public key (33 bytes for P-256). This matches what
    /// we observed live from WA Android's Link-a-Device QR (33-byte
    /// field in key 0 of the decoded CBOR map).
    pub fn generate_new() -> (Self, StaticSecret) {
        let static_secret = StaticSecret::random(&mut OsRng);
        let peer_identity = static_secret
            .public_key()
            .to_encoded_point(/* compressed = */ true)
            .as_bytes()
            .to_vec();
        let mut secret = [0u8; 16];
        OsRng.fill_bytes(&mut secret);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as u32)
            .unwrap_or(0);
        let handshake = Self {
            peer_identity,
            secret: secret.to_vec(),
            // Both well-known domains (cable.ua5v.com = 0,
            // cable.auth.com = 1). The phone picks whichever it
            // prefers; matching Chromium's QR-publisher behavior.
            known_domains_count: 2,
            timestamp,
            supports_linking_info: false,
            // SHORTCAKE companion-link is always an assertion
            // (we already have a session; we need the phone to
            // sign a fresh GetAssertion challenge for it).
            request_type: RequestType::GetAssertion,
            supports_non_discoverable_make_credential: None,
        };
        (handshake, static_secret)
    }

    /// Parse a `FIDO:/<digits>` URI directly into a `HandshakeV2`.
    /// Strips the prefix, base10-decodes, then CBOR-decodes.
    pub fn from_fido_uri(uri: &str) -> Result<Self, CableError> {
        let body = uri
            .strip_prefix(base10::URL_PREFIX)
            .ok_or_else(|| CableError::MissingPrefix(uri.to_string()))?;
        if body.is_empty() {
            return Err(CableError::EmptyBody);
        }
        let bytes = base10::decode(body)?;
        Self::from_cbor_bytes(&bytes)
    }

    /// Parse the CBOR-encoded HandshakeV2 bytes (after base10 decode)
    /// into the struct.
    pub fn from_cbor_bytes(bytes: &[u8]) -> Result<Self, CableError> {
        let v: Value =
            ciborium::de::from_reader(bytes).map_err(|e| CableError::Cbor(e.to_string()))?;
        let entries = match v {
            Value::Map(entries) => entries,
            other => {
                return Err(CableError::WrongType {
                    field: 0,
                    expected: "map",
                    got: value_kind(&other),
                })
            }
        };

        let mut peer_identity: Option<Vec<u8>> = None;
        let mut secret: Option<Vec<u8>> = None;
        let mut known_domains_count: Option<u64> = None;
        let mut timestamp: Option<u32> = None;
        let mut supports_linking_info: Option<bool> = None;
        let mut request_type: Option<RequestType> = None;
        let mut supports_non_discoverable_make_credential: Option<bool> = None;

        for (k, val) in entries {
            let key_int = match k {
                Value::Integer(i) => i128::from(i),
                other => {
                    return Err(CableError::NonIntegerKey(format!("{other:?}")));
                }
            };
            let key: u8 = match key_int.try_into() {
                Ok(k) => k,
                Err(_) => return Err(CableError::UnknownKey(key_int)),
            };
            match key {
                0 => peer_identity = Some(extract_bytes(key, val)?),
                1 => secret = Some(extract_bytes(key, val)?),
                2 => known_domains_count = Some(extract_uint(key, val)?),
                3 => {
                    let n = extract_uint(key, val)?;
                    timestamp = Some(u32::try_from(n).map_err(|_| CableError::WrongType {
                        field: 3,
                        expected: "uint32",
                        got: "uint>2^32",
                    })?);
                }
                4 => supports_linking_info = Some(extract_bool(key, val)?),
                5 => {
                    let s = extract_text(key, val)?;
                    request_type = Some(RequestType::from_str(&s)?);
                }
                6 => {
                    supports_non_discoverable_make_credential = Some(extract_bool(key, val)?);
                }
                _ => return Err(CableError::UnknownKey(key_int)),
            }
        }

        Ok(HandshakeV2 {
            peer_identity: peer_identity.ok_or(CableError::MissingField(0))?,
            secret: secret.ok_or(CableError::MissingField(1))?,
            known_domains_count: known_domains_count.unwrap_or(0),
            timestamp: timestamp.ok_or(CableError::MissingField(3))?,
            supports_linking_info: supports_linking_info.unwrap_or(false),
            request_type: request_type.ok_or(CableError::MissingField(5))?,
            supports_non_discoverable_make_credential,
        })
    }

    /// Encode the struct to canonical CBOR bytes. Keys are sorted by
    /// `(decimal-length, lexicographic)` per CTAP2.
    pub fn to_cbor_bytes(&self) -> Result<Vec<u8>, CableError> {
        let mut entries: Vec<(Value, Value)> = Vec::with_capacity(7);
        entries.push((
            Value::Integer(0.into()),
            Value::Bytes(self.peer_identity.clone()),
        ));
        entries.push((Value::Integer(1.into()), Value::Bytes(self.secret.clone())));
        entries.push((
            Value::Integer(2.into()),
            Value::Integer(self.known_domains_count.into()),
        ));
        entries.push((
            Value::Integer(3.into()),
            Value::Integer((self.timestamp as u64).into()),
        ));
        entries.push((
            Value::Integer(4.into()),
            Value::Bool(self.supports_linking_info),
        ));
        entries.push((
            Value::Integer(5.into()),
            Value::Text(self.request_type.as_str().to_string()),
        ));
        if let Some(b) = self.supports_non_discoverable_make_credential {
            entries.push((Value::Integer(6.into()), Value::Bool(b)));
        }
        // CTAP2 canonical: sort by (key length in decimal digits, then lex).
        entries.sort_by(|a, b| {
            let ka = int_value(&a.0);
            let kb = int_value(&b.0);
            let ka_str = ka.to_string();
            let kb_str = kb.to_string();
            ka_str.len().cmp(&kb_str.len()).then(ka_str.cmp(&kb_str))
        });

        let mut out = Vec::new();
        ciborium::ser::into_writer(&Value::Map(entries), &mut out)
            .map_err(|e| CableError::Cbor(e.to_string()))?;
        Ok(out)
    }

    /// Encode the struct to a `FIDO:/<digits>` URI ready for QR rendering.
    pub fn to_fido_uri(&self) -> Result<String, CableError> {
        let cbor = self.to_cbor_bytes()?;
        let digits = base10::encode(&cbor);
        Ok(format!("{}{}", base10::URL_PREFIX, digits))
    }
}

// ── internal helpers ─────────────────────────────────────────────────

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Integer(_) => "integer",
        Value::Bytes(_) => "bytes",
        Value::Text(_) => "text",
        Value::Bool(_) => "bool",
        Value::Null => "null",
        Value::Array(_) => "array",
        Value::Map(_) => "map",
        Value::Float(_) => "float",
        Value::Tag(_, _) => "tag",
        _ => "other",
    }
}

fn int_value(v: &Value) -> i128 {
    match v {
        Value::Integer(i) => i128::from(*i),
        _ => 0,
    }
}

fn extract_bytes(field: u8, v: Value) -> Result<Vec<u8>, CableError> {
    match v {
        Value::Bytes(b) => Ok(b),
        other => Err(CableError::WrongType {
            field,
            expected: "bytes",
            got: value_kind(&other),
        }),
    }
}

fn extract_uint(field: u8, v: Value) -> Result<u64, CableError> {
    match v {
        Value::Integer(i) => u64::try_from(i128::from(i)).map_err(|_| CableError::WrongType {
            field,
            expected: "uint",
            got: "negative-or-overflow",
        }),
        other => Err(CableError::WrongType {
            field,
            expected: "uint",
            got: value_kind(&other),
        }),
    }
}

fn extract_bool(field: u8, v: Value) -> Result<bool, CableError> {
    match v {
        Value::Bool(b) => Ok(b),
        other => Err(CableError::WrongType {
            field,
            expected: "bool",
            got: value_kind(&other),
        }),
    }
}

fn extract_text(field: u8, v: Value) -> Result<String, CableError> {
    match v {
        Value::Text(s) => Ok(s),
        other => Err(CableError::WrongType {
            field,
            expected: "text",
            got: value_kind(&other),
        }),
    }
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_new_round_trips_through_fido_uri() {
        let (h, _sk) = HandshakeV2::generate_new();
        // peer_identity must be compressed SEC1 P-256 = 33 bytes, prefix
        // is 0x02 (even Y) or 0x03 (odd Y).
        assert_eq!(h.peer_identity.len(), 33);
        assert!(
            h.peer_identity[0] == 0x02 || h.peer_identity[0] == 0x03,
            "compressed SEC1 prefix must be 0x02 or 0x03, got 0x{:02x}",
            h.peer_identity[0]
        );
        // secret is 16 random bytes.
        assert_eq!(h.secret.len(), 16);
        // request_type defaults to GetAssertion.
        assert_eq!(h.request_type, RequestType::GetAssertion);
        // supports_linking_info defaults to false.
        assert!(!h.supports_linking_info);
        // supports_non_discoverable_make_credential defaults to None.
        assert_eq!(h.supports_non_discoverable_make_credential, None);
        // Round-trip through the FIDO URI codec.
        let uri = h.to_fido_uri().expect("encode");
        assert!(uri.starts_with("FIDO:/"));
        let h2 = HandshakeV2::from_fido_uri(&uri).expect("decode");
        assert_eq!(h2.peer_identity, h.peer_identity);
        assert_eq!(h2.secret, h.secret);
        assert_eq!(h2.request_type, h.request_type);
    }

    #[test]
    fn generate_new_produces_different_secrets_each_call() {
        let (h1, _) = HandshakeV2::generate_new();
        let (h2, _) = HandshakeV2::generate_new();
        assert_ne!(h1.secret, h2.secret, "secret must be fresh per call");
        assert_ne!(
            h1.peer_identity, h2.peer_identity,
            "keypair must be fresh per call"
        );
    }

    #[test]
    fn generate_new_timestamp_is_recent() {
        let (h, _) = HandshakeV2::generate_new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        // Within 5 seconds of now.
        assert!(
            h.timestamp <= now,
            "timestamp {} > now {}",
            h.timestamp,
            now
        );
        assert!(
            now - h.timestamp <= 5,
            "timestamp {} is {} seconds behind now {}",
            h.timestamp,
            now - h.timestamp,
            now
        );
    }

    /// The exact URI captured from official WA Android's
    /// "Link a Device" flow on 2026-07-08, scanned with a generic
    /// QR reader and pasted to chat. This is the ground-truth
    /// HandshakeV2 the encoder must reproduce.
    const CAPTURED_URI: &str = "FIDO:/450667960436000384212746765638726635029113873858466150978817481746737139187585179964034382683425543718266291918030680810069082498271112126385317319279362107096654083076";

    #[test]
    fn decode_captured_wa_uri_matches_known_fields() {
        let h = HandshakeV2::from_fido_uri(CAPTURED_URI).expect("capture must decode");
        // Field 0 — peer_identity. Empirically 33 bytes (32-byte
        // curve25519 pubkey + 1 routing byte).
        assert_eq!(h.peer_identity.len(), 33, "peer_identity size drift");
        // First 4 bytes pin the prefix so future wire-format changes
        // surface as a regression here.
        assert_eq!(
            &h.peer_identity[..4],
            &[0x03, 0x1c, 0xa0, 0xc2],
            "peer_identity prefix drift"
        );
        // Trailing byte of peer_identity is the +1 deviation from a
        // bare curve25519 pubkey. Lock it too.
        assert_eq!(
            h.peer_identity[32], 0x16,
            "peer_identity trailing tag drift"
        );
        // Field 1 — secret. 16 bytes, matching webauthn-rs exactly.
        assert_eq!(h.secret.len(), 16, "secret size drift");
        assert_eq!(
            &h.secret[..4],
            &[0xde, 0x26, 0x7a, 0xb1],
            "secret prefix drift"
        );
        // Field 2 — known_domains_count = 2.
        assert_eq!(h.known_domains_count, 2);
        // Field 3 — timestamp (Unix epoch). The capture was made at
        // ~2026-07-08T20:53:01Z. Accept a window so this test doesn't
        // bit-rot on a re-capture.
        let ts = h.timestamp as i64;
        assert!(
            (1_783_545_000..=1_783_546_000).contains(&ts),
            "timestamp {ts} outside expected window"
        );
        // Field 4 — supports_linking_info = false.
        assert!(!h.supports_linking_info);
        // Field 5 — request_type = 'ga' (GetAssertion).
        assert_eq!(h.request_type, RequestType::GetAssertion);
        // Field 6 — absent in the capture.
        assert_eq!(h.supports_non_discoverable_make_credential, None);
    }

    #[test]
    fn round_trip_captured_uri() {
        let h1 = HandshakeV2::from_fido_uri(CAPTURED_URI).expect("decode");
        let uri2 = h1.to_fido_uri().expect("encode");
        assert_eq!(uri2, CAPTURED_URI, "encode must round-trip exactly");
    }

    #[test]
    fn canonical_key_ordering_preserved() {
        // Manually construct a struct with fields in REVERSE order to
        // prove the encoder sorts them.
        let h = HandshakeV2 {
            supports_non_discoverable_make_credential: Some(true),
            request_type: RequestType::MakeCredential,
            supports_linking_info: true,
            timestamp: 1_700_000_000,
            known_domains_count: 0,
            secret: vec![0xab; 16],
            peer_identity: vec![0xcd; 33],
        };
        let bytes = h.to_cbor_bytes().expect("encode");
        // Find the byte offset of each key in the encoded map.
        // Keys 0..=6 as CBOR unsigned ints each take 1 byte.
        let positions: Vec<(u8, usize)> = (0u8..=6)
            .filter_map(|k| {
                let needle = [k];
                bytes.windows(1).position(|w| w == needle).map(|p| (k, p))
            })
            .collect();
        // All 7 keys present.
        assert_eq!(positions.len(), 7, "missing keys in encoded map");
        // Positions strictly ascending (canonical order 0..6).
        let positions_only: Vec<usize> = positions.iter().map(|(_, p)| *p).collect();
        let mut sorted = positions_only.clone();
        sorted.sort();
        assert_eq!(positions_only, sorted, "keys not in canonical order");
    }

    #[test]
    fn decode_rejects_missing_prefix() {
        let err = HandshakeV2::from_fido_uri("https://evil.example/").unwrap_err();
        assert!(matches!(err, CableError::MissingPrefix(_)));
    }

    #[test]
    fn decode_rejects_empty_body() {
        let err = HandshakeV2::from_fido_uri("FIDO:/").unwrap_err();
        assert!(matches!(err, CableError::EmptyBody));
    }

    #[test]
    fn decode_rejects_non_digit_body() {
        let err = HandshakeV2::from_fido_uri("FIDO:/abcd").unwrap_err();
        assert!(matches!(err, CableError::ContainsNonDigitChars));
    }

    #[test]
    fn decode_rejects_non_map_cbor() {
        // Base10-encode a CBOR integer ("42" as a 1-byte unsigned int).
        // base10 encodes 1 byte → 3 digits. So input is "042".
        let err = HandshakeV2::from_fido_uri("FIDO:/042").unwrap_err();
        assert!(matches!(err, CableError::WrongType { .. }), "got {err:?}");
    }

    #[test]
    fn encode_drops_optional_false_field_6() {
        // `supports_non_discoverable_make_credential = None` should be
        // omitted from the CBOR (matches the WA capture, which omits key 6).
        let h = HandshakeV2 {
            peer_identity: vec![0; 33],
            secret: vec![0; 16],
            known_domains_count: 0,
            timestamp: 0,
            supports_linking_info: false,
            request_type: RequestType::GetAssertion,
            supports_non_discoverable_make_credential: None,
        };
        let bytes = h.to_cbor_bytes().expect("encode");
        // No 0x06 byte should appear (would be the CBOR uint key for 6).
        assert!(
            !bytes.contains(&0x06),
            "field 6 present when None: {:02x?}",
            bytes
        );
    }
}
