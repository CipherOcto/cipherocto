# Mission: 0862l — Per-Mission Key Isolation

## Status

Planned

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4; RFC-0853 (Overlay Cryptography) §6 (Mission Cryptography)

## Summary

Add per-mission key isolation to the sync carrier layer. PRIVATE missions encrypt sync payloads with mission-specific keys; PUBLIC missions send in clear text.

This is a Phase 4 requirement per RFC-0862 §Implementation Phases Phase 4: "Per-mission key isolation (PRIVATE missions encrypted; PUBLIC missions in clear)".

## Design

### New module: `octo-sync/src/mission_crypto.rs`

```rust
/// Mission privacy level (per RFC-0862 §4.3.1).
pub enum MissionPrivacy {
    /// Encrypted with mission-specific key. Only trusted peers can decrypt.
    Private,
    /// Sent in clear text. Any peer can read.
    Public,
}

/// Per-mission encryption context.
pub struct MissionCrypto {
    /// The mission's AEAD key (derived from mission_root_key via HKDF-BLAKE3).
    key: [u8; 32],
    /// The mission's privacy level.
    privacy: MissionPrivacy,
}

impl MissionCrypto {
    /// Encrypt a payload for a PRIVATE mission.
    pub fn encrypt(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, SyncError> {
        match self.privacy {
            MissionPrivacy::Public => Ok(plaintext.to_vec()),
            MissionPrivacy::Private => {
                // AEAD encrypt with mission key
                // ...
            }
        }
    }

    /// Decrypt a payload from a PRIVATE mission.
    pub fn decrypt(&self, ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, SyncError> {
        match self.privacy {
            MissionPrivacy::Public => Ok(ciphertext.to_vec()),
            MissionPrivacy::Private => {
                // AEAD decrypt with mission key
                // ...
            }
        }
    }
}
```

### Integration with carrier layer

In `MultiCarrierSync::broadcast`, before sending:
```rust
let encrypted = self.crypto.encrypt(envelope, &domain_bytes)?;
carrier.send(&encrypted).await
```

On receive, the sync engine decrypts before applying.

## Acceptance Criteria

- [ ] `MissionCrypto` struct with encrypt/decrypt methods
- [ ] PRIVATE missions use AEAD encryption (HKDF-BLAKE3 key derivation)
- [ ] PUBLIC missions send payloads in clear text
- [ ] Carrier layer integrates encryption transparently
- [ ] Unit tests for: encrypt/decrypt round-trip, public passthrough, wrong key fails
- [ ] Integration test: PRIVATE mission sync works end-to-end

## Dependencies

- **Requires:** `0862g` (cross-carrier sync), `0862d` (OCrypt mission key ring)
- **Required by:** none

## Complexity

Low (~100 lines). Leverages existing `MissionKeyRing` from 0862d.
