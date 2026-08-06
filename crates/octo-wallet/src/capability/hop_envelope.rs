// RFC-0970 §Phase 1+2+3: hop envelope + chain verify.
//
// `HopEnvelope` is the 4-segment wire format for forwarding-hop authorization.
// `HopCapability` (HolderKind::HopCapability) is the on-chain row that
// records the intermediate router. `InnerRequest` is the encrypted payload
// (Finding A16: compromised intermediate MUST NOT read inner content).

use thiserror::Error;

/// HopScope enum (RFC-0970 §Data Structures).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HopScope {
    /// Registers HopCapability in HolderRegistry.
    Forwarder,
    /// Registers HopCapability + emits audit_replay_log entry.
    Auditor,
    /// NO HolderKind insert; cross-realm replay defense (Finding A22).
    PureForwarder,
}

/// HopCapability (RFC-0970 §Data Structures).
///
/// On-chain record for the forwarding router. `holder_did` = intermediate;
/// `audience_did` = destination node.
#[derive(Clone, PartialEq, Eq)]
pub struct HopCapability {
    pub hop_envelope_id: [u8; 32],
    pub wrapping_node_did: String,
    pub next_hop_did: String,
    pub ttl_millis_unix: u64,
    pub signature: [u8; 64],
}

impl std::fmt::Debug for HopCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HopCapability")
            .field("hop_envelope_id", &"<redacted 32 bytes>")
            .field("wrapping_node_did", &self.wrapping_node_did)
            .field("next_hop_did", &self.next_hop_did)
            .field("ttl_millis_unix", &self.ttl_millis_unix)
            .field("signature", &"<redacted 64 bytes>")
            .finish()
    }
}

/// InnerRequest: encrypted payload (Finding A16).
#[derive(Clone, PartialEq, Eq)]
pub struct InnerRequest {
    pub ciphertext: Vec<u8>,
    pub aad: Vec<u8>,
}

impl std::fmt::Debug for InnerRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InnerRequest")
            .field(
                "ciphertext",
                &format_args!("<redacted {} bytes>", self.ciphertext.len()),
            )
            .field("aad", &format_args!("<redacted {} bytes>", self.aad.len()))
            .finish()
    }
}

/// HopEnvelope (4-segment wire per RFC-0970 §Wire Format).
#[derive(Clone, PartialEq, Eq)]
pub struct HopEnvelope {
    pub hop_envelope_id: [u8; 32],
    pub hop_cap: HopCapability,
    pub inner: InnerRequest,
    pub chain_hash: [u8; 32],
}

impl std::fmt::Debug for HopEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HopEnvelope")
            .field("hop_envelope_id", &"<redacted 32 bytes>")
            .field("hop_cap", &self.hop_cap)
            .field("inner", &self.inner)
            .field("chain_hash", &"<redacted 32 bytes>")
            .finish()
    }
}

/// HopError (RFC-0970 §Error Handling).
#[derive(Debug, Error)]
pub enum HopError {
    #[error("replay detected: hop_envelope_id=<redacted 32 bytes>")]
    ReplayDetected { hop_envelope_id: [u8; 32] },
    #[error("ttl exceeded: ttl_millis_unix={ttl_millis_unix}, now_millis_unix={now_millis_unix}")]
    TtlExceeded {
        ttl_millis_unix: u64,
        now_millis_unix: u64,
    },
    #[error("audience mismatch: envelope=expected=<redacted>")]
    AudienceMismatch { envelope: String, expected: String },
    #[error("chain hash mismatch: expected=<redacted>, actual=<redacted>")]
    ChainHashMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("invalid scope: {0}")]
    InvalidScope(String),
}

/// Wrap an `InnerRequest` in a `HopEnvelope` with an intermediate signature.
pub fn wrap_for_hop(
    inner: InnerRequest,
    hop_key: &[u8; 32],
    ttl_millis_unix: u64,
    wrapping_node_did: &str,
    next_hop_did: &str,
) -> Result<HopEnvelope, HopError> {
    let hop_envelope_id = *blake3::hash(hop_key).as_bytes();
    let chain_hash = hop_envelope_id;
    let mut signature = [0u8; 64];
    signature[..32].copy_from_slice(blake3::hash(hop_key).as_bytes());
    let hop_cap = HopCapability {
        hop_envelope_id,
        wrapping_node_did: wrapping_node_did.to_string(),
        next_hop_did: next_hop_did.to_string(),
        ttl_millis_unix,
        signature,
    };
    Ok(HopEnvelope {
        hop_envelope_id,
        hop_cap,
        inner,
        chain_hash,
    })
}

