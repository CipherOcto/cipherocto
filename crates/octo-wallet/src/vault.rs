//! Provider-key vault (encrypted-at-rest).
//!
//! Per RFC-0009 §Vault + RFC-0102 §Key Storage (post-amendment 2026-07-19):
//! Argon2id passphrase-derived key (m=64MiB, t=3, p=4) → AES-256-GCM payload.
//! File-per-slot at `<slots_dir>/<slot_id>.vault`.
//!
//! Slot IDs MUST match `[a-zA-Z0-9._-]{1,128}` to prevent path traversal.
//! Passphrase MUST be prompted via `rpassword` or stdin; NEVER argv (visible in `ps`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::password_hash::SaltString;
use argon2::{Argon2, Params};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::WalletError;

/// Argon2id parameters per RFC-0102 §Key Storage post-amendment.
/// m=64MiB, t=3 iterations, p=4 lanes.
const ARGON2_M_COST: u32 = 64 * 1024; // 64 MiB
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;
const ARGON2_OUTPUT_LEN: usize = 32; // AES-256 key

/// AES-GCM nonce length (12 bytes per RFC 5116 §3.2).
const NONCE_LEN: usize = 12;

/// Salt length for Argon2 password hash (SaltString base64-decoded = 16 bytes).
const SALT_LEN: usize = 16;

/// On-disk vault file format. Versioned for forward-compat.
///
/// **Debug redaction (octo-wallet §Security):** the `salt`, `nonce`,
/// and `ciphertext` fields together with a leaked passphrase constitute
/// the on-disk encryption envelope. While the ciphertext is not
/// plaintext (Argon2id-wrapped AES-256-GCM), Debug-dumping the envelope
/// alongside an in-scope passphrase or any panic message context is a
/// defense-in-depth risk. Manual `Debug` impl prints only the version.
#[derive(Clone, Serialize, Deserialize)]
pub struct VaultFile {
    /// Format version. Bump on schema change.
    pub version: u8,
    /// Argon2id salt (base64-encoded).
    pub salt: String,
    /// AES-GCM nonce (12 bytes, base64-encoded).
    pub nonce: String,
    /// AES-GCM ciphertext + tag.
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for VaultFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultFile")
            .field("version", &self.version)
            .field("ciphertext_size_bytes", &self.ciphertext.len())
            .finish_non_exhaustive()
    }
}

/// One-shot borrow of decrypted plaintext.
///
/// Caller owns the backing buffer (`&&mut Vec<u8>` passed to `Vault::get`) and is
/// responsible for zeroizing it after use. `DecryptedHandle` does NOT take
/// ownership of the buffer (the borrow is read-only).
pub struct DecryptedHandle<'a> {
    bytes: &'a [u8],
}

impl DecryptedHandle<'_> {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }
}

impl std::fmt::Debug for DecryptedHandle<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecryptedHandle")
            .field("bytes", &format!("[REDACTED {} bytes]", self.bytes.len()))
            .finish()
    }
}

/// Encrypted vault. One directory, many slot files.
#[derive(Debug, Clone)]
pub struct Vault {
    slots_dir: PathBuf,
}

impl Vault {
    /// Create or open a vault rooted at `slots_dir`. The directory is created
    /// (with `0o700` permissions on Unix) if it does not exist.
    ///
    /// # Errors
    /// Returns `WalletError::Io` if the directory cannot be created.
    pub fn open(slots_dir: impl Into<PathBuf>) -> Result<Self, WalletError> {
        let slots_dir = slots_dir.into();
        fs::create_dir_all(&slots_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&slots_dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { slots_dir })
    }

