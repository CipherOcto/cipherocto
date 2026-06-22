# Mission: 0862d — OCrypt Mission-Key Ring

## Status

Draft (awaiting adversarial review)

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §4.3.1 Identity, key hierarchy, and trust, §Appendix B Mission Key Derivation, §DatabaseSyncAdapter Trait (v1.1.0)

## Summary

Implement the OCrypt mission-key ring: derive the `transport_key` (for `SyncSummary.hmac`) and the `execution_key` (for ChaCha20-Poly1305 AEAD on `SyncSegment` / `WalTailChunk` payloads) from the `mission_root_key` via `HKDF-BLAKE3(mission_root_key, "sync:v1", mission_id)`. The new HKDF context `"sync:v1"` is to be documented in RFC-0853 §6 (Mission Cryptography).

This mission is split out of `0862-base` for parallel execution. It depends on `0862-base` for the identity derivation, but ships independently as a focused crypto module.

## Design

### New module: `octo-sync/src/keyring.rs` (leaf workspace at `cipherocto/octo-sync/src/keyring.rs`)

```rust
use octo_network::ocrypt::hkdf_blake3;  // RFC-0853 §1.1: HKDF-BLAKE3

pub struct MissionKeyRing {
    mission_id: [u8; 32],
    transport_key: [u8; 32],
    execution_key: [u8; 32],
}

impl MissionKeyRing {
    /// Derive the per-mission key ring from the mission_root_key.
    ///
    /// Per RFC-0862 §4.3.1 and §Appendix B:
    ///   HKDF-BLAKE3(salt="sync:v1", ikm=mission_root_key, info=mission_id)
    /// produces a 64-byte OKM split into:
    ///   - transport_key  (first 32 bytes): used for SyncSummary.hmac
    ///   - execution_key  (next 32 bytes):  used for ChaCha20-Poly1305 AEAD
    pub fn derive(mission_root_key: &[u8; 32], mission_id: [u8; 32]) -> Self {
        let mut okm = [0u8; 64];
        hkdf_blake3(b"sync:v1", mission_root_key, &mission_id, &mut okm)
            .expect("HKDF-BLAKE3 expand must succeed for 64-byte output");

        Self {
            mission_id,
            transport_key: okm[0..32].try_into().unwrap(),
            execution_key: okm[32..64].try_into().unwrap(),
        }
    }

    pub fn transport_key(&self) -> &[u8; 32] {
        &self.transport_key
    }

    pub fn execution_key(&self) -> &[u8; 32] {
        &self.execution_key
    }

    /// Compute the HMAC for a SyncSummary.
    /// HMAC-BLAKE3(transport_key, summary_body || node_id)
    pub fn summary_hmac(&self, summary_body: &[u8], node_id: &[u8; 32]) -> [u8; 32] {
        use blake3::keyed_hash;
        let mut input = Vec::with_capacity(summary_body.len() + 32);
        input.extend_from_slice(summary_body);
        input.extend_from_slice(node_id);
        *keyed_hash(&self.transport_key, &input).as_bytes()
    }

    /// AEAD encrypt a payload (used for SyncSegment and WalTailChunk).
    /// ChaCha20-Poly1305(execution_key, nonce=counter, aad=AAD)
    pub fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> (Vec<u8>, [u8; 12]) {
        use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, Aead, KeyInit};
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.execution_key));
        let nonce = [0u8; 12];  // per-mission counter; production MUST use a counter or random nonce
        let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce), chacha20poly1305::aead::Payload {
            msg: plaintext,
            aad,
        }).expect("ChaCha20-Poly1305 encrypt");
        (ciphertext, nonce)
    }

    /// AEAD decrypt a payload.
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8; 12], aad: &[u8]) -> Result<Vec<u8>> {
        use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce, Aead, KeyInit};
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.execution_key));
        cipher.decrypt(Nonce::from_slice(nonce), chacha20poly1305::aead::Payload {
            msg: ciphertext,
            aad,
        }).map_err(|_| SyncError::DecryptionFailed)
    }
}
```

