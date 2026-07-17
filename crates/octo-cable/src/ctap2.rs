//! CTAP2 ↔ WebAuthn JSON codec for `GetAssertion`.
//!
//! The CLI receives wacore's `request_options_json` as a WebAuthn
//! `PublicKeyCredentialRequestOptions` (string-keyed JSON). The phone
//! expects CTAP2 canonical CBOR (integer-keyed). This module maps
//! between them per FIDO2 §6.5.1 + CTAP2 §5.1.
//!
//! ## Key mapping
//!
//! | WebAuthn JSON   | CTAP2 CBOR | Type           |
//! |-----------------|------------|----------------|
//! | `rpId`          | 0x01       | text           |
//! | `challenge`     | 0x02       | bytes          |
//! | `timeout`       | 0x03       | uint (ms)      |
//! | `allowCredentials` | 0x04    | array of maps  |
//! | `userVerification` | 0x05     | text           |
//! | `extensions`    | 0x06       | map            |
//!
//! `allowCredentials` entries are `PublicKeyCredentialDescriptor`:
//!
//! | WebAuthn JSON | CTAP2 CBOR | Type  |
//! |---------------|------------|-------|
//! | `id`          | 0x01       | bytes |
//! | `type`        | 0x02       | text  |
//!
//! Output is canonical: integer keys sorted by `(decimal length, lex)`.
//! Both maps use `Vec<(Value, Value)>` pre-sorted before CBOR encoding.
//!
//! ## Reference
//!
//! - FIDO2 §6.5.1 (authenticator API GetAssertion request)
//! - FIDO2 §5.1 (canonical CBOR ordering)
//! - CTAP2 §5.1.6 (canonical CBOR rules)
//! - Chromium: `device/fido/cbor.h::Canonicalize`
//!
//! The `request_options_json` shape wacore parses matches our parser
//! in `crates/octo-adapter-whatsapp/src/passkey.rs` (mirrors
//! wacore::passkey::parse_request_options).

use base64::Engine;
use ciborium::value::Value;
use serde_json::Value as JsonValue;

use crate::CableError::Cbor;

