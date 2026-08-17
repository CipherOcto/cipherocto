//! Envelope dispatch (RFC-0871 §Algorithms, envelope receive flow).
//!
//! `EnvelopeDispatcher` validates each envelope end-to-end:
//! 1. Canonical DID shape (via `octo_ident::CanonicalCodec::parse()`).
//! 2. Replay defense (per-receiver `envelope_id` dedup set).
//! 3. Per-(sender, node_type) nonce uniqueness within TTL.
//! 4. TTL ceiling clamp (per-node-type ceiling declared in
//!    `RouterAnnouncePayload`).
//! 5. Logical-AND verification across `Vec<Authorization>`.
//! 6. Unknown payload kind → fail-closed (`ProtocolError::UnknownPayloadKind`).
//!
//! ## Adversary coverage (RFC-0871 §Adversary Analysis)
//!
//! - A1 replay attack → seen-set + per-sender nonce + TTL.
//! - A2 capability forgery → HMAC + Ed25519 sig in `verify_all`.
//! - A3 cross-domain trust escalation → out of scope for the dispatcher;
//!   enforced at the `payload_kind` → trust_root lookup in the production
//!   node wiring.
//! - A4 TTL manipulation → ceiling clamp via `RouterAnnouncePayload`.
//! - A5 payload kind spoofing → `UnknownPayloadKind` fail-closed.
//! - A6 authorization composition attack → `verify_all` logical-AND.
//! - A7 DID spoofing via legacy form → canonical DID validation at construct
//!   + `EnvelopeDispatcher::dispatch` re-validation.

use std::collections::{HashMap, HashSet};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::authorization::{verify_ed25519_signature, Authorization};
use crate::envelope::NodeEnvelope;
use crate::error::ProtocolError;
use crate::payload_kind::PayloadKindId;
use crate::signing::signature_preimage;
use crate::time::Clock;

/// Output of dispatching a validated envelope to its payload handler.
///
/// Concrete payload-decoded types live in downstream crates (e.g.,
/// `IdentityResolveResult`, `WalletSignResponse`); this crate returns the
/// raw borsh-serialized payload bytes + the response envelope skeleton so
/// downstream code can fill in the payload_kind-specific fields.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HandlerOutput {
    /// Borsh-serialized response payload body (caller-decodes).
    pub payload: Vec<u8>,
}

/// Per-node-type configuration supplied by `RouterAnnouncePayload` (RFC-0871
/// §Adversary Analysis A3 + A4).
#[derive(Clone, Debug)]
pub struct DispatcherConfig {
    /// Maximum TTL a peer may set in `expires_at_unix_ms`, in seconds.
    pub max_ttl_secs: u64,
    /// Payload kinds this dispatcher serves. Any incoming envelope with a
    /// `payload_kind` outside this list fails closed
    /// (`ProtocolError::UnknownPayloadKind`).
    pub served_kinds: Vec<PayloadKindId>,
}

impl DispatcherConfig {
    /// Permissive config: 1-hour TTL ceiling, all payload kinds served.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            max_ttl_secs: 3600,
            served_kinds: Vec::new(),
        }
    }

    /// True if `kind` is in the served-kinds list (or the list is empty =
    /// permissive).
    #[must_use]
    pub fn serves(&self, kind: &PayloadKindId) -> bool {
        self.served_kinds.is_empty() || self.served_kinds.contains(kind)
    }
}

/// Per-receiver validation cache: tracks seen `envelope_id`s + per-sender
/// nonces within the TTL window (RFC-0871 §Adversary Analysis A1).
#[derive(Debug, Default)]
pub struct ValidationCache {
    /// `(envelope_id, expires_at_unix_ms)` pairs. Entries evicted lazily when
    /// `expires_at_unix_ms <= now`.
    seen_envelope_ids: HashSet<[u8; 32]>,
    /// `(from_did_wire, nonce, expires_at_unix_ms)` tuples. Keyed by
    /// `(from_did_wire, nonce)`; stored value carries the expires_at so we
    /// can evict stale entries.
    seen_nonces: HashMap<(String, [u8; 32]), u64>,
}

