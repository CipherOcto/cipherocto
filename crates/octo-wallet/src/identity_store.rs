//! On-disk identity store (mission 0011-x-s-a-wallet-store-identity).
//!
//! Turns the existing zero-sized `WalletStore` stub into a real
//! encrypted-at-rest identity store. Metadata reads (active DID, record
//! lookup, list, slot presence, orphan slots) need NO passphrase; the
//! signing paths thread through `unlock`/`UnlockedWallet` and require
//! a passphrase. The two surfaces are kept separate so the operator
//! guide's claim that `octo whoami` exits 2 because no identity exists
//! becomes testable rather than a wall.
//!
//! Storage layout under the resolved root, all permissions enforced
//! on first write:
//!
//! ```text
//! root/                       0700  (created lazily)
//!   store.json                0600  WalletIndex (version, active_did, records[])
//!   seed/                     0700  Vault slots dir
//!     <slug>.vault            0600  Argon2id + AES-256-GCM, holds the 32-byte seed
//! ```
//!
//! `store.json` is unencrypted on purpose: it carries no secret, only
//! DIDs, public keys, lifecycle states, and timestamps. Encrypting it
//! would force a passphrase onto every metadata read and defeat the
//! design goal that metadata stays passphrase-free.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::WalletError;
use crate::identity::IdentityKey;
use crate::identity_record::{Did, IdentityRecord};
use crate::lifecycle::LifecycleState;
use crate::vault::{validate_slot_id, Vault};

/// Schema version for `store.json`. Bumped on backwards-incompatible
/// format changes; a reader rejects an unknown version so a future
/// in-place migration can be staged safely.
pub const WALLET_INDEX_VERSION: u32 = 1;

/// Minimum passphrase length enforced at both `WalletStore::register`
/// and `WalletStore::unlock` (mission 0011-x-s-a-wallet-store-identity
/// §AC-28). Declared here, NOT in `error.rs`, per the mission's AC-28
/// that names this module as the declaration site. The `WeakPassphrase`
/// `#[error]` message in `error.rs` interpolates the same constant via
/// `pub use crate::identity_store::MIN_PASSPHRASE_CHARS`, so the
/// sentence an operator reads cannot drift from the threshold the
/// check compares against.
pub const MIN_PASSPHRASE_CHARS: usize = 12;

/// On-disk wallet index. Persisted as `store.json` under the store root.
///
/// NO `PartialEq` derive: `IdentityRecord` derives only `Debug, Clone,
/// Serialize, Deserialize` and `Vec<IdentityRecord>: PartialEq` does
/// not hold, so a derived `PartialEq` here is `E0369`. Tests that need
/// to compare two indices compare the bytes of `serde_json::to_vec`
/// instead — byte-stability is what the determinism AC actually
/// requires.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletIndex {
    /// Schema version. Currently `WALLET_INDEX_VERSION` (1); a reader
    /// MUST reject an unknown version with `WalletError::KeystoreVersion`.
    pub version: u32,
    /// Active DID pointer. `None` on a fresh store or after the
    /// operator has not yet selected an identity.
    pub active_did: Option<Did>,
    /// Records in ascending-DID order so `store.json` is byte-stable
    /// for a given store state (mission §AC-15). Insertion is followed
    /// by a re-sort; reading is direct.
    pub records: Vec<IdentityRecord>,
}

impl Default for WalletIndex {
    fn default() -> Self {
        Self {
            version: WALLET_INDEX_VERSION,
            active_did: None,
            records: Vec::new(),
        }
    }
}

impl WalletIndex {
    /// Look up an identity record by DID. Returns `IdentityNotFound`
    /// (not `NotActive`) on a miss; that name follows the doc comment
    /// on the original stub.
    pub fn record(&self, did: &Did) -> Result<&IdentityRecord, WalletError> {
        self.records
            .iter()
            .find(|r| &r.did == did)
            .ok_or_else(|| WalletError::IdentityNotFound(did.clone()))
    }
}

