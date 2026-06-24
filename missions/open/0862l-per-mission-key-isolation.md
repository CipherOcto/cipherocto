# Mission: 0862l — Per-Mission Key Isolation

## Status

Planned

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4; RFC-0853 (Overlay Cryptography) §6 (Mission Cryptography)

## Summary

Integrate per-mission key isolation into the sync carrier layer. PRIVATE missions encrypt sync payloads with mission-specific keys; PUBLIC missions send in clear text.

This is a Phase 4 requirement per RFC-0862 §Implementation Phases Phase 4: "Per-mission key isolation (PRIVATE missions encrypted; PUBLIC missions in clear)".

## Design

### What already exists

- `MissionKeyRing` (`octo-sync/src/keyring.rs:52,145`) already provides `encrypt(plaintext, aad) -> (ciphertext, nonce)` and `decrypt(ciphertext, nonce, aad) -> Result<Vec<u8>>` with ChaCha20-Poly1305 AEAD.
- `MultiCarrierSync` (`octo-sync/src/carrier.rs`) broadcasts raw `&[u8]` envelopes without encryption.

### What's missing: integration glue

The gap is **not** a new crypto module — it's wiring the existing `MissionKeyRing` into the carrier layer:

1. **Privacy level metadata**: Store whether a mission is PRIVATE or PUBLIC in `SyncConfig` or a new `MissionPrivacy` enum.
2. **Carrier-layer encryption**: `MultiCarrierSync::broadcast` must encrypt PRIVATE payloads before sending.
3. **Receiver-side decryption**: The sync engine must decrypt before applying.

### New module: `octo-sync/src/mission_crypto.rs`

```rust
/// Mission privacy level (per RFC-0862 §4.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionPrivacy {
    /// Encrypted with mission-specific key. Only trusted peers can decrypt.
    Private,
    /// Sent in clear text. Any peer can read.
    Public,
}

/// Wrapper that adds privacy-aware encryption to the carrier layer.
///
/// Uses the existing `MissionKeyRing` for AEAD operations.
pub struct MissionCrypto {
    /// The mission's key ring (already has encrypt/decrypt).
    keyring: Arc<MissionKeyRing>,
    /// The mission's privacy level.
    privacy: MissionPrivacy,
}

impl MissionCrypto {
    /// Encrypt a payload. PUBLIC missions return plaintext passthrough.
    pub fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> (Vec<u8>, [u8; 12]) {
        match self.privacy {
            MissionPrivacy::Public => (plaintext.to_vec(), [0u8; 12]),
            MissionPrivacy::Private => self.keyring.encrypt(plaintext, aad),
        }
    }

    /// Decrypt a payload. PUBLIC missions return ciphertext passthrough.
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8; 12], aad: &[u8]) -> Result<Vec<u8>, SyncError> {
        match self.privacy {
            MissionPrivacy::Public => Ok(ciphertext.to_vec()),
            MissionPrivacy::Private => self.keyring.decrypt(ciphertext, nonce, aad),
        }
    }
}
```

### Integration with carrier layer

`MultiCarrierSync` gains a `crypto: Option<Arc<MissionCrypto>>` field. In `broadcast`:
```rust
let (payload, nonce) = match &self.crypto {
    Some(crypto) => {
        let (ct, n) = crypto.encrypt(envelope, &domain_bytes);
        (ct, Some(n))
    }
    None => (envelope.to_vec(), None),
};
// For PRIVATE missions, nonce is prepended: [12-byte nonce][ciphertext]
// For PUBLIC missions, payload is plaintext (nonce is zeros)
carrier.send(&payload).await
```

On receive, the sync engine extracts the nonce (first 12 bytes for PRIVATE missions), then calls `decrypt(ciphertext, &nonce, aad)`.

## Acceptance Criteria

- [ ] `MissionPrivacy` enum (Private/Public)
- [ ] `MissionCrypto` struct wrapping `MissionKeyRing` with encrypt/decrypt
- [ ] `MultiCarrierSync` gains `crypto: Option<Arc<MissionCrypto>>` field
- [ ] `broadcast` encrypts PRIVATE payloads before sending
- [ ] PUBLIC missions send payloads in clear text (passthrough)
- [ ] Unit tests: encrypt/decrypt round-trip, public passthrough, wrong key fails
- [ ] Integration test: PRIVATE mission sync works end-to-end

## Dependencies

- **Requires:** `0862g` (cross-carrier sync), `0862d` (OCrypt mission key ring)
- **Required by:** none

## Complexity

Low (~80 lines). Leverages existing `MissionKeyRing` encrypt/decrypt. Main work is integration glue.

## Changelog

- **Round 1** (2026-06-23): Fixed redundant MissionCrypto design — leverages existing MissionKeyRing. Added integration details for carrier layer. Clarified what's new vs what exists.
