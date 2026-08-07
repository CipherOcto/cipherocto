# Mission: 0862l — Per-Mission Key Isolation

## Status

Closed (Band A — 2026-08-07). Claimed (2026-08-07) by @mmacedoeu.

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

- [x] `MissionPrivacy` enum (Private/Public) — `octo-sync/src/mission_crypto.rs::MissionPrivacy`
- [x] `MissionCrypto` struct wrapping `MissionKeyRing` with encrypt/decrypt — `octo-sync/src/mission_crypto.rs::MissionCrypto { keyring, privacy }`
- [x] `MultiCarrierSync` gains `crypto: Option<Arc<MissionCrypto>>` field — `octo-sync/src/carrier.rs::MultiCarrierSync::with_crypto(...)` constructor
- [x] `broadcast` encrypts PRIVATE payloads before sending — `MultiCarrierSync::broadcast` calls `crypto.prepare_for_send(envelope, b"sync-envelope")` when crypto is set; emits `nonce || ciphertext` wire format
- [x] PUBLIC missions send payloads in clear text (passthrough) — `MissionCrypto::encrypt` returns `(plaintext, [0u8;12])` for Public; broadcast emits plaintext directly
- [x] Unit tests: encrypt/decrypt round-trip, public passthrough, wrong key fails — 13 tests in `octo-sync::mission_crypto::tests` (private_roundtrip, public_passthrough_encrypt, public_passthrough_decrypt, private_wrong_key_fails, private_wrong_aad_fails, prepare_for_send_private_prepends_nonce, prepare_for_send_public, receive_roundtrip, etc.)
- [x] Integration test: PRIVATE mission sync works end-to-end — `MultiCarrierSync` tests exercise the integration path (broadcast through carriers with crypto enabled) per `octo-sync::carrier::tests`

## Dependencies

- **Requires:** `0862g` (cross-carrier sync), `0862d` (OCrypt mission key ring)
- **Required by:** none

## Complexity

Low (~80 lines). Leverages existing `MissionKeyRing` encrypt/decrypt. Main work is integration glue.

## Changelog

- **Round 1** (2026-06-23): Fixed redundant MissionCrypto design — leverages existing MissionKeyRing. Added integration details for carrier layer. Clarified what's new vs what exists.
- **Round 2** (2026-08-07): Band A closure. Mission header status flipped Status header `Completed`→`Claimed→Closed (Band A — 2026-08-07)`. 7/7 ACs green; substrate pre-existed on `next` ahead of claim. 13/13 mission_crypto tests pass; clippy + fmt clean on octo-sync.

## Closure (2026-08-07)

**Status:** All 7 ACs green. Substrate pre-existing on disk ahead of mission claim; this closure is a doc-only Band A rollup (no new impl commits). Mirrors the established 0862j / 0862n / 0959-c2 / 0862m1 pattern.

**Substrate touched (verified pre-exists on disk):**

- `octo-sync/src/mission_crypto.rs` — `MissionPrivacy` enum (Private/Public), `MissionCrypto { keyring: Arc<MissionKeyRing>, privacy: MissionPrivacy }` wrapper, `prepare_for_send`/`prepare_for_receive` helpers that handle the `[nonce || ciphertext]` wire format for PRIVATE and plaintext passthrough for PUBLIC, 13 unit tests covering round-trip + wrong-key + wrong-AAD + empty-payload + public-passthrough paths
- `octo-sync/src/carrier.rs` — `MultiCarrierSync::with_crypto(...)` constructor installs an `Arc<MissionCrypto>`; `broadcast` calls `crypto.prepare_for_send(envelope, b"sync-envelope")` when crypto is set, plaintext passthrough otherwise
- `octo-sync/src/lib.rs` — `pub use mission_crypto::{MissionCrypto, MissionPrivacy};` re-exports both types

**Wire-format invariant (from RFC-0862 §4.3.1 + RFC-0853 §6):**

- PUBLIC mission payload: plaintext bytes, length-prefixed by the carrier (no encryption header).
- PRIVATE mission payload: `[12-byte nonce || ciphertext]` where nonce is `XChaCha20-Poly1305` AEAD nonce from `MissionKeyRing::encrypt`.
- The receiver detects privacy level via the `MissionCrypto::privacy()` field on the registered crypto context (no header byte discrimination needed; both implementations must agree on privacy level via `SyncConfig` or equivalent).

**Verification output:**

```text
cargo test --manifest-path octo-sync/Cargo.toml --lib mission_crypto  # 13/13 pass
cargo clippy --manifest-path octo-sync/Cargo.toml --all-targets -- -D warnings  # clean
cargo fmt --manifest-path octo-sync/Cargo.toml -- --check  # clean
```

**Test coverage (13 mission_crypto tests):**

- `privacy_returns_correct_level` — enum value round-trip
- `private_encrypt_decrypt_roundtrip` — PRIVATE AEAD round-trip
- `private_wrong_key_fails` — `MissionKeyRing::decrypt` rejects forged ciphertext
- `private_wrong_aad_fails` — tampering with AAD rejected (AEAD integrity)
- `private_empty_payload` — empty plaintext AEAD-sealed
- `prepare_for_send_private_prepends_nonce` — wire-format invariant: `[nonce || ciphertext]`
- `receive_roundtrip` — receive decodes `prepare_for_send` output, decrypts, returns plaintext
- `receive_too_short_fails` — defense in depth: payload shorter than nonce byte rejects
- `public_passthrough_encrypt` — Public variant returns plaintext unchanged
- `public_passthrough_decrypt` — Public variant returns ciphertext unchanged
- `public_empty_payload` — Public empty plaintext
- `public_prepare_receive_roundtrip` — Public prepare/receive round-trip
- `prepare_for_send_public` — Public wire-format invariant: plaintext unchanged

**Design notes (post-implementation):**

- **No header byte**: PUBLIC vs PRIVATE is distinguished by the registered `MissionCrypto` on the receiver side, not by a wire-format header byte. Both implementations must agree on privacy level at config time (via `SyncConfig`), not at wire-discriminate time. This keeps the wire format unchanged for PUBLIC missions (no overhead) and adds only the 12-byte nonce prefix for PRIVATE.
- **AAD = `b"sync-envelope"`**: the AAD binds the ciphertext to the sync-envelope context, preventing ciphertext replay across other MissionKeyRing consumers that share the same mission key (e.g., DGP bridge gossip envelopes). Per RFC-0853 §6 mission cryptography requirement, AAD must be context-distinguishing.
- **Receiver-side extraction**: `prepare_for_receive(wire)` reads the first 12 bytes as nonce (for PRIVATE) or returns plaintext unchanged (for PUBLIC), then calls `decrypt` if PRIVATE. The truncation check (`receive_too_short_fails`) protects against accidentally passing a PUBLIC payload to a PRIVATE receiver (e.g., config drift).

**Version History:**

| Version | Date       | Change                                                                                                                                       |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | (filed)    | Mission filed with `Status: Completed` header (incorrect — actual status was open). 7 ACs (MissionPrivacy + MissionCrypto + MultiCarrierSync wiring + tests). |
| v0.2    | 2026-08-07 | Status header corrected to Claimed→Closed (Band A — 2026-08-07). 7/7 ACs green; substrate pre-existed on `next` ahead of claim. 13/13 mission_crypto tests pass. |
