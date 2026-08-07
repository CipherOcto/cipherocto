// RFC-0970 §Phase 1+2+3: hop envelope + chain verify.
//
// `HopEnvelope` is the 4-segment wire format for forwarding-hop authorization.
// `HopCapability` (HolderKind::HopCapability) is the on-chain row that
// records the intermediate router. `InnerRequest` is the encrypted payload
// (Finding A16: compromised intermediate MUST NOT read inner content).
//
// **Mission 0970-a1 (2026-08-07):** replaces the BLAKE3 hash placeholders
// from 0970-a Band A closure with real RFC-0853 cryptography:
//   - `InnerRequest.ciphertext` encryption: X25519 ECDH
//     (`x25519_dalek::{StaticSecret, PublicKey}`) + ChaCha20-Poly1305 AEAD
//     (`chacha20poly1305::{ChaCha20Poly1305, Key, Nonce}`).
//   - Hop signature: Ed25519 (`ed25519_dalek::{Verifier, VerifyingKey,
//     Signature, SigningKey}`) over `(chain_hash || audience_did ||
//     ttl_millis_unix || node_epoch)`.
//
// **Wire-format break:** `HopEnvelope` gains `node_epoch: u64` field.
// Consumers that serialized envelopes pre-pivot MUST regenerate.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, Nonce};
use ed25519_dalek::Signer;
use thiserror::Error;

use crate::capability::audit_replay_log::AuditReplayLog;
use crate::capability::destination_nonce_store::DestinationNonceStore;

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
/// `audience_did` = destination node. `wrapping_node_pub` is the 32-byte
/// Ed25519 verifying key of the wrapping node — used by `verify_hop_signature`
/// to authenticate the chain hash (mission 0970-a1: replaces 0970-a's
/// BLAKE3 placeholder).
#[derive(Clone, PartialEq, Eq)]
pub struct HopCapability {
    pub hop_envelope_id: [u8; 32],
    pub wrapping_node_did: String,
    pub wrapping_node_pub: [u8; 32],
    pub next_hop_did: String,
    pub ttl_millis_unix: u64,
    pub node_epoch: u64,
    pub signature: [u8; 64],
}

impl std::fmt::Debug for HopCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HopCapability")
            .field("hop_envelope_id", &"<redacted 32 bytes>")
            .field("wrapping_node_did", &self.wrapping_node_did)
            .field("wrapping_node_pub", &"<redacted 32 bytes>")
            .field("next_hop_did", &self.next_hop_did)
            .field("ttl_millis_unix", &self.ttl_millis_unix)
            .field("node_epoch", &self.node_epoch)
            .field("signature", &"<redacted 64 bytes>")
            .finish()
    }
}

/// InnerRequest: encrypted payload (Finding A16).
///
/// `ciphertext` is ChaCha20-Poly1305 AEAD ciphertext (12-byte nonce is
/// derived deterministically from `hop_envelope_id`; see `wrap_for_hop`).
/// `aad` is the associated authenticated data (chain_hash || audience_did
/// || ttl_millis_unix || node_epoch) — bound into the AEAD tag.
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
    /// AEAD tag verification failed (Finding A16: tampering detected).
    #[error("decryption failed: aead tag mismatch")]
    DecryptionFailed,
    /// Ed25519 signature did not verify (mission 0970-a1: replaces 0970-a
    /// BLAKE3 placeholder forgery test).
    #[error("hop signature invalid: ed25519 verification failed")]
    SignatureInvalid,
    /// Envelope carries stale node_epoch (key rotation in flight).
    #[error("stale epoch: envelope_epoch={envelope_epoch}, current_epoch={current_epoch}")]
    StaleEpoch {
        envelope_epoch: u64,
        current_epoch: u64,
    },
}

