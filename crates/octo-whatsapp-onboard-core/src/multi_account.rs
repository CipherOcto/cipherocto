//! Multi-account session store (mission 0850p-a-multi-account).
//!
//! Phase 1 of the multi-account migration: filesystem-only index
//! (`~/.local/share/octo/whatsapp/index.json` by default). The
//! index maps `account_id` (e.g., a phone number) to a per-account
//! `AccountEntry { session_path, config_path, linked_at, last_used_at }`.
//!
//! Phase 2 (future) will migrate the index to a stoolap DB
//! (`CipherOcto/stoolap` branch `feat/blockchain-sql`). The
//! filesystem index is forward-compatible: the on-disk format
//! matches the `account_index` stoolap table schema.
//!
//! ## CLI integration
//!
//! - `session list` — read all `AccountEntry`s from the index
//! - `session use <ACCOUNT_ID>` — set the active account
//!   (writes a symlink at `<base>/active`)
//! - `session import <DB>` — register an existing session DB
//! - `session export <ACCOUNT_ID> --out <BUNDLE>` — produce a
//!   portable tar.gz bundle (mission 0850p-a-session-export)
//! - `whoami --store <PATH>` — use a specific index file
//!
//! ## Backward compatibility
//!
//! The single-DB-per-host path (`--session-path`) is preserved
//! for operators not ready to migrate. The CLI defaults to the
//! multi-account index if it exists; falls back to the legacy
//! single-DB path otherwise.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Per-account entry in the multi-account index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountEntry {
    /// The account identifier (typically an E.164 phone number
    /// without the leading `+`, e.g., `15551234567`).
    pub account_id: String,
    /// Path to the per-account stoolap session DB.
    pub session_path: PathBuf,
    /// Path to the per-account WhatsAppConfig.json.
    pub config_path: PathBuf,
    /// Unix epoch seconds when the account was first linked.
    pub linked_at: i64,
    /// Unix epoch seconds when the account was last used (CLI run).
    #[serde(default)]
    pub last_used_at: i64,
}

/// Multi-account session store backed by a filesystem index file.
///
/// `MultiAccountStore` is constructed from a path to an index file
/// (default: `~/.local/share/octo/whatsapp/index.json`). The file
/// is a JSON-serialized `IndexFile { accounts: BTreeMap<String, AccountEntry> }`.
///
/// All operations are blocking (std::fs) and synchronous. The CLI
/// does not need async here because the operations are O(1) or
/// O(N) where N is small (<100 accounts in practice).
pub struct MultiAccountStore {
    path: PathBuf,
    index: IndexFile,
}

/// The on-disk index file format.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct IndexFile {
    /// Map from `account_id` to `AccountEntry`. BTreeMap for stable
    /// serialization order (helps diffs and tests).
    accounts: BTreeMap<String, AccountEntry>,
}

