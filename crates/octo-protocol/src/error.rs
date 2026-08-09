//! Protocol errors (RFC-0871 §Error Handling).

use octo_ident::DidError;

use thiserror::Error;

/// Errors surfaceable by NodeEnvelope construction, dispatch, and verification.
///
/// Per RFC-0871 §Error Handling, each variant maps to a specific caller-domain
/// failure mode. Receivers should propagate these via the `NodeEnvelope`
/// response (per RFC-0871 §Algorithms step 6).
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// `from_did` did not validate via `octo_ident::CanonicalCodec::parse()`.
    /// Per RFC-0871 §Adversary Analysis A7 (DID spoofing defense).
    #[error("invalid DID shape: {0}")]
    InvalidDid(String),

    /// Envelope expired (`expires_at_unix_ms <= now`). Per RFC-0871
    /// §Adversary Analysis A4 (TTL manipulation defense) and §Test Vectors TV2.
    #[error("envelope expired: now={now_unix_ms}, expires_at_unix_ms={expires_at_unix_ms}")]
    Expired {
        /// Current unix-time milliseconds at the receiver.
        now_unix_ms: u64,
        /// Sender-supplied TTL ceiling (unix-time milliseconds).
        expires_at_unix_ms: u64,
    },

    /// Replay detected: `envelope_id` already in the receiver's seen-set.
    /// Per RFC-0871 §Adversary Analysis A1 + §Test Vectors TV3.
    #[error("replay detected: envelope_id={0:?}")]
    ReplayDetected([u8; 32]),

    /// Nonce reuse within TTL: `nonce` was already observed for
    /// `(from_did, node_type)` within the TTL window.
    #[error("nonce reuse: from_did={from_did}, nonce={nonce:?}")]
    NonceReuse {
        /// Sender DID that re-used the nonce.
        from_did: String,
        /// Re-used nonce.
        nonce: [u8; 32],
    },

    /// Unknown payload kind (no handler registered). Per RFC-0871
    /// §Adversary Analysis A5 + RFC-0965 §3.2 fail-closed pattern.
    #[error("unknown payload kind: {0:?}")]
    UnknownPayloadKind([u8; 16]),

    /// One or more authorizations failed verification (logical AND per
    /// RFC-0871 §Adversary Analysis A6).
    #[error("authorization failed: {0}")]
    AuthorizationFailed(String),

    /// Unknown authorization discriminator in `Authorization::Raw`. Per
    /// RFC-0871 §Compatibility (forward-compat via Raw, fail-closed if no
    /// handler registered).
    #[error("unknown authorization discriminator: {0:?}")]
    UnknownAuthDiscriminator([u8; 16]),

    /// TTL exceeded the per-node-type ceiling declared in `RouterAnnouncePayload`.
    /// Per RFC-0871 §Adversary Analysis A4 (TTL manipulation defense).
    #[error(
        "TTL ceiling exceeded: requested={requested_unix_ms_offset_secs}s, ceiling={ceiling_secs}s"
    )]
    TtlCeilingExceeded {
        /// TTL offset in seconds (sender-supplied).
        requested_unix_ms_offset_secs: u64,
        /// Per-node-type ceiling (seconds).
        ceiling_secs: u64,
    },

    /// Borsh (de)serialization error.
    #[error("serialization error: {0}")]
    SerializationError(String),
}

impl From<DidError> for ProtocolError {
    fn from(e: DidError) -> Self {
        ProtocolError::InvalidDid(e.to_string())
    }
}

impl From<borsh::io::Error> for ProtocolError {
    fn from(e: borsh::io::Error) -> Self {
        ProtocolError::SerializationError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_did_error_to_protocol_error() {
        let e: ProtocolError = DidError::UnrecognizedShape.into();
        assert!(matches!(e, ProtocolError::InvalidDid(_)));
    }

    #[test]
    fn replay_detected_carries_envelope_id() {
        let id = [0xab; 32];
        let e = ProtocolError::ReplayDetected(id);
        assert_eq!(
            e.to_string(),
            format!("replay detected: envelope_id={:?}", id)
        );
    }

    #[test]
    fn expired_carries_timestamps() {
        let e = ProtocolError::Expired {
            now_unix_ms: 1000,
            expires_at_unix_ms: 500,
        };
        assert!(e.to_string().contains("now=1000"));
        assert!(e.to_string().contains("expires_at_unix_ms=500"));
    }
}