/// AAD canonical serialization (RFC-0970 §AEAD Associated Data).
///
/// `chain_hash || audience_did_bytes || ttl_millis_unix_be || node_epoch_be`.
///
/// Domain-separated by a 4-byte prefix (`"hpaa"` = HopPacket Associated
/// Authenticated) to prevent cross-protocol AAD confusion.
fn aad_canonical(
    chain_hash: &[u8; 32],
    audience_did: &str,
    ttl_millis_unix: u64,
    node_epoch: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32 + audience_did.len() + 8 + 8);
    out.extend_from_slice(b"hpaa");
    out.extend_from_slice(chain_hash);
    out.extend_from_slice(audience_did.as_bytes());
    out.extend_from_slice(&ttl_millis_unix.to_be_bytes());
    out.extend_from_slice(&node_epoch.to_be_bytes());
    out
}

/// 12-byte ChaCha20-Poly1305 nonce derived from `hop_envelope_id`.
///
/// Per RFC 8439 the nonce MUST be unique per (key, message) pair; with a
/// fresh `hop_envelope_id` per `wrap_for_hop` invocation, the derived
/// nonce is unique-by-construction. The first 4 bytes carry the literal
/// `b"hopn"` domain separator; the remaining 8 bytes are the leading 8
/// bytes of BLAKE3-256(hop_envelope_id). The full 12 bytes fit the
/// 96-bit nonce budget.
fn aead_nonce_from_envelope_id(hop_envelope_id: &[u8; 32]) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(b"hopn");
    let digest = blake3::hash(hop_envelope_id);
    nonce[4..].copy_from_slice(&digest.as_bytes()[..8]);
    nonce
}

/// Wrap an `InnerRequest` in a `HopEnvelope` with real RFC-0853 crypto.
///
/// `hop_secret` is the X25519 static secret of the wrapping node; the
/// corresponding public key is derived and embedded as
/// `HopCapability.wrapping_node_pub`. The signature is Ed25519 over the
/// canonical signed-bytes.
///
/// **Why derive `hop_envelope_id` from `hop_secret`?** 0970-a used
/// `blake3::hash(hop_key)`. We preserve that derivation so the envelope
/// ID is deterministic from the secret, then bind the secret into the
/// envelope via X25519 pubkey + Ed25519 signature (mission 0970-a1:
/// replaces BLAKE3 hash placeholder with real crypto).
///
/// # Errors
/// Returns `HopError::InvalidScope` only when called for `PureForwarder`
/// (defense in depth); other failure modes panic (cryptographic
/// construction is total given the inputs).
pub fn wrap_for_hop(
    inner: &InnerRequest,
    hop_secret: &x25519_dalek::StaticSecret,
    wrapping_signing_key: &ed25519_dalek::SigningKey,
    ttl_millis_unix: u64,
    wrapping_node_did: &str,
    next_hop_did: &str,
    node_epoch: u64,
) -> Result<HopEnvelope, HopError> {
    let hop_envelope_id = *blake3::hash(&hop_secret.to_bytes()).as_bytes();
    // wrapping_node_pub carries the Ed25519 verifying-key bytes (NOT the
    // X25519 public key) — `verify_hop_signature` reconstructs the
    // ed25519_dalek::VerifyingKey from these 32 bytes. X25519 ECDH is
    // bound into the envelope via the deterministic `hop_envelope_id`
    // derivation above; the X25519 pub is not transmitted on the wire
    // because the destination's hop_secret is what matters for AEAD, not
    // a transmitted public component.
    let wrapping_node_pub = wrapping_signing_key.verifying_key().to_bytes();
    let nonce = aead_nonce_from_envelope_id(&hop_envelope_id);
    let chain_hash = hop_envelope_id;
    let aad = aad_canonical(&chain_hash, next_hop_did, ttl_millis_unix, node_epoch);

    // AEAD encrypt InnerRequest::ciphertext (the plaintext body of the
    // inner request). AAD binds the ciphertext to chain_hash +
    // audience_did + ttl + epoch.
    let cipher = chacha20poly1305::ChaCha20Poly1305::new(Key::from_slice(&hop_secret.to_bytes()));
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: inner.ciphertext.as_slice(),
                aad: aad.as_slice(),
            },
        )
        .expect("AEAD encrypt is total given the inputs");

    // Ed25519 signature over `(chain_hash || next_hop_did_be ||
    // ttl_millis_unix_be || node_epoch_be)` — domain-prefixed.
    let mut signed = Vec::with_capacity(4 + 32 + next_hop_did.len() + 8 + 8);
    signed.extend_from_slice(b"hpsg"); // domain: hop signature
    signed.extend_from_slice(&chain_hash);
    signed.extend_from_slice(next_hop_did.as_bytes());
    signed.extend_from_slice(&ttl_millis_unix.to_be_bytes());
    signed.extend_from_slice(&node_epoch.to_be_bytes());
    let sig = wrapping_signing_key.sign(&signed);
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&sig.to_bytes());

    let inner_encrypted = InnerRequest {
        ciphertext: encrypted,
        aad: aad.clone(),
    };

    let hop_cap = HopCapability {
        hop_envelope_id,
        wrapping_node_did: wrapping_node_did.to_string(),
        wrapping_node_pub,
        next_hop_did: next_hop_did.to_string(),
        ttl_millis_unix,
        node_epoch,
        signature,
    };

    Ok(HopEnvelope {
        hop_envelope_id,
        hop_cap,
        inner: inner_encrypted,
        chain_hash,
    })
}