impl MultiAccountStore {
    /// Open (or create) the multi-account index at `path`. If the
    /// file does not exist, an empty index is created in memory and
    /// a `save()` will write it to disk on the first mutation.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let index = if path.exists() {
            let bytes = fs::read(&path).map_err(|e| CoreError::Read {
                path: path.clone(),
                source: e,
            })?;
            serde_json::from_slice(&bytes).map_err(|e| CoreError::Parse {
                path: path.clone(),
                source: e,
            })?
        } else {
            IndexFile::default()
        };
        Ok(Self { path, index })
    }

    /// Open the default index at `~/.local/share/octo/whatsapp/index.json`.
    /// Falls back to the system data dir if HOME is unset.
    pub fn open_default() -> Result<Self> {
        let base = default_index_base_dir();
        fs::create_dir_all(&base).map_err(|e| CoreError::InvalidSessionPath {
            path: base.clone(),
            reason: format!("create base: {e}"),
        })?;
        Self::open(base.join("index.json"))
    }

    /// List all accounts in the index, sorted by `account_id`.
    pub fn list(&self) -> Vec<AccountEntry> {
        let mut v: Vec<AccountEntry> = self.index.accounts.values().cloned().collect();
        v.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        v
    }

    /// Look up an account by ID. Returns `None` if the account is
    /// not in the index.
    pub fn get(&self, account_id: &str) -> Option<&AccountEntry> {
        self.index.accounts.get(account_id)
    }

    /// Register a new account in the index. The session DB and
    /// config file must already exist on disk (typically written
    /// by `pair-link` or `qr-link`). The `linked_at` is set to
    /// the current time.
    pub fn import(
        &mut self,
        account_id: &str,
        session_path: &Path,
        config_path: &Path,
    ) -> Result<AccountEntry> {
        if !session_path.exists() {
            return Err(CoreError::InvalidSessionPath {
                path: session_path.to_path_buf(),
                reason: "session DB does not exist".to_string(),
            });
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let entry = AccountEntry {
            account_id: account_id.to_string(),
            session_path: session_path.to_path_buf(),
            config_path: config_path.to_path_buf(),
            linked_at: now,
            last_used_at: now,
        };
        self.index
            .accounts
            .insert(account_id.to_string(), entry.clone());
        self.save()?;
        Ok(entry)
    }

    /// Set the active account. Writes a symlink at
    /// `<base>/active` -> `<account.session_path>`. The symlink
    /// is read by `whoami` and `adapter start` to pick the
    /// default session.
    pub fn use_account(&mut self, account_id: &str) -> Result<AccountEntry> {
        let entry = self
            .index
            .accounts
            .get(account_id)
            .cloned()
            .ok_or_else(|| CoreError::InvalidSessionPath {
                path: PathBuf::from(account_id),
                reason: "account not in index".to_string(),
            })?;
        let base = self.path.parent().ok_or_else(|| CoreError::InvalidSessionPath {
            path: self.path.clone(),
            reason: "no parent dir".to_string(),
        })?;
        let active = base.join("active");
        // Remove any existing symlink/file.
        let _ = fs::remove_file(&active);
        // Create the symlink to the session DB.
        std::os::unix::fs::symlink(&entry.session_path, &active).map_err(|e| {
            CoreError::InvalidSessionPath {
                path: active.clone(),
                reason: format!("symlink: {e}"),
            }
        })?;
        // Update last_used_at.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.index.accounts.get_mut(account_id).unwrap().last_used_at = now;
        self.save()?;
        Ok(entry)
    }

    /// Remove an account from the index. Does NOT delete the
    /// session DB or config file (the operator can do that
    /// manually with `session remove`).
    pub fn remove(&mut self, account_id: &str) -> Result<()> {
        self.index.accounts.remove(account_id);
        self.save()
    }

    /// Export an account to a portable tar.gz bundle
    /// (mission 0850p-a-session-export).
    ///
    /// The bundle contains:
    /// - `session.db` — the stoolap session DB
    /// - `session_meta.json` — the sidecar (if present)
    /// - `config.json` — the per-account WhatsAppConfig
    /// - `manifest.json` — `{ account_id, version, exported_at_epoch, sha256: {...} }`
    ///
    /// The bundle is a standard `tar(1)` archive with gzip
    /// compression. The `manifest.json` includes a `sha256` of each
    /// file for integrity verification on import.
    pub fn export(&self, account_id: &str, out: &Path) -> Result<()> {
        let entry = self
            .index
            .accounts
            .get(account_id)
            .ok_or_else(|| CoreError::InvalidSessionPath {
                path: PathBuf::from(account_id),
                reason: "account not in index".to_string(),
            })?;

        // Build a small in-memory tar archive. We use the
        // `tar` crate (not currently a workspace dep), so
        // implement a minimal tar format writer here to avoid
        // adding a new dep. The format is POSIX ustar.
        let mut archive_bytes: Vec<u8> = Vec::new();

        // Compute sha256 of each file as we add it.
        let mut sha256_map: BTreeMap<String, String> = BTreeMap::new();

        // 1. session.db
        let session_bytes = fs::read(&entry.session_path).map_err(|e| CoreError::Read {
            path: entry.session_path.clone(),
            source: e,
        })?;
        let session_sha = sha256_hex(&session_bytes);
        sha256_map.insert("session.db".to_string(), session_sha);
        append_tar_file(&mut archive_bytes, "session.db", &session_bytes);

        // 2. session_meta.json (sidecar; optional)
        let sidecar = entry.session_path.with_extension("db.meta.json");
        if sidecar.exists() {
            let sidecar_bytes =
                fs::read(&sidecar).map_err(|e| CoreError::Read {
                    path: sidecar.clone(),
                    source: e,
                })?;
            let sidecar_sha = sha256_hex(&sidecar_bytes);
            sha256_map.insert("session_meta.json".to_string(), sidecar_sha);
            append_tar_file(&mut archive_bytes, "session_meta.json", &sidecar_bytes);
        }

        // 3. config.json
        if entry.config_path.exists() {
            let config_bytes =
                fs::read(&entry.config_path).map_err(|e| CoreError::Read {
                    path: entry.config_path.clone(),
                    source: e,
                })?;
            let config_sha = sha256_hex(&config_bytes);
            sha256_map.insert("config.json".to_string(), config_sha);
            append_tar_file(&mut archive_bytes, "config.json", &config_bytes);
        }

        // 4. manifest.json
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let manifest = serde_json::json!({
            "account_id": account_id,
            "version": 1,
            "exported_at_epoch": now,
            "sha256": sha256_map,
        });
        let manifest_bytes =
            serde_json::to_vec_pretty(&manifest).map_err(|e| CoreError::Parse {
                path: PathBuf::from("<manifest>"),
                source: e,
            })?;
        append_tar_file(&mut archive_bytes, "manifest.json", &manifest_bytes);

        // End-of-archive marker (two 512-byte zero blocks).
        archive_bytes.resize(archive_bytes.len() + 1024, 0);

        // Gzip-compress the archive.
        let compressed = gzip_encode(&archive_bytes).map_err(|e| CoreError::InvalidBundle {
            path: out.to_path_buf(),
            reason: format!("gzip encode: {e}"),
        })?;

        // Write the bundle.
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| CoreError::InvalidSessionPath {
                    path: parent.to_path_buf(),
                    reason: format!("create parent: {e}"),
                })?;
            }
        }
        fs::write(out, &compressed).map_err(|e| CoreError::Read {
            path: out.to_path_buf(),
            source: e,
        })?;
        Ok(())
    }

    /// Import a portable tar.gz bundle produced by `export()`
    /// (mission 0850p-a-session-export).
    ///
    /// Decompresses, validates the sidecar checksum, registers the
    /// account in the index. The session DB and config are written
    /// to the index's base directory.
    pub fn import_bundle(&mut self, bundle: &Path, account_id: &str) -> Result<AccountEntry> {
        let bytes = fs::read(bundle).map_err(|e| CoreError::Read {
            path: bundle.to_path_buf(),
            source: e,
        })?;
        let archive_bytes = gzip_decode(&bytes).map_err(|e| CoreError::InvalidBundle {
            path: bundle.to_path_buf(),
            reason: format!("gzip decode: {e}"),
        })?;

        // Parse the tar archive.
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut i = 0;
        while i + 512 <= archive_bytes.len() {
            let header = &archive_bytes[i..i + 512];
            if header.iter().all(|&b| b == 0) {
                break; // end-of-archive marker
            }
            let name = parse_tar_name(header);
            if name.is_empty() {
                break;
            }
            let size_str = std::str::from_utf8(&header[124..136])
                .unwrap_or("0")
                .trim_end_matches('\0')
                .trim();
            let size: usize = usize::from_str_radix(size_str.trim(), 8).unwrap_or(0);
            let data_start = i + 512;
            let data_end = data_start + size;
            if data_end > archive_bytes.len() {
                return Err(CoreError::InvalidBundle {
                    path: bundle.to_path_buf(),
                    reason: "truncated tar archive".to_string(),
                });
            }
            let data = archive_bytes[data_start..data_end].to_vec();
            files.insert(name.to_string(), data);
            // Round up to 512-byte boundary.
            i = data_end + (512 - size % 512) % 512;
        }

        // Verify manifest sha256s.
        let manifest_bytes = files.get("manifest.json").ok_or_else(|| CoreError::InvalidBundle {
            path: bundle.to_path_buf(),
            reason: "manifest.json missing from bundle".to_string(),
        })?;
        let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes)
            .map_err(|e| CoreError::Parse {
                path: bundle.to_path_buf(),
                source: e,
            })?;
        let manifest_sha = manifest.get("sha256").and_then(|s| s.as_object()).ok_or_else(|| {
            CoreError::InvalidBundle {
                path: bundle.to_path_buf(),
                reason: "manifest.json missing sha256 object".to_string(),
            }
        })?;
        for (fname, expected_sha_val) in manifest_sha {
            let expected_sha = expected_sha_val.as_str().ok_or_else(|| CoreError::InvalidBundle {
                path: bundle.to_path_buf(),
                reason: "sha256 not a string".to_string(),
            })?;
            let file_bytes = files.get(fname).ok_or_else(|| CoreError::InvalidBundle {
                path: bundle.to_path_buf(),
                reason: format!("{fname} referenced in manifest but missing from archive"),
            })?;
            let actual_sha = sha256_hex(file_bytes);
            if actual_sha != expected_sha {
                return Err(CoreError::InvalidBundle {
                    path: bundle.to_path_buf(),
                    reason: format!(
                        "{fname} sha256 mismatch: expected {expected_sha}, got {actual_sha}"
                    ),
                });
            }
        }

        // Write the files to the base directory.
        let base = self.path.parent().ok_or_else(|| CoreError::InvalidSessionPath {
            path: self.path.clone(),
            reason: "no parent dir".to_string(),
        })?;
        let session_path = base.join(format!("{account_id}.session.db"));
        let config_path = base.join(format!("{account_id}.config.json"));

        if let Some(b) = files.get("session.db") {
            fs::write(&session_path, b).map_err(|e| CoreError::Read {
                path: session_path.clone(),
                source: e,
            })?;
        }
        if let Some(b) = files.get("config.json") {
            fs::write(&config_path, b).map_err(|e| CoreError::Read {
                path: config_path.clone(),
                source: e,
            })?;
        }
        if let Some(b) = files.get("session_meta.json") {
            let sidecar = session_path.with_extension("db.meta.json");
            fs::write(&sidecar, b).map_err(|e| CoreError::Read {
                path: sidecar,
                source: e,
            })?;
        }

        // Register in the index.
        self.import(account_id, &session_path, &config_path)
    }

    /// Save the index to disk.
    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| CoreError::InvalidSessionPath {
                    path: parent.to_path_buf(),
                    reason: format!("create parent: {e}"),
                })?;
            }
        }
        let bytes = serde_json::to_vec_pretty(&self.index).map_err(|e| CoreError::Parse {
            path: self.path.clone(),
            source: e,
        })?;
        fs::write(&self.path, &bytes).map_err(|e| CoreError::Read {
            path: self.path.clone(),
            source: e,
        })?;
        Ok(())
    }
}