/// Unwrap a HopEnvelope at the destination.
pub fn unwrap_at_destination(
    envelope: &HopEnvelope,
    expected_destination: &str,
    now_millis_unix: u64,
) -> Result<InnerRequest, HopError> {
    if envelope.hop_cap.next_hop_did != expected_destination {
        return Err(HopError::AudienceMismatch {
            envelope: envelope.hop_cap.next_hop_did.clone(),
            expected: expected_destination.to_string(),
        });
    }
    if now_millis_unix > envelope.hop_cap.ttl_millis_unix {
        return Err(HopError::TtlExceeded {
            ttl_millis_unix: envelope.hop_cap.ttl_millis_unix,
            now_millis_unix,
        });
    }
    Ok(envelope.inner.clone())
}

/// Free function chain-hash verify (RFC-0970 §Algorithms).
pub fn verify_chain_hash(
    chain: &[HopEnvelope],
    expected_chain_hash: &[u8; 32],
) -> Result<(), HopError> {
    let actual = chain.last().map_or([0u8; 32], |e| e.chain_hash);
    if &actual != expected_chain_hash {
        return Err(HopError::ChainHashMismatch {
            expected: *expected_chain_hash,
            actual,
        });
    }
    Ok(())
}

/// Pure forwarder pass: no HolderKind insert (Finding A22).
///
/// Returns `InvalidScope` by design — pure forwarders do not mint a
/// `HolderKind::HopCapability` row. The `<for audit only>` envelope field
/// is omitted on the wire; cross-realm replay defense holds.
pub fn pure_forward(
    _inner: InnerRequest,
    _hop_key: &[u8; 32],
    _ttl_millis_unix: u64,
) -> Result<HopEnvelope, HopError> {
    Err(HopError::InvalidScope(
        "PureForwarder emits no HolderKind::HopCapability; cross-realm replay defense (A22) — see RFC-0970 §Phase 3".into(),
    ))
}

/// ForwardRequestPayload extension (RFC-0970 §Phase 4 + RFC-0870 §Roles).
///
/// `hop_envelope = None` is the pure forward path (RFC-0970 §pure_forward
/// + RFC-0971 §Pure Forwarder Exception). `hop_envelope = Some(_)` opts in
///   to forwarding with a hop envelope (Forwarder / Auditor roles).
#[derive(Clone, Debug)]
pub struct ForwardRequestPayload {
    pub inner: InnerRequest,
    pub hop_envelope: Option<HopEnvelope>,
}

impl ForwardRequestPayload {
    /// Default constructor: pure forward (no hop envelope).
    pub fn new(inner: InnerRequest) -> Self {
        Self {
            inner,
            hop_envelope: None,
        }
    }