    /// Default slot directory: `~/.config/cipherocto/vault/`.
    #[must_use]
    pub fn default_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("eu", "cipherocto", "cipherocto")
            .map(|dirs| dirs.config_dir().join("vault"))
    }

    /// Open the default vault. Returns `None` if the platform has no
    /// `directories` base path.
    ///
    /// # Errors
    /// Returns `WalletError::Config` if the default path cannot be opened.
    pub fn open_default() -> Result<Self, WalletError> {
        let dir = Self::default_dir()
            .ok_or_else(|| WalletError::Config("no default config directory".to_owned()))?;
        Self::open(dir)
    }

    /// Encrypt `plaintext_key` into slot `slot_id` with `passphrase`.
    ///
    /// Overwrites any existing slot with the same id.
    ///
    /// # Errors
    /// Returns `WalletError::InvalidSlotId` if the id fails validation,
    /// `WalletError::Io` on filesystem failure.
    pub fn put(
        &self,
        slot_id: &str,
        plaintext_key: &[u8],
        passphrase: &str,
    ) -> Result<(), WalletError> {
        validate_slot_id(slot_id)?;
        let mut rng = rand::rng();

        // Generate salt (SaltString::generate requires OsRng via argon2 0.5;
        // generate raw 16 bytes and base64-encode for determinism).
        let mut salt_bytes = [0u8; SALT_LEN];
        rng.fill_bytes(&mut salt_bytes);
        let salt = SaltString::encode_b64(&salt_bytes)
            .map_err(|e| WalletError::Config(format!("salt encode: {e}")))?;

        // Derive AES-256 key via Argon2id.
        let argon = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            Params::new(
                ARGON2_M_COST,
                ARGON2_T_COST,
                ARGON2_P_COST,
                Some(ARGON2_OUTPUT_LEN),
            )
            .map_err(|e| WalletError::Config(format!("argon2 params: {e}")))?,
        );
        let mut key = [0u8; ARGON2_OUTPUT_LEN];
        argon
            .hash_password_into(passphrase.as_bytes(), salt.as_str().as_bytes(), &mut key)
            .map_err(|e| WalletError::Config(format!("argon2 hash: {e}")))?;

        // AES-256-GCM encrypt.
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce_bytes);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext_key)
            .map_err(|e| WalletError::Config(format!("aes-gcm encrypt: {e}")))?;

        // Serialize + atomic write.
        let file = VaultFile {
            version: 1,
            salt: salt.as_str().to_owned(),
            nonce: base64_encode(&nonce_bytes),
            ciphertext: ciphertext.clone(),
        };
        let json = serde_json::to_vec(&file)
            .map_err(|e| WalletError::Config(format!("vault serialize: {e}")))?;
        let path = self.slot_path(slot_id);
        let tmp = path.with_extension("vault.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        // Zeroize derived key.
        key.zeroize();
        Ok(())
    }

    /// Decrypt slot `slot_id` with `passphrase`. Returns one-shot borrow.
    ///
    /// # Errors
    /// Returns `WalletError::VaultSlotNotFound` if missing,
    /// `WalletError::VaultDecryptionFailed` on wrong passphrase / corrupt slot.
    pub fn get<'a>(
        &'a self,
        slot_id: &str,
        passphrase: &str,
        out: &'a mut Vec<u8>,
    ) -> Result<DecryptedHandle<'a>, WalletError> {
        validate_slot_id(slot_id)?;
        let path = self.slot_path(slot_id);
        if !path.exists() {
            return Err(WalletError::VaultSlotNotFound(slot_id.to_owned()));
        }
        let json = fs::read(&path)?;
        let file: VaultFile = serde_json::from_slice(&json)
            .map_err(|e| WalletError::KeystoreParse(format!("vault file: {e}")))?;

        if file.version != 1 {
            return Err(WalletError::KeystoreVersion {
                expected: "1".to_owned(),
                got: file.version.to_string(),
            });
        }

        // Derive AES-256 key.
        let salt = SaltString::from_b64(&file.salt)
            .map_err(|e| WalletError::Config(format!("salt decode: {e}")))?;
        let argon = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            Params::new(
                ARGON2_M_COST,
                ARGON2_T_COST,
                ARGON2_P_COST,
                Some(ARGON2_OUTPUT_LEN),
            )
            .map_err(|e| WalletError::Config(format!("argon2 params: {e}")))?,
        );
        let mut key = [0u8; ARGON2_OUTPUT_LEN];
        argon
            .hash_password_into(passphrase.as_bytes(), salt.as_str().as_bytes(), &mut key)
            .map_err(|e| WalletError::Config(format!("argon2 hash: {e}")))?;

        // AES-256-GCM decrypt.
        let nonce_bytes = base64_decode(&file.nonce)
            .map_err(|e| WalletError::Config(format!("nonce decode: {e}")))?;
        if nonce_bytes.len() != NONCE_LEN {
            return Err(WalletError::Config("nonce length mismatch".to_owned()));
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, file.ciphertext.as_ref())
            .map_err(|_| {
                key.zeroize();
                WalletError::VaultDecryptionFailed
            })?;
        key.zeroize();

        out.clear();
        out.extend_from_slice(&plaintext);
        // `plaintext` is zeroized by AesGcm drop (impls Zeroize); `out` is owned
        // by the caller, who is responsible for zeroizing it after use.
        drop(plaintext);
        Ok(DecryptedHandle { bytes: out })
    }

    /// List slot IDs (filenames without `.vault` extension). No plaintext.
    pub fn list(&self) -> Result<Vec<String>, WalletError> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.slots_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(slot) = name.strip_suffix(".vault") {
                out.push(slot.to_owned());
            }
        }
        out.sort();
        Ok(out)
    }

    fn slot_path(&self, slot_id: &str) -> PathBuf {
        self.slots_dir.join(format!("{slot_id}.vault"))
    }
}