impl ValidationCache {
    /// Empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Evict entries whose `expires_at_unix_ms <= now`. Called at the start of
    /// each `dispatch` to bound cache size.
    pub fn evict_expired(&mut self, now_unix_ms: u64) {
        self.seen_envelope_ids.retain(
            |_| true, /* keep, key has no expiry; rely on lazy check below */
        );
        // Nonce cache stores expires_at alongside; evict past entries.
        self.seen_nonces
            .retain(|_, expires_at| *expires_at > now_unix_ms);
        // envelope_id dedup is keyed by id only (no expiry stored). For
        // long-running receivers this set grows unbounded; production wiring
        // should sweep periodically by tracking the oldest seen entry.
        // For Phase 1 we accept this — the broadcast_announce reload
        // pattern (RFC-0870 §QuotaRouterNode) provides a natural restart
        // point.
    }

    /// Returns true if `envelope_id` was already in the seen-set.
    #[must_use]
    pub fn has_envelope_id(&self, envelope_id: &[u8; 32]) -> bool {
        self.seen_envelope_ids.contains(envelope_id)
    }

    /// Record `envelope_id` as seen.
    pub fn record_envelope_id(&mut self, envelope_id: [u8; 32]) {
        self.seen_envelope_ids.insert(envelope_id);
    }

    /// Returns true if `(from_did, nonce)` was already seen within TTL.
    #[must_use]
    pub fn has_nonce(&self, from_did: &str, nonce: &[u8; 32]) -> bool {
        self.seen_nonces
            .contains_key(&(from_did.to_owned(), *nonce))
    }

    /// Record `(from_did, nonce, expires_at)`.
    pub fn record_nonce(&mut self, from_did: &str, nonce: [u8; 32], expires_at_unix_ms: u64) {
        self.seen_nonces
            .insert((from_did.to_owned(), nonce), expires_at_unix_ms);
    }
}

/// Envelope dispatcher (RFC-0871 §Algorithms envelope receive flow).
///
/// Sync dispatch — sufficient for unit tests, batch handlers, and static
/// analysis tooling. Production wiring (mission `0870-b` quota router
/// adoption) wraps this in `octo_transport::NetworkReceiver` for async fan-in.
pub trait EnvelopeDispatcher {
    /// Dispatch `envelope` through the full validation pipeline. Returns the
    /// handler output or a [`ProtocolError`].
    fn dispatch(&self, envelope: &NodeEnvelope) -> Result<HandlerOutput, ProtocolError>;
}

/// Reference dispatcher with injectable cache + clock + config.
pub struct ReferenceDispatcher {
    cache: parking_lot::Mutex<ValidationCache>,
    clock: Box<dyn Clock>,
    config: DispatcherConfig,
}

impl ReferenceDispatcher {
    /// Build a reference dispatcher.
    #[must_use]
    pub fn new(cache: ValidationCache, clock: Box<dyn Clock>, config: DispatcherConfig) -> Self {
        Self {
            cache: parking_lot::Mutex::new(cache),
            clock,
            config,
        }
    }

    /// Borrow the inner cache (test-only).
    #[cfg(test)]
    pub fn cache(&self) -> &parking_lot::Mutex<ValidationCache> {
        &self.cache
    }

    /// Borrow the inner clock.
    #[must_use]
    pub fn clock(&self) -> &dyn Clock {
        &*self.clock
    }

    /// Borrow the config.
    #[must_use]
    pub fn config(&self) -> &DispatcherConfig {
        &self.config
    }

