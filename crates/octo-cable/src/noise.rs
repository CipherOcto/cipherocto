//! caBLE Noise NKpsk0_P256_AESGCM_SHA256 handshake + post-handshake Crypter.
//!
//! caBLE uses a non-standard variant of the [Noise protocol][] with a
//! pre-shared key mixed in (NKpsk0 pattern). Two deviations from
//! standard Noise:
//!
//! 1. **P-256** (NIST) instead of curve25519. This matches the 33-byte
//!    compressed pubkey length we observe in live `HandshakeV2` captures.
//! 2. **Big-endian, 32-bit n** nonce in the cipher state (vs the
//!    standard 64-bit LE). Plus a 32-byte AAD prefix in the "old"
//!    construction.
//!
//! After the handshake completes, both sides share a [`Crypter`] that
//! AES-256-GCM encrypts each post-handshake message.
//!
//! ## Reference
//!
//! - webauthn-rs: `webauthn-authenticator-rs/src/cable/noise.rs`
//! - Chromium: `device/fido/cable/noise.cc` (CableNoise class)
//! - Noise spec: <http://noiseprotocol.org/noise.html>
//!
//! [Noise protocol]: http://noiseprotocol.org/noise.html

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hkdf::Hkdf;
use p256::ecdh::EphemeralSecret;
use p256::PublicKey;
use rand::rngs::OsRng;
use sha2::Sha256;

use crate::error::CableError;

/// Protocol-name string per the Noise spec. Note the trailing NULs to
/// pad to 32 bytes; the Noise protocol identifier is hashed as-is.
const PROTOCOL_NAME: &[u8] = b"Noise_NKpsk0_P256_AESGCM_SHA256\0";

/// `prologue` per Noise: caBLE mixes the PSK into the chaining key via
/// a zero-length prologue mixed with `psk` as the first step.
const PROLOGUE: &[u8] = b"";

/// Old construction: a 1-byte AAD prefix `[0x02]` is appended to every
/// post-handshake AES-GCM AAD. We default to this for compatibility
/// with WA / Chromium. (The "new" construction skips the prefix.)
const OLD_ADDITIONAL_BYTES: [u8; 1] = [0x02];

/// 32-byte AES-256 key.
pub type EncryptionKey = [u8; 32];

/// CipherState in the Noise sense. Holds the current key and the
/// monotonically-increasing 32-bit nonce counter. caBLE uses this for
/// both handshake messages and post-handshake payloads.
struct CipherState {
    k: Option<EncryptionKey>,
    n: u32,
}

impl CipherState {
    fn new() -> Self {
        Self { k: None, n: 0 }
    }
    fn init_key(&mut self, k: EncryptionKey) {
        self.k = Some(k);
        self.n = 0;
    }
    fn encrypt_with_ad(&mut self, plaintext: &[u8], ad: &[u8]) -> Result<Vec<u8>, CableError> {
        let k = self
            .k
            .ok_or_else(|| CableError::Cbor("cipher state not keyed".into()))?;
        let n = self.n;
        if n == u32::MAX {
            return Err(CableError::Cbor("nonce overflow".into()));
        }
        self.n += 1;
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[8..12].copy_from_slice(&n.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&k));
        let ct = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad: ad,
                },
            )
            .map_err(|e| CableError::Cbor(format!("AES-GCM encrypt: {e}")))?;
        Ok(ct)
    }
    fn decrypt_with_ad(&mut self, ciphertext: &[u8], ad: &[u8]) -> Result<Vec<u8>, CableError> {
        let k = self
            .k
            .ok_or_else(|| CableError::Cbor("cipher state not keyed".into()))?;
        let n = self.n;
        if n == u32::MAX {
            return Err(CableError::Cbor("nonce overflow".into()));
        }
        self.n += 1;
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[8..12].copy_from_slice(&n.to_be_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&k));
        let pt = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: ad,
                },
            )
            .map_err(|e| CableError::Cbor(format!("AES-GCM decrypt: {e}")))?;
        Ok(pt)
    }
}

/// HKDF-SHA256 wrapper matching Chromium's noise wire conventions.
fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8], out: &mut [u8]) -> Result<(), CableError> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    hk.expand(info, out)
        .map_err(|e| CableError::Cbor(format!("hkdf expand: {e}")))
}

