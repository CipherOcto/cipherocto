//! Deterministic randomness derivation (RFC-0853 §11)

use crate::ocrypt::error::CryptoError;

/// Derive deterministic randomness for consensus-critical paths.
///
/// output = HKDF-BLAKE3(salt=context, ikm=seed, info=epoch_bytes, length=output_len)
///
/// Forbidden sources for consensus: OS entropy, hardware RNG, platform APIs.
pub fn derive_deterministic_random(
    seed: &[u8],
    context: &[u8],
    epoch: u64,
    output_len: usize,
) -> Result<Vec<u8>, CryptoError> {
    let info = epoch.to_be_bytes();
    let mut output = vec![0u8; output_len];
    super::hkdf_blake3(context, seed, &info, &mut output);
    Ok(output)
}

/// Derive a deterministic 32-byte nonce from seed + context + epoch.
pub fn derive_deterministic_nonce(
    seed: &[u8],
    context: &[u8],
    epoch: u64,
) -> Result<[u8; 32], CryptoError> {
    let bytes = derive_deterministic_random(seed, context, epoch, 32)?;
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(&bytes);
    Ok(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_random_deterministic() {
        let seed = [0x42u8; 32];
        let ctx = b"test_context";
        let r1 = derive_deterministic_random(&seed, ctx, 100, 32).unwrap();
        let r2 = derive_deterministic_random(&seed, ctx, 100, 32).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_deterministic_random_different_epochs() {
        let seed = [0x42u8; 32];
        let ctx = b"test_context";
        let r1 = derive_deterministic_random(&seed, ctx, 100, 32).unwrap();
        let r2 = derive_deterministic_random(&seed, ctx, 101, 32).unwrap();
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_deterministic_random_different_contexts() {
        let seed = [0x42u8; 32];
        let r1 = derive_deterministic_random(&seed, b"ctx1", 100, 32).unwrap();
        let r2 = derive_deterministic_random(&seed, b"ctx2", 100, 32).unwrap();
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_deterministic_random_output_length() {
        let seed = [0x42u8; 32];
        let r = derive_deterministic_random(&seed, b"ctx", 100, 64).unwrap();
        assert_eq!(r.len(), 64);
    }

    #[test]
    fn test_deterministic_nonce_size() {
        let seed = [0x42u8; 32];
        let nonce = derive_deterministic_nonce(&seed, b"ctx", 100).unwrap();
        assert_eq!(nonce.len(), 32);
    }

    #[test]
    fn test_deterministic_nonce_deterministic() {
        let seed = [0x42u8; 32];
        let n1 = derive_deterministic_nonce(&seed, b"ctx", 100).unwrap();
        let n2 = derive_deterministic_nonce(&seed, b"ctx", 100).unwrap();
        assert_eq!(n1, n2);
    }

    #[test]
    fn test_deterministic_random_different_seeds() {
        let s1 = [0x42u8; 32];
        let s2 = [0x43u8; 32];
        let r1 = derive_deterministic_random(&s1, b"ctx", 100, 32).unwrap();
        let r2 = derive_deterministic_random(&s2, b"ctx", 100, 32).unwrap();
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_deterministic_random_small_output() {
        let seed = [0x42u8; 32];
        let r = derive_deterministic_random(&seed, b"ctx", 100, 1).unwrap();
        assert_eq!(r.len(), 1);
    }
}