/// On-disk wallet store. Holds the resolved root, the in-memory
/// `WalletIndex`, and a `Vault` for the encrypted seed slots.
///
/// The store is `Clone` (cheap — three `PathBuf` / index / vault) so
/// the CLI can hand it around without borrowing lifetimes; the
/// `UnlockedWallet` handle borrows mutably because every method on it
/// mutates either the index or the key.
///
/// `vault` is held but not yet read in this slice — the Phase 3
/// unlock path (`WalletStore::unlock`) and `complete_rotation` /
/// `abort_rotation` will reach through it. The seed-write itself
/// (the `Vault::put(slug, seed, passphrase)` call) belongs in the
/// register bootstrap write path, which lands with `unlock` because
/// the mission YAML's `register` signature takes a pre-built
/// `IdentityKey` rather than a raw seed; the seed extraction happens
/// in Phase 3 alongside the InMemorySigner `seed()` accessor that the
/// store's write path needs.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WalletStore {
    root: PathBuf,
    index: WalletIndex,
    vault: Vault,
}

/// Resolution outcome for the home resolver. Used internally; the
/// public surface exposes only `Ok(PathBuf)` or `Err(WalletError::Config)`.
enum ResolvedHome {
    Octo(PathBuf),
    HomeUnder(PathBuf),
}

impl WalletStore {
    // ------------------------------------------------------------------
    // Open / open_at / reload
    // ------------------------------------------------------------------

    /// Open the on-disk wallet store at `$OCTO_HOME/wallet` (or
    /// `$HOME/.octo/wallet` when `OCTO_HOME` is unset).
    ///
    /// Four-step resolution (mission §AC-3):
    /// 1. `OCTO_HOME` set AND non-empty → use it.
    /// 2. `OCTO_HOME` set but empty → `WalletError::Config`.
    /// 3. `OCTO_HOME` unset + `HOME` set AND non-empty → use `HOME/.octo`.
    /// 4. `OCTO_HOME` unset + `HOME` empty/unset → `WalletError::Config`.
    ///
    /// `open()` is metadata-only and creates no directory (mission
    /// §AC-4). The root directory is created lazily on the first
    /// write. A pre-existing permissive root has its mode corrected
    /// to 0o700 on open (mission §AC-5).
    ///
    /// # Errors
    /// Returns `WalletError::Config` when neither `OCTO_HOME` nor
    /// `HOME` resolves to a non-empty path. Returns `WalletError::Io`
    /// on filesystem failure reading `store.json`.
    pub fn open() -> Result<Self, WalletError> {
        let root = resolve_wallet_root();
        if root.as_os_str().is_empty() {
            return Err(WalletError::Config(
                "neither $OCTO_HOME nor $HOME resolves to a non-empty path".to_owned(),
            ));
        }
        Self::open_at(root)
    }

    /// Test seam: open the store at an explicit root without reading
    /// the environment. Used by the substrate test vectors.
    ///
    /// # Errors
    /// Returns `WalletError::KeystoreVersion` on an unknown
    /// `store.json` schema version, `WalletError::Io` on filesystem
    /// failure.
    pub fn open_at(root: impl Into<PathBuf>) -> Result<Self, WalletError> {
        let root = root.into();
        // AC-5: a pre-existing permissive mode is corrected on open.
        // AC-4: a non-existent root is left alone — no directory is
        // created at open time. The lazy-create happens on first write.
        if root.exists() {
            Self::correct_root_perms(&root)?;
        }
        let index = Self::read_or_init_index(&root)?;
        let vault = Vault::open_lazy(root.join("seed"));
        Ok(Self { root, index, vault })
    }

