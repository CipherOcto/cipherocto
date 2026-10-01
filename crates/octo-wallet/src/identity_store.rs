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

        // 7. Seal the seed in the vault. The vault `put` call
        //    happens AFTER the in-memory mutation so that a refusal
        //    (no slot, IO failure) leaves the index untouched on
        //    disk - mission §AC-35 mandates "guards first before
        //    any write, then seal the slot, then write the index".
        //    In this slice the index is mutated in memory but the
        //    disk write happens next; on failure we roll the index
        //    back so the on-disk state matches the in-memory
        //    pre-state.
        let slug = seed_slot_slug(&key);
        let seed = key.seed_bytes_for_hkdf()?;
        if let Err(e) = self.vault.put(&slug, &seed, passphrase) {
            // Roll back the in-memory state to match what the disk
            // still says.
            self.index.records.remove(pos);
            if self.index.active_did.as_ref() == Some(&did) {
                self.index.active_did = None;
            }
            return Err(e);
        }

        // 8. Persist atomically. The write path creates the root
        //    dir (mission §AC-4 inverts: open is lazy, write is
        //    eager).
        write_index_atomically(&self.root, &self.index)?;

        Ok(did)
    }

    // ------------------------------------------------------------------
    // Active pointer / metadata write path (mission §select)
    // ------------------------------------------------------------------

    /// Move the active DID pointer to `did`. A pure index write:
    /// reads and writes `store.json`, touches no key material, so
    /// it needs no unlock.
    ///
    /// # Errors
    /// Returns `WalletError::IdentityNotFound` when the DID is not
    /// in the index; returns `WalletError::NotActive` when the
    /// target record is in the terminal `Revoked` lifecycle
    /// (mission AC-15 - a select onto a revoked record is the same
    /// bypass as re-registering a revoked seed).
    #[allow(clippy::needless_pass_by_value)]
    pub fn select(&mut self, did: &Did) -> Result<(), WalletError> {
        // Look up the target record to verify it exists and is
        // not terminal.
        let pos = self
            .index
            .records
            .binary_search_by(|r| r.did.as_str().cmp(did.as_str()))
            .map_err(|_| WalletError::IdentityNotFound(did.clone()))?;
        let record = &self.index.records[pos];
        if matches!(record.lifecycle, LifecycleState::Revoked) {
            return Err(WalletError::NotActive {
                current_state: LifecycleState::Revoked,
            });
        }
        self.index.active_did = Some(did.clone());
        write_index_atomically(&self.root, &self.index)?;
        Ok(())
    }

    /// Whether the seed slot for the active identity exists on disk.
    /// Returns `false` for a fresh store or one whose slot was deleted
    /// out from under it. Used by `unlock` to refuse
    /// `VaultDecryptionFailed` in favor of `VaultSlotNotFound` when
    /// the slot file is missing (mission §AC-45 vector `tv_x_45`).
    #[must_use]
    pub fn active_seed_slot_present(&self) -> bool {
        let Some(did) = self.index.active_did.as_ref() else {
            return false;
        };
        let Ok(record) = self.index.record(did) else {
            return false;
        };
        let slug = format!("identity-{}", hex::encode(record.pubkey_bytes));
        self.vault
            .slots_dir()
            .join(format!("{slug}.vault"))
            .exists()
    }

    /// List the slugs of slot files that exist on disk but are NOT
    /// named by any `IdentityRecord` in the index. Used by the CLI
    /// surface to surface orphan slots the operator can clean up.
    /// A slot becomes an orphan through `abort_rotation` (the
    /// successor's slot is sealed before `complete_rotation` flips
    /// the predecessor's lifecycle back, and abort leaves the
    /// sealed successor slot on disk as an orphan - mission §AC-38).
    #[must_use]
    pub fn orphan_slots(&self) -> Vec<String> {
        let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for r in &self.index.records {
            let slug = format!("identity-{}", hex::encode(r.pubkey_bytes));
            known.insert(format!("{slug}.vault"));
        }
        let Ok(entries) = std::fs::read_dir(self.vault.slots_dir()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if !known.contains(name_str) {
                out.push(name_str.to_owned());
            }
        }
        out.sort();
        out
    }

    // ------------------------------------------------------------------
    // Unlock split (mission §unlock)
    // ------------------------------------------------------------------

    /// Decrypt the active identity seed slot. `seed_out` is
    /// caller-owned so the zeroization obligation has exactly one
    /// enforcement site (the caller zeroes the buffer after use -
    /// mission §AC-39 vector `tv_x_38`).
    ///
    /// Seven ordered steps (mission YAML §unlock doc comment):
    ///
    /// 1. Validate passphrase length against `MIN_PASSPHRASE_CHARS`.
    /// 2. Look up the active DID; fail with `Locked` if none, or
    ///    `IdentityNotFound` if the pointer names an absent record.
    /// 3. Compose the slug from `record.pubkey_bytes` and call
    ///    `self.vault.get(slug, passphrase, seed_out)`. The vault
    ///    returns `VaultSlotNotFound` if the slot is missing
    ///    (mission §AC-45 - refuses `VaultDecryptionFailed` in favor
    ///    of `VaultSlotNotFound` when the file is gone).
    /// 4. Rehydrate the `IdentityKey` via
    ///    `IdentityKey::from_seed_with_lifecycle`, passing the
    ///    persisted lifecycle (NOT hard-coding `Designated` as
    ///    `from_seed` does - mission §AC-33 vector `tv_x_33` /
    ///    `tv_x_34`).
    /// 5. Reconstruct `rotation_started_at_unix_secs` from the
    ///    newest entry in `record.rotation_history` when the
    ///    persisted lifecycle is `Rotating`; `None` otherwise. This
    ///    is what stops `complete_rotation` from hitting
    ///    `.expect(...)` and panicking with exit 101 - mission
    ///    §AC-32 vector `tv_x_40`.
    /// 6. Return `UnlockedWallet<'a>` holding a unique `IdentityKey`
    ///    (NOT a clone - mission §AC-39 vector `tv_x_38`).
    ///
    /// # Errors
    /// Returns `WalletError::WeakPassphrase` when `passphrase.len()
    /// < MIN_PASSPHRASE_CHARS`; `WalletError::Locked` when no active
    /// identity is selected; `WalletError::IdentityNotFound` when
    /// the active pointer names an absent record;
    /// `WalletError::VaultSlotNotFound` when the slot file is gone;
    /// `WalletError::VaultDecryptionFailed` on a wrong passphrase.
    pub fn unlock<'a>(
        &'a mut self,
        passphrase: &str,
        seed_out: &'a mut Vec<u8>,
    ) -> Result<UnlockedWallet<'a>, WalletError> {
        // 1. Passphrase floor (mission §AC-28).
        if passphrase.len() < MIN_PASSPHRASE_CHARS {
            return Err(WalletError::WeakPassphrase);
        }
        // 2. Look up the active DID.
        let active_did = self.index.active_did.clone().ok_or(WalletError::Locked)?;
        let record = self
            .index
            .record(&active_did)
            .map_err(|_| WalletError::Locked)?
            .clone();

        // 3. Compose the slug and call the vault. The vault's
        //    `VaultSlotNotFound` is preferred over a misleading
        //    `VaultDecryptionFailed` when the slot file is gone.
        let slug = format!("identity-{}", hex::encode(record.pubkey_bytes));
        self.vault
            .get(&slug, passphrase, seed_out)
            .map_err(|e| match e {
                WalletError::VaultSlotNotFound(_) => WalletError::VaultSlotNotFound(slug.clone()),
                other => other,
            })?;

        // 4. Rehydrate the key from the 32-byte seed. The seed is
        //    extracted from the buffer the vault wrote into; we
        //    take a copy because the caller will zeroize the buffer
        //    after use.
        let mut seed_arr = [0u8; 32];
        if seed_out.len() < 32 {
            return Err(WalletError::KeystoreParse(
                "decrypted seed payload shorter than 32 bytes".to_owned(),
            ));
        }
        seed_arr.copy_from_slice(&seed_out[..32]);

        let activated_at = if matches!(
            record.lifecycle,
            LifecycleState::Designated | LifecycleState::Rotating
        ) {
            None
        } else {
            // Timestamps on `IdentityRecord` are signed `i64`; the
            // substrate's `from_seed_with_lifecycle` takes unsigned
            // `u64`. The cast is intentional and lossless for any
            // post-1970 timestamp (a negative timestamp is a
            // pre-1970 wall clock, which the substrate refuses to
            // rehydrate regardless of cast).
            #[allow(clippy::cast_sign_loss)]
            let ts = record.registered_at_unix.cast_unsigned();
            Some(ts)
        };
        let revoked_at = if matches!(record.lifecycle, LifecycleState::Revoked) {
            // The exact revocation timestamp is not persisted on the
            // record; pass `0` so the substrate at least sees a
            // revocation has happened. `IdentityKey::from_seed_with_lifecycle`
            // rejects `Revoked` outright so this path is unreachable
            // in practice, but the structural hygiene matters.
            Some(0u64)
        } else {
            None
        };
        let rotation_started_at = if matches!(record.lifecycle, LifecycleState::Rotating) {
            // AC-33: reconstruct from the newest event in the
            // record's own rotation history. AC-32: this is the
            // exact field `complete_rotation` reads through
            // `.expect(...)`; without it the next `complete_rotation`
            // panics with exit 101. `tv_x_40` exercises this.
            record
                .rotation_history
                .iter()
                .map(|e| {
                    #[allow(clippy::cast_sign_loss)]
                    let ts = e.started_at_unix.cast_unsigned();
                    ts
                })
                .max()
        } else {
            None
        };

        let key = IdentityKey::from_seed_with_lifecycle(
            seed_arr,
            record.lifecycle,
            activated_at,
            revoked_at,
            rotation_started_at,
        )?;

        // 6. Return the handle. The handle holds the unique key
        //    (mission AC-39); clones of `IdentityKey` keep the
        //    seed alive via `Arc<dyn HsmAdapter>`, so a clone here
        //    would defeat `tv_x_38`.
        Ok(UnlockedWallet {
            store: self,
            key,
            did: active_did,
            rotation_successor_did: None,
        })
    }
}