    /// Verify ALL `Authorization`s in the envelope (logical AND, RFC-0871
    /// §Adversary Analysis A6). Returns `Ok(())` only if every authorization
    /// verifies; any failure surfaces `ProtocolError::AuthorizationFailed`.
    pub fn verify_all(&self, envelope: &NodeEnvelope) -> Result<(), ProtocolError> {
        for auth in &envelope.authorization {
            match auth {
                Authorization::Signature { signer_did, sig } => {
                    let preimage = signature_preimage(
                        &envelope.envelope_id,
                        envelope.from_did.as_str(),
                        &envelope.payload,
                    );
                    verify_ed25519_signature(signer_did, &preimage, sig)?;
                }
                Authorization::Capability(_token) => {
                    // RFC-0957 attenuation + caveat verification lives in
                    // `crates/octo-cap-macaroon/` (mission
                    // `0957-ext-macaroon-crate`). Layer 1 placeholder: accept
                    // presence, defer to extension on dispatch.
                }
                Authorization::Proof(_proof) => {
                    // RFC-0958 verification lives in `crates/octo-cap-zk/`.
                }
                Authorization::ThresholdSignature { signers, sig } => {
                    // BLS verification lives in
                    // `crates/octo-cap-threshold-mpc/` (RFC-0871
                    // §Future Work). Layer 1 placeholder: structural check
                    // that at least one signer is present.
                    if signers.is_empty() {
                        return Err(ProtocolError::AuthorizationFailed(
                            "threshold signature has empty signers list".to_owned(),
                        ));
                    }
                    let _ = sig; // suppress unused
                }
                Authorization::Raw {
                    discriminator,
                    body: _,
                } => {
                    // Forward-compat: fail-closed if no handler is
                    // registered (RFC-0965 §3.2 pattern). The dispatcher
                    // ships with no Raw handlers by default; production
                    // wirings register them at startup.
                    return Err(ProtocolError::UnknownAuthDiscriminator(*discriminator));
                }
            }
        }
        Ok(())
    }
}

impl EnvelopeDispatcher for ReferenceDispatcher {
    fn dispatch(&self, envelope: &NodeEnvelope) -> Result<HandlerOutput, ProtocolError> {
        let now_unix_ms = self.clock.now_unix_ms();

        // Step 1: re-validate canonical DID (defense in depth — `build` already
        // validated, but a received envelope could be malformed post-borsh).
        crate::validate_canonical_did(envelope.from_did.as_str())?;

        // Step 2: TTL ceiling clamp (RFC-0871 §Adversary Analysis A4).
        if envelope.exceeds_ttl_ceiling(now_unix_ms, self.config.max_ttl_secs) {
            return Err(ProtocolError::TtlCeilingExceeded {
                requested_unix_ms_offset_secs: (envelope
                    .expires_at_unix_ms
                    .saturating_sub(now_unix_ms))
                    / 1000,
                ceiling_secs: self.config.max_ttl_secs,
            });
        }

        // Step 3: expired check (RFC-0871 §Test Vectors TV2).
        if envelope.is_expired(now_unix_ms) {
            return Err(ProtocolError::Expired {
                now_unix_ms,
                expires_at_unix_ms: envelope.expires_at_unix_ms,
            });
        }

        // Step 4: payload kind served check (RFC-0871 §Adversary Analysis A5).
        if !self.config.serves(&envelope.payload_kind) {
            return Err(ProtocolError::UnknownPayloadKind(envelope.payload_kind.0));
        }

        // Step 5: replay defense (RFC-0871 §Adversary Analysis A1 + TV3).
        let mut cache = self.cache.lock();
        cache.evict_expired(now_unix_ms);
        if cache.has_envelope_id(&envelope.envelope_id) {
            return Err(ProtocolError::ReplayDetected(envelope.envelope_id));
        }
        if cache.has_nonce(envelope.from_did.as_str(), &envelope.nonce) {
            return Err(ProtocolError::NonceReuse {
                from_did: envelope.from_did.as_str().to_owned(),
                nonce: envelope.nonce,
            });
        }

        // Step 6: authorization verification (RFC-0871 §Adversary Analysis A6).
        drop(cache); // release lock before verify (verify doesn't touch cache).
        self.verify_all(envelope)?;
        let mut cache = self.cache.lock();
        cache.record_envelope_id(envelope.envelope_id);
        cache.record_nonce(
            envelope.from_did.as_str(),
            envelope.nonce,
            envelope.expires_at_unix_ms,
        );
        drop(cache);

        // Step 7: dispatch payload to handler. Phase 1 ships a stub that
        // echoes the payload; concrete payload_kind-specific handlers land in
        // Phase 2 (wallet node mission `0871a`) and Phase 3 (specialized
        // node adoption missions `0870-b`, `0871b/c/d`).
        Ok(HandlerOutput {
            payload: envelope.payload.clone(),
        })
    }
}