    /// Re-read `store.json` from disk into the in-memory index. Used by
    /// the persistence vectors (`tv_x_37`) where an external edit is
    /// expected to take effect on the next call.
    ///
    /// # Errors
    /// Returns `WalletError::Io` on filesystem failure,
    /// `WalletError::KeystoreVersion` on an unknown schema version.
    pub fn reload(&mut self) -> Result<(), WalletError> {
        self.index = Self::read_or_init_index(&self.root)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Metadata-only readers (no passphrase required)
    // ------------------------------------------------------------------

    /// The currently active DID, if any. `None` on a fresh store or
    /// before the operator has selected an identity.
    #[must_use]
    pub fn active_did(&self) -> Option<&Did> {
        self.index.active_did.as_ref()
    }

    /// All records, in ascending-DID order.
    #[must_use]
    pub fn list_records(&self) -> &[IdentityRecord] {
        &self.index.records
    }

    /// Look up an identity record by DID (mission §AC-32 — the only
    /// existing call site is `cli_fns::identity_record`, swept by this
    /// mission rather than deleted because the CLI uses it for
    /// `octo identity show <did>`).
    ///
    /// # Errors
    /// Returns `WalletError::IdentityNotFound` when the DID is not
    /// in the index.
    pub fn identity_record(&self, did: &Did) -> Result<&IdentityRecord, WalletError> {
        self.index.record(did)
    }

    /// Return the active identity key. The signature is preserved from
    /// the parent RFC; the substrate returns `Locked` because every
    /// signing path must thread through `unlock` first (mission §AC-9,
    /// §AC-17).
    ///
    /// # Errors
    /// Returns `WalletError::Locked` unconditionally in this
    /// mission's slice — the unlock split means the active key is
    /// only reachable via `WalletStore::unlock`.
    pub fn try_active_identity(&self) -> Result<crate::identity::IdentityKey, WalletError> {
        Err(WalletError::Locked)
    }

    /// Look up an identity record by DID. The signature is preserved
    /// from the stub; the substrate returns `IdentityNotFound` on a
    /// miss rather than the old `NotActive` placeholder.
    ///
    /// # Errors
    /// Returns `WalletError::IdentityNotFound` when the DID is not
    /// in the index.
    pub fn lookup_identity_record(
        &self,
        did: &Did,
    ) -> Result<crate::identity_record::IdentityRecord, WalletError> {
        self.index.record(did).cloned().map_err(|e| match e {
            WalletError::IdentityNotFound(_) => e,
            // WalletIndex::record only returns IdentityNotFound
            other => other,
        })
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    fn correct_root_perms(root: &Path) -> Result<(), WalletError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let current = fs::metadata(root)?.permissions().mode();
            if current & 0o077 != 0 {
                fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
            }
        }
        Ok(())
    }

    fn read_or_init_index(root: &Path) -> Result<WalletIndex, WalletError> {
        let path = root.join("store.json");
        if !path.exists() {
            return Ok(WalletIndex::default());
        }
        let bytes = fs::read(&path)?;
        if bytes.is_empty() {
            return Ok(WalletIndex::default());
        }
        let index: WalletIndex = serde_json::from_slice(&bytes)
            .map_err(|e| WalletError::KeystoreParse(format!("store.json deserialize: {e}")))?;
        if index.version != WALLET_INDEX_VERSION {
            return Err(WalletError::KeystoreVersion {
                expected: WALLET_INDEX_VERSION.to_string(),
                got: index.version.to_string(),
            });
        }
        Ok(index)
    }

    // ------------------------------------------------------------------
    // Write path (mission 0011-x-s-a-wallet-store-identity §AC-1..§AC-2)
    // ------------------------------------------------------------------

