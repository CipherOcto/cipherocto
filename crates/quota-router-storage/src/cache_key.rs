//! Cache-key derivation (RFC-0959 §Data Structures `cache_key`).
//!
//! `cache_key(prompt_tokens: &[u32]) -> [u8; 32]`
//! = `BLAKE3_keyed(CACHE_KEY_DOMAIN, canonical_u32s_le_bytes(prompt_tokens))`
//!
//! BLAKE3's native keyed-hash mode is used (NOT HMAC-BLAKE3) — matches the
//! convention established by mission 0957-a (capability token macaroon) and
//! avoids the HMAC construction overhead since BLAKE3 already supports a
//! 32-byte key natively.
//!
//! The `CACHE_KEY_DOMAIN` is exactly 32 bytes (BLAKE3 keyed-hash key
//! constraint); RFC-0959 specifies `b"cipherocto/cache-key/v1..."` (23 chars
//! + 9 dots = 32 bytes). Padding dots after `v1` are literal '.' characters,
//! not zero bytes — so the key embeds the version `v1` in printable form.

#![allow(clippy::doc_lazy_continuation)]

use blake3::Hash;

/// 32-byte BLAKE3 keyed-hash key (RFC-0959 §Data Structures CACHE_KEY_DOMAIN).
/// Exactly 32 bytes: `b"cipherocto/cache-key/v1........."` (9 trailing dots = padding).
pub const CACHE_KEY_DOMAIN: [u8; 32] = *b"cipherocto/cache-key/v1.........";

/// Canonical u32 LE encoding for a slice of token IDs (deterministic across
/// independent nodes; u32 LE is the platform-native byte order on all
/// targets CipherOcto supports).
#[must_use]
pub fn canonical_prompt_bytes(prompt_tokens: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(prompt_tokens.len() * 4);
    for &t in prompt_tokens {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out
}

/// Compute `cache_key` per RFC-0959 §Data Structures.
///
/// BLAKE3 keyed-hash over the canonical LE encoding of `prompt_tokens`.
/// Identical prompts → identical keys; distinct → divergent (BLAKE3
/// collision resistance 2^-128 per the BLAKE3 spec).
#[must_use]
pub fn cache_key(prompt_tokens: &[u32]) -> [u8; 32] {
    let bytes = canonical_prompt_bytes(prompt_tokens);
    let mut hasher = blake3::Hasher::new_keyed(&CACHE_KEY_DOMAIN);
    hasher.update(&bytes);
    *hasher.finalize().as_bytes()
}

/// Hash the request body bytes (convenience for callers that already have
/// bytes rather than token IDs).
#[must_use]
pub fn cache_key_from_bytes(body: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&CACHE_KEY_DOMAIN);
    hasher.update(body);
    *hasher.finalize().as_bytes()
}

/// Compute the cache_key_hash explicitly without going through `blake3::Hash`
/// (used by tests / introspection).
#[must_use]
pub fn cache_key_hash_value(prompt_tokens: &[u32]) -> Hash {
    let bytes = canonical_prompt_bytes(prompt_tokens);
    let mut hasher = blake3::Hasher::new_keyed(&CACHE_KEY_DOMAIN);
    hasher.update(&bytes);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_domain_is_32_bytes() {
        assert_eq!(CACHE_KEY_DOMAIN.len(), 32);
        // Sanity: starts with the printable version prefix.
        assert!(CACHE_KEY_DOMAIN.starts_with(b"cipherocto/cache-key/v1"));
    }

    #[test]
    fn identical_prompts_produce_identical_keys() {
        let p = [1u32, 2, 3, 4, 5];
        assert_eq!(cache_key(&p), cache_key(&p));
    }

    #[test]
    fn distinct_prompts_produce_divergent_keys() {
        let a = [1u32, 2, 3, 4, 5];
        let b = [1u32, 2, 3, 4, 6];
        assert_ne!(cache_key(&a), cache_key(&b));
    }

    #[test]
    fn empty_prompt_is_valid() {
        // BLAKE3 keyed-hash over empty input is valid (32-byte output).
        let k = cache_key(&[]);
        assert_eq!(k.len(), 32);
    }

    #[test]
    fn cache_key_differs_from_unkeyed_hash() {
        // The keyed-hash MUST differ from the unkeyed BLAKE3 hash of the
        // same bytes (RFC-0959 §Adversary A5 mitigation: keyed mode prevents
        // pre-computation attacks across deployments).
        let p = [42u32; 16];
        let keyed = cache_key(&p);
        let unkeyed = *blake3::hash(&canonical_prompt_bytes(&p)).as_bytes();
        assert_ne!(keyed, unkeyed);
    }

    #[test]
    fn cache_key_from_bytes_matches_canonical() {
        // Manually computed canonical bytes should match the &[u32] form.
        let tokens = [1u32, 2, 3];
        let canonical = canonical_prompt_bytes(&tokens);
        assert_eq!(cache_key(&tokens), cache_key_from_bytes(&canonical));
    }
}