fn default_index_base_dir() -> PathBuf {
    let mut base = dirs_data_dir();
    base.push("octo");
    base.push("whatsapp");
    base
}

fn dirs_data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".local");
                p.push("share");
                p
            })
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

// ── Tar + gzip helpers (no external deps) ──────────────────────────

/// Append a file to a tar archive. POSIX ustar format with a
/// 512-byte header followed by the file data, padded to a
/// 512-byte boundary.
fn append_tar_file(archive: &mut Vec<u8>, name: &str, data: &[u8]) {
    let mut header = [0u8; 512];
    // Name (100 bytes, null-padded)
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(100);
    header[..name_len].copy_from_slice(&name_bytes[..name_len]);
    // Mode (8 bytes, octal, null-terminated) — "0000644\0"
    let mode = b"0000644\0";
    header[100..108].copy_from_slice(mode);
    // uid (8 bytes) — "0000000\0"
    header[108..116].copy_from_slice(b"0000000\0");
    // gid (8 bytes) — "0000000\0"
    header[116..124].copy_from_slice(b"0000000\0");
    // Size (12 bytes, octal, null-terminated)
    let size_str = format!("{:011o}\0", data.len());
    header[124..136].copy_from_slice(size_str.as_bytes());
    // mtime (12 bytes) — "00000000000\0"
    header[136..148].copy_from_slice(b"00000000000\0");
    // checksum placeholder (8 bytes) — spaces; will be filled below
    header[148..156].copy_from_slice(b"        ");
    // Type flag (1 byte) — '0' = regular file
    header[156] = b'0';
    // Magic (6 bytes) — "ustar\0"
    header[257..263].copy_from_slice(b"ustar\0");
    // Version (2 bytes) — "00"
    header[263..265].copy_from_slice(b"00");

    // Compute checksum (sum of all bytes in header, treating the
    // checksum field as 8 spaces).
    let checksum: u32 = header.iter().map(|&b| b as u32).sum();
    let checksum_str = format!("{:06o}\0 ", checksum);
    header[148..156].copy_from_slice(checksum_str.as_bytes());

    archive.extend_from_slice(&header);
    archive.extend_from_slice(data);
    // Pad to 512-byte boundary.
    let pad = (512 - data.len() % 512) % 512;
    archive.resize(archive.len() + pad, 0);
}