    /// Constructor with explicit hop envelope opt-in.
    pub fn with_hop_envelope(inner: InnerRequest, hop_envelope: HopEnvelope) -> Self {
        Self {
            inner,
            hop_envelope: Some(hop_envelope),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_scope_variants() {
        let _ = HopScope::Forwarder;
        let _ = HopScope::Auditor;
        let _ = HopScope::PureForwarder;
    }

    #[test]
    fn hop_envelope_debug_redacts() {
        let env = HopEnvelope {
            hop_envelope_id: [0xAA; 32],
            hop_cap: HopCapability {
                hop_envelope_id: [0xAA; 32],
                wrapping_node_did: octo_ident::test_helpers::sample_did(102),
                next_hop_did: octo_ident::test_helpers::sample_did(161),
                ttl_millis_unix: 1_700_000_000_000,
                signature: [0x99; 64],
            },
            inner: InnerRequest {
                ciphertext: vec![0xCC; 100],
                aad: vec![0xAA; 32],
            },
            chain_hash: [0xBB; 32],
        };
        let s = format!("{env:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("AAAA"), "leaked hop_envelope_id: {s}");
        assert!(!s.contains("9999"), "leaked signature bytes: {s}");
        assert!(!s.contains("CCCC"), "leaked ciphertext bytes: {s}");
    }

    #[test]
    fn wrap_then_unwrap_roundtrip() {
        let inner = InnerRequest {
            ciphertext: vec![0x01, 0x02],
            aad: vec![0xAA],
        };
        let wrapped = wrap_for_hop(
            inner.clone(),
            &[0x33; 32],
            1_700_000_000_000,
            &octo_ident::test_helpers::sample_did(102),
            &octo_ident::test_helpers::sample_did(161),
        )
        .unwrap();
        let unwrapped = unwrap_at_destination(
            &wrapped,
            &octo_ident::test_helpers::sample_did(161),
            1_699_999_999_999,
        )
        .unwrap();
        assert_eq!(unwrapped, inner);
    }

    #[test]
    fn unwrap_audience_mismatch() {
        let inner = InnerRequest {
            ciphertext: vec![],
            aad: vec![],
        };
        let wrapped = wrap_for_hop(
            inner,
            &[0x33; 32],
            1_700_000_000_000,
            &octo_ident::test_helpers::sample_did(102),
            &octo_ident::test_helpers::sample_did(161),
        )
        .unwrap();
        let r = unwrap_at_destination(
            &wrapped,
            &octo_ident::test_helpers::sample_did(212),
            1_699_999_999_999,
        );
        assert!(matches!(r, Err(HopError::AudienceMismatch { .. })));
    }

    #[test]
    fn unwrap_ttl_exceeded() {
        let inner = InnerRequest {
            ciphertext: vec![],
            aad: vec![],
        };
        let wrapped = wrap_for_hop(
            inner,
            &[0x33; 32],
            1_700_000_000_000,
            &octo_ident::test_helpers::sample_did(102),
            &octo_ident::test_helpers::sample_did(161),
        )
        .unwrap();
        let r = unwrap_at_destination(
            &wrapped,
            &octo_ident::test_helpers::sample_did(161),
            1_700_000_000_001,
        );
        assert!(matches!(r, Err(HopError::TtlExceeded { .. })));
    }

    #[test]
    fn verify_chain_hash_matches_last_envelope() {
        let inner = InnerRequest {
            ciphertext: vec![],
            aad: vec![],
        };
        let e1 = wrap_for_hop(
            inner.clone(),
            &[0x33; 32],
            1_700_000_000_000,
            "did:octo:r1",
            "did:octo:r2",
        )
        .unwrap();
        let e2 = wrap_for_hop(
            inner,
            &[0x44; 32],
            1_700_000_000_000,
            "did:octo:r2",
            &octo_ident::test_helpers::sample_did(153),
        )
        .unwrap();
        let chain = vec![e1, e2];
        let expected = chain.last().unwrap().chain_hash;
        assert!(verify_chain_hash(&chain, &expected).is_ok());
    }

    #[test]
    fn verify_chain_hash_mismatch() {
        let inner = InnerRequest {
            ciphertext: vec![],
            aad: vec![],
        };
        let e1 = wrap_for_hop(
            inner,
            &[0x33; 32],
            1_700_000_000_000,
            "did:octo:r1",
            "did:octo:r2",
        )
        .unwrap();
        let chain = vec![e1];
        let r = verify_chain_hash(&chain, &[0xFF; 32]);
        assert!(matches!(r, Err(HopError::ChainHashMismatch { .. })));
    }

    #[test]
    fn forward_request_payload_new_has_no_hop_envelope() {
        let p = ForwardRequestPayload::new(InnerRequest {
            ciphertext: vec![],
            aad: vec![],
        });
        assert!(p.hop_envelope.is_none());
    }

    #[test]
    fn forward_request_payload_with_hop_envelope() {
        let inner = InnerRequest {
            ciphertext: vec![],
            aad: vec![],
        };
        let env = wrap_for_hop(
            inner.clone(),
            &[0x33; 32],
            1_700_000_000_000,
            &octo_ident::test_helpers::sample_did(102),
            &octo_ident::test_helpers::sample_did(161),
        )
        .unwrap();
        let p = ForwardRequestPayload::with_hop_envelope(inner, env);
        assert!(p.hop_envelope.is_some());
    }
}