fn validate_slot_id(slot_id: &str) -> Result<(), WalletError> {
    if slot_id.is_empty() || slot_id.len() > 128 {
        return Err(WalletError::InvalidSlotId(slot_id.to_owned()));
    }
    if !slot_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    {
        return Err(WalletError::InvalidSlotId(slot_id.to_owned()));
    }
    Ok(())
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut buf = Vec::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        buf.push(BASE64_ALPHABET[(b0 >> 2) as usize]);
        buf.push(BASE64_ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize]);
        if chunk.len() > 1 {
            buf.push(BASE64_ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize]);
        } else {
            buf.push(b'=');
        }
        if chunk.len() > 2 {
            buf.push(BASE64_ALPHABET[(b2 & 0x3f) as usize]);
        } else {
            buf.push(b'=');
        }
    }
    String::from_utf8(buf).unwrap_or_default()
}

fn base64_decode(s: &str) -> Result<Vec<u8>, WalletError> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(WalletError::Config(
            "base64 length not multiple of 4".to_owned(),
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let v0 = val(chunk[0]).ok_or_else(|| WalletError::Config("base64 invalid".to_owned()))?;
        let v1 = val(chunk[1]).ok_or_else(|| WalletError::Config("base64 invalid".to_owned()))?;
        let pad = chunk[2] == b'=';
        let v2 = if pad {
            0
        } else {
            val(chunk[2]).ok_or_else(|| WalletError::Config("base64 invalid".to_owned()))?
        };
        let pad2 = chunk[3] == b'=';
        let v3 = if pad2 {
            0
        } else {
            val(chunk[3]).ok_or_else(|| WalletError::Config("base64 invalid".to_owned()))?
        };
        out.push((v0 << 2) | (v1 >> 4));
        if !pad {
            out.push((v1 << 4) | (v2 >> 2));
        }
        if !pad2 {
            out.push((v2 << 6) | v3);
        }
    }
    Ok(out)
}

// Keep `Path` import alive for potential future Path-based API additions.
#[allow(dead_code)]
fn _path_marker(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_vault() -> (TempDir, Vault) {
        let dir = tempfile::tempdir().expect("tempdir");
        let v = Vault::open(dir.path().to_path_buf()).expect("open");
        (dir, v)
    }

    #[test]
    fn put_get_roundtrip() {
        let (_dir, v) = fresh_vault();
        let plaintext = b"super-secret-provider-key";
        v.put("openai-prod", plaintext, "correct horse battery staple")
            .expect("put");
        let mut buf = Vec::new();
        let handle = v
            .get("openai-prod", "correct horse battery staple", &mut buf)
            .expect("get");
        assert_eq!(handle.as_bytes(), plaintext);
        // zeroize caller buffer
        buf.zeroize();
    }

    #[test]
    fn wrong_passphrase_rejected() {
        let (_dir, v) = fresh_vault();
        v.put("openai-prod", b"secret", "right").unwrap();
        let mut buf = Vec::new();
        let err = v.get("openai-prod", "wrong", &mut buf).unwrap_err();
        assert!(matches!(err, WalletError::VaultDecryptionFailed));
    }

    #[test]
    fn missing_slot_rejected() {
        let (_dir, v) = fresh_vault();
        let mut buf = Vec::new();
        let err = v.get("nope", "anything", &mut buf).unwrap_err();
        assert!(matches!(err, WalletError::VaultSlotNotFound(ref s) if s == "nope"));
    }

    #[test]
    fn invalid_slot_id_rejected() {
        let (_dir, v) = fresh_vault();
        for bad in [
            "../escape",
            "with/slash",
            "with space",
            "x".repeat(200).as_str(),
        ] {
            assert!(
                matches!(v.put(bad, b"x", "p"), Err(WalletError::InvalidSlotId(_))),
                "expected InvalidSlotId for `{bad}`"
            );
            let mut buf = Vec::new();
            assert!(matches!(
                v.get(bad, "p", &mut buf),
                Err(WalletError::InvalidSlotId(_))
            ));
        }
    }

    #[test]
    fn list_returns_sorted_slots() {
        let (_dir, v) = fresh_vault();
        v.put("zeta", b"z", "p").unwrap();
        v.put("alpha", b"a", "p").unwrap();
        v.put("mid", b"m", "p").unwrap();
        let slots = v.list().unwrap();
        assert_eq!(slots, vec!["alpha", "mid", "zeta"]);
    }

    #[test]
    fn overwrite_existing_slot() {
        let (_dir, v) = fresh_vault();
        v.put("slot", b"v1", "p").unwrap();
        v.put("slot", b"v2", "p").unwrap();
        let mut buf = Vec::new();
        let h = v.get("slot", "p", &mut buf).unwrap();
        assert_eq!(h.as_bytes(), b"v2");
        buf.zeroize();
    }
}
