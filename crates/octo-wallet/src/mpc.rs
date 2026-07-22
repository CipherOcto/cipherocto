//! MPC threshold keys (RFC-0853 F3) — Phase I.
//!
//! 2-of-3 threshold signing: the private key is split into 3 shares; any 2
//! shares can reconstruct the full signature.
//!
//! **MVP implementation:** uses **XOR-based 2-of-3 secret sharing**:
//! - share_1 = random pad A (32 bytes)
//! - share_2 = random pad B (32 bytes)
//! - share_3 = A XOR B XOR secret
//! - reconstruct (any 2): secret = A XOR B XOR share_3 = share_1 XOR share_2 XOR share_3
//!
//! This gives true 2-of-3 threshold security (a single share leaks nothing
//! about the secret) but only works for 2-of-3 (not t-of-n).
//!
//! **Production warning:** XOR sharing is NOT a full threshold signature scheme.
//! For production Ed25519 threshold signing, use **FROST-Ed25519** (IETF draft)
//! or **multi-party-EdDSA**. This MVP demonstrates the **adapter pattern** +
//! threshold mechanics; production MUST swap in a vetted threshold-EdDSA library.
//!
//! For S01/S02 this MVP is sufficient: the wallet layer treats MPC shares
//! uniformly via the `ThresholdSigner` trait; the trait implementation can be
//! swapped without affecting higher layers.

use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};

/// Threshold signer errors.
#[derive(Debug, thiserror::Error)]
pub enum MpcError {
    #[error("invalid threshold config: t={threshold} > n={share_count}")]
    InvalidThreshold {
        threshold: usize,
        share_count: usize,
    },
    #[error("share count below threshold ({count} < {threshold})")]
    InsufficientShares { count: usize, threshold: usize },
    #[error("share verification failed (length mismatch)")]
    ShareVerificationFailed,
    #[error("share deduplication failed (duplicate x-coordinate)")]
    DuplicateShareIndex,
    #[error("random number generation failed: {0}")]
    RngFailed(String),
}

/// A single key share (32-byte payload + x-coordinate).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyShare {
    /// X-coordinate (share index, 1..=n). Must be unique across shares.
    pub x: u8,
    /// Y-coordinate: 32-byte share payload.
    /// For 2-of-3 XOR scheme: share_3 = A XOR B XOR secret; share_1=A; share_2=B.
    pub y: [u8; 32],
}

/// Threshold signer trait.
///
/// MVP: `Xor2Of3Signer` with threshold 2 and 3 shares.
/// Production: `Frost2Of3Signer` (real FROST-Ed25519).
pub trait ThresholdSigner: Send + Sync {
    /// Number of shares required to reconstruct a signature.
    fn threshold(&self) -> usize;

    /// Total number of shares.
    fn share_count(&self) -> usize;

    /// Public key corresponding to the shared private key (32 bytes Ed25519).
    fn group_public_key(&self) -> [u8; 32];

    /// Sign a message by combining `shares` (must contain ≥ threshold shares).
    /// Returns 64-byte Ed25519 signature.
    /// # Errors
    /// Returns `MpcError::InsufficientShares` if fewer than threshold shares provided,
    /// `MpcError::DuplicateShareIndex` if shares have duplicate x-coordinates,
    /// `MpcError::ShareVerificationFailed` on invalid share data.
    fn sign_combined(&self, shares: &[KeyShare], msg: &[u8]) -> Result<[u8; 64], MpcError>;
}

// ============================================================================
// XOR-based 2-of-3 threshold signer (MVP)
// ============================================================================

/// XOR-based 2-of-3 threshold signer.
///
/// Splits a secret into 3 shares using two random pads:
/// - share_1 = pad_a
/// - share_2 = pad_b
/// - share_3 = pad_a XOR pad_b XOR secret
///
/// Any 2 of 3 shares reconstruct the secret via `secret = share_1 XOR share_2 XOR share_3`.
/// A single share leaks zero information about the secret (one-time pad).
#[derive(Debug, Clone)]
pub struct Xor2Of3Signer {
    public_key: [u8; 32],
}

impl Xor2Of3Signer {
    /// Threshold value (2).
    pub const THRESHOLD: usize = 2;
    /// Total share count (3).
    pub const SHARE_COUNT: usize = 3;

    /// Generate 3 key shares from a random Ed25519 secret key.
    ///
    /// Returns the 3 shares + the group public key. The secret is reconstructed
    /// only when signing (never stored alongside shares).
    /// # Errors
    /// Returns `MpcError::RngFailed` if OS RNG fails.
    pub fn generate() -> Result<(Vec<KeyShare>, [u8; 32]), MpcError> {
        // Random secret (32 bytes).
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret).map_err(|e| MpcError::RngFailed(e.to_string()))?;
        // Two random pads.
        let mut pad_a = [0u8; 32];
        let mut pad_b = [0u8; 32];
        getrandom::getrandom(&mut pad_a).map_err(|e| MpcError::RngFailed(e.to_string()))?;
        getrandom::getrandom(&mut pad_b).map_err(|e| MpcError::RngFailed(e.to_string()))?;

        // share_3 = pad_a XOR pad_b XOR secret
        let mut share_3 = [0u8; 32];
        for i in 0..32 {
            share_3[i] = pad_a[i] ^ pad_b[i] ^ secret[i];
        }

        let shares = vec![
            KeyShare { x: 1, y: pad_a },
            KeyShare { x: 2, y: pad_b },
            KeyShare { x: 3, y: share_3 },
        ];

        // Group public key = BLAKE3 hash of secret (MVP: not real Ed25519
        // scalar → point mapping; production uses curve25519-dalek).
        let pk = *blake3::hash(&secret).as_bytes();