### RFC-0853 amendment

The new HKDF context `"sync:v1"` must be documented in `rfcs/draft/networking/0853-overlay-cryptography.md` §6 (Mission Cryptography). This is a **§10.3 amendment** to RFC-0853.

The amendment text:
> **HKDF Context `"sync:v1"`:** The Stoolap Data Sync Protocol (RFC-0862) uses `HKDF-BLAKE3(mission_root_key, "sync:v1", mission_id)` to derive two 32-byte subkeys per mission:
> - `transport_key` (first 32 bytes of OKM): used for `HMAC-BLAKE3(transport_key, summary_body || node_id)` in `SyncSummary.hmac`.
> - `execution_key` (next 32 bytes of OKM): used for `ChaCha20-Poly1305(execution_key, nonce=counter, aad=AAD)` AEAD encryption of `SyncSegment` and `WalTailChunk` payloads.
>
> Both keys are per-mission; rotation of `mission_id` (not just `identity_epoch`) invalidates both.

### AAD binding

Per RFC-0862 §4.3.1, the AEAD AAD is:
```
aad = envelope_id || sender_ephemeral_public || mission_id || logical_timestamp || sequence
```

This binds the ciphertext to the envelope identity, sender, mission, timestamp, and sequence number. The same AAD format is used for both encryption and decryption.

## Acceptance Criteria

- [ ] `octo-sync/src/keyring.rs` (in the `octo-sync/` leaf workspace) exists with `MissionKeyRing` struct
- [ ] `MissionKeyRing::derive(mission_root_key, mission_id)` produces both `transport_key` and `execution_key` via HKDF-BLAKE3
- [ ] The HKDF call uses `octo_network::ocrypt::hkdf_blake3(salt="sync:v1", ikm=mission_root_key, info=mission_id)` (the cipherocto convention; salt is `"sync:v1"`, info is `mission_id`)
- [ ] The `transport_key` is the first 32 bytes of the OKM
- [ ] The `execution_key` is the next 32 bytes of the OKM
- [ ] `summary_hmac(summary_body, node_id)` returns `HMAC-BLAKE3(transport_key, summary_body || node_id)`
- [ ] `encrypt(plaintext, aad)` returns `(ciphertext, nonce)` with `ChaCha20-Poly1305(execution_key, nonce, aad)`
- [ ] `decrypt(ciphertext, nonce, aad)` returns `plaintext` and verifies the AEAD tag
- [ ] Round-trip test: `decrypt(encrypt(p, aad).0, encrypt(p, aad).1, aad) == p`
- [ ] Different `mission_root_key` produces different `transport_key` and `execution_key`
- [ ] Different `mission_id` produces different `transport_key` and `execution_key` (mission isolation)
- [ ] HMAC binding: different `transport_key` produces different HMAC
- [ ] HMAC binding: different `node_id` produces different HMAC
- [ ] RFC-0853 §6 amendment documenting `"sync:v1"` HKDF context is added

## Tests

- **Unit:**
  - `derive` is deterministic: same input → same output
  - `derive` with different `mission_root_key` → different keys
  - `derive` with different `mission_id` → different keys
  - `transport_key` is the first 32 bytes of OKM
  - `execution_key` is the next 32 bytes of OKM
  - `transport_key != execution_key`
  - `summary_hmac` is deterministic
  - `summary_hmac` with different `transport_key` → different HMAC
  - `summary_hmac` with different `node_id` → different HMAC
  - `encrypt/decrypt` round-trip
  - `decrypt` with tampered ciphertext → fails
  - `decrypt` with wrong AAD → fails
  - `decrypt` with wrong nonce → fails

- **Integration:**
  - Two `MissionKeyRing` instances with same `mission_root_key` and `mission_id` produce identical keys
  - Two `MissionKeyRing` instances with different `mission_id` produce different keys (cross-mission isolation)
  - HMAC computed by writer matches HMAC computed by reader (same `transport_key`)

## Dependencies