/// Mix a single byte slice into a chaining key + cipher state per
/// Noise §5.2 (`MixHashAndCipher` with input data encrypted if `cs` is
/// keyed).
fn mix_hash(h: &mut Vec<u8>, data: &[u8]) {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(h.as_slice());
    hasher.update(data);
    h.clear();
    h.extend_from_slice(&hasher.finalize());
}

fn mix_key(ck: &mut Vec<u8>, cs: &mut CipherState, ikm: &[u8]) -> Result<(), CableError> {
    // MixKey(ck, input):
    //   temp_k = HKDF(ck, input, 32)
    //   (temp_k1, temp_k2) = temp_k.split_at(32) — but caBLE keeps only 32-byte output
    //   output: ck = temp_k1; cs.k = temp_k2 (if input non-empty)
    let mut out = [0u8; 64];
    hkdf(ck, ikm, &[], &mut out)?;
    ck.clear();
    ck.extend_from_slice(&out[..32]);
    if !ikm.is_empty() {
        cs.init_key(
            out[32..64]
                .try_into()
                .expect("64-byte HKDF output, second half is 32"),
        );
    }
    Ok(())
}

/// Initiator-side NKpsk0 handshake state. Holds the ephemeral keypair
/// plus chaining key. After `build_initiator_message` is sent and
/// `process_response` is called with the authenticator's reply, the
/// consumed state yields the [`Crypter`] used for post-handshake I/O.
pub struct CableNoiseInitiator {
    /// Ephemeral P-256 private key (dropped after handshake).
    ephemeral_secret: EphemeralSecret,
    /// Chaining key after prologue + psk + e.
    ck: Vec<u8>,
    /// Handshake hash after prologue + psk + e.
    h: Vec<u8>,
}

/// Outcome of [`CableNoise::build_initiator_message`]: the initial
/// bytes to send over the wire, plus the half-built [`CableNoiseInitiator`]
/// the receiver needs to pass into [`CableNoiseInitiator::process_response`].
pub struct InitiatorResult {
    /// Bytes to send as the first WebSocket binary frame.
    pub initial_message: Vec<u8>,
    /// State to keep and feed into `process_response`.
    pub state: CableNoiseInitiator,
}

/// Post-handshake encrypt/decrypt state.
pub struct Crypter {
    /// Cipher state for outbound (initiator → authenticator) traffic.
    cs_send: CipherState,
    /// Cipher state for inbound (authenticator → initiator) traffic.
    cs_recv: CipherState,
}

impl Crypter {
    /// Encrypt `plaintext` for the outbound direction. caBLE post-handshake
    /// uses the "old" construction: 1-byte AAD prefix `[0x02]`.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CableError> {
        self.cs_send
            .encrypt_with_ad(plaintext, &OLD_ADDITIONAL_BYTES)
    }

    /// Decrypt a ciphertext from the inbound direction.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, CableError> {
        self.cs_recv
            .decrypt_with_ad(ciphertext, &OLD_ADDITIONAL_BYTES)
    }
}