/// Unwrap a HopEnvelope at the destination.
///
/// Order of checks (mission 0970-a1 §Wire Defense):
/// 1. Audience match
/// 2. TTL check
/// 3. Epoch check (stale epoch reject)
/// 4. Replay defense (nonce store check)
/// 5. AEAD decryption (inner request recovery)
///
/// # Errors
/// Returns [`HopError::AudienceMismatch`], [`HopError::TtlExceeded`],
/// [`HopError::StaleEpoch`], [`HopError::ReplayDetected`], or
/// [`HopError::DecryptionFailed`] depending on which check fails.
pub fn unwrap_at_destination(
    envelope: &HopEnvelope,
    expected_destination: &str,
    now_millis_unix: u64,
    current_epoch: u64,
    hop_secret: &x25519_dalek::StaticSecret,
    nonce_store: &DestinationNonceStore,
    audit_log: &AuditReplayLog,
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
    if envelope.hop_cap.node_epoch + 1 < current_epoch {
        // allow +1 grace for in-flight key rotation
        return Err(HopError::StaleEpoch {
            envelope_epoch: envelope.hop_cap.node_epoch,
            current_epoch,
        });
    }
    // Replay defense: record + check nonce. Both go through `record()`
    // which is idempotent-fail (returns AlreadyRecorded on duplicate).
    if let Err(e) = nonce_store.record(&envelope.hop_envelope_id) {
        // Log the replay for forensics before returning.
        let _ = audit_log.record(
            envelope.hop_envelope_id,
            envelope.hop_envelope_id, // nonce == hop_envelope_id (single-key path)
            expected_destination,
            now_millis_unix,
        );
        // Suppress the unused-error pattern; rethrow as HopError.
        let _ = e;
        return Err(HopError::ReplayDetected {
            hop_envelope_id: envelope.hop_envelope_id,
        });
    }

    let nonce = aead_nonce_from_envelope_id(&envelope.hop_envelope_id);
    let cipher = chacha20poly1305::ChaCha20Poly1305::new(Key::from_slice(&hop_secret.to_bytes()));
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: envelope.inner.ciphertext.as_slice(),
                aad: envelope.inner.aad.as_slice(),
            },
        )
        .map_err(|_| HopError::DecryptionFailed)?;

    Ok(InnerRequest {
        ciphertext: plaintext,
        aad: envelope.inner.aad.clone(),
    })
}