- **Requires:**
  - `0862-base` — for identity (consumes `OverlayIdentity.public_key`), **`DatabaseSyncAdapter` trait**, the `KeyRingStub` interface
  - RFC-0853 §1.1 (HKDF-BLAKE3 definition)
  - RFC-0853 §6 (Mission Cryptography — needs amendment for `"sync:v1"`)

- **Required by:**
  - `0862-base` (the base mission provides a keyring stub that 0862d fills in with the full `MissionKeyRing` implementation; the base mission does NOT implement the key derivation itself)
  - `0862a` (uses `execution_key` for `WalTailChunk` encryption)
  - `0862b` (uses `transport_key` for `SyncSummary.hmac`)
  - `0862c` (uses `execution_key` for `SyncSegment` encryption)

## Blockers / Dependencies

- **Blocked by:** RFC-0853 acceptance (RFC-0853 is currently Draft; this mission cannot start until RFC-0853 is Accepted)
- **Requires amendment to:** RFC-0853 §6 (Mission Cryptography) — add `"sync:v1"` HKDF context
- **Blocks:** `0862-base` integration, `0862a`, `0862b`, `0862c`

## Description

The mission-key ring is the cryptographic foundation of Sync. It derives the per-mission subkeys used for authentication (`transport_key` for `SyncSummary.hmac`) and confidentiality (`execution_key` for AEAD). The new HKDF context `"sync:v1"` extends RFC-0853's mission cryptography to include the Sync sub-protocol.

## Technical Details

### Why HKDF-BLAKE3 (not HKDF-SHA-256)?

RFC-0853 §1.1 specifies HKDF-BLAKE3 as the cipherocto standard. The Stoolap fork's `octo_determin` dependency also uses BLAKE3. Consistency with the rest of the stack.

### Why two separate keys (not one)?

Per RFC-0853 §1 and the security considerations in RFC-0862 §Adversary Analysis, using a single key for both HMAC and AEAD is a known anti-pattern (the "key separation" principle). Splitting into `transport_key` and `execution_key` allows one to be rotated independently of the other (e.g., if a `transport_key` is compromised, `execution_key` is still safe).

### AAD binding details

The AAD includes `logical_timestamp` and `sequence` to prevent replay attacks even within the same mission. Per RFC-0853 §7, the replay cache (1h or 10K entries) catches replays; the AAD is the cryptographic defense.

### Pitfalls

- **Don't use the same `nonce` for two different envelopes.** Each envelope MUST have a unique `nonce`. The current implementation uses `nonce = [0u8; 12]` for simplicity; production MUST use a counter or random nonce.
- **Don't derive `transport_key` and `execution_key` separately.** Derive them in a single HKDF call to ensure they're independent.
- **Don't store the `mission_root_key` in the keyring.** Only store the derived `transport_key` and `execution_key`; the root key is held by the mission layer.
- **Don't rotate the keys on every envelope.** Rotate only on `mission_id` change (which is a hard rotation) or on `identity_epoch` change (per RFC-0853 §12, 24h grace period).

---

**Mission Type:** Implementation
**Priority:** Critical
**Phase:** 1 (Core / MVE)
**RFC Section Coverage:** §4.3.1 Identity, key hierarchy, and trust; §Appendix B

## Type Coverage

This mission implements the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `MissionKeyRing` | The concrete implementation of the `KeyRing` trait; holds the derived `transport_key` and `execution_key` for a mission |
| `transport_key` (32 bytes) | The first 32 bytes of the HKDF-BLAKE3 OKM; used for `SyncSummary.hmac` via `HMAC-BLAKE3` |
| `execution_key` (32 bytes) | The next 32 bytes of the HKDF-BLAKE3 OKM; used for `ChaCha20-Poly1305` AEAD on `SyncSegment` and `WalTailChunk` payloads |

The mission does NOT implement the `KeyRing` trait itself (an interface stub) — that is in mission 0862-base. This mission fills in the trait's implementation. See the Type Coverage table in 0862-base for the full mapping.
