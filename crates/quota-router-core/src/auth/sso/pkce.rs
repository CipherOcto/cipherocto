//! PKCE (Proof Key for Code Exchange) for OAuth2 Authorization Code flow.
//!
//! RFC-7636 compliant PKCE implementation with S256 challenge method.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// PKCE challenge method — S256 is mandatory per RFC-0949.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum PkceMethod {
    S256,
}

/// PKCE challenge pair: code_verifier + code_challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkceChallenge {
    /// Code verifier (43-128 chars, unreserved characters).
    pub code_verifier: String,
    /// Code challenge = BASE64URL(SHA256(code_verifier)).
    pub code_challenge: String,
    /// Challenge method — always S256.
    pub method: PkceMethod,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge pair.
    ///
    /// code_verifier: 43 characters of random unreserved characters [A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~"
    /// code_challenge: BASE64URL(SHA256(code_verifier))
    pub fn generate() -> Self {
        let code_verifier = generate_code_verifier(43);
        let code_challenge = compute_challenge(&code_verifier);

        Self {
            code_verifier,
            code_challenge,
            method: PkceMethod::S256,
        }
    }

    /// Verify a code_verifier against the stored code_challenge.
    pub fn verify(&self, code_verifier: &str) -> bool {
        let expected = compute_challenge(code_verifier);
        expected == self.code_challenge
    }
}

/// Generate a random code_verifier of the given length.
///
/// Characters: [A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~"
/// Length must be between 43 and 128 per RFC-7636.
fn generate_code_verifier(len: usize) -> String {
    assert!(
        (43..=128).contains(&len),
        "code_verifier length must be 43-128"
    );

    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// Compute S256 challenge: BASE64URL(SHA256(code_verifier))
fn compute_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_challenge_generation() {
        let challenge = PkceChallenge::generate();
        assert_eq!(challenge.method, PkceMethod::S256);
        assert!(
            challenge.code_verifier.len() >= 43 && challenge.code_verifier.len() <= 128,
            "code_verifier length {} not in 43-128",
            challenge.code_verifier.len()
        );
        assert!(!challenge.code_challenge.is_empty());
    }

    #[test]
    fn test_pkce_verify_success() {
        let challenge = PkceChallenge::generate();
        assert!(challenge.verify(&challenge.code_verifier));
    }

    #[test]
    fn test_pkce_verify_failure() {
        let challenge = PkceChallenge::generate();
        assert!(!challenge
            .verify("wrong_verifier_value_that_is_long_enough_to_pass_length_check_1234567890"));
    }

    #[test]
    fn test_code_challenge_is_base64url() {
        let challenge = PkceChallenge::generate();
        // Base64URL without padding: only [A-Za-z0-9_-]
        assert!(challenge
            .code_challenge
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn test_deterministic_challenge() {
        // Same verifier must produce same challenge
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge1 = compute_challenge(verifier);
        let challenge2 = compute_challenge(verifier);
        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn test_different_verifiers_different_challenges() {
        let c1 = PkceChallenge::generate();
        let c2 = PkceChallenge::generate();
        assert_ne!(c1.code_verifier, c2.code_verifier);
        assert_ne!(c1.code_challenge, c2.code_challenge);
    }
}
