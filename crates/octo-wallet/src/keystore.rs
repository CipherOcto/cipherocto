//! Starkli-shaped keystore import/export.
//!
//! Per S01 plan §3 Step 2 + mission `0102-a-wallet-foundation` §Starkli Keystore Divergence.
//!
//! **DIVERGENCE NOTE (R2):** Starkli v0.3+ uses `chacha20-poly1305` + `Argon2id` JSON
//! format to wrap **Stark-curve** private keys (not Ed25519). CipherOcto's wallet
//! substrate is **Ed25519** (per RFC-0009 §Identity Key Format + RFC-0853 §Ed25519
//! substrate). This module exports a Starkli-shaped JSON envelope but wraps
//! Ed25519 seeds. The wire format diverges from Starkli in:
//!
//! 1. Cipher payload = 32-byte Ed25519 seed (not Stark-curve scalar).
//! 2. `public_key` field = 32-byte Ed25519 verifying key (not Stark-curve point).
//!
//! The envelope structure (cipher, kdf, kdfparams, ciphertext, nonce, mac) mirrors
//! the keystone/Web3 secret storage spec so cross-impl tools that respect that
//! shape can interoperate at the envelope level. Per mission §Starkli Keystore
//! Divergence, this divergence is documented in RFC-0102 addendum (TODO: file
//! amendment).
//!
//! Per S01 plan Step 2 acceptance: `round-trip test: import known vector → export → diff = none`.

use std::fs;
use std::io::Write;
use std::path::Path;

use argon2::{Argon2, Params};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::WalletError;
use crate::identity::IdentityKey;

/// Argon2id parameters matching Starkli v0.3+ defaults.
const ARGON2_M_COST: u32 = 64 * 1024; // 64 MiB (in KiB)
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;
const ARGON2_OUTPUT_LEN: usize = 32;

/// Salt length (16 bytes raw).
const SALT_LEN: usize = 16;

/// ChaCha20-Poly1305 nonce length (12 bytes per RFC 8439).
const NONCE_LEN: usize = 12;

/// Keystore file envelope (Starkli-shaped). Versioned for forward-compat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystoreFile {
    pub version: u8,
    pub crypto: Crypto,
    #[serde(rename = "public_key")]
    pub public_key: String,
    /// Hex-encoded cipher identifier (e.g. `chacha20-poly1305`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cipher_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crypto {
    pub cipher: String,
    pub ciphertext: String,
    pub cipherparams: CipherParams,
    pub kdf: String,
    pub kdfparams: KdfParams,
    /// Hex-encoded MAC tag (ChaCha20-Poly1305 returns ciphertext||tag;
    /// we split the last 16 bytes into `mac` for spec compliance).
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CipherParams {
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub salt: String,
    #[serde(rename = "timecost")]
    pub time_cost: u32,
    #[serde(rename = "memorycost")]
    pub memory_cost: u32,
    pub parallelism: u32,
    #[serde(rename = "outputlen")]
    pub output_len: usize,
}

/// Starkli-shaped keystore importer/exporter for Ed25519 identity keys.
///
/// **See module-level divergence note.** Not interchangeable with Starkli v0.3+
/// at the key-payload level (Stark-curve vs Ed25519); the envelope shape is
/// compatible so the file structure can be parsed by tools respecting keystone
/// conventions.
#[derive(Debug, Default, Clone, Copy)]
pub struct StarkliCompat;

impl StarkliCompat {
    /// Import an identity key from a Starkli-shaped keystore file.
    ///
    /// # Errors
    /// Returns `WalletError::KeystoreParse` on malformed JSON,
    /// `WalletError::KeystoreVersion` on unknown version,
    /// `WalletError::VaultDecryptionFailed` on wrong passphrase / corrupt MAC.
    pub fn import(&self, path: &Path, passphrase: &str) -> Result<IdentityKey, WalletError> {
        let json = fs::read(path)?;
        let file: KeystoreFile = serde_json::from_slice(&json)
            .map_err(|e| WalletError::KeystoreParse(format!("keystore json: {e}")))?;

        if file.version != 1 {
            return Err(WalletError::KeystoreVersion {
                expected: "1".to_owned(),
                got: file.version.to_string(),
            });
        }

        // Decrypt: combine ciphertext + MAC tag (Keystone spec splits these fields).
        // `aead 0.5` `decrypt` expects the full AEAD ciphertext (ct || tag);
        // the tag is the last 16 bytes. Authenticity is verified internally.
        let ct = hex_decode(&file.crypto.ciphertext)
            .map_err(|e| WalletError::KeystoreParse(format!("ciphertext hex: {e}")))?;
        let mac_bytes = hex_decode(&file.crypto.mac)
            .map_err(|e| WalletError::KeystoreParse(format!("mac hex: {e}")))?;
        if mac_bytes.len() != 16 {
            return Err(WalletError::KeystoreParse(format!(
                "mac must be 16 bytes, got {}",
                mac_bytes.len()
            )));
        }
        let mut combined = Vec::with_capacity(ct.len() + mac_bytes.len());
        combined.extend_from_slice(&ct);
        combined.extend_from_slice(&mac_bytes);

        // Derive key via Argon2id.
        let salt_bytes = hex_decode(&file.crypto.kdfparams.salt)
            .map_err(|e| WalletError::KeystoreParse(format!("salt hex: {e}")))?;
        let argon = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            Params::new(
                file.crypto.kdfparams.memory_cost,
                file.crypto.kdfparams.time_cost,
                file.crypto.kdfparams.parallelism,
                Some(file.crypto.kdfparams.output_len),
            )
            .map_err(|e| WalletError::KeystoreParse(format!("argon2 params: {e}")))?,
        );
        let mut key = [0u8; ARGON2_OUTPUT_LEN];
        argon
            .hash_password_into(passphrase.as_bytes(), &salt_bytes, &mut key)
            .map_err(|e| WalletError::KeystoreParse(format!("argon2 hash: {e}")))?;