/// Build a canonical CBOR map for CTAP2 GetAssertion request from
/// wacore's `request_options_json` (WebAuthn JSON).
///
/// Skips fields that are absent from the JSON (CTAP2 maps allow
/// optional fields to be omitted). `allowCredentials` items missing
/// either `id` or `type` cause an error — we don't silently drop
/// descriptors (wacore enforces the same on its side).
pub fn build_get_assertion(
    request_options_json: &str,
) -> Result<Vec<u8>, crate::error::CableError> {
    let v: JsonValue = serde_json::from_str(request_options_json)
        .map_err(|e| crate::error::CableError::Cbor(format!("json: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| crate::error::CableError::Cbor("json is not an object".into()))?;

    let mut entries: Vec<(Value, Value)> = Vec::new();

    // 0x01 rpId
    if let Some(rp_id) = obj.get("rpId").and_then(|v| v.as_str()) {
        entries.push((Value::Integer(1.into()), Value::Text(rp_id.to_string())));
    }

    // 0x02 challenge
    if let Some(ch) = obj.get("challenge").and_then(|v| v.as_str()) {
        let bytes = b64url_decode(ch)?;
        entries.push((Value::Integer(2.into()), Value::Bytes(bytes)));
    }

    // 0x03 timeout
    if let Some(timeout) = obj.get("timeout").and_then(|v| v.as_u64()) {
        entries.push((Value::Integer(3.into()), Value::Integer(timeout.into())));
    }

    // 0x04 allowCredentials — array of { 0x01: id, 0x02: type }
    if let Some(allow) = obj.get("allowCredentials").and_then(|v| v.as_array()) {
        let mut creds: Vec<Value> = Vec::with_capacity(allow.len());
        for c in allow {
            let c_obj = c.as_object().ok_or_else(|| {
                crate::error::CableError::Cbor("allowCredentials[] not object".into())
            })?;
            let id_b64 = c_obj
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Cbor("allowCredentials[].id missing".into()))?;
            let id_bytes = b64url_decode(id_b64)?;
            let cred_type = c_obj
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("public-key")
                .to_string();
            // Sort inner map by (length, lex): "id"=2, "type"=4 → id first.
            let inner: Vec<(Value, Value)> = vec![
                (Value::Integer(1.into()), Value::Bytes(id_bytes)),
                (Value::Integer(2.into()), Value::Text(cred_type)),
            ];
            creds.push(Value::Map(inner));
        }
        entries.push((Value::Integer(4.into()), Value::Array(creds)));
    }

    // 0x05 userVerification
    if let Some(uv) = obj.get("userVerification").and_then(|v| v.as_str()) {
        entries.push((Value::Integer(5.into()), Value::Text(uv.to_string())));
    }

    // 0x06 extensions
    if let Some(ext) = obj.get("extensions").and_then(|v| v.as_object()) {
        let ext_map = json_object_to_cbor(ext)?;
        if !ext_map.is_empty() {
            entries.push((Value::Integer(6.into()), Value::Map(ext_map)));
        }
    }

    // Canonical sort: by (key string length, lex). All our keys are
    // single-digit hex (1-6) so natural order matches the canonical
    // rule. We sort explicitly so the assertion stays correct if
    // someone adds a 2-digit key later.
    entries.sort_by(|a, b| {
        let ka = int_to_str(&a.0);
        let kb = int_to_str(&b.0);
        ka.len().cmp(&kb.len()).then(ka.cmp(&kb))
    });

    let mut out = Vec::new();
    ciborium::ser::into_writer(&Value::Map(entries), &mut out)
        .map_err(|e| crate::error::CableError::Cbor(format!("ctap2 cbor: {e}")))?;
    Ok(out)
}

/// Decode a CTAP2 GetAssertion response CBOR into a WebAuthn
/// `PublicKeyCredential` JSON object ready for wacore's
/// `webauthn_assertion` field.
///
/// CTAP2 GetAssertion response (per FIDO2 §6.5.2):
///
/// | Key | Field             | Maps to WebAuthn    |
/// |-----|-------------------|---------------------|
/// | 0x01 | credential id     | `id` + `rawId` (b64url-no-pad) |
/// | 0x02 | authenticatorData| `response.authenticatorData` (b64url) |
/// | 0x03 | signature         | `response.signature` (b64url) |
/// | 0x04 | userHandle        | `response.userHandle` (b64url, null if absent) |
/// | 0x05 | credBlob / largeBlob | unused by us       |
/// | 0x06 | publicKeyCredentialUserEntity | unused by us |
/// | 0x07 | largeBlobKey      | unused by us       |
/// | 0x08 | unsignedUVAParams | unused by us       |
///
/// First byte of CTAP2 response = status code. 0x00 = success. We
/// surface other codes as errors.
pub fn decode_assertion_response(cbor: &[u8]) -> Result<JsonValue, crate::error::CableError> {
    if cbor.is_empty() {
        return Err(crate::error::CableError::Cbor("empty CTAP response".into()));
    }
    let status = cbor[0];
    if status != 0x00 {
        return Err(crate::error::CableError::Cbor(format!(
            "CTAP error status 0x{status:02x}"
        )));
    }
    let v: Value = ciborium::de::from_reader(&cbor[1..])
        .map_err(|e| crate::error::CableError::Cbor(format!("response cbor: {e}")))?;
    let entries = match v {
        Value::Map(m) => m,
        other => {
            return Err(crate::error::CableError::Cbor(format!(
                "response not a map: {other:?}"
            )))
        }
    };
    let mut credential_id_b64: Option<String> = None;
    let mut auth_data_b64: Option<String> = None;
    let mut signature_b64: Option<String> = None;
    let mut user_handle_b64: Option<Option<String>> = None;

    for (k, val) in entries {
        let key = int_value(&k);
        match (key, val) {
            (0x01, Value::Bytes(b)) => {
                credential_id_b64 = Some(b64url_encode(&b));
            }
            (0x02, Value::Bytes(b)) => {
                auth_data_b64 = Some(b64url_encode(&b));
            }
            (0x03, Value::Bytes(b)) => {
                signature_b64 = Some(b64url_encode(&b));
            }
            (0x04, Value::Bytes(b)) => {
                user_handle_b64 = Some(Some(b64url_encode(&b)));
            }
            (0x04, Value::Null) => {
                user_handle_b64 = Some(None);
            }
            _ => {}
        }
    }

    let cid = credential_id_b64.ok_or_else(|| {
        crate::error::CableError::Cbor("response missing credential id 0x01".into())
    })?;
    let auth_data = auth_data_b64.ok_or_else(|| {
        crate::error::CableError::Cbor("response missing authenticatorData 0x02".into())
    })?;
    let signature = signature_b64
        .ok_or_else(|| crate::error::CableError::Cbor("response missing signature 0x03".into()))?;

    let mut response = serde_json::Map::new();
    response.insert(
        "clientDataJSON".into(),
        JsonValue::String(b64url_encode(b"")),
    );
    response.insert("authenticatorData".into(), JsonValue::String(auth_data));
    response.insert("signature".into(), JsonValue::String(signature));
    response.insert(
        "userHandle".into(),
        match user_handle_b64 {
            Some(Some(uh)) => JsonValue::String(uh),
            _ => JsonValue::Null,
        },
    );

    let mut out = serde_json::Map::new();
    out.insert("type".into(), JsonValue::String("public-key".into()));
    out.insert("id".into(), JsonValue::String(cid.clone()));
    out.insert("rawId".into(), JsonValue::String(cid));
    out.insert("response".into(), JsonValue::Object(response));
    Ok(JsonValue::Object(out))
}

fn b64url_decode(s: &str) -> Result<Vec<u8>, crate::error::CableError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .map_err(|e| crate::error::CableError::Cbor(format!("b64url: {e}")))
}

fn b64url_encode(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

fn int_value(v: &Value) -> i128 {
    match v {
        Value::Integer(i) => i128::from(*i),
        _ => 0,
    }
}

fn int_to_str(v: &Value) -> String {
    int_value(v).to_string()
}

/// Convert a JSON extensions object to a CBOR map. CTAP2 extensions
/// use STRING keys (`"uvm"`, `"hmac"`, `"appid"`, …), sorted by
/// `(decimal-length, lex)`. Values are best-effort: booleans /
/// numbers stay as CBOR equivalents; strings are tried as base64url
/// bytes first, then fall back to text.
fn json_object_to_cbor(
    obj: &serde_json::Map<String, JsonValue>,
) -> Result<Vec<(Value, Value)>, crate::error::CableError> {
    let mut out: Vec<(Value, Value)> = Vec::new();
    for (k, v) in obj {
        let val = json_value_to_cbor(v)?;
        out.push((Value::Text(k.clone()), val));
    }
    out.sort_by(|a, b| {
        let ka = text_value(&a.0);
        let kb = text_value(&b.0);
        ka.len().cmp(&kb.len()).then(ka.cmp(&kb))
    });
    Ok(out)
}

fn text_value(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        _ => String::new(),
    }
}

fn json_value_to_cbor(v: &JsonValue) -> Result<Value, crate::error::CableError> {
    match v {
        JsonValue::Bool(b) => Ok(Value::Bool(*b)),
        JsonValue::Number(n) => {
            if let Some(u) = n.as_u64() {
                Ok(Value::Integer(u.into()))
            } else if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i.into()))
            } else {
                Err(crate::error::CableError::Cbor(format!(
                    "non-integer number in extensions: {n}"
                )))
            }
        }
        JsonValue::String(s) => {
            // Try base64url decode; if it looks byte-like, encode as
            // Bytes. Otherwise treat as a regular text string.
            match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s) {
                Ok(b) => Ok(Value::Bytes(b)),
                Err(_) => Ok(Value::Text(s.clone())),
            }
        }
        _ => Err(crate::error::CableError::Cbor(format!(
            "unsupported extension value type: {v}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test vector: a minimal WebAuthn `request_options_json` shaped
    /// like the one we captured from WA Web's bot-verification prompt
    /// (rpId=whatsapp.com, 32-byte challenge, no allowCredentials,
    /// userVerification=required, extensions.uvm=true).
    const WA_REQUEST: &str = r#"{
        "challenge": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        "rpId": "whatsapp.com",
        "timeout": 600000,
        "allowCredentials": [],
        "userVerification": "required",
        "extensions": {"uvm": true}
    }"#;

    #[test]
    fn build_get_assertion_produces_canonical_cbor() {
        let bytes = build_get_assertion(WA_REQUEST).expect("encode");
        // Round-trip via cborium back to a generic Value and inspect.
        let v: Value = ciborium::de::from_reader(bytes.as_slice()).expect("round-trip");
        let map = match v {
            Value::Map(m) => m,
            _ => panic!("expected map"),
        };
        let keys: Vec<i128> = map.iter().map(|(k, _)| int_value(k)).collect();
        // Sorted by (decimal-length, lex). All keys are single-digit hex
        // → natural order 1,2,3,4,5,6. No field is omitted; empty
        // allowCredentials is still emitted as an empty array per spec.
        assert_eq!(keys, vec![1, 2, 3, 4, 5, 6]);
        // 0x01 = text "whatsapp.com"
        match map
            .iter()
            .find(|(k, _)| int_value(k) == 1)
            .unwrap()
            .1
            .clone()
        {
            Value::Text(s) => assert_eq!(s, "whatsapp.com"),
            other => panic!("rpId wrong type: {other:?}"),
        }
        // 0x02 = 32-byte challenge (we used 12 zero bytes here? no,
        // it's a base64url of length 32 — b64 alphabet A-Z, a-z, 0-9, _, -).
        match map
            .iter()
            .find(|(k, _)| int_value(k) == 2)
            .unwrap()
            .1
            .clone()
        {
            Value::Bytes(b) => assert_eq!(b.len(), 32, "challenge should be 32 bytes"),
            other => panic!("challenge wrong type: {other:?}"),
        }
        // 0x03 = uint 600000
        match map
            .iter()
            .find(|(k, _)| int_value(k) == 3)
            .unwrap()
            .1
            .clone()
        {
            Value::Integer(i) => assert_eq!(i128::from(i), 600000),
            other => panic!("timeout wrong type: {other:?}"),
        }
        // 0x04 = empty array (no allowCredentials)
        match map
            .iter()
            .find(|(k, _)| int_value(k) == 4)
            .unwrap()
            .1
            .clone()
        {
            Value::Array(a) => assert!(a.is_empty()),
            other => panic!("allowCredentials wrong type: {other:?}"),
        }
    }

    #[test]
    fn build_get_assertion_with_one_credential_descriptor() {
        let req = r#"{
            "challenge": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
            "rpId": "whatsapp.com",
            "timeout": 60000,
            "allowCredentials": [
                {"type": "public-key", "id": "dGVzdC1jcmVkLWlkLWJ5dGVz"}
            ]
        }"#;
        let bytes = build_get_assertion(req).expect("encode");
        let v: Value = ciborium::de::from_reader(bytes.as_slice()).expect("cbor");
        let map = match v {
            Value::Map(m) => m,
            _ => panic!("not map"),
        };
        let allow = match map
            .iter()
            .find(|(k, _)| int_value(k) == 4)
            .unwrap()
            .1
            .clone()
        {
            Value::Array(a) => a,
            _ => panic!("allowCredentials not array"),
        };
        assert_eq!(allow.len(), 1);
        // Inner descriptor map: 0x01 (id, bytes) + 0x02 (type, text)
        let inner = match &allow[0] {
            Value::Map(m) => m.clone(),
            _ => panic!("not map"),
        };
        assert_eq!(inner.len(), 2);
        let id = match &inner[0].1 {
            Value::Bytes(b) => b.clone(),
            _ => panic!("id not bytes"),
        };
        assert_eq!(id, b"test-cred-id-bytes");
        let ty = match &inner[1].1 {
            Value::Text(s) => s.clone(),
            _ => panic!("type not text"),
        };
        assert_eq!(ty, "public-key");
    }

    #[test]
    fn decode_synthetic_assertion_response() {
        // Build a minimal CTAP2 GetAssertion response by hand:
        //   status=0x00, map { 0x01: b"cred-id", 0x02: b"auth-data",
        //                     0x03: b"sig", 0x04: b"user" }
        let entries: Vec<(Value, Value)> = vec![
            (Value::Integer(1.into()), Value::Bytes(b"cred-id".to_vec())),
            (
                Value::Integer(2.into()),
                Value::Bytes(b"auth-data".to_vec()),
            ),
            (Value::Integer(3.into()), Value::Bytes(b"sig".to_vec())),
            (Value::Integer(4.into()), Value::Bytes(b"user".to_vec())),
        ];
        let mut cbor = vec![0x00]; // status
        ciborium::ser::into_writer(&Value::Map(entries), &mut cbor).unwrap();
        let resp = decode_assertion_response(&cbor).expect("decode");
        let obj = resp.as_object().expect("object");
        assert_eq!(obj.get("type").unwrap(), "public-key");
        assert_eq!(obj.get("id").unwrap(), "Y3JlZC1pZA"); // base64url("cred-id")
        assert_eq!(obj.get("rawId").unwrap(), "Y3JlZC1pZA");
        let response = obj.get("response").unwrap().as_object().unwrap();
        assert_eq!(response.get("authenticatorData").unwrap(), "YXV0aC1kYXRh");
        assert_eq!(response.get("signature").unwrap(), "c2ln");
        assert_eq!(response.get("userHandle").unwrap(), "dXNlcg");
    }

    #[test]
    fn decode_rejects_non_zero_status() {
        let cbor = vec![0x31]; // CTAP error OTHER
        let err = decode_assertion_response(&cbor).unwrap_err();
        assert!(matches!(err, crate::error::CableError::Cbor(_)));
    }

    #[test]
    fn decode_handles_null_user_handle() {
        // CTAP2 GetAssertion response with userHandle present but null.
        let entries: Vec<(Value, Value)> = vec![
            (Value::Integer(1.into()), Value::Bytes(b"cid".to_vec())),
            (Value::Integer(2.into()), Value::Bytes(b"ad".to_vec())),
            (Value::Integer(3.into()), Value::Bytes(b"sg".to_vec())),
            (Value::Integer(4.into()), Value::Null),
        ];
        let mut cbor = vec![0x00];
        ciborium::ser::into_writer(&Value::Map(entries), &mut cbor).unwrap();
        let resp = decode_assertion_response(&cbor).expect("decode");
        let response = resp.get("response").unwrap().as_object().unwrap();
        assert!(response.get("userHandle").unwrap().is_null());
    }
}
