// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Session 2 of the wacore-webauthn plan (RFC-0909). Wire-format helper for the
// WhatsApp adapter's passkey (SHORTCAKE_PASSKEY) link flow.
//
// Public view: a normalised `AssertionRequest` + `UserVerification` enum that
// downstream callers (the `CallbackAuthenticator` and any future authenticator
// impls) can drive without having to re-parse the upstream JSON.
//
// Field shape mirrors upstream's `whatsapp_rust::passkey::AssertionRequest`
// (`wacore/src/passkey/mod.rs:69-86`) so a future
// `impl From<our::AssertionRequest> for upstream::AssertionRequest` (or a plain
// type alias) is trivial. The parser deliberately rejects all variants of
// `PasskeyError::InvalidOptions` so the host can distinguish "bad options"
// from "no credential" / "user cancelled" / "authenticator backend error".

use serde::Deserialize;

/// User-verification policy from the server's `PublicKeyCredentialRequestOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserVerification {
    Required,
    Preferred,
    Discouraged,
}

/// WebAuthn assertion request, parsed from the server's
/// `<passkey_request_options>` JSON. `challenge` and `allow_credentials[*].id`
/// are base64url-decoded; `raw_options_json` is preserved verbatim so callers
/// that forward to Android Credential Manager can pass the original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionRequest {
    pub challenge: Vec<u8>,
    pub rp_id: Option<String>,
    pub allow_credentials: Vec<Vec<u8>>,
    pub user_verification: UserVerification,
    pub timeout_ms: Option<u64>,
    pub raw_options_json: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PasskeyError {
    #[error("invalid passkey options: {0}")]
    InvalidOptions(String),
    #[error("assertion failed: {0}")]
    AssertionFailed(String),
    #[error("authenticator not registered")]
    NotRegistered,
    #[error("operation timed out after {0:?}")]
    Timeout(std::time::Duration),
    /// Mirrors upstream's `NoCredential` / `Cancelled` / `Backend` / `Flow`
    /// variants for the `PasskeyAuthenticator` trait impl below. We collapse
    /// upstream's 4 categories into one free-form `Upstream(String)` here so
    /// the downstream enum stays small; the message is enough for the operator
    /// log.
    #[error("upstream passkey error: {0}")]
    Upstream(String),
}

impl AssertionRequest {
    pub fn parse(json: &[u8]) -> Result<Self, PasskeyError> {
        // `PublicKeyCredentialRequestOptions` uses camelCase keys; mirror
        // upstream `parse_request_options` (`src/passkey/mod.rs:164-225`).
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            /// base64url-no-pad challenge.
            challenge: String,
            /// Relying-party id; upstream treats this as mandatory in
            /// `parse_request_options` (`src/passkey/mod.rs:164-225`). We
            /// mirror that by accepting an empty string as `None` so
            /// `PublicKeyCredentialRequestOptions` payloads that omit
            /// `rpId` (the WebAuthn spec allows it) still parse cleanly.
            #[serde(default)]
            rp_id: String,
            #[serde(default = "default_uv")]
            user_verification: String,
            #[serde(default = "default_timeout")]
            timeout: u64,
            #[serde(default)]
            allow_credentials: Vec<RawCred>,
        }
        #[derive(Deserialize)]
        struct RawCred {
            id: String,
        }
        fn default_uv() -> String {
            "preferred".to_string()
        }
        fn default_timeout() -> u64 {
            60_000
        }

        let raw: Raw = serde_json::from_slice(json)
            .map_err(|e| PasskeyError::InvalidOptions(format!("parse: {e}")))?;
        let challenge = base64_url_decode(&raw.challenge)
            .map_err(|e| PasskeyError::InvalidOptions(format!("challenge: {e}")))?;
        let user_verification = match raw.user_verification.as_str() {
            "required" => UserVerification::Required,
            "preferred" => UserVerification::Preferred,
            "discouraged" => UserVerification::Discouraged,
            other => {
                return Err(PasskeyError::InvalidOptions(format!(
                    "user_verification: {other}"
                )));
            }
        };
        let allow_credentials = raw
            .allow_credentials
            .into_iter()
            .map(|c| base64_url_decode(&c.id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PasskeyError::InvalidOptions(format!("credential id: {e}")))?;

        Ok(Self {
            challenge,
            rp_id: if raw.rp_id.is_empty() {
                None
            } else {
                Some(raw.rp_id)
            },
            allow_credentials,
            user_verification,
            timeout_ms: Some(raw.timeout),
            raw_options_json: String::from_utf8_lossy(json).into_owned(),
        })
    }
}

fn base64_url_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_options_minimal() {
        let json = br#"{
            "challenge": "Y2hhbGxlbmdlLWJ5dGVz",
            "rpId": "web.whatsapp.com",
            "userVerification": "preferred",
            "timeout": 60000,
            "allowCredentials": []
        }"#;
        let req = AssertionRequest::parse(json).expect("must parse");
        assert_eq!(req.rp_id.as_deref(), Some("web.whatsapp.com"));
        assert_eq!(req.user_verification, UserVerification::Preferred);
        assert_eq!(req.timeout_ms, Some(60_000));
        assert!(req.allow_credentials.is_empty());
    }

    #[test]
    fn parses_allow_credentials_with_decoded_ids() {
        // Two credential ids: raw bytes "abc" base64url-no-pad = "YWJj".
        let json = br#"{
            "challenge": "AA",
            "rpId": "web.whatsapp.com",
            "userVerification": "required",
            "timeout": 30000,
            "allowCredentials": [{"id": "YWJj"}, {"id": "AA"}]
        }"#;
        let req = AssertionRequest::parse(json).expect("must parse");
        assert_eq!(req.allow_credentials, vec![b"abc".to_vec(), vec![0x00u8]]);
        assert_eq!(req.user_verification, UserVerification::Required);
    }

    #[test]
    fn rejects_unknown_user_verification() {
        let json = br#"{
            "challenge": "AA",
            "rpId": "web.whatsapp.com",
            "userVerification": "suggested",
            "timeout": 60000,
            "allowCredentials": []
        }"#;
        let err = AssertionRequest::parse(json).expect_err("must fail");
        assert!(matches!(err, PasskeyError::InvalidOptions(_)));
    }

    #[test]
    fn rejects_bad_base64_in_challenge() {
        let json = br#"{
            "challenge": "!!!not_base64!!!",
            "rpId": "web.whatsapp.com",
            "userVerification": "preferred",
            "timeout": 60000,
            "allowCredentials": []
        }"#;
        let err = AssertionRequest::parse(json).expect_err("must fail");
        assert!(matches!(err, PasskeyError::InvalidOptions(_)));
    }

    #[test]
    fn parses_missing_rp_id_as_none() {
        let json = br#"{
            "challenge": "AA",
            "userVerification": "discouraged",
            "timeout": 60000,
            "allowCredentials": []
        }"#;
        let req = AssertionRequest::parse(json).expect("must parse");
        assert!(req.rp_id.is_none());
        assert_eq!(req.user_verification, UserVerification::Discouraged);
    }

    #[test]
    fn raw_options_json_is_preserved_verbatim() {
        let json = br#"{ "challenge": "AA", "rpId": "x" }"#;
        let req = AssertionRequest::parse(json).expect("must parse");
        assert_eq!(req.raw_options_json.as_bytes(), json);
    }
}