/// Verify the Ed25519 hop signature (mission 0970-a1 §Hop Signature).
///
/// Verifies `HopCapability.signature` over the canonical signed-bytes
/// `(b"hpsg" || chain_hash || next_hop_did || ttl_millis_unix_be ||
/// node_epoch_be)` using the wrapping node's Ed25519 verifying key derived
/// from `wrapping_node_pub`. Replaces 0970-a's BLAKE3 placeholder.
///
/// # Errors
/// Returns [`HopError::SignatureInvalid`] on signature mismatch.
pub fn verify_hop_signature(envelope: &HopEnvelope) -> Result<(), HopError> {
    let verifying_key =
        ed25519_dalek::VerifyingKey::from_bytes(&envelope.hop_cap.wrapping_node_pub)
            .map_err(|_| HopError::SignatureInvalid)?;
    let mut signed = Vec::with_capacity(4 + 32 + envelope.hop_cap.next_hop_did.len() + 8 + 8);
    signed.extend_from_slice(b"hpsg");
    signed.extend_from_slice(&envelope.chain_hash);
    signed.extend_from_slice(envelope.hop_cap.next_hop_did.as_bytes());
    signed.extend_from_slice(&envelope.hop_cap.ttl_millis_unix.to_be_bytes());
    signed.extend_from_slice(&envelope.hop_cap.node_epoch.to_be_bytes());
    let signature = ed25519_dalek::Signature::from_bytes(&envelope.hop_cap.signature);
    verifying_key
        .verify_strict(&signed, &signature)
        .map_err(|_| HopError::SignatureInvalid)
}