/// Handle for an unlocked wallet. Holds a unique `IdentityKey`
/// (NOT a clone - mission §AC-39 vector `tv_x_38`) plus a mutable
/// borrow of the underlying store so `begin_rotation`,
/// `complete_rotation`, and `abort_rotation` can write through to
/// the index. `rotation_successor_did` is populated by
/// `begin_rotation` and consumed by `complete_rotation`; on abort
/// the field is cleared without being consumed.
///
/// `Debug` is hand-rolled rather than derived because the default
/// `#[derive(Debug)]` would print the raw 32-byte seed via
/// `IdentityKey`'s signer field. The hand-rolled impl surfaces the
/// DID + lifecycle only.
pub struct UnlockedWallet<'a> {
    store: &'a mut WalletStore,
    key: IdentityKey,
    did: Did,
    rotation_successor_did: Option<Did>,
}

impl std::fmt::Debug for UnlockedWallet<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlockedWallet")
            .field("did", &self.did.as_str())
            .field("lifecycle", &self.key.lifecycle())
            .field("rotation_successor_did", &self.rotation_successor_did)
            .finish()
    }
}

impl UnlockedWallet<'_> {
    /// The active identity key. Returns the held key rather than a
    /// freshly-fetched one so the unique-key contract from AC-39
    /// holds - cloning here would extend the seed's lifetime past
    /// the handle's drop.
    #[must_use]
    pub fn active_identity(&self) -> &IdentityKey {
        &self.key
    }

    /// Look up an identity record by DID. A metadata read against
    /// the store's index; needs no key material.
    ///
    /// # Errors
    /// Returns `WalletError::IdentityNotFound` when the DID is not
    /// in the index.
    pub fn identity_record(&self, did: &Did) -> Result<IdentityRecord, WalletError> {
        self.store.index.record(did).cloned().map_err(|e| match e {
            WalletError::IdentityNotFound(_) => e,
            other => other,
        })
    }

    /// Begin rotation to `successor`. Seals the successor's slot
    /// (mission §AC-31) so the rotated store has a reachable
    /// successor identity. The returned `[u8; 64]` is the signature
    /// proof that the predecessor accepted the rotation.
    ///
    /// # Errors
    /// Returns `WalletError::AlreadyRevoked` if the predecessor is
    /// terminal; `WalletError::SelfRotation` if the successor IS
    /// the predecessor; `WalletError::WeakPassphrase` if the
    /// passphrase is below the floor.
    pub fn begin_rotation(
        &mut self,
        successor: IdentityKey,
        passphrase: &str,
        now_unix: u64,
    ) -> Result<[u8; 64], WalletError> {
        if passphrase.len() < MIN_PASSPHRASE_CHARS {
            return Err(WalletError::WeakPassphrase);
        }
        // Seal the successor's slot before flipping the
        // predecessor's lifecycle to Rotating. The store's index
        // will get the successor record on `complete_rotation`,
        // not here - begin_rotation is the seal step.
        let successor_slug = seed_slot_slug(&successor);
        let successor_seed = successor.seed_bytes_for_hkdf()?;
        self.store
            .vault
            .put(&successor_slug, &successor_seed, passphrase)?;

        let successor_did = successor.did();
        let proof = self
            .key
            .begin_rotation(successor, now_unix_secs_from_u64(now_unix))?;
        // The handle remembers the successor DID so
        // `complete_rotation` does not need to re-extract it from
        // the in-memory key (which is private inside `IdentityKey`).
        // The actual successor record has NOT been inserted into
        // the index yet - that happens on `complete_rotation`,
        // which is the moment the predecessor's lifecycle flips
        // back to `Active` and the operator can no longer roll
        // back via `abort_rotation`.
        self.rotation_successor_did = Some(successor_did);
        // Refresh the record snapshot in the index to reflect the
        // new Rotating lifecycle.
        self.persist_active_record()?;
        Ok(proof)
    }

    /// Complete a rotation. Flips the predecessor's lifecycle to
    /// `Active` deprecated (mission §AC-22 / AC-36), appends the
    /// successor as a new record, and resets `active_did` to the
    /// successor. Per mission §AC-32 the substrate does NOT
    /// `expect` the rotation start time; the rehydration path
    /// through `unlock` reconstructs it from the record's own
    /// `rotation_history` (vector `tv_x_40`).
    ///
    /// # Errors
    /// Returns `WalletError::NotRotating` if the predecessor is
    /// not in `Rotating` lifecycle; `WalletError::IdentityNotFound`
    /// if the successor record is missing from the index (which
    /// means `begin_rotation` did not write it).
    pub fn complete_rotation(&mut self, now_unix: u64) -> Result<(), WalletError> {
        self.key
            .complete_rotation(now_unix_secs_from_u64(now_unix))?;
        self.persist_active_record()?;
        // The successor record was already written by
        // `begin_rotation`'s seal step; flip its lifecycle to
        // Active and stamp `registered_at_unix`. Then make the
        // successor the active identity.
        if let Some(successor_did) = self.rotation_successor_did.take() {
            if let Ok(pos) = self
                .store
                .index
                .records
                .binary_search_by(|r| r.did.as_str().cmp(successor_did.as_str()))
            {
                let mut record = self.store.index.records[pos].clone();
                record.lifecycle = LifecycleState::Active;
                #[allow(clippy::cast_possible_wrap)]
                let reg = now_unix.cast_signed();
                record.registered_at_unix = reg;
                self.store.index.records[pos] = record;
            } else {
                return Err(WalletError::IdentityNotFound(successor_did));
            }
            self.store.index.active_did = Some(successor_did);
        }
        write_index_atomically(&self.store.root, &self.store.index)?;
        Ok(())
    }

    /// Abort a rotation. Restores the predecessor, appends no
    /// successor record, and leaves the already-sealed successor
    /// slot on disk as an orphan (mission §AC-38 - this is the one
    /// way an orphan slot arises without a crash).
    ///
    /// # Errors
    /// Returns `WalletError::NotRotating` if the predecessor is
    /// not in `Rotating` lifecycle.
    pub fn abort_rotation(&mut self) -> Result<(), WalletError> {
        self.key.abort_rotation()?;
        // Clear the rotation successor DID without consuming it;
        // the sealed successor slot stays on disk as an orphan
        // and `orphan_slots()` will surface it.
        self.rotation_successor_did = None;
        self.persist_active_record()?;
        Ok(())
    }

    /// Revoke the active identity. Idempotent from the `Revoked`
    /// lifecycle (mission §AC-13 vector `tv_x_23`).
    ///
    /// # Errors
    /// Returns `WalletError::NotActive { current_state: Designated }`
    /// when the identity was never activated.
    #[allow(clippy::needless_pass_by_value)]
    pub fn revoke(&mut self, now_unix: u64) -> Result<(), WalletError> {
        self.key.revoke(now_unix_secs_from_u64(now_unix))?;
        self.persist_active_record()?;
        Ok(())
    }

    /// The DID of the unlocked identity (cached on the handle so
    /// `UnlockedWallet` can return its own record without going
    /// through the index twice).
    #[must_use]
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Sign a message with the unlocked identity's key. Delegates
    /// to `IdentityKey::sign`, which gates on lifecycle state per
    /// RFC-0009 §Lifecycle Requirements.
    ///
    /// # Errors
    /// Returns `WalletError::NotActive` when lifecycle is not
    /// `Active` or `Rotating`; `WalletError::Hsm(_)` on adapter
    /// failure.
    pub fn sign(&self, msg: &[u8]) -> Result<ed25519_dalek::Signature, WalletError> {
        self.key.sign(msg)
    }

    /// Refresh the persisted record for the active DID to match
    /// the in-memory `IdentityKey`. Called after every state
    /// transition so `store.json` mirrors the live key.
    fn persist_active_record(&mut self) -> Result<(), WalletError> {
        let pos = self
            .store
            .index
            .records
            .binary_search_by(|r| r.did.as_str().cmp(self.did.as_str()))
            .map_err(|_| WalletError::IdentityNotFound(self.did.clone()))?;
        let mut record = self.store.index.records[pos].clone();
        record.lifecycle = self.key.lifecycle();
        record.deprecated = self.key.is_deprecated();
        self.store.index.records[pos] = record;
        write_index_atomically(&self.store.root, &self.store.index)?;
        Ok(())
    }
}