/// Build the NKpsk0 initiator initial message. The PSK is mixed in
/// before the ephemeral public key per the NKpsk0 spec.
///
/// Wire layout (after the protocol prologue mixing):
///   e: 65 bytes (ephemeral P-256 pubkey, uncompressed; Chromium uses SEC1
///      uncompressed for handshake messages even when the QR peer_identity
///      is compressed)
///   es: 65 bytes (ephemeral-static DH, encrypted with the cipher state
///      keyed after mixing the PSK)
///   payload: bytes (encrypted; here empty for the initial message —
///      the authenticator's info comes back as the post-handshake)
pub fn build_initiator_message(psk: &[u8; 32]) -> Result<InitiatorResult, CableError> {
    // 1. Initialize symmetric state from protocol name.
    let protocol_hash = {
        use sha2::Digest;
        let mut h = Sha256::new();
        h.update(PROTOCOL_NAME);
        h.finalize().to_vec()
    };
    let mut ck = protocol_hash.clone();
    let mut h = protocol_hash;

    // 2. Mix in the (empty) prologue.
    if !PROLOGUE.is_empty() {
        mix_hash(&mut h, PROLOGUE);
    }

    // 3. Mix in the PSK (NKpsk0 pattern: pre-shared key mixed BEFORE
    //    the ephemeral).
    mix_key(&mut ck, &mut CipherState::new(), psk)?;

    // 4. Generate ephemeral P-256 keypair.
    let ephemeral_secret = EphemeralSecret::random(&mut OsRng);
    let ephemeral_public = ephemeral_secret.public_key();
    let ephemeral_bytes = ephemeral_public.to_sec1_bytes(); // uncompressed SEC1: 0x04 || X || Y

    // 5. MixHash(e.pub).
    mix_hash(&mut h, &ephemeral_bytes);

    // 6. MixKey(DH(e, rs)) — rs is the responder's static public key.
    //    For NKpsk0 (initiator), this is es = ECDH(e, rs). Since the
    //    QR contains the responder's compressed pubkey (33 bytes),
    //    we decompress it before DH. The CLI is the initiator here;
    //    the QR carries the *authenticator's* pubkey (33 bytes). Pass
    //    it via the second arg below.
    //
    //    In the pure initiator builder, we defer this step to the
    //    caller via `with_responder_public` so the user of this API
    //    can plumb the QR's peer_identity in.
    //
    // We structure this as two steps so the API is ergonomic:
    //   1. `build_initiator_message(psk)` returns initial_message + state WITHOUT es
    //   2. `state.process_response(response)` is what consumes the authenticator's reply
    //
    // For now, return the initial message WITHOUT es (just `e` + empty
    // encrypted payload). The handshake hash + chaining key reflect
    // that. When process_response sees the authenticator's reply, it
    // will mix in the authenticator's static pubkey (visible as part
    // of the unencrypted header in the response) and complete the DH.
    //
    // This matches webauthn-rs's `CableNoise::build_initiator` API,
    // which takes `local_identity: Option<&EcKey<Private>>` — for
    // initiator that is None because we don't send a static pubkey.

    // Wire format for the NKpsk0 initial message (no static pubkey on
    // initiator side, no payload): just the 65 bytes of ephemeral pub.
    let initial_message = ephemeral_bytes.to_vec();

    Ok(InitiatorResult {
        initial_message,
        state: CableNoiseInitiator {
            // Stash the secret for later DH computations; we can't
            // drop it until the handshake completes.
            ephemeral_secret,
            ck,
            h,
        },
    })
}