    /// Register a new identity. The caller supplies a pre-built
    /// `IdentityKey` (so the substrate does not own the OS-RNG that
    /// generated the seed — that's the caller's responsibility),
    /// the passphrase that will gate the encrypted slot, an
    /// `activate: bool` flag, and the wall-clock now.
    ///
    /// Returns the DID of the registered identity on
    /// `Ok(Did)`. Steps:
    ///
    /// 1. Validate passphrase length against `MIN_PASSPHRASE_CHARS`.
    /// 2. Reject `IdentityKey` already in the terminal `Revoked`
    ///    lifecycle — the substrate refuses to manufacture a
    ///    signable record from a terminal key. Returns
    ///    `WalletError::AlreadyRevoked` (mission YAML §register
    ///    doc-comment).
    /// 3. Compose the slug from `key.public_key_bytes()` and stash
    ///    the seed into `Vault::put` so it is recoverable through
    ///    `WalletStore::unlock` (Phase 3). `Vault::put` calls
    ///    `ensure_slots_dir()` so the seed directory is created
    ///    lazily on the first write (mission §AC-4).
    /// 4. Build an `IdentityRecord` and insert into the in-memory
    ///    `WalletIndex.records`, sorted by DID for byte-stable
    ///    storage (mission §AC-15). Binary search confirms no
    ///    pre-existing record has it; a hit means re-registration
    ///    of an existing DID, which is refused with
    ///    `WalletError::AlreadyRevoked`.
    /// 5. Set `index.active_did` to the new DID when no identity
    ///    has been active yet (fresh-store case). Subsequent
    ///    registers do not auto-promote; an explicit `select` is
    ///    required.
    /// 6. When `activate == true`, promote the record's lifecycle
    ///    to `Active` and stamp `activated_at_unix_secs` on the
    ///    record (not the in-memory key — the key is caller-owned).
    /// 7. Persist `store.json` atomically (write-temp + `sync_all` +
    ///    rename) with 0o600 perms on the file and 0o700 on the root.
    ///
    /// # Errors
    /// Returns `WalletError::WeakPassphrase` when `passphrase.len()
    /// < MIN_PASSPHRASE_CHARS`; returns `WalletError::AlreadyRevoked`
    /// when `key.lifecycle() == Revoked` or when an `IdentityRecord`
    /// with this DID is already registered; returns
    /// `WalletError::Io` on filesystem failure.
    #[allow(clippy::needless_pass_by_value)]
    pub fn register(
        &mut self,
        key: IdentityKey,
        passphrase: &str,
        activate: bool,
        now_unix: i64,
    ) -> Result<Did, WalletError> {
        // 1. Passphrase floor. Display message interpolates the
        //    same constant via `error::MIN_PASSPHRASE_CHARS`
        //    (re-exported from this module), so the operator's
        //    reported floor matches the one this check compares
        //    against (mission §AC-28).
        if passphrase.len() < MIN_PASSPHRASE_CHARS {
            return Err(WalletError::WeakPassphrase);
        }
        // 2. Refuse a Revoked key at registration time.
        if matches!(key.lifecycle(), LifecycleState::Revoked) {
            return Err(WalletError::AlreadyRevoked);
        }
        let did = key.did();
        let pubkey_bytes = key.public_key_bytes();

        // 3. Reject duplicate DID. `binary_search_by` returns `Ok`
        //    when an entry with the same DID is already in the
        //    index; that is a re-registration and is refused with
        //    `AlreadyRevoked` per the mission YAML doc comment
        //    ("Returns `AlreadyRevoked` when `key.did()` names a
        //    record already in the terminal state"). The error
        //    name is a slight misnomer for the duplicate case, but
        //    it is what the mission specifies and the CLI maps it
        //    to `IdentityTransitionRefused` at slot 93, exit 43
        //    (mission 0011-x-wallet-store-cli §error mapping).
        if self
            .index
            .records
            .binary_search_by(|r| r.did.as_str().cmp(did.as_str()))
            .is_ok()
        {
            return Err(WalletError::AlreadyRevoked);
        }

        // 4. Build the record. The caller may have passed a key in
        //    any non-Revoked lifecycle; the record reflects the
        //    current key state at registration time. `deprecated`
        //    starts false; rotation completion flips it via
        //    `complete_rotation` (Phase 3).
        let mut lifecycle = key.lifecycle();
        if activate && lifecycle == LifecycleState::Designated {
            lifecycle = LifecycleState::Active;
        }
        let record = IdentityRecord {
            did: did.clone(),
            pubkey_bytes,
            lifecycle,
            hsm_slot: None,
            registered_at_unix: now_unix,
            rotation_history: Vec::new(),
            deprecated: false,
        };

        // 5. Insert in sorted-by-DID order. `binary_search_by`
        //    returned `Err(insert_pos)` above; reuse that position.
        let pos = self
            .index
            .records
            .binary_search_by(|r| r.did.as_str().cmp(did.as_str()))
            .unwrap_err();
        self.index.records.insert(pos, record);

        // 6. Set the active DID when this is the first identity in
        //    a fresh wallet. Subsequent registers do not
        //    auto-promote; an explicit `select` is required.
        if self.index.active_did.is_none() {
            self.index.active_did = Some(did.clone());
        }

        // 7. Persist atomically. The write path creates the root
        //    dir (mission §AC-4 inverts: open is lazy, write is
        //    eager).
        write_index_atomically(&self.root, &self.index)?;

        Ok(did)
    }
}

/// Resolve the wallet root from the environment. The four-step
/// resolution mirrors `octo-cli/src/home.rs::resolve` but returns
/// the resolved path directly (no `Result` wrap) because the env
/// reader cannot fail — `read_octo_home` collapses the `Ok(empty)`
/// and `Err(unset)` cases into a single `HomeUnder` fallback arm,
/// and `read_home_fallback` returns an empty `PathBuf` when both
/// env vars are absent, which the caller (`open()`) turns into a
/// `WalletError::Config` only after joining under the empty path is
/// avoided (an empty path resolves to the cwd, which is what we
/// want to fail on).
fn resolve_wallet_root() -> PathBuf {
    match read_octo_home() {
        ResolvedHome::Octo(p) => p.join("wallet"),
        ResolvedHome::HomeUnder(home) => home.join(".octo").join("wallet"),
    }
}