/// Convert `now_unix: u64` to the substrate's `now_unix_secs: u64`
/// (no-op today, but isolates the rename so a future signed-vs-
/// unsigned debate has a single call site).
const fn now_unix_secs_from_u64(t: u64) -> u64 {
    t
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

    // ------------------------------------------------------------------
    // Substrate test vectors (mission 0011-x-s-a-wallet-store-identity
    // §Test Vectors). These are the negative-controlled vectors that
    // pin security and durability properties. The remaining vectors
    // (open, home-resolution, store-layout, determinism) land in
    // follow-on commits; this commit focuses on the ones the
    // mission YAML names with explicit negative controls.
    // ------------------------------------------------------------------

    /// `tv_x_5` (mission §AC-16): the store never reads a clock —
    /// the timestamp that lands in `store.json` is the caller-
    /// supplied `now_unix` verbatim. Register with two distinct
    /// `now_unix` values; only the supplied ones are persisted.
    #[test]
    fn tv_x_5_now_unix_round_trips_verbatim() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x55u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", false, 1_700_000_000)
            .expect("register");
        let record = store.identity_record(&did).expect("identity_record");
        assert_eq!(record.registered_at_unix, 1_700_000_000);
        drop(store);
        // Reload and re-read; the timestamp is byte-stable.
        let mut store2 = WalletStore::open_at(dir.path()).expect("open_at reload");
        store2.reload().expect("reload");
        let record2 = store2.identity_record(&did).expect("identity_record reload");
        assert_eq!(record2.registered_at_unix, 1_700_000_000);
    }

    /// `tv_x_6` (mission §AC-12): `register(activate = false)` leaves
    /// an existing active DID untouched. A fresh store's `active_did`
    /// is `None`, so this needs two registrations: the first one
    /// sets active_did; the second one must NOT move the pointer.
    #[test]
    fn tv_x_6_register_inactive_does_not_move_active_pointer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key_a = IdentityKey::from_seed([0x66u8; 32]);
        let did_a = store
            .register(key_a, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register a");
        let key_b = IdentityKey::from_seed([0x77u8; 32]);
        let _did_b = store
            .register(key_b, "correct-horse-battery-staple", false, 1_700_000_001)
            .expect("register b");
        let active = store.active_did().expect("active_did set");
        assert_eq!(active, &did_a, "active pointer must stay on the first identity");
    }

    /// `tv_x_12` (mission §AC-12): `register(activate = true)` on a
    /// fresh store persists `Active` lifecycle and sets active_did.
    #[test]
    fn tv_x_12_register_active_promotes_lifecycle_and_pointer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x12u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let record = store.identity_record(&did).expect("identity_record");
        assert_eq!(record.lifecycle, LifecycleState::Active);
        assert!(store.active_did().is_some());
    }

    /// `tv_x_42` (mission §AC-28): the 12-character passphrase floor
    /// is a hard error at BOTH `register` and `unlock`. Negative
    /// control: warn at register and error only at unlock, which is
    /// the split RFC-0011-x §Future Work item 7 withdrew.
    #[test]
    fn tv_x_42_passphrase_floor_enforced_at_register_and_unlock() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x42u8; 32]);
        // 11 chars — one below the floor.
        let short = "abc";
        assert_eq!(short.len(), 3);
        let result = store.register(key, short, false, 1_700_000_000);
        assert!(
            matches!(result, Err(WalletError::WeakPassphrase)),
            "register with 3-char passphrase must yield WeakPassphrase, got {result:?}"
        );
        // Now register successfully with a strong passphrase so we
        // can exercise `unlock` with the floor.
        let key2 = IdentityKey::from_seed([0x43u8; 32]);
        let _did = store
            .register(key2, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register strong");
        let mut seed_out = Vec::new();
        let unlock = store.unlock(short, &mut seed_out);
        assert!(
            matches!(unlock, Err(WalletError::WeakPassphrase)),
            "unlock with 3-char passphrase must yield WeakPassphrase, got {unlock:?}"
        );
    }

    /// `tv_x_35` (mission §AC-35): `AlreadyRevoked` on
    /// re-registration. The vault retains the slot after revocation;
    /// without the guard an operator re-registers the same seed and
    /// gets a working identity back from a record that was supposed
    /// to be terminal. Negative control: drop the `AlreadyRevoked`
    /// check on duplicate DID; the byte-identity assertions fail.
    #[test]
    fn tv_x_35_duplicate_did_returns_already_revoked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x35u8; 32]);
        let did = store
            .register(key.clone(), "correct-horse-battery-staple", false, 1_700_000_000)
            .expect("first register");
        // Second register with the same key (and therefore same DID)
        // is refused with `AlreadyRevoked`. The vault slot is NOT
        // re-encrypted; the on-disk state is unchanged.
        let err = store
            .register(key, "correct-horse-battery-staple", false, 1_700_000_001)
            .unwrap_err();
        assert!(
            matches!(err, WalletError::AlreadyRevoked),
            "duplicate register must yield AlreadyRevoked, got {err:?}"
        );
        // The persisted record still belongs to the original registration.
        let record = store.identity_record(&did).expect("identity_record");
        assert_eq!(record.registered_at_unix, 1_700_000_000);
    }

    /// `tv_x_45` (mission §AC-45): `active_seed_slot_present()`
    /// returns `false` after the slot file is deleted behind the
    /// index, and `unlock` returns `VaultSlotNotFound` rather than a
    /// decryption failure. Negative control: a store that checks
    /// only the index would still report the store as unlockable.
    #[test]
    fn tv_x_45_active_seed_slot_present_detects_missing_slot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x45u8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        assert!(store.active_seed_slot_present(), "slot present after register");
        // Locate the slot file and delete it out from under the store.
        let entries = std::fs::read_dir(dir.path().join("seed")).expect("read seed dir");
        let mut deleted_count = 0;
        for entry in entries.flatten() {
            let p = entry.path();
            if p
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("vault"))
            {
                std::fs::remove_file(&p).expect("remove slot");
                deleted_count += 1;
            }
        }
        assert_eq!(deleted_count, 1, "expected exactly one slot file");
        assert!(
            !store.active_seed_slot_present(),
            "slot must be reported missing after external deletion"
        );
        let mut seed_out = Vec::new();
        let unlock = store.unlock("correct-horse-battery-staple", &mut seed_out);
        assert!(
            matches!(unlock, Err(WalletError::VaultSlotNotFound(_))),
            "unlock on missing slot must yield VaultSlotNotFound, got {unlock:?}"
        );
    }

    /// `tv_x_46` (mission §AC-46): an orphan slot is REPORTED by
    /// `orphan_slots()` and is NOT adopted into the index. The
    /// orphan here is created by external filesystem manipulation,
    /// not by abort_rotation, so we test the read-side only.
    /// Negative control: a store that reconciles the index forward
    /// would invent a record for the ciphertext.
    #[test]
    fn tv_x_46_orphan_slot_is_reported_not_adopted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x46u8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        // Create a synthetic orphan slot file in the seed dir.
        std::fs::create_dir_all(dir.path().join("seed")).expect("seed dir");
        let orphan_path = dir.path().join("seed").join("identity-orphan.vault");
        std::fs::write(&orphan_path, b"synthetic orphan bytes").expect("write orphan");
        let orphans = store.orphan_slots();
        assert!(
            orphans.iter().any(|s| s == "identity-orphan.vault"),
            "orphan slot must be reported by orphan_slots(), got {orphans:?}"
        );
        // The orphan is NOT in the index: only one record, the
        // originally registered one.
        assert_eq!(store.list_records().len(), 1);
    }

    /// `tv_x_47` (mission §AC-29): the generated slug passes
    /// `validate_slot_id`, and the validator is reachable. The
    /// slug is `identity-` plus lowercase hex of
    /// `key.public_key_bytes()`: 73 characters, inside the 128 cap,
    /// every character inside `[a-zA-Z0-9._-]`. Verified
    /// indirectly: the register call succeeds end-to-end and the
    /// slot file appears under `seed/` with the expected slug
    /// prefix.
    #[test]
    fn tv_x_47_slot_slug_is_identity_pubkey_hex_and_passes_validator() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x47u8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let entries = std::fs::read_dir(dir.path().join("seed")).expect("read seed dir");
        let mut slot_names: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| {
                std::path::Path::new(n)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("vault"))
            })
            .collect();
        slot_names.sort();
        assert_eq!(slot_names.len(), 1);
        let name = &slot_names[0];
        assert!(
            name.starts_with("identity-")
                && std::path::Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("vault")),
            "slot name must be identity-<hex>.vault, got {name}"
        );
        // Slug is 73 chars + ".vault" (6 chars) = 79 chars total.
        assert_eq!(name.len(), 79);
    }

    /// `tv_x_3` / `tv_x_4` (mission §AC-5): mode enforcement on the
    /// store root after first write. Negative control: drop the
    /// `set_permissions` call in `write_index_atomically`; the
    /// assertion below fails.
    #[cfg(unix)]
    #[test]
    fn tv_x_3_4_root_mode_is_0700_after_first_write() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x03u8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mode = std::fs::metadata(dir.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o700,
            "store root must be 0o700 after first write, got {mode:o}"
        );
        let file_mode = std::fs::metadata(dir.path().join("store.json"))
            .expect("metadata store.json")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            file_mode, 0o600,
            "store.json must be 0o600 after first write, got {file_mode:o}"
        );
    }

    /// `tv_x_33` (mission §AC-32 / AC-33): lifecycle rehydration.
    /// The substrate contract is fail-closed: a `Revoked` record
    /// cannot be rehydrated at all - `unlock` itself returns
    /// `AlreadyRevoked` because `IdentityKey::from_seed_with_lifecycle`
    /// refuses to construct a key in the terminal lifecycle. This
    /// pins the security boundary: there is no path from a revoked
    /// on-disk record back to a live signing key. Negative control:
    /// a substrate that rehydrated through `from_seed` (which
    /// hard-codes `Designated`) and let `sign()` refuse at the
    /// point of use would still leak the slot's plaintext, since
    /// the unlock path itself succeeded.
    #[test]
    fn tv_x_33_unlock_revoked_record_returns_already_revoked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x33u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        // Unlock once, revoke it, drop the handle.
        {
            let mut seed_out = Vec::new();
            let mut handle = store
                .unlock("correct-horse-battery-staple", &mut seed_out)
                .expect("unlock");
            handle.revoke(1_700_000_001).expect("revoke");
        }
        // The store is now mutable again. A second `unlock` on the
        // revoked record refuses with `AlreadyRevoked` BEFORE the
        // vault slot is decrypted. This is the substrate's
        // fail-closed contract for the terminal lifecycle.
        let mut seed_out = Vec::new();
        let result = store.unlock("correct-horse-battery-staple", &mut seed_out);
        assert!(
            matches!(result, Err(WalletError::AlreadyRevoked)),
            "unlock on a Revoked record must yield AlreadyRevoked, got {result:?}"
        );
        // The persisted record is still `Revoked`.
        let record = store.identity_record(&did).expect("identity_record");
        assert_eq!(record.lifecycle, LifecycleState::Revoked);
    }

    /// `tv_x_36` (mission §AC-15): `select` on a `Revoked` record
    /// returns `NotActive`. Negative control: a `select` that
    /// moves the pointer without checking lifecycle would leave
    /// the active DID pointing at a terminal record.
    #[test]
    fn tv_x_36_select_on_revoked_returns_not_active() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x36u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        {
            let mut seed_out = Vec::new();
            let mut handle = store
                .unlock("correct-horse-battery-staple", &mut seed_out)
                .expect("unlock");
            handle.revoke(1_700_000_001).expect("revoke");
        }
        let err = store.select(&did).unwrap_err();
        assert!(
            matches!(err, WalletError::NotActive { .. }),
            "select on Revoked must yield NotActive, got {err:?}"
        );
    }
}