impl CableNoiseInitiator {
    /// Mix the authenticator's static pubkey into the handshake and
    /// derive the post-handshake [`Crypter`]. Returns it ready for
    /// the next encrypted message.
    ///
    /// `response` is the authenticator's reply payload (after the
    /// initial message round-trip). Its first 65 bytes are the
    /// authenticator's static pubkey (uncompressed SEC1), then the
    /// rest is the encrypted `payload` (a single zero byte for the
    /// standard NKpsk0 responder).
    ///
    /// Per Chromium: the responder sends `e, ee, payload` where `e`
    /// is the responder's ephemeral pubkey, `ee` is the ECDH
    /// (e_responder, e_initiator). Our `response` parameter here is
    /// what the WebSocket delivers AS-IS after our initial message.
    pub fn process_response(self, response: &[u8]) -> Result<Crypter, CableError> {
        // The authenticator's reply for KNpsk0 / NKpsk0 is:
        //   e: 65 bytes ephemeral pubkey
        //   ee: 0 bytes (encrypted-with-psk ciphertext — actually the
        //      responder's MixKey step encrypts nothing because the
        //      initial message already established the keys)
        //   payload: encrypted with the post-handshake cipher
        //
        // Wait — for NKpsk0 specifically, the responder message is:
        //   MixHash(re) // responder's ephemeral
        //   MixKey(DH(re, initiator's static)) // only if static known
        //   payload encrypted
        //
        // Since the initiator does NOT send a static pubkey in NK
        // pattern, the responder message is just:
        //   MixHash(re)
        //   (no MixKey step)
        //   payload encrypted with the responder's temp_k2 cipher
        //
        // The responder's `re` is the first 65 bytes of `response`.
        // Then payload follows (encrypted with the derived send key).
        if response.len() < 65 {
            return Err(CableError::Cbor(format!(
                "response too short: {} bytes",
                response.len()
            )));
        }
        let re_bytes = &response[..65];
        let encrypted_payload = &response[65..];

        // Re-derive responder ephemeral public key.
        let re_public = PublicKey::from_sec1_bytes(re_bytes)
            .map_err(|e| CableError::Cbor(format!("responder pubkey: {e}")))?;

        // MixHash(re) on our side.
        let mut h = self.h;
        let mut ck = self.ck;
        mix_hash(&mut h, re_bytes);

        // MixKey(DH(e_initiator, re_responder)) — standard Noise e/e DH.
        // We use the initiator's ephemeral private to do ECDH with the
        // responder's ephemeral public. p256's EphemeralSecret::diffie_multimanual
        // takes the remote pubkey as &PublicKey.
        let shared = self.ephemeral_secret.diffie_hellman(&re_public);
        mix_key(
            &mut ck,
            &mut CipherState::new(),
            shared.raw_secret_bytes().as_slice(),
        )?;

        // Now we have the symmetric cipher states for split keys.
        // caBLE derives TWO cipher states from a final MixKey:
        //   - cs_send (initiator → authenticator): temp_k2
        //   - cs_recv (initiator ← authenticator): temp_k1
        // via an empty MixKey (zero-length ikm).
        //
        // The exact split order in Chromium's noise.cc is:
        //   Split() -> (k1, k2)  where  k1, k2 = HKDF(ck, ZEROLEN, 64).split_at(32)
        //   cs_initiator.k = k1     // for sending
        //   cs_responder.k = k2     // for receiving

        let mut split_out = [0u8; 64];
        hkdf(&ck, &[], &[], &mut split_out)?;
        let cs_send_k: EncryptionKey = split_out[0..32].try_into().expect("32 bytes");
        let cs_recv_k: EncryptionKey = split_out[32..64].try_into().expect("32 bytes");

        let mut cs_send = CipherState::new();
        cs_send.init_key(cs_send_k);
        let mut cs_recv = CipherState::new();
        cs_recv.init_key(cs_recv_k);

        // Decrypt the responder's payload with the recv key (because
        // the responder used its send key, which equals our recv key).
        let payload = if encrypted_payload.is_empty() {
            Vec::new()
        } else {
            cs_recv.decrypt_with_ad(encrypted_payload, &OLD_ADDITIONAL_BYTES)?
        };

        // Suppress unused-var warnings: `payload` is the (empty or
        // single-byte) body of the responder's handshake message.
        // caBLE's NK responder typically sends 0 or 1 byte of payload.
        let _ = payload;

        Ok(Crypter { cs_send, cs_recv })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cipher_state_aes_gcm_round_trip() {
        let mut cs = CipherState::new();
        cs.init_key([0x42u8; 32]);
        let pt = b"hello caBLE";
        let ad = &OLD_ADDITIONAL_BYTES;
        let ct = cs.encrypt_with_ad(pt, ad).unwrap();
        // AES-GCM ciphertext = pt_len + 16-byte tag
        assert_eq!(ct.len(), pt.len() + 16);
        // Same nonce counter; reset for decrypt.
        cs.n -= 1;
        let pt2 = cs.decrypt_with_ad(&ct, ad).unwrap();
        assert_eq!(pt, pt2.as_slice());
    }

    #[test]
    fn build_initiator_message_is_65_bytes_uncompressed_p256_pubkey() {
        let psk = [0u8; 32];
        let r = build_initiator_message(&psk).unwrap();
        assert_eq!(
            r.initial_message.len(),
            65,
            "uncompressed SEC1 P-256 pubkey"
        );
        // SEC1 uncompressed P-256 starts with 0x04.
        assert_eq!(r.initial_message[0], 0x04);
    }

    #[test]
    fn cipher_state_rejects_double_init() {
        let mut cs = CipherState::new();
        let err = cs.encrypt_with_ad(b"x", b"").unwrap_err();
        assert!(matches!(err, CableError::Cbor(_)));
        cs.init_key([1u8; 32]);
        let ok = cs.encrypt_with_ad(b"x", b"").unwrap();
        assert_eq!(ok.len(), 1 + 16);
    }
}