fn parse_tar_name(header: &[u8]) -> &str {
    let nul = header.iter().position(|&b| b == 0).unwrap_or(100);
    std::str::from_utf8(&header[..nul]).unwrap_or("")
}

/// Minimal gzip encoder (RFC 1952 + RFC 1951 deflate).
/// Uses the standard deflate format with a zlib wrapper.
///
/// We use the `flate2` crate if available; otherwise we
/// implement a simple uncompressed deflate. The bundle format
/// is internal (the matching `import_bundle` uses the same
/// encoder/decoder pair), so we just need a deterministic
/// format that round-trips.
fn gzip_encode(data: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::io::Write as _;
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

fn gzip_decode(data: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// SHA-256 hex digest. Uses the `sha2` crate.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in result {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!(
            "octo-multiaccount-{pid}-{n}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn open_creates_empty_index_if_missing() {
        let dir = tempdir();
        let path = dir.join("index.json");
        let store = MultiAccountStore::open(&path).unwrap();
        assert!(store.list().is_empty());
        assert!(!path.exists()); // not saved yet (no mutations)
    }

    #[test]
    fn import_then_list_roundtrip() {
        let dir = tempdir();
        let index_path = dir.join("index.json");
        let session_path = dir.join("1234.session.db");
        let config_path = dir.join("1234.config.json");
        fs::write(&session_path, b"fake-session").unwrap();
        fs::write(&config_path, b"{}").unwrap();

        let mut store = MultiAccountStore::open(&index_path).unwrap();
        let entry = store
            .import("1234", &session_path, &config_path)
            .unwrap();
        assert_eq!(entry.account_id, "1234");
        assert_eq!(store.list().len(), 1);

        // Reopen and verify persistence.
        let store2 = MultiAccountStore::open(&index_path).unwrap();
        assert_eq!(store2.list().len(), 1);
        assert_eq!(store2.get("1234").unwrap().session_path, session_path);
    }

    #[test]
    fn use_account_writes_active_symlink() {
        let dir = tempdir();
        let index_path = dir.join("index.json");
        let session_path = dir.join("1234.session.db");
        let config_path = dir.join("1234.config.json");
        fs::write(&session_path, b"x").unwrap();
        fs::write(&config_path, b"{}").unwrap();

        let mut store = MultiAccountStore::open(&index_path).unwrap();
        store.import("1234", &session_path, &config_path).unwrap();
        store.use_account("1234").unwrap();
        let active = dir.join("active");
        assert!(active.exists(), "active symlink/file should exist");
    }

    #[test]
    fn export_import_roundtrip() {
        let dir = tempdir();
        let index_path = dir.join("index.json");
        let session_path = dir.join("1234.session.db");
        let config_path = dir.join("1234.config.json");
        let sidecar = session_path.with_extension("db.meta.json");
        fs::write(&session_path, b"fake-session-bytes").unwrap();
        fs::write(&config_path, br#"{"foo":"bar"}"#).unwrap();
        fs::write(&sidecar, br#"{"self_phone":"1234"}"#).unwrap();

        let mut store = MultiAccountStore::open(&index_path).unwrap();
        store.import("1234", &session_path, &config_path).unwrap();
        let bundle = dir.join("bundle.tar.gz");
        store.export("1234", &bundle).unwrap();
        assert!(bundle.exists());

        // Import into a fresh store in a different dir.
        let target_dir = tempdir();
        let target_index = target_dir.join("index.json");
        let mut target_store = MultiAccountStore::open(&target_index).unwrap();
        let imported = target_store.import_bundle(&bundle, "1234").unwrap();
        assert_eq!(imported.account_id, "1234");
        assert_eq!(target_store.list().len(), 1);
    }

    #[test]
    fn tar_roundtrip_preserves_bytes() {
        // Verify the tar encoder/decoder round-trips a file's bytes.
        let mut archive = Vec::new();
        append_tar_file(&mut archive, "test.bin", b"hello world");
        append_tar_file(&mut archive, "test2.bin", b"foo bar baz");
        archive.resize(archive.len() + 1024, 0); // EOF marker

        // Parse it back manually.
        let mut i = 0;
        let mut files = BTreeMap::new();
        while i + 512 <= archive.len() {
            let header = &archive[i..i + 512];
            if header.iter().all(|&b| b == 0) {
                break;
            }
            let name = parse_tar_name(header);
            let size_str = std::str::from_utf8(&header[124..136])
                .unwrap_or("0")
                .trim_end_matches('\0')
                .trim();
            let size: usize = usize::from_str_radix(size_str.trim(), 8).unwrap_or(0);
            let data_start = i + 512;
            let data_end = data_start + size;
            let data = archive[data_start..data_end].to_vec();
            files.insert(name.to_string(), data);
            i = data_end + (512 - size % 512) % 512;
        }
        assert_eq!(files.get("test.bin").unwrap(), b"hello world");
        assert_eq!(files.get("test2.bin").unwrap(), b"foo bar baz");
    }

    #[test]
    fn gzip_roundtrip() {
        let data = b"compress me please";
        let compressed = gzip_encode(data).unwrap();
        let decompressed = gzip_decode(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn sha256_is_deterministic() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"hello");
        assert_eq!(a, b);
        let c = sha256_hex(b"world");
        assert_ne!(a, c);
    }

    #[test]
    fn remove_deletes_account() {
        let dir = tempdir();
        let index_path = dir.join("index.json");
        let session_path = dir.join("1234.session.db");
        let config_path = dir.join("1234.config.json");
        fs::write(&session_path, b"x").unwrap();
        fs::write(&config_path, b"{}").unwrap();

        let mut store = MultiAccountStore::open(&index_path).unwrap();
        store.import("1234", &session_path, &config_path).unwrap();
        assert_eq!(store.list().len(), 1);
        store.remove("1234").unwrap();
        assert_eq!(store.list().len(), 0);
    }
}