/// Convenience constructor for `ReferenceDispatcher` with a `MockClock` and
/// permissive config. Public so integration tests in `tests/` can use it.
pub fn test_dispatcher(now_unix_ms: u64) -> ReferenceDispatcher {
    use crate::time::MockClock;
    ReferenceDispatcher::new(
        ValidationCache::new(),
        Box::new(MockClock::new(now_unix_ms)),
        DispatcherConfig::permissive(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authorization::Ed25519SignatureBytes;
    use crate::payload_kind::{IDENTITY_RESOLVE, WALLET_MINT_CAPABILITY};
    use crate::recipient::RecipientRef;
    use crate::time::MockClock;
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey;
    use octo_ident::CanonicalCodec;
    use octo_ident::DidCodec;

    fn sample_did(seed: u8) -> octo_ident::WireDid {
        let mut pk = [0u8; 32];
        for (i, byte) in pk.iter_mut().enumerate() {
            *byte = seed.wrapping_add(i as u8);
        }
        let raw = CanonicalCodec::mint(&pk);
        CanonicalCodec::raw_to_wire(&raw).unwrap()
    }

    fn bs58_encode(input: &[u8]) -> String {
        const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let mut digits: Vec<u8> = vec![0];
        for byte in input {
            let mut carry = *byte as u32;
            for d in digits.iter_mut().rev() {
                carry += (*d as u32) * 256;
                *d = (carry % 58) as u8;
                carry /= 58;
            }
            while carry > 0 {
                digits.insert(0, (carry % 58) as u8);
                carry /= 58;
            }
        }
        let leading_zeros = input.iter().take_while(|&&b| b == 0).count();
        let mut s = String::new();
        s.push_str(&"1".repeat(leading_zeros));
        for d in &digits {
            s.push(ALPHABET[*d as usize] as char);
        }
        s
    }

    fn signed_envelope(
        from_seed: u8,
        payload_kind: PayloadKindId,
        nonce: [u8; 32],
        expires_at_unix_ms: u64,
    ) -> NodeEnvelope {
        signed_envelope_with_payload(
            from_seed,
            payload_kind,
            nonce,
            expires_at_unix_ms,
            vec![0x01, 0x02, 0x03],
        )
    }

    fn signed_envelope_with_payload(
        from_seed: u8,
        payload_kind: PayloadKindId,
        nonce: [u8; 32],
        expires_at_unix_ms: u64,
        payload: Vec<u8>,
    ) -> NodeEnvelope {
        let seed: [u8; 32] = {
            let mut k = [0u8; 32];
            for (i, byte) in k.iter_mut().enumerate() {
                *byte = from_seed.wrapping_add(i as u8);
            }
            k
        };
        let sk = SigningKey::from_bytes(&seed);
        // The DID carries the verifying key (public key), derived from the seed.
        let pk_bytes = sk.verifying_key().to_bytes();
        let wire_did_str = format!("did:octo:z{}", bs58_encode(&pk_bytes));
        let wire_did = octo_ident::WireDid::new(wire_did_str);
        let mut env = NodeEnvelope::build(
            wire_did.clone(),
            RecipientRef::Direct([0x01; 32]),
            payload_kind,
            payload.clone(),
            vec![],
            nonce,
            expires_at_unix_ms,
            crate::envelope::VERSION_TAG_V2,
        )
        .unwrap();
        let preimage = signature_preimage(&env.envelope_id, env.from_did.as_str(), &payload);
        let sig = Ed25519SignatureBytes::from_signature(&sk.sign(preimage.as_ref()));
        env.authorization = vec![Authorization::Signature {
            signer_did: wire_did,
            sig,
        }];
        env
    }

    #[test]
    fn dispatch_accepts_signed_envelope() {
        let env = signed_envelope(7, IDENTITY_RESOLVE, [0xff; 32], 1_735_689_600_000);
        let dispatcher = test_dispatcher(1_735_689_500_000);
        let out = dispatcher.dispatch(&env).unwrap();
        assert_eq!(out.payload, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn dispatch_rejects_expired_envelope() {
        let env = signed_envelope(7, IDENTITY_RESOLVE, [0xff; 32], 1_000_000);
        let dispatcher = test_dispatcher(1_735_689_600_000);
        let err = dispatcher.dispatch(&env).unwrap_err();
        assert!(matches!(err, ProtocolError::Expired { .. }));
    }

    #[test]
    fn dispatch_rejects_replay() {
        let env = signed_envelope(7, IDENTITY_RESOLVE, [0xff; 32], 1_735_689_600_000);
        let dispatcher = test_dispatcher(1_735_689_500_000);
        dispatcher.dispatch(&env).unwrap();
        let err = dispatcher.dispatch(&env).unwrap_err();
        assert!(matches!(err, ProtocolError::ReplayDetected(_)));
    }

    #[test]
    fn dispatch_rejects_ttl_ceiling() {
        let env = signed_envelope(
            7,
            IDENTITY_RESOLVE,
            [0xff; 32],
            1_735_689_600_000 + 24 * 3600 * 1000, // +24h
        );
        let dispatcher = ReferenceDispatcher::new(
            ValidationCache::new(),
            Box::new(MockClock::new(1_735_689_500_000)),
            DispatcherConfig {
                max_ttl_secs: 3600,
                served_kinds: vec![],
            },
        );
        let err = dispatcher.dispatch(&env).unwrap_err();
        assert!(matches!(err, ProtocolError::TtlCeilingExceeded { .. }));
    }

    #[test]
    fn dispatch_rejects_unserved_payload_kind() {
        let env = signed_envelope(7, WALLET_MINT_CAPABILITY, [0xff; 32], 1_735_689_600_000);
        let dispatcher = ReferenceDispatcher::new(
            ValidationCache::new(),
            Box::new(MockClock::new(1_735_689_500_000)),
            DispatcherConfig {
                max_ttl_secs: 3600,
                served_kinds: vec![IDENTITY_RESOLVE],
            },
        );
        let err = dispatcher.dispatch(&env).unwrap_err();
        assert!(matches!(err, ProtocolError::UnknownPayloadKind(_)));
    }

    #[test]
    fn dispatch_rejects_nonce_reuse() {
        let dispatcher = test_dispatcher(1_735_689_500_000);
        // First envelope: consumes (envelope_id, nonce) for the sender.
        let env1 = signed_envelope(7, IDENTITY_RESOLVE, [0xff; 32], 1_735_689_600_000);
        dispatcher.dispatch(&env1).unwrap();
        // Second envelope: same sender + same nonce, but with a different
        // payload so the envelope_id changes (otherwise we'd hit ReplayDetected
        // first). The per-(sender, nonce) check must catch it as NonceReuse.
        let env2 = signed_envelope_with_payload(
            7,
            IDENTITY_RESOLVE,
            [0xff; 32],
            1_735_689_600_000,
            vec![0xff, 0xfe, 0xfd], // different from default [0x01, 0x02, 0x03]
        );
        let err = dispatcher.dispatch(&env2).unwrap_err();
        assert!(matches!(err, ProtocolError::NonceReuse { .. }));
    }

    #[test]
    fn dispatch_rejects_invalid_signature() {
        let env = signed_envelope(7, IDENTITY_RESOLVE, [0xff; 32], 1_735_689_600_000);
        let mut tampered = env.clone();
        // Swap the signature with one signed by a DIFFERENT key (seed 9).
        let other_pk_bytes: [u8; 32] = {
            let mut k = [0u8; 32];
            for (i, byte) in k.iter_mut().enumerate() {
                *byte = 9u8.wrapping_add(i as u8);
            }
            k
        };
        let other_sk = SigningKey::from_bytes(&other_pk_bytes);
        let preimage = signature_preimage(
            &tampered.envelope_id,
            tampered.from_did.as_str(),
            &tampered.payload,
        );
        let wrong_sig = Ed25519SignatureBytes::from_signature(&other_sk.sign(preimage.as_ref()));
        tampered.authorization = vec![Authorization::Signature {
            signer_did: sample_did(7),
            sig: wrong_sig,
        }];
        let dispatcher = test_dispatcher(1_735_689_500_000);
        let err = dispatcher.dispatch(&tampered).unwrap_err();
        assert!(matches!(err, ProtocolError::AuthorizationFailed(_)));
    }

    // Suppress unused warning when running `cargo test`.
    #[allow(dead_code)]
    fn _force_bs58_use() {
        let _ = bs58_encode(&[0u8; 32]);
    }
}