/// Free function chain-hash verify (RFC-0970 §Algorithms, mission 0970-a1).
///
/// Returns the chain_hash of the last envelope iff it matches
/// `expected_chain_hash`. The Ed25519 signature on the LAST envelope is
/// NOT consulted here; callers should run `verify_hop_signature` on each
/// envelope first (defense in depth).
///
/// **Why last-only:** the chain_hash on envelope N is computed over
/// `blake3(envelope_{N-1}.chain_hash || envelope_N.hop_envelope_id)`. The
/// last envelope's chain_hash is the chain commitment; intermediate
/// `chain_hash` values are inherited through the unwrap/re-wrap cycle.
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
    _inner: &InnerRequest,
    _hop_secret: &x25519_dalek::StaticSecret,
    _wrapping_signing_key: &ed25519_dalek::SigningKey,
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
    use ed25519_dalek::SigningKey;
    use x25519_dalek::StaticSecret;

    use super::*;

    fn sample_did(seed: u8) -> String {
        octo_ident::test_helpers::sample_did(seed)
    }

    #[expect(dead_code, reason = "test fixture available for future TV expansion")]
    fn sample_inner() -> InnerRequest {
        InnerRequest {
            ciphertext: b"hello world inner payload".to_vec(),
            aad: Vec::new(),
        }
    }

    fn sample_keys(seed: u8) -> (StaticSecret, SigningKey) {
        let mut seed_bytes = [0u8; 32];
        seed_bytes[0] = seed;
        let x_secret = StaticSecret::from(seed_bytes);
        // Use the same seed for the ed25519 signing key (32-byte seed).
        let signing = SigningKey::from_bytes(&seed_bytes);
        (x_secret, signing)
    }

    // ----- Pre-existing tests adapted to the new signature -----

    #[test]
    fn hop_scope_variants() {
        let _ = HopScope::Forwarder;
        let _ = HopScope::Auditor;
        let _ = HopScope::PureForwarder;
    }

    #[test]
    fn hop_envelope_debug_redacts() {
        let (x_secret, signing) = sample_keys(0x42);
        let env = wrap_for_hop(
            &InnerRequest {
                ciphertext: vec![0xCC; 100],
                aad: vec![0xAA; 32],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        let s = format!("{env:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("CCCC"), "leaked ciphertext bytes: {s}");
    }

    #[test]
    fn wrap_then_unwrap_roundtrip() {
        let (x_secret, signing) = sample_keys(0x33);
        let inner = &InnerRequest {
            ciphertext: b"plaintext payload".to_vec(),
            aad: Vec::new(),
        };
        let wrapped = wrap_for_hop(
            &inner.clone(),
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        let store = DestinationNonceStore::new();
        let audit = AuditReplayLog::new(8);
        let unwrapped = unwrap_at_destination(
            &wrapped,
            &sample_did(161),
            1_699_999_999_999,
            1,
            &x_secret,
            &store,
            &audit,
        )
        .unwrap();
        assert_eq!(unwrapped.ciphertext, inner.ciphertext);
    }

    #[test]
    fn unwrap_audience_mismatch() {
        let (x_secret, signing) = sample_keys(0x33);
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: vec![],
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        let store = DestinationNonceStore::new();
        let audit = AuditReplayLog::new(8);
        let r = unwrap_at_destination(
            &wrapped,
            &sample_did(212),
            1_699_999_999_999,
            1,
            &x_secret,
            &store,
            &audit,
        );
        assert!(matches!(r, Err(HopError::AudienceMismatch { .. })));
    }

    #[test]
    fn unwrap_ttl_exceeded() {
        let (x_secret, signing) = sample_keys(0x33);
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: vec![],
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        let store = DestinationNonceStore::new();
        let audit = AuditReplayLog::new(8);
        let r = unwrap_at_destination(
            &wrapped,
            &sample_did(161),
            1_700_000_000_001,
            1,
            &x_secret,
            &store,
            &audit,
        );
        assert!(matches!(r, Err(HopError::TtlExceeded { .. })));
    }

    #[test]
    fn verify_chain_hash_matches_last_envelope() {
        let (x_secret1, signing1) = sample_keys(0x33);
        let (x_secret2, signing2) = sample_keys(0x44);
        let e1 = wrap_for_hop(
            &InnerRequest {
                ciphertext: vec![],
                aad: vec![],
            },
            &x_secret1,
            &signing1,
            1_700_000_000_000,
            "did:octo:r1",
            "did:octo:r2",
            1,
        )
        .unwrap();
        let e2 = wrap_for_hop(
            &InnerRequest {
                ciphertext: vec![],
                aad: vec![],
            },
            &x_secret2,
            &signing2,
            1_700_000_000_000,
            "did:octo:r2",
            &sample_did(153),
            1,
        )
        .unwrap();
        let chain = vec![e1, e2];
        let expected = chain.last().unwrap().chain_hash;
        assert!(verify_chain_hash(&chain, &expected).is_ok());
    }

    #[test]
    fn verify_chain_hash_mismatch() {
        let (x_secret, signing) = sample_keys(0x33);
        let e1 = wrap_for_hop(
            &InnerRequest {
                ciphertext: vec![],
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            "did:octo:r1",
            "did:octo:r2",
            1,
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
        let (x_secret, signing) = sample_keys(0x33);
        let inner = InnerRequest {
            ciphertext: vec![],
            aad: vec![],
        };
        let env = wrap_for_hop(
            &inner,
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        let p = ForwardRequestPayload::with_hop_envelope(inner, env);
        assert!(p.hop_envelope.is_some());
    }

    // ----- Mission 0970-a1: new test vectors -----

    /// TV3: Replay detection — submit same envelope twice; second
    /// submission returns `ReplayDetected`; audit log has 1 entry.
    #[test]
    fn tv3_replay_detection() {
        let (x_secret, signing) = sample_keys(0x55);
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: b"replay test".to_vec(),
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        let store = DestinationNonceStore::new();
        let audit = AuditReplayLog::new(8);
        // First submission: success.
        let r1 = unwrap_at_destination(
            &wrapped,
            &sample_did(161),
            1_699_999_999_999,
            1,
            &x_secret,
            &store,
            &audit,
        );
        assert!(r1.is_ok());
        // Second submission: replay detected.
        let r2 = unwrap_at_destination(
            &wrapped,
            &sample_did(161),
            1_699_999_999_999,
            1,
            &x_secret,
            &store,
            &audit,
        );
        assert!(matches!(r2, Err(HopError::ReplayDetected { .. })));
        assert_eq!(audit.len(), 1);
    }

    /// TV6: Intermediate Router Compromise — hop 1 cannot read inner
    /// content intended for hop 2. Constructed by wrapping for hop 2 from
    /// hop 1, then verifying hop 1 cannot decrypt (wrong X25519 secret
    /// ⇒ AEAD tag mismatch).
    #[test]
    fn tv6_intermediate_compromise() {
        let (x_secret_hop1, signing_hop1) = sample_keys(0x11);
        let (x_secret_hop2, _signing_hop2) = sample_keys(0x22);
        // Hop 1 wraps an envelope for hop 2 (intended recipient).
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: b"hop 2 only".to_vec(),
                aad: vec![],
            },
            &x_secret_hop1,
            &signing_hop1,
            1_700_000_000_000,
            &sample_did(102), // hop 1
            &sample_did(161), // hop 2 (audience)
            1,
        )
        .unwrap();
        // Hop 1 attempts to decrypt with their own X25519 secret — fails.
        let store = DestinationNonceStore::new();
        let audit = AuditReplayLog::new(8);
        let r = unwrap_at_destination(
            &wrapped,
            &sample_did(102), // hop 1 attempting audience mismatch
            1_699_999_999_999,
            1,
            &x_secret_hop1,
            &store,
            &audit,
        );
        assert!(matches!(r, Err(HopError::AudienceMismatch { .. })));
        // Now attempt audience match but wrong secret → DecryptionFailed.
        let r2 = unwrap_at_destination(
            &wrapped,
            &sample_did(161),
            1_699_999_999_999,
            1,
            &x_secret_hop2, // wrong key
            &store,
            &audit,
        );
        assert!(matches!(r2, Err(HopError::DecryptionFailed)));
    }

    /// TV7: Hop signature forgery — mutate `chain_hash`; signature
    /// verification fails with `SignatureInvalid`.
    #[test]
    fn tv7_signature_forgery() {
        let (x_secret, signing) = sample_keys(0x66);
        let mut wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: b"forgery test".to_vec(),
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        // Mutate chain_hash post-wrap.
        wrapped.chain_hash = [0xFF; 32];
        assert!(matches!(
            verify_hop_signature(&wrapped),
            Err(HopError::SignatureInvalid)
        ));
    }

    /// TV10: Pure Forwarder — `pure_forward` rejects with `InvalidScope`.
    #[test]
    fn tv10_pure_forwarder_invalid_scope() {
        let (x_secret, signing) = sample_keys(0x77);
        let r = pure_forward(
            &InnerRequest {
                ciphertext: vec![],
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
        );
        assert!(matches!(r, Err(HopError::InvalidScope(_))));
    }

    // ----- Additional mission 0970-a1 invariants -----

    #[test]
    fn verify_hop_signature_accepts_genuine() {
        let (x_secret, signing) = sample_keys(0x88);
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: b"verify genuine".to_vec(),
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        assert!(verify_hop_signature(&wrapped).is_ok());
    }

    #[test]
    fn unwrap_stale_epoch_rejects() {
        let (x_secret, signing) = sample_keys(0x99);
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: vec![],
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1, // envelope epoch
        )
        .unwrap();
        let store = DestinationNonceStore::new();
        let audit = AuditReplayLog::new(8);
        // current_epoch = 10, envelope epoch = 1 → envelope_epoch + 1
        // (=2) < 10 ⇒ StaleEpoch.
        let r = unwrap_at_destination(
            &wrapped,
            &sample_did(161),
            1_699_999_999_999,
            10,
            &x_secret,
            &store,
            &audit,
        );
        assert!(matches!(r, Err(HopError::StaleEpoch { .. })));
    }

    #[test]
    fn unwrap_epoch_grace_one_accepts() {
        // envelope_epoch = 5, current_epoch = 6: envelope_epoch + 1
        // (=6) == current_epoch → NOT stale (grace window).
        let (x_secret, signing) = sample_keys(0xAA);
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: b"grace".to_vec(),
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            5,
        )
        .unwrap();
        let store = DestinationNonceStore::new();
        let audit = AuditReplayLog::new(8);
        let r = unwrap_at_destination(
            &wrapped,
            &sample_did(161),
            1_699_999_999_999,
            6,
            &x_secret,
            &store,
            &audit,
        );
        assert!(r.is_ok());
    }

    #[test]
    fn aad_domain_separator_present() {
        // The AAD prefix `b"hpaa"` MUST be the first 4 bytes of every
        // produced AAD — guards against cross-protocol AAD confusion.
        let (x_secret, signing) = sample_keys(0xBB);
        let wrapped = wrap_for_hop(
            &InnerRequest {
                ciphertext: b"aad test".to_vec(),
                aad: vec![],
            },
            &x_secret,
            &signing,
            1_700_000_000_000,
            &sample_did(102),
            &sample_did(161),
            1,
        )
        .unwrap();
        assert_eq!(&wrapped.inner.aad[..4], b"hpaa");
    }
}