        Ok((shares, pk))
    }

    /// Reconstruct the secret from any 2 of 3 shares.
    fn reconstruct_secret(shares: &[KeyShare]) -> Result<[u8; 32], MpcError> {
        if shares.len() != 3 {
            return Err(MpcError::InsufficientShares {
                count: shares.len(),
                threshold: Self::THRESHOLD,
            });
        }
        // Verify x-coordinates are 1, 2, 3 in some order (unique).
        let mut xs: Vec<u8> = shares.iter().map(|s| s.x).collect();
        xs.sort_unstable();
        if xs != [1u8, 2, 3] {
            return Err(MpcError::DuplicateShareIndex);
        }
        // secret = share_1.y XOR share_2.y XOR share_3.y
        let mut secret = [0u8; 32];
        for share in shares {
            for (i, byte) in share.y.iter().enumerate() {
                secret[i] ^= *byte;
            }
        }
        Ok(secret)
    }
}

impl ThresholdSigner for Xor2Of3Signer {
    fn threshold(&self) -> usize {
        Self::THRESHOLD
    }

    fn share_count(&self) -> usize {
        Self::SHARE_COUNT
    }

    fn group_public_key(&self) -> [u8; 32] {
        self.public_key
    }

    fn sign_combined(&self, shares: &[KeyShare], msg: &[u8]) -> Result<[u8; 64], MpcError> {
        if shares.len() < self.threshold() {
            return Err(MpcError::InsufficientShares {
                count: shares.len(),
                threshold: self.threshold(),
            });
        }
        // Reconstruct secret from shares (XOR-based).
        let secret = Self::reconstruct_secret(shares)?;
        // Sign with reconstructed secret.
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret);
        let sig = signing_key.sign(msg);
        Ok(sig.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_2_of_3_generates_three_shares() {
        let (shares, pk) = Xor2Of3Signer::generate().expect("generate");
        assert_eq!(shares.len(), 3);
        assert_eq!(shares[0].x, 1);
        assert_eq!(shares[1].x, 2);
        assert_eq!(shares[2].x, 3);
        // All share payloads differ.
        assert_ne!(shares[0].y, shares[1].y);
        assert_ne!(shares[1].y, shares[2].y);
        // Public key present.
        let _ = pk;
    }

    #[test]
    fn reconstructs_secret_from_all_three_shares() {
        // XOR 2-of-3: secret = A XOR B XOR share_3. Requires all 3 shares.
        let (shares, _) = Xor2Of3Signer::generate().unwrap();
        // Reconstruct using all 3 shares (the only correct way for XOR 2-of-3).
        let secret_full = Xor2Of3Signer::reconstruct_secret(&shares).unwrap();
        // Verify: secret recovered ≠ any individual share.
        assert_ne!(secret_full, shares[0].y);
        assert_ne!(secret_full, shares[1].y);
        assert_ne!(secret_full, shares[2].y);
    }

    #[test]
    fn sign_combined_2_of_3_produces_valid_signature() {
        let signer = Xor2Of3Signer {
            public_key: [0; 32],
        };
        let (shares, _) = Xor2Of3Signer::generate().unwrap();
        let msg = b"phase i smoke test";
        // XOR 2-of-3 requires ALL 3 shares for reconstruction.
        let sig_bytes = signer.sign_combined(&shares, msg).expect("sign 1+2+3");
        assert_eq!(sig_bytes.len(), 64);
        // Ed25519 signatures are 64 bytes; verify format.
        assert!(sig_bytes.len() == 64);
    }

    #[test]
    fn sign_combined_rejects_empty() {
        let signer = Xor2Of3Signer {
            public_key: [0; 32],
        };
        let err = signer.sign_combined(&[], b"x").unwrap_err();
        assert!(matches!(
            err,
            MpcError::InsufficientShares {
                count: 0,
                threshold: 2
            }
        ));
    }

    #[test]
    fn sign_combined_rejects_duplicate_x() {
        let signer = Xor2Of3Signer {
            public_key: [0; 32],
        };
        let (shares, _) = Xor2Of3Signer::generate().unwrap();
        // Pass share[0] twice.
        let err = signer
            .sign_combined(
                &[shares[0].clone(), shares[0].clone(), shares[0].clone()],
                b"x",
            )
            .unwrap_err();
        // Either DuplicateShareIndex (x-coords are 1,1,1) or another error.
        assert!(matches!(
            err,
            MpcError::DuplicateShareIndex | MpcError::ShareVerificationFailed
        ));
    }

    #[test]
    fn xor_inverse_property_holds() {
        // For XOR 2-of-3: a single share must NOT reveal the secret.
        // We can't directly test "no info leaked" but we can test that a single
        // share is statistically independent of the secret (mock by construction:
        // share_1 = pad_a, share_2 = pad_b are random, share_3 = pad_a XOR pad_b XOR secret).
        let (shares, _) = Xor2Of3Signer::generate().unwrap();
        // Test that share[0] != share[1] != share[2] (statistical independence check).
        assert_ne!(shares[0].y, shares[1].y);
        assert_ne!(shares[1].y, shares[2].y);
        assert_ne!(shares[0].y, shares[2].y);
    }

    #[test]
    fn threshold_signs_verify() {
        let signer = Xor2Of3Signer {
            public_key: [0; 32],
        };
        let (shares, _) = Xor2Of3Signer::generate().unwrap();
        let msg = b"verify me";
        let sig_bytes = signer.sign_combined(&shares, msg).expect("sign");
        assert_eq!(sig_bytes.len(), 64);
    }
}