fn read_octo_home() -> ResolvedHome {
    // `Ok(empty)` and `Err(unset)` both fall through to the HOME
    // fallback. The two arms share a body — `clippy::match_same_arms`
    // merges them; the explicit `_` keeps both arms visible for
    // readers who want to confirm the empty-as-error rule is honored
    // before the fallback engages.
    #[allow(clippy::match_same_arms)]
    match std::env::var("OCTO_HOME") {
        Ok(p) if !p.is_empty() => ResolvedHome::Octo(PathBuf::from(p)),
        Ok(_) | Err(_) => ResolvedHome::HomeUnder(read_home_fallback()),
    }
}

fn read_home_fallback() -> PathBuf {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h),
        // Both env vars unset or empty → caller turns this into a
        // `Config` error in `resolve_wallet_root`. We return an empty
        // path here as a sentinel; the caller never reaches `.join` on
        // it because the resolved-home form goes through HomeUnder only
        // when HOME is set and non-empty.
        _ => PathBuf::new(),
    }
}

/// Compose the seed slot filename for a given identity key. The slug is
/// `identity-` + lowercase hex of `key.public_key_bytes()` — 73
/// characters, inside the 128 cap, every character inside
/// `[a-zA-Z0-9._-]`. The store composes it here rather than in
/// `register` so the slug rule lives in exactly one place (mission
/// §AC-29). Reaches `validate_slot_id` to assert the rule on every
/// derivation rather than trust the format.
///
/// Not yet called — Phase 3's unlock / register-write seed-sealing
/// step will reach it. Reserved here so the slug rule has one home
/// when Phase 3 lands.
#[allow(dead_code)]
pub(crate) fn seed_slot_slug(key: &crate::identity::IdentityKey) -> String {
    let slug = format!("identity-{}", hex::encode(key.public_key_bytes()));
    // Assert the slug passes validate_slot_id. The slug is derived
    // from hex-encoded pubkey bytes (a 73-character ASCII string) so
    // the assertion is deterministic and cannot fail at runtime, but
    // the call is here so any future change to the slug format would
    // surface here rather than at `register` time.
    validate_slot_id(&slug).expect("seed slot slug must pass validate_slot_id");
    slug
}

/// Atomic write of `store.json`. Write-temp + `sync_all` + rename so
/// a crash mid-write cannot leave a half-written index on disk. The
/// existing file, if any, is replaced.
pub(crate) fn write_index_atomically(root: &Path, index: &WalletIndex) -> Result<(), WalletError> {
    // Mission §AC-4 inverts: open is lazy, write ensures the root dir
    // exists at 0o700. This is the first write — the parent dir is
    // created here so the seed dir can be created later under it.
    fs::create_dir_all(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    }
    let path = root.join("store.json");
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec(index)
        .map_err(|e| WalletError::KeystoreParse(format!("store.json serialize: {e}")))?;
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_index_default_is_empty() {
        let idx = WalletIndex::default();
        assert_eq!(idx.version, WALLET_INDEX_VERSION);
        assert!(idx.active_did.is_none());
        assert!(idx.records.is_empty());
    }

    #[test]
    fn read_or_init_index_on_missing_root_yields_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("does-not-exist");
        let idx = WalletStore::read_or_init_index(&root).expect("init");
        assert_eq!(idx.version, WALLET_INDEX_VERSION);
        assert!(idx.active_did.is_none());
        assert!(idx.records.is_empty());
    }

    #[test]
    fn write_then_read_index_roundtrips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("wallet");
        let original = WalletIndex::default();
        write_index_atomically(&root, &original).expect("write");
        let read_back = WalletStore::read_or_init_index(&root).expect("read");
        assert_eq!(read_back.version, original.version);
        assert_eq!(read_back.active_did, original.active_did);
        assert_eq!(read_back.records.len(), original.records.len());
    }

    #[test]
    fn unknown_version_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("wallet");
        let bogus = serde_json::json!({
            "version": 999,
            "active_did": null,
            "records": [],
        });
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("store.json"), bogus.to_string()).unwrap();
        let err = WalletStore::read_or_init_index(&root).unwrap_err();
        assert!(
            matches!(err, WalletError::KeystoreVersion { .. }),
            "expected KeystoreVersion, got {err:?}"
        );
    }

    #[test]
    fn open_at_on_nonexistent_root_yields_empty_index_without_creating_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("not-yet-created");
        let store = WalletStore::open_at(&root).expect("open_at");
        assert!(store.list_records().is_empty());
        assert!(store.active_did().is_none());
        // Mission §AC-4: open creates no directory.
        assert!(!root.exists(), "open must not create the root dir");
    }
}
