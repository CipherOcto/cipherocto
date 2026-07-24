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

/// Maximum length of an `account_id`. Phone numbers are at most
/// 15 digits (E.164); we cap at 64 to allow prefixed formats
/// (e.g., `wa-` + 15 digits).
pub const MAX_ACCOUNT_ID_LEN: usize = 64;

/// Validate an `account_id` string. Rejects empty strings,
/// path-traversal sequences (`..`), path separators (`/` and
/// `\`), NUL bytes, and oversize values.
///
/// This guards against malicious input that would let a
/// caller of `import_bundle` / `use_account` write files
/// outside the base directory.
pub fn validate_account_id(account_id: &str) -> Result<()> {
    if account_id.is_empty() {
        return Err(CoreError::InvalidSessionPath {
            path: PathBuf::from(account_id),
            reason: "account_id is empty".to_string(),
        });
    }
    if account_id.len() > MAX_ACCOUNT_ID_LEN {
        return Err(CoreError::InvalidSessionPath {
            path: PathBuf::from(account_id),
            reason: format!("account_id too long (max {MAX_ACCOUNT_ID_LEN})"),
        });
    }
    if account_id == ".." || account_id == "." {
        return Err(CoreError::InvalidSessionPath {
            path: PathBuf::from(account_id),
            reason: "account_id is a relative-path component".to_string(),
        });
    }
    if account_id.contains('/') || account_id.contains('\\') || account_id.contains('\0') {
        return Err(CoreError::InvalidSessionPath {
            path: PathBuf::from(account_id),
            reason: "account_id contains a path separator or NUL byte".to_string(),
        });
    }
    Ok(())
}

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
    ///
    /// Self-heal: if the index file does not exist or is empty, scan
    /// the base directory for legacy (pre-onboard-core) accounts —
    /// directories named `<id>/session.db/` or `<id>.session.db/`
    /// paired with a `<id>.meta.json` (or `<id>.session.db.meta.json`)
    /// sibling — and import them into the index. Idempotent: once
    /// the index has any entry the discovery step is skipped.
    pub fn open_default() -> Result<Self> {
        let base = default_index_base_dir();
        fs::create_dir_all(&base).map_err(|e| CoreError::InvalidSessionPath {
            path: base.clone(),
            reason: format!("create base: {e}"),
        })?;
        let mut store = Self::open(base.join("index.json"))?;
        if store.index.accounts.is_empty() {
            // Self-heal once: pull legacy accounts into the index.
            let discovered = Self::discover_from_disk(&base);
            if !discovered.is_empty() {
                for entry in discovered {
                    store.index.accounts.insert(entry.account_id.clone(), entry);
                }
                store.save()?;
            }
        }
        Ok(store)
    }

    /// Scan `base_dir` for legacy (pre-onboard-core) accounts and
    /// synthesise an `AccountEntry` for each. Used as a one-shot
    /// self-heal fallback from `open_default` when no index file
    /// exists. Never mutates the store or filesystem on its own —
    /// the caller decides whether to persist.
    ///
    /// Two on-disk layouts are recognised:
    ///
    /// * **Pattern A** — legacy flat shape: `<base>/<id>.session.db/`
    ///   (dir) paired with `<base>/<id>.session.db.meta.json`.
    ///   Example: `bak_main_phone.session.db/`,
    ///   `logout.session.db/`.
    ///
    /// * **Pattern B** — legacy per-account-directory shape:
    ///   `<base>/<id>/session.db/` (dir) paired with
    ///   `<base>/<id>.meta.json`. This is the canonical case for
    ///   the live `default` account.
    ///
    /// Entries whose meta.json is broken (`*.session.db.broken-*`
    /// remnants), whose `account_id` fails `validate_account_id`,
    /// or whose session directory is missing are silently skipped
    /// — discovery is best-effort.
    pub fn discover_from_disk(base_dir: &Path) -> Vec<AccountEntry> {
        let mut entries: Vec<AccountEntry> = Vec::new();
        let read_dir = match fs::read_dir(base_dir) {
            Ok(rd) => rd,
            Err(_) => return entries,
        };
        for dir_entry in read_dir.flatten() {
            let path = dir_entry.path();
            if !path.is_file() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            // Two accepted meta.json naming shapes:
            //   `<id>.meta.json`           (Pattern B)
            //   `<id>.session.db.meta.json` (Pattern A)
            let id: &str = if let Some(stripped) = name.strip_suffix(".meta.json") {
                if let Some(prefix) = stripped.strip_suffix(".session.db") {
                    prefix
                } else {
                    stripped
                }
            } else {
                continue;
            };
            if id.is_empty() || validate_account_id(id).is_err() {
                continue;
            }
            // Locate the session directory for this account.
            let session_pattern_a = base_dir.join(format!("{id}.session.db"));
            let session_pattern_b = base_dir.join(id).join("session.db");
            let session_path = if session_pattern_a.is_dir() {
                session_pattern_a
            } else if session_pattern_b.is_dir() {
                session_pattern_b
            } else {
                continue;
            };
            // Parse linked_at from the meta.json. The legacy file uses
            // ISO 8601; we accept RFC 3339 / UTC-suffixed forms.
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            #[derive(Deserialize)]
            struct MetaFile {
                #[serde(default)]
                linked_at: String,
            }
            let linked_at = serde_json::from_slice::<MetaFile>(&bytes)
                .ok()
                .and_then(|m| parse_iso8601_to_unix(&m.linked_at))
                .unwrap_or(0);
            entries.push(AccountEntry {
                account_id: id.to_string(),
                session_path,
                config_path: path,
                linked_at,
                last_used_at: 0,
            });
        }
        entries.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        entries
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

    /// Returns the directory the index file lives in. Used by callers
    /// that want to re-scan the on-disk layout via `discover_from_disk`
    /// without having to plumb the base dir through separately.
    pub fn base_dir(&self) -> &Path {
        self.path
            .parent()
            .expect("MultiAccountStore path always has a parent directory")
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
        validate_account_id(account_id)?;
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
        validate_account_id(account_id)?;
        let entry = self
            .index
            .accounts
            .get(account_id)
            .cloned()
            .ok_or_else(|| CoreError::InvalidSessionPath {
                path: PathBuf::from(account_id),
                reason: "account not in index".to_string(),
            })?;
        let base = self
            .path
            .parent()
            .ok_or_else(|| CoreError::InvalidSessionPath {
                path: self.path.clone(),
                reason: "no parent dir".to_string(),
            })?;
        let active = base.join("active");
        // Remove any existing symlink/file.
        let _ = fs::remove_file(&active);
        // Create the symlink to the session DB. On Windows, fall
        // back to copying the file (symlinks need admin/dev mode).
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&entry.session_path, &active).map_err(|e| {
                CoreError::InvalidSessionPath {
                    path: active.clone(),
                    reason: format!("symlink: {e}"),
                }
            })?;
        }
        #[cfg(not(unix))]
        {
            fs::copy(&entry.session_path, &active).map_err(|e| CoreError::InvalidSessionPath {
                path: active.clone(),
                reason: format!("copy fallback: {e}"),
            })?;
        }
        // Update last_used_at.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.index
            .accounts
            .get_mut(account_id)
            .unwrap()
            .last_used_at = now;
        self.save()?;
        Ok(entry)
    }

    /// Remove an account from the index. Does NOT delete the
    /// session DB or config file (the operator can do that
    /// manually with `session remove`).
    pub fn remove(&mut self, account_id: &str) -> Result<()> {
        validate_account_id(account_id)?;
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
        validate_account_id(account_id)?;
        let entry =
            self.index
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
            let sidecar_bytes = fs::read(&sidecar).map_err(|e| CoreError::Read {
                path: sidecar.clone(),
                source: e,
            })?;
            let sidecar_sha = sha256_hex(&sidecar_bytes);
            sha256_map.insert("session_meta.json".to_string(), sidecar_sha);
            append_tar_file(&mut archive_bytes, "session_meta.json", &sidecar_bytes);
        }

        // 3. config.json
        if entry.config_path.exists() {
            let config_bytes = fs::read(&entry.config_path).map_err(|e| CoreError::Read {
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
        validate_account_id(account_id)?;
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
        let manifest_bytes =
            files
                .get("manifest.json")
                .ok_or_else(|| CoreError::InvalidBundle {
                    path: bundle.to_path_buf(),
                    reason: "manifest.json missing from bundle".to_string(),
                })?;
        let manifest: serde_json::Value =
            serde_json::from_slice(manifest_bytes).map_err(|e| CoreError::Parse {
                path: bundle.to_path_buf(),
                source: e,
            })?;
        let manifest_sha = manifest
            .get("sha256")
            .and_then(|s| s.as_object())
            .ok_or_else(|| CoreError::InvalidBundle {
                path: bundle.to_path_buf(),
                reason: "manifest.json missing sha256 object".to_string(),
            })?;
        for (fname, expected_sha_val) in manifest_sha {
            let expected_sha =
                expected_sha_val
                    .as_str()
                    .ok_or_else(|| CoreError::InvalidBundle {
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
        let base = self
            .path
            .parent()
            .ok_or_else(|| CoreError::InvalidSessionPath {
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

/// Resolves the default base directory the index file lives in
/// (`<data_dir>/octo/whatsapp/`, honours `OCTO_WHATSAPP_DATA_DIR`).
/// `pub` so downstream crates (e.g. the runtime daemon's
/// `accounts.list` handler) can fall back to the env-derived path
/// when the store fails to initialise at boot.
pub fn default_index_base_dir() -> PathBuf {
    let mut base = dirs_data_dir();
    base.push("octo");
    base.push("whatsapp");
    base
}

/// Minimal RFC 3339 / ISO 8601-UTC parser. Accepts shapes like
/// `2026-07-09T11:41:47Z` and `2026-07-09T11:41:47.123Z`. Returns
/// `None` on any deviation — discovery is best-effort and the
/// caller falls back to `0` for the `linked_at` field.
fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    // Indices:        0123456789012345678
    // Required shape: YYYY-MM-DDTHH:MM:SS[.fff]Z
    let bytes = s.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || (bytes[10] != b'T' && bytes[10] != b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let parse_int = |a: usize, b: usize| -> Option<i64> {
        std::str::from_utf8(&bytes[a..b]).ok()?.parse::<i64>().ok()
    };
    let y = parse_int(0, 4)?;
    let mo = parse_int(5, 7)?;
    let d = parse_int(8, 10)?;
    let h = parse_int(11, 13)?;
    let mi = parse_int(14, 16)?;
    let sec = parse_int(17, 19)?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from 1970-01-01 to YYYY-01-01 (proleptic Gregorian).
    let years_from_epoch = y - 1970;
    let leap_years =
        (y - 1) / 4 - (y - 1) / 100 + (y - 1) / 400 - (1969 / 4 - 1969 / 100 + 1969 / 400);
    let mut day_of_year: i64 = 0;
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..mo {
        day_of_year += month_days[(m - 1) as usize];
    }
    if mo > 2 && is_leap_year(y) {
        day_of_year += 1;
    }
    let days = years_from_epoch * 365 + leap_years + (d - 1) + day_of_year;
    Some(days * 86_400 + h * 3600 + mi * 60 + sec)
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
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
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
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

    /// Process-wide mutex serialising tests that mutate
    /// `XDG_DATA_HOME` / `HOME`. `MultiAccountStore::open_default`
    /// reads those env vars at call time, so concurrent tests in
    /// the same process can observe each other's overrides and
    /// read the wrong base dir. Hold this guard for the entire
    /// duration of any `open_default` test.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let entry = store.import("1234", &session_path, &config_path).unwrap();
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

    #[test]
    fn validate_account_id_accepts_normal() {
        assert!(validate_account_id("15551234567").is_ok());
        assert!(validate_account_id("wa-15551234567").is_ok());
        assert!(validate_account_id("a").is_ok());
    }

    #[test]
    fn validate_account_id_rejects_empty() {
        assert!(validate_account_id("").is_err());
    }

    #[test]
    fn validate_account_id_rejects_path_traversal() {
        assert!(validate_account_id("..").is_err());
        assert!(validate_account_id(".").is_err());
        assert!(validate_account_id("../etc/passwd").is_err());
        assert!(validate_account_id("foo/bar").is_err());
        assert!(validate_account_id("foo\\bar").is_err());
        assert!(validate_account_id("foo\0bar").is_err());
    }

    #[test]
    fn validate_account_id_rejects_oversize() {
        let long = "a".repeat(MAX_ACCOUNT_ID_LEN + 1);
        assert!(validate_account_id(&long).is_err());
    }

    #[test]
    fn import_rejects_path_traversal_account_id() {
        let dir = tempdir();
        let index_path = dir.join("index.json");
        let session_path = dir.join("1234.session.db");
        let config_path = dir.join("1234.config.json");
        fs::write(&session_path, b"x").unwrap();
        fs::write(&config_path, b"{}").unwrap();

        let mut store = MultiAccountStore::open(&index_path).unwrap();
        assert!(store
            .import("../evil", &session_path, &config_path)
            .is_err());
        assert!(store.import("..", &session_path, &config_path).is_err());
        assert!(store
            .import("foo/bar", &session_path, &config_path)
            .is_err());
    }

    #[test]
    fn remove_rejects_path_traversal_account_id() {
        let dir = tempdir();
        let index_path = dir.join("index.json");
        let session_path = dir.join("1234.session.db");
        let config_path = dir.join("1234.config.json");
        fs::write(&session_path, b"x").unwrap();
        fs::write(&config_path, b"{}").unwrap();

        let mut store = MultiAccountStore::open(&index_path).unwrap();
        store.import("1234", &session_path, &config_path).unwrap();
        // Path-traversal account_ids must be rejected.
        assert!(store.remove("../evil").is_err());
        assert!(store.remove("..").is_err());
        assert!(store.remove("foo/bar").is_err());
        // And the index must still contain the original account.
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn export_rejects_path_traversal_account_id() {
        let dir = tempdir();
        let index_path = dir.join("index.json");
        let session_path = dir.join("1234.session.db");
        let config_path = dir.join("1234.config.json");
        fs::write(&session_path, b"x").unwrap();
        fs::write(&config_path, b"{}").unwrap();

        let mut store = MultiAccountStore::open(&index_path).unwrap();
        store.import("1234", &session_path, &config_path).unwrap();
        // Path-traversal account_ids must be rejected by export.
        assert!(store.export("../evil", &dir.join("out.tar.gz")).is_err());
        assert!(store.export("..", &dir.join("out.tar.gz")).is_err());
    }

    // ── discover_from_disk + open_default self-heal (2026-07-13) ────
    //
    // Background: prior to the `onboard-core` multi-account index,
    // accounts lived on disk as `<id>.meta.json` + `<id>.session.db/`
    // siblings. The index file (`index.json`) didn't exist until an
    // explicit `import()` call was made. Operators who linked an
    // account via the pre-onboard-core `qr-link` flow and never ran
    // `import` saw an empty `daemon.accounts.list` even though their
    // account was clearly on disk and active.
    //
    // The fix: `open_default` self-heals by scanning the base dir
    // for legacy accounts when the index is empty and importing
    // them once. `discover_from_disk` does the actual scan.
    //
    // The tests below pin the discovery contract on both supported
    // on-disk shapes (flat `<id>.session.db/` and per-account-dir
    // `<id>/session.db/`), reject broken-session remnants, and
    // verify the open_default self-heal is idempotent.

    /// Helper: create a Pattern-A legacy account (flat `<id>.session.db/`
    /// + `<id>.session.db.meta.json`).
    fn make_legacy_pattern_a(base: &Path, id: &str, linked_at: &str) {
        let session = base.join(format!("{id}.session.db"));
        fs::create_dir_all(&session).unwrap();
        let meta = base.join(format!("{id}.session.db.meta.json"));
        fs::write(
            &meta,
            format!(
                r#"{{"self_phone":"123","linked_at":"{linked_at}","mode":"qr-link","groups":[]}}"#
            ),
        )
        .unwrap();
    }

    /// Helper: create a Pattern-B legacy account (per-account-dir
    /// `<id>/session.db/` + `<id>.meta.json`).
    fn make_legacy_pattern_b(base: &Path, id: &str, linked_at: &str) {
        let account_dir = base.join(id);
        fs::create_dir_all(account_dir.join("session.db")).unwrap();
        let meta = base.join(format!("{id}.meta.json"));
        fs::write(
            &meta,
            format!(
                r#"{{"self_phone":"123","linked_at":"{linked_at}","mode":"qr-link","groups":[]}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn discover_from_disk_pattern_a() {
        let base = tempdir();
        make_legacy_pattern_a(&base, "bak_main_phone", "2026-06-26T20:10:20Z");
        let found = MultiAccountStore::discover_from_disk(&base);
        assert_eq!(found.len(), 1);
        let e = &found[0];
        assert_eq!(e.account_id, "bak_main_phone");
        assert_eq!(e.session_path, base.join("bak_main_phone.session.db"));
        assert_eq!(
            e.config_path,
            base.join("bak_main_phone.session.db.meta.json")
        );
        assert_eq!(
            e.linked_at,
            parse_iso8601_to_unix("2026-06-26T20:10:20Z").unwrap()
        );
    }

    #[test]
    fn discover_from_disk_pattern_b() {
        let base = tempdir();
        make_legacy_pattern_b(&base, "default", "2026-07-09T11:41:47Z");
        let found = MultiAccountStore::discover_from_disk(&base);
        assert_eq!(found.len(), 1);
        let e = &found[0];
        assert_eq!(e.account_id, "default");
        assert_eq!(e.session_path, base.join("default").join("session.db"));
        assert_eq!(e.config_path, base.join("default.meta.json"));
        assert_eq!(
            e.linked_at,
            parse_iso8601_to_unix("2026-07-09T11:41:47Z").unwrap()
        );
    }

    #[test]
    fn discover_from_disk_handles_multiple_sorted_by_id() {
        let base = tempdir();
        make_legacy_pattern_a(&base, "zebra", "2026-01-01T00:00:00Z");
        make_legacy_pattern_b(&base, "alpha", "2026-02-02T00:00:00Z");
        make_legacy_pattern_a(&base, "middle", "2026-03-03T00:00:00Z");
        let found = MultiAccountStore::discover_from_disk(&base);
        let ids: Vec<&str> = found.iter().map(|e| e.account_id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn discover_from_disk_skips_meta_without_session() {
        // Orphan meta.json with no matching session dir on either
        // pattern must be skipped — otherwise `accounts.list` would
        // claim an account exists when its session is gone.
        let base = tempdir();
        fs::write(
            base.join("orphan.meta.json"),
            br#"{"self_phone":"x","linked_at":"2026-01-01T00:00:00Z","mode":"qr-link","groups":[]}"#,
        )
        .unwrap();
        let found = MultiAccountStore::discover_from_disk(&base);
        assert!(found.is_empty());
    }

    #[test]
    fn discover_from_disk_skips_broken_renames() {
        // `*.session.db.broken-*` are the renamed remnants of failed
        // session opens (see persistence-failure handler). They must
        // NOT be discovered — they're known-bad.
        let base = tempdir();
        fs::create_dir_all(base.join("dead.session.db.broken-12345")).unwrap();
        fs::write(
            base.join("dead.session.db.meta.json"),
            br#"{"linked_at":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        // Also a real legacy account alongside — should still be found.
        make_legacy_pattern_b(&base, "live", "2026-02-02T00:00:00Z");
        let found = MultiAccountStore::discover_from_disk(&base);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].account_id, "live");
    }

    #[test]
    fn discover_from_disk_missing_base_dir_returns_empty() {
        let ghost = std::env::temp_dir().join(format!(
            "octo-multiaccount-nonexistent-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(MultiAccountStore::discover_from_disk(&ghost).is_empty());
    }

    #[test]
    fn parse_iso8601_to_unix_known_timestamp() {
        // 2026-07-09T11:41:47Z. Verified externally:
        //   datetime(2026,7,9,11,41,47,tz=UTC).timestamp() == 1783597307
        let secs = parse_iso8601_to_unix("2026-07-09T11:41:47Z").unwrap();
        assert_eq!(secs, 1_783_597_307);
    }

    #[test]
    fn parse_iso8601_to_unix_rejects_malformed() {
        assert!(parse_iso8601_to_unix("").is_none());
        assert!(parse_iso8601_to_unix("not-a-date").is_none());
        assert!(parse_iso8601_to_unix("2026-13-01T00:00:00Z").is_none()); // bad month
        assert!(parse_iso8601_to_unix("2026-01-32T00:00:00Z").is_none()); // bad day
        assert!(parse_iso8601_to_unix("2026-01-01").is_none()); // missing time
        assert!(parse_iso8601_to_unix("2026-01-01T00:00:00").is_none()); // missing Z
    }

    #[test]
    fn open_default_self_heals_legacy_accounts() {
        // Simulate the user's state: no `index.json`, but legacy
        // accounts on disk. `open_default` should auto-import them.
        //
        // We can't override the base dir for `open_default` (it uses
        // XDG_DATA_HOME / HOME directly), so we shadow the env vars.
        // Hold ENV_LOCK for the whole test to avoid races with
        // sibling tests that mutate the same env vars.
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let base = tempdir();
        let prior_xdg = std::env::var_os("XDG_DATA_HOME");
        let prior_home = std::env::var_os("HOME");
        // SAFETY: env-mutation is test-local and serial under cargo's
        // default test runner (each #[test] gets its own thread but
        // `std::env::set_var` is process-global; the next test that
        // touches HOME/XDG_DATA_HOME will reset). We do restore at
        // the end to avoid leaking the override to sibling tests.
        std::env::set_var("XDG_DATA_HOME", &base);
        std::env::set_var("HOME", &base);

        // `open_default` resolves to `<XDG_DATA_HOME>/octo/whatsapp/`,
        // not `<XDG_DATA_HOME>/` directly — see `default_index_base_dir`.
        let wa_dir = base.join("octo").join("whatsapp");
        fs::create_dir_all(&wa_dir).unwrap();

        // Lay down two legacy accounts at the resolved base.
        make_legacy_pattern_b(&wa_dir, "default", "2026-07-09T11:41:47Z");
        make_legacy_pattern_a(&wa_dir, "bak_main_phone", "2026-06-26T20:10:20Z");

        // Run the production boot path.
        let store = MultiAccountStore::open_default().unwrap();

        // Discovery must have populated the in-memory index.
        let entries = store.list();
        assert_eq!(entries.len(), 2, "expected 2 auto-imported accounts");
        let ids: Vec<&str> = entries.iter().map(|e| e.account_id.as_str()).collect();
        assert_eq!(ids, vec!["bak_main_phone", "default"]);

        // And persisted the new index file so the next boot is cheap.
        let index_path = wa_dir.join("index.json");
        assert!(
            index_path.exists(),
            "open_default must persist after self-heal"
        );
        // File must parse as a valid IndexFile (would fail on broken JSON).
        let bytes = fs::read(&index_path).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_slice(&bytes).expect("self-healed index.json must be valid JSON");
        assert!(
            parsed.get("accounts").is_some(),
            "index must have 'accounts' key"
        );

        // Restore env to avoid leaking into sibling tests.
        if let Some(v) = prior_xdg {
            std::env::set_var("XDG_DATA_HOME", v);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }
        if let Some(v) = prior_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn open_default_idempotent_when_already_populated() {
        // When `index.json` already has an entry, discovery must NOT
        // run (no extra entries, no scan overhead, no surprise
        // re-import of files the user might have removed).
        let _env_guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let base = tempdir();
        let prior_xdg = std::env::var_os("XDG_DATA_HOME");
        let prior_home = std::env::var_os("HOME");
        std::env::set_var("XDG_DATA_HOME", &base);
        std::env::set_var("HOME", &base);

        // `open_default` resolves to `<XDG_DATA_HOME>/octo/whatsapp/`,
        // not `<XDG_DATA_HOME>/` directly — see `default_index_base_dir`.
        let wa_dir = base.join("octo").join("whatsapp");
        fs::create_dir_all(&wa_dir).unwrap();

        // First boot: discover 2 legacy accounts.
        make_legacy_pattern_b(&wa_dir, "default", "2026-07-09T11:41:47Z");
        make_legacy_pattern_a(&wa_dir, "bak_main_phone", "2026-06-26T20:10:20Z");
        let store1 = MultiAccountStore::open_default().unwrap();
        assert_eq!(store1.list().len(), 2);

        // Drop one legacy account on disk after boot — it should
        // still appear in subsequent boots because the index has it.
        fs::remove_dir_all(wa_dir.join("bak_main_phone.session.db")).unwrap();
        fs::remove_file(wa_dir.join("bak_main_phone.session.db.meta.json")).unwrap();

        // Second boot: index already populated → no re-discovery.
        let store2 = MultiAccountStore::open_default().unwrap();
        assert_eq!(
            store2.list().len(),
            2,
            "index entries persist; discovery is skipped on populated index"
        );

        if let Some(v) = prior_xdg {
            std::env::set_var("XDG_DATA_HOME", v);
        } else {
            std::env::remove_var("XDG_DATA_HOME");
        }
        if let Some(v) = prior_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }
}