        // ChaCha20-Poly1305 decrypt.
        let nonce_bytes = hex_decode(&file.crypto.cipherparams.nonce)
            .map_err(|e| WalletError::KeystoreParse(format!("nonce hex: {e}")))?;
        if nonce_bytes.len() != NONCE_LEN {
            return Err(WalletError::KeystoreParse(
                "nonce length mismatch".to_owned(),
            ));
        }
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, combined.as_slice())
            .map_err(|_| WalletError::VaultDecryptionFailed)?;

        if plaintext.len() != 32 {
            return Err(WalletError::KeystoreParse(format!(
                "expected 32-byte Ed25519 seed, got {} bytes",
                plaintext.len()
            )));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&plaintext);
        Ok(IdentityKey::from_seed(seed))
    }

    /// Export an identity key to a Starkli-shaped keystore file.
    ///
    /// Atomic write: `.tmp` + rename. File mode 0o600 on Unix.
    ///
    /// # Errors
    /// Returns `WalletError::Io` on filesystem failure,
    /// `WalletError::KeystoreParse` on internal serialization failure.
    pub fn export(
        &self,
        key: &IdentityKey,
        path: &Path,
        passphrase: &str,
    ) -> Result<(), WalletError> {
        let mut rng = rand::rng();
        let mut salt_bytes = [0u8; SALT_LEN];
        rng.fill_bytes(&mut salt_bytes);

        // Derive key.
        let argon = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            Params::new(
                ARGON2_M_COST,
                ARGON2_T_COST,
                ARGON2_P_COST,
                Some(ARGON2_OUTPUT_LEN),
            )
            .map_err(|e| WalletError::KeystoreParse(format!("argon2 params: {e}")))?,
        );
        let mut aead_key = [0u8; ARGON2_OUTPUT_LEN];
        argon
            .hash_password_into(passphrase.as_bytes(), &salt_bytes, &mut aead_key)
            .map_err(|e| WalletError::KeystoreParse(format!("argon2 hash: {e}")))?;

        // Encrypt.
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce_bytes);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&aead_key));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let seed = key.seed_bytes();
        // AEAD encrypt produces ciphertext || tag (16-byte Poly1305 tag).
        let ct_with_tag = cipher
            .encrypt(nonce, seed.as_ref())
            .map_err(|e| WalletError::KeystoreParse(format!("chacha encrypt: {e}")))?;
        // Split for spec compliance (Keystone/Web3 Secret Storage stores
        // ciphertext + tag in separate fields). Our `mac` field carries the
        // 16-byte tag verbatim.
        let tag_start = ct_with_tag.len() - 16;
        let ct = &ct_with_tag[..tag_start];
        let tag = &ct_with_tag[tag_start..];

        let file = KeystoreFile {
            version: 1,
            crypto: Crypto {
                cipher: "chacha20-poly1305".to_owned(),
                ciphertext: hex_encode(ct),
                cipherparams: CipherParams {
                    nonce: hex_encode(&nonce_bytes),
                },
                kdf: "argon2id".to_owned(),
                kdfparams: KdfParams {
                    salt: hex_encode(&salt_bytes),
                    time_cost: ARGON2_T_COST,
                    memory_cost: ARGON2_M_COST,
                    parallelism: ARGON2_P_COST,
                    output_len: ARGON2_OUTPUT_LEN,
                },
                mac: hex_encode(tag),
            },
            public_key: hex_encode(&key.public_key_bytes()),
            cipher_name: None,
        };

        let json = serde_json::to_vec_pretty(&file)
            .map_err(|e| WalletError::KeystoreParse(format!("keystore serialize: {e}")))?;
        let tmp = path.with_extension("keystore.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

fn hex_decode(s: &str) -> Result<Vec<u8>, WalletError> {
    hex::decode(s).map_err(|e| WalletError::KeystoreParse(format!("hex decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystore.json");
        let key = IdentityKey::generate().unwrap();
        let public_before = key.public_key_bytes();

        let compat = StarkliCompat;
        compat
            .export(&key, &path, "correct horse battery staple")
            .expect("export");

        let imported = compat
            .import(&path, "correct horse battery staple")
            .expect("import");
        assert_eq!(imported.public_key_bytes(), public_before);
    }

    #[test]
    fn wrong_passphrase_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystore.json");
        let key = IdentityKey::generate().unwrap();
        StarkliCompat.export(&key, &path, "right").unwrap();
        let err = StarkliCompat.import(&path, "wrong").unwrap_err();
        assert!(matches!(err, WalletError::VaultDecryptionFailed));
    }

    #[test]
    fn exported_json_has_starkli_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keystore.json");
        let key = IdentityKey::generate().unwrap();
        StarkliCompat.export(&key, &path, "p").unwrap();
        let json = fs::read_to_string(&path).unwrap();
        // Verify Starkli-shaped envelope keys present.
        for k in [
            "version",
            "crypto",
            "cipher",
            "ciphertext",
            "kdf",
            "kdfparams",
            "public_key",
        ] {
            assert!(json.contains(k), "missing key `{k}` in: {json}");
        }
        assert!(json.contains("chacha20-poly1305"));
        assert!(json.contains("argon2id"));
    }
}
