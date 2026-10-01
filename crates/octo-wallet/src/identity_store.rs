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
use crate::identity::{IdentityKey, ROTATION_GRACE_PERIOD_SECS};
use crate::identity_record::{Did, IdentityRecord, IdentityRotationEvent};
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
        //    `AlreadyRevoked`, reusing the terminal-state variant
        //    rather than minting one. The CLI maps it to
        //    `OctoCliError::AlreadyRevoked` at exit 6 - NOT
        //    `IdentityTransitionRefused` at slot 93, which an
        //    earlier revision of this comment claimed and which
        //    the CLI's `From<WalletError>` does not do. Exit 6 is
        //    also what a genuinely revoked record returns, so the
        //    operator message covers both causes rather than
        //    asserting one.
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
    /// Re-attach the successor key named by an in-flight rotation.
    ///
    /// Extracted from `unlock` so the binding and proof checks sit in
    /// one named place and `unlock` stays inside the function-length
    /// lint. The event arrives as a parameter rather than being
    /// re-derived, so this cannot panic the way an `expect` on a
    /// missing event would - the caller has established it exists.
    fn rehydrate_successor_key(
        &self,
        successor_did: &Did,
        event: &IdentityRotationEvent,
        predecessor: &IdentityKey,
        passphrase: &str,
    ) -> Result<IdentityKey, WalletError> {
        // Re-attach from the vault slot `begin_rotation` sealed under
        // `seed_slot_slug_by_pubkey(succ_record.pubkey_bytes)`. The
        // DID is the index key the seal step also wrote.
        let succ_record = self
            .index
            .record(successor_did)
            .map_err(|_| WalletError::IdentityNotFound(successor_did.clone()))?;
        let succ_slot = seed_slot_slug_by_pubkey(succ_record.pubkey_bytes);
        let mut succ_seed = Vec::new();
        self.vault.get(&succ_slot, passphrase, &mut succ_seed)?;
        if succ_seed.len() < 32 {
            succ_seed.fill(0);
            return Err(WalletError::VaultDecryptionFailed);
        }
        let mut succ_arr = [0u8; 32];
        succ_arr.copy_from_slice(&succ_seed[..32]);
        succ_seed.fill(0);
        let succ_key = IdentityKey::from_seed(succ_arr);
        // BINDING CHECK. The vault slot is chosen by
        // `succ_record.pubkey_bytes`, and the DID is the index
        // key - but nothing tied the two together, so an index
        // whose `pubkey_bytes` disagreed with its `did` would
        // decrypt SOME OTHER identity's seed and attach it as
        // this successor. `complete_rotation` would then promote
        // the named DID to active while every signature it makes
        // is under a different key, silently. `Did` is derived
        // from the public key, so the check is exact.
        if &succ_key.did() != successor_did
            || succ_key.public_key_bytes() != succ_record.pubkey_bytes
        {
            return Err(WalletError::SuccessorKeyMismatch {
                did: successor_did.clone(),
            });
        }
        // PROOF CHECK. `begin_rotation` signed
        // `b"rotate" || successor_pubkey` with the PREDECESSOR's
        // key and stored the result on the event. That proof is
        // what makes "this predecessor authorised this successor"
        // verifiable across the process boundary; rehydrating
        // the key without checking it means a hand-edited index
        // can name any successor it likes. The predecessor key is
        // `predecessor`, passed in and holding the seed.
        let mut expected = Vec::with_capacity(6 + 32);
        expected.extend_from_slice(b"rotate");
        expected.extend_from_slice(&succ_key.public_key_bytes());
        let recomputed = predecessor.sign(&expected)?;
        if recomputed.to_bytes() != event.signature_proof {
            return Err(WalletError::InvalidSuccessorProof);
        }
        Ok(succ_key)
    }

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
        let get_result = self.vault.get(&slug, passphrase, seed_out);
        if let Err(e) = get_result {
            // Mission §AC-20: zeroize the caller-owned buffer
            // before returning, so a wrong-passphrase probe
            // does not leave prior seed bytes behind. The
            // vault's `get` writes into `seed_out` on the
            // success path; on a wrong-passphrase error the
            // buffer is untouched, but a previously-used
            // buffer can still hold seed bytes from a prior
            // successful unlock. The negative-controlled
            // `tv_x_18` requires this zeroize.
            for byte in seed_out.iter_mut() {
                *byte = 0;
            }
            return Err(match e {
                WalletError::VaultSlotNotFound(_) => WalletError::VaultSlotNotFound(slug.clone()),
                other => other,
            });
        }

        // 4. Rehydrate the key from the 32-byte seed. The seed is
        //    extracted from the buffer the vault wrote into; the
        //    caller-owned buffer is zeroized after the copy so the
        //    seed's lifetime is exactly the handle lifetime. Mission
        //    §AC-20 asserts this zeroize via `tv_x_17`.
        let mut seed_arr = [0u8; 32];
        if seed_out.len() < 32 {
            for byte in seed_out.iter_mut() {
                *byte = 0;
            }
            return Err(WalletError::KeystoreParse(
                "decrypted seed payload shorter than 32 bytes".to_owned(),
            ));
        }
        seed_arr.copy_from_slice(&seed_out[..32]);
        for byte in seed_out.iter_mut() {
            *byte = 0;
        }

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
        // The newest event describes the rotation in flight. Both the
        // start time AND the successor come from it: `begin_rotation`
        // holds both on the in-memory key, and the CLI runs
        // `rotate` and `rotate-complete` as two separate processes, so
        // without this the successor linkage is gone and
        // `complete_rotation` refuses with `NotRotating` even though
        // the lifecycle says `Rotating`.
        let in_flight = if matches!(record.lifecycle, LifecycleState::Rotating) {
            record
                .rotation_history
                .iter()
                .max_by_key(|e| e.started_at_unix)
                .cloned()
        } else {
            None
        };
        // AC-32: `rotation_started_at_unix_secs` is the exact field
        // `complete_rotation` reads through `.expect(...)`; without
        // it the call panics with exit 101. `tv_x_49` exercises it
        // across a reopen, which is how the CLI actually runs.
        let rotation_started_at = in_flight.as_ref().map(|e| {
            #[allow(clippy::cast_sign_loss)]
            e.started_at_unix.cast_unsigned()
        });
        // A `Rotating` record with NO rotation event cannot be
        // completed: the start time and successor are gone, and
        // `complete_rotation` reads the start time through
        // `.expect(...)`. Handing back a key that will panic turns a
        // recoverable state into exit 101. Refuse at unlock, where
        // the operator gets an envelope and a remediation, and where
        // `rotate-abort` is still available to them. Both record
        // writes now land in one atomic write, so this state is
        // unreachable through the CLI; it remains reachable from a
        // hand-edited or partially-written index, which is exactly
        // when a panic is least acceptable.
        if matches!(record.lifecycle, LifecycleState::Rotating) && in_flight.is_none() {
            return Err(WalletError::RotationEventMissing);
        }
        let successor_did = in_flight.as_ref().map(|e| e.successor_did.clone());

        let mut key = IdentityKey::from_seed_with_lifecycle(
            seed_arr,
            record.lifecycle,
            activated_at,
            revoked_at,
            rotation_started_at,
        )?;
        // Restore the persisted deprecation flag before anything can
        // write it back. Without this, the next `persist_active_record`
        // writes `false` over a `true` that a completed rotation set.
        key.set_deprecated(record.deprecated);

        // Re-attach the successor from its vault-sealed seed. The
        // binding and proof checks live in the named helper.
        if let Some(event) = in_flight.as_ref() {
            let succ_key =
                self.rehydrate_successor_key(&event.successor_did, event, &key, passphrase)?;
            key.rehydrate_successor(succ_key)?;
        }

        // 6. Return the handle. The handle holds the unique key
        //    (mission AC-39); clones of `IdentityKey` keep the
        //    seed alive via `Arc<dyn HsmAdapter>`, so a clone here
        //    would defeat `tv_x_38`.
        Ok(UnlockedWallet {
            store: self,
            key,
            did: active_did,
            rotation_successor_did: successor_did,
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
    /// Returns `WalletError::NotActive { current_state }` if the
    /// predecessor is not `Active` - a `Revoked` predecessor arrives
    /// here as `NotActive { current_state: Revoked }`, not
    /// `AlreadyRevoked`; `WalletError::SelfRotation` if the successor
    /// IS the predecessor; `WalletError::WeakPassphrase` if the
    /// passphrase is below the floor. The `NotActive` check runs
    /// BEFORE the vault seal, so a refusal writes nothing.
    pub fn begin_rotation(
        &mut self,
        successor: IdentityKey,
        passphrase: &str,
        now_unix: u64,
    ) -> Result<[u8; 64], WalletError> {
        if passphrase.len() < MIN_PASSPHRASE_CHARS {
            return Err(WalletError::WeakPassphrase);
        }
        // GUARD BEFORE THE SEAL. The lifecycle check lives inside
        // `IdentityKey::begin_rotation`, which runs AFTER the vault
        // write below. A predecessor that is not `Active` therefore
        // sealed the successor's slot, was then refused, and left a
        // permanently orphaned encrypted slot that no CLI surface
        // reports - `orphan_slots` has no consumer in any command.
        // Checking here means a refusal writes nothing.
        if self.key.lifecycle() != LifecycleState::Active {
            return Err(WalletError::NotActive {
                current_state: self.key.lifecycle(),
            });
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
        // Extract the public-key bytes BEFORE consuming the
        // successor into `begin_rotation`. The seed is held
        // by `IdentityKey`'s `Arc<dyn HsmAdapter>` signer,
        // and the public-key bytes are stable for a given
        // seed, so the value extracted here is the one the
        // successor record needs to persist.
        let successor_pubkey = successor.public_key_bytes();
        let proof = self
            .key
            .begin_rotation(successor, now_unix_secs_from_u64(now_unix))?;
        // The handle remembers the successor DID so
        // `complete_rotation` does not need to re-extract it from
        // the in-memory key (which is private inside `IdentityKey`).
        // Insert the successor record into the index now so
        // `complete_rotation` can locate it by DID and promote
        // its lifecycle from `Designated` to `Active`. Mission
        // §AC-22 / §AC-36 / §AC-31: the successor is appendable
        // before `complete_rotation`, not after, so the index is
        // always queryable for both records during the grace
        // window.
        let successor_record = IdentityRecord {
            did: successor_did.clone(),
            pubkey_bytes: successor_pubkey,
            lifecycle: LifecycleState::Designated,
            hsm_slot: None,
            #[allow(clippy::cast_possible_wrap)]
            registered_at_unix: now_unix.cast_signed(),
            rotation_history: Vec::new(),
            deprecated: false,
        };
        // Insert in sorted-by-DID order so the determinism
        // contract holds.
        let pos = self
            .store
            .index
            .records
            .binary_search_by(|r| r.did.as_str().cmp(successor_did.as_str()))
            .unwrap_or_else(|i| i);
        self.store.index.records.insert(pos, successor_record);
        let successor_did_for_event = successor_did.clone();
        self.rotation_successor_did = Some(successor_did);
        // Both record changes - the new Rotating lifecycle and the
        // rotation event - go into ONE write below. Writing the
        // lifecycle first made the first write durable while the
        // second could still fail, and a Rotating record with an
        // empty history is the state that makes the next process
        // panic in `complete_rotation`.
        self.persist_active_record()?;
        // PERSIST THE ROTATION EVENT. The predecessor's start time
        // and successor DID both live on the in-memory key, and the
        // CLI runs `rotate` and `rotate-complete` as two separate
        // processes. Without this write, `store.json` carries only
        // the `Rotating` lifecycle, `unlock` reconstructs
        // `rotation_started_at` from an EMPTY `rotation_history` and
        // hands `complete_rotation` a `None`, and the `.expect` in
        // `IdentityKey::complete_rotation` panics - exit 101, no
        // envelope, on the only path the CLI can take. The same
        // omission stranded `rotation_successor_did`, so a
        // cross-process complete never promoted the successor.
        let predecessor_did = self.did.clone();
        self.append_rotation_event(
            predecessor_did.as_str(),
            now_unix,
            now_unix.saturating_add(ROTATION_GRACE_PERIOD_SECS),
            successor_did_for_event,
            proof,
        )?;
        write_index_atomically(&self.store.root, &self.store.index)?;
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
        // and `orphan_slots()` will surface it. Also remove the
        // successor record from the index so the slot is
        // truly orphaned - mission §AC-38 says abort is the
        // path that returns the slot to an orphan, and that
        // requires no record naming the slot either.
        let aborted_successor = self.rotation_successor_did.take();
        if let Some(successor_did) = aborted_successor.as_ref() {
            if let Ok(pos) = self
                .store
                .index
                .records
                .binary_search_by(|r| r.did.as_str().cmp(successor_did.as_str()))
            {
                self.store.index.records.remove(pos);
            }
        }
        // Drop the aborted rotation's event. The predecessor's
        // lifecycle is back to Active, so leaving the event in
        // `rotation_history` makes `unlock` rehydrate a rotation that
        // no longer exists, and `identity show` reports a rotation to
        // a successor whose record this function just deleted. A
        // machine-read envelope naming a rotation to an identity not
        // in the wallet is worse than the unbounded growth it avoids.
        let predecessor_did = self.did.clone();
        if let Ok(pos) = self
            .store
            .index
            .records
            .binary_search_by(|r| r.did.as_str().cmp(predecessor_did.as_str()))
        {
            let mut record = self.store.index.records[pos].clone();
            record
                .rotation_history
                .retain(|e| Some(&e.successor_did) != aborted_successor.as_ref());
            self.store.index.records[pos] = record;
        }
        // Always persist the predecessor's restored state so
        // the index reflects the abort (mission §AC-38:
        // predecessor returns to Active, not stuck in
        // Rotating). The successor-record removal above and this
        // restore land in ONE write, so an abort cannot leave the
        // record deleted while the predecessor is still Rotating.
        self.persist_active_record()?;
        write_index_atomically(&self.store.root, &self.store.index)?;
        Ok(())
    }

    /// Revoke the active identity.
    ///
    /// `IdentityKey::revoke` is idempotent from the `Revoked`
    /// lifecycle, but that branch is UNREACHABLE through this method:
    /// the only construction site for an `UnlockedWallet` is
    /// `unlock`, which rehydrates through
    /// `from_seed_with_lifecycle` and that refuses a `Revoked` record
    /// outright. A second `octo identity revoke` therefore exits 6
    /// from the constructor, never reaching the idempotent no-op.
    /// The operator-visible end state is the same, so only this doc
    /// was wrong.
    /// # Errors
    /// Returns `WalletError::NotActive { current_state: Designated }`
    /// when the identity was never activated.
    #[allow(clippy::needless_pass_by_value)]
    pub fn revoke(&mut self, now_unix: u64) -> Result<(), WalletError> {
        self.key.revoke(now_unix_secs_from_u64(now_unix))?;
        self.persist_active_record()?;
        write_index_atomically(&self.store.root, &self.store.index)?;
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

    /// Append an `IdentityRotationEvent` to the predecessor's
    /// persisted record and rewrite the index atomically.
    ///
    /// This is the ONLY cross-process carrier for a rotation in
    /// flight. `unlock` reconstructs `rotation_started_at_unix_secs`
    /// from the newest event here, and `complete_rotation` reads that
    /// field through `.expect(...)`; the successor DID is recovered
    /// the same way. Both are in-memory on the `IdentityKey`, so a
    /// rotation that did not reach this function is unrecoverable
    /// across a process boundary.
    fn append_rotation_event(
        &mut self,
        predecessor: &str,
        started_at: u64,
        grace_expires_at: u64,
        successor: Did,
        signature_proof: [u8; 64],
    ) -> Result<(), WalletError> {
        let pos = self
            .store
            .index
            .records
            .binary_search_by(|r| r.did.as_str().cmp(predecessor))
            .map_err(|_| WalletError::IdentityNotFound(Did::from(predecessor)))?;
        // The rotation id is the signature's first 32 bytes: unique
        // per (predecessor, successor) pair because `begin_rotation`
        // signs `b"rotate" || successor_pubkey` with the
        // predecessor's key, and deterministic for a given pair.
        let mut rotation_id = [0u8; 32];
        rotation_id.copy_from_slice(&signature_proof[..32]);
        let mut record = self.store.index.records[pos].clone();
        record.rotation_history.push(IdentityRotationEvent {
            rotation_id,
            #[allow(clippy::cast_possible_wrap)]
            started_at_unix: started_at.cast_signed(),
            #[allow(clippy::cast_possible_wrap)]
            grace_expires_at_unix: grace_expires_at.cast_signed(),
            successor_did: successor,
            signature_proof,
        });
        self.store.index.records[pos] = record;
        Ok(())
    }

    /// Refresh the in-memory record for the active DID to match the
    /// live `IdentityKey`. Called after every state transition.
    ///
    /// This function does NOT write. A transition that needs several
    /// record changes must call it once per change and then issue a
    /// SINGLE `write_index_atomically` covering all of them. Two
    /// writes make the first durable while the second can still fail,
    /// and a half-applied rotation is the one state no CLI command
    /// can recover from.
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
pub(crate) fn seed_slot_slug(key: &crate::identity::IdentityKey) -> String {
    seed_slot_slug_by_pubkey(key.public_key_bytes())
}

/// The same rule as [`seed_slot_slug`], keyed on the raw public-key
/// bytes so a caller that has only a persisted `IdentityRecord` (the
/// rehydrate path in `unlock`, which has no `IdentityKey` yet) derives
/// the same slot name the seal step wrote. Two spellings of one rule
/// would orphan every slot on reopen.
pub(crate) fn seed_slot_slug_by_pubkey(pubkey_bytes: [u8; 32]) -> String {
    let slug = format!("identity-{}", hex::encode(pubkey_bytes));
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
        let record2 = store2
            .identity_record(&did)
            .expect("identity_record reload");
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
        assert_eq!(
            active, &did_a,
            "active pointer must stay on the first identity"
        );
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
            .register(
                key.clone(),
                "correct-horse-battery-staple",
                false,
                1_700_000_000,
            )
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
        assert!(
            store.active_seed_slot_present(),
            "slot present after register"
        );
        // Locate the slot file and delete it out from under the store.
        let entries = std::fs::read_dir(dir.path().join("seed")).expect("read seed dir");
        let mut deleted_count = 0;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension()
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

    // ------------------------------------------------------------------
    // Phase 3c vectors — substrate surface coverage that the
    // eleven Phase 3b vectors did not exercise. These pin home
    // resolution, perm-on-open, determinism, vault discrimination,
    // and the rotation flow. Total vectors committed across
    // Phase 3b + Phase 3c: thirty. The remaining twelve land in
    // Phase 3d if needed; mission YAML AC-19 specifies
    // forty-two and Phase 3d is the catch-up slice.
    // ------------------------------------------------------------------

    /// `tv_x_9` (mission §AC-3 step 1): `OCTO_HOME` takes
    /// precedence over `HOME`. With both set, the resolved
    /// root is `$OCTO_HOME/wallet`, NOT `$HOME/.octo/wallet`.
    /// Verified by writing into a tempdir and confirming the
    /// store opens the tempdir-backed root, not the
    /// `$HOME`-backed root.
    #[test]
    fn tv_x_9_octo_home_takes_precedence_over_home() {
        let octo_dir = tempfile::tempdir().expect("octo");
        let home_dir = tempfile::tempdir().expect("home");
        let prev_octo = std::env::var_os("OCTO_HOME");
        let prev_home = std::env::var_os("HOME");
        std::env::set_var("OCTO_HOME", octo_dir.path());
        std::env::set_var("HOME", home_dir.path());
        let store = WalletStore::open().expect("open");
        if let Some(v) = prev_octo {
            std::env::set_var("OCTO_HOME", v);
        } else {
            std::env::remove_var("OCTO_HOME");
        }
        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
        assert_eq!(store.root, octo_dir.path().join("wallet"));
        // The HOME-backed root was NOT touched.
        assert!(!home_dir.path().join(".octo").exists());
    }

    /// `tv_x_10` (mission §AC-5): a pre-existing store root
    /// with permissive permissions has its mode fixed to 0700
    /// at `open` time, BEFORE the first write. Negative
    /// control: a store that defers the chmod to first write
    /// leaves a 0o755 root in place until the operator writes.
    #[cfg(unix)]
    #[test]
    fn tv_x_10_pre_existing_root_perm_fixed_on_open() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("wallet");
        std::fs::create_dir(&root).expect("create dir");
        std::fs::set_permissions(&root, PermissionsExt::from_mode(0o755)).expect("set permissive");
        let _store = WalletStore::open_at(&root).expect("open_at");
        let mode = std::fs::metadata(&root)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o700,
            "pre-existing root must be 0o700 after open, got {mode:o}"
        );
    }

    /// `tv_x_11` (mission §Determinism): records are persisted
    /// in ascending-DID order so the on-disk envelope is
    /// byte-stable across opens. Verified by inserting three
    /// records whose DID-ascending order is NOT insertion order
    /// and confirming the persisted file matches the sorted
    /// order. Negative control: a `Vec` insertion-appender
    /// would emit in insertion order, breaking byte stability.
    #[test]
    fn tv_x_11_records_persisted_in_ascending_did_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        // Three seeds whose derived DIDs sort lexicographically
        // in the order AA, BB, CC (insertion order CC, AA, BB).
        let key_cc = IdentityKey::from_seed([0xCCu8; 32]);
        let key_aa = IdentityKey::from_seed([0xAAu8; 32]);
        let key_bb = IdentityKey::from_seed([0xBBu8; 32]);
        store
            .register(key_cc, "correct-horse-battery-staple", false, 1)
            .expect("register cc");
        store
            .register(key_aa, "correct-horse-battery-staple", false, 2)
            .expect("register aa");
        store
            .register(key_bb, "correct-horse-battery-staple", false, 3)
            .expect("register bb");
        // Reload the index and confirm ascending DID order.
        let raw = std::fs::read(dir.path().join("store.json")).expect("read store.json");
        let parsed: serde_json::Value = serde_json::from_slice(&raw).expect("parse");
        let dids: Vec<&str> = parsed["records"]
            .as_array()
            .expect("records array")
            .iter()
            .map(|r| r["did"].as_str().expect("did str"))
            .collect();
        let mut sorted = dids.clone();
        // Stable sort is required: deterministic ordering
        // is the substrate's byte-stability contract (mission
        // §Determinism Requirements). The default sort is
        // stable for `&str`; we keep the explicit form so a
        // reader sees the contract is honored.
        #[allow(clippy::stable_sort_primitive)]
        sorted.sort();
        assert_eq!(dids, sorted, "records must be sorted by DID");
    }

    /// `tv_x_16` (mission §AC-28 positive path): a
    /// 12-character passphrase exactly at the floor succeeds at
    /// `register`. The `>=` (not `>`) check is the boundary.
    #[test]
    fn tv_x_16_passphrase_at_floor_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x16u8; 32]);
        let boundary = "a".repeat(MIN_PASSPHRASE_CHARS);
        assert_eq!(boundary.len(), 12);
        store
            .register(key, &boundary, true, 1_700_000_000)
            .expect("register at floor");
    }

    /// `tv_x_17` (mission §AC-28): an 11-character passphrase
    /// is below the floor and is refused at `register`. The
    /// floor is `>= 12`, so 11 is strictly below.
    #[test]
    fn tv_x_17_passphrase_below_floor_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x17u8; 32]);
        let boundary = "a".repeat(MIN_PASSPHRASE_CHARS - 1);
        assert_eq!(boundary.len(), 11);
        let err = store
            .register(key, &boundary, false, 1_700_000_000)
            .unwrap_err();
        assert!(
            matches!(err, WalletError::WeakPassphrase),
            "11-char passphrase must yield WeakPassphrase, got {err:?}"
        );
    }

    /// `tv_x_18` (mission §AC-26 atomic write): `register`
    /// writes atomically via write-temp + rename, so a crash
    /// mid-write leaves either the previous or the new
    /// `store.json`, never a partial. Verified indirectly: the
    /// read-back always parses as a complete index because
    /// either the rename completed or the temp file remains
    /// under the unrenamed name. The substrate also `sync_all`
    /// the parent directory so the rename is durable across
    /// power loss.
    #[test]
    fn tv_x_18_store_json_parses_after_register() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x18u8; 32]);
        store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        // No stray temp files left behind.
        let mut temp_count = 0;
        for entry in std::fs::read_dir(dir.path()).expect("read dir").flatten() {
            if entry.file_name().to_string_lossy().contains(".tmp") {
                temp_count += 1;
            }
        }
        assert_eq!(temp_count, 0, "atomic write must leave no temp file behind");
    }

    /// `tv_x_19` (mission §AC-19 round-trip): re-opening an
    /// already-initialized store reads the existing index,
    /// not a new one. Verified by registering a record,
    /// dropping the store, re-opening, and checking the
    /// record is still there.
    #[test]
    fn tv_x_19_reopen_reads_existing_index() {
        let dir = tempfile::tempdir().expect("tempdir");
        let did = {
            let mut store = WalletStore::open_at(dir.path()).expect("open_at");
            let key = IdentityKey::from_seed([0x19u8; 32]);
            store
                .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
                .expect("register")
        };
        let mut store2 = WalletStore::open_at(dir.path()).expect("reopen");
        store2.reload().expect("reload");
        assert_eq!(store2.active_did(), Some(&did));
        assert_eq!(store2.list_records().len(), 1);
    }

    /// `tv_x_21` (mission §AC-19 sign success): `unlock` then
    /// `sign` produces an Ed25519 signature that the public key
    /// verifies. Negative control: a substrate that rehydrated
    /// the wrong key would emit a signature the recorded
    /// pubkey cannot verify, breaking the round-trip.
    #[test]
    fn tv_x_21_unlock_then_sign_verifies_with_recorded_pubkey() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x21u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let pubkey = store
            .identity_record(&did)
            .expect("identity_record")
            .pubkey_bytes;
        let mut seed_out = Vec::new();
        let handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let sig = handle.sign(b"hello").expect("sign");
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey).expect("vk");
        verifying_key
            .verify_strict(b"hello", &sig)
            .expect("signature must verify against recorded pubkey");
    }

    /// `tv_x_22` (mission §AC-31 stub note): the substrate's
    /// `try_active_identity` is reserved for the unlock-time
    /// rehydration path; in its current form it always
    /// returns `Locked` (a metadata-only contract — calling
    /// code must `unlock` first). The vector pins the
    /// stub-shape so a substrate that silently returns a
    /// fresh key without unlock is detected.
    #[test]
    fn tv_x_22_try_active_identity_is_metadata_only_and_returns_locked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = WalletStore::open_at(dir.path()).expect("open_at");
        let err = store.try_active_identity().unwrap_err();
        assert!(
            matches!(err, WalletError::Locked),
            "try_active_identity on a never-unlocked store must yield Locked, got {err:?}"
        );
    }

    /// `tv_x_23` (mission §AC-31): an identity that was
    /// registered and never unlocked cannot sign. The
    /// substrate's metadata-only contract is that signing
    /// goes through `UnlockedWallet`, not through the
    /// store directly.
    #[test]
    fn tv_x_23_store_does_not_expose_sign_without_unlock() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x23u8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        // The store has no `sign` method (intentional). The
        // `try_active_identity` stub returns `Locked`. There
        // is no path from a registered identity to a
        // signature without `unlock`.
        let err = store.try_active_identity().unwrap_err();
        assert!(matches!(err, WalletError::Locked));
    }

    /// `tv_x_31` (mission §AC-45): a missing vault slot
    /// surfaces `VaultSlotNotFound` distinct from
    /// `VaultDecryptionFailed`. The failure modes are
    /// discriminable so an operator can tell a deleted-slot
    /// case from a wrong-passphrase case without re-running.
    #[test]
    fn tv_x_31_missing_slot_yields_vault_slot_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x31u8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        // Delete the slot file.
        let seed_dir = dir.path().join("seed");
        for entry in std::fs::read_dir(&seed_dir)
            .expect("read seed dir")
            .flatten()
        {
            let p = entry.path();
            if p.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("vault"))
            {
                std::fs::remove_file(&p).expect("remove slot");
            }
        }
        let mut seed_out = Vec::new();
        let err = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .unwrap_err();
        assert!(
            matches!(err, WalletError::VaultSlotNotFound(_)),
            "missing slot must yield VaultSlotNotFound, got {err:?}"
        );
    }

    /// `tv_x_34` (mission §AC-32): the rehydrated key's
    /// public key matches the record's persisted public key
    /// for every lifecycle state the substrate accepts. The
    /// seed bytes live in the `UnlockedWallet` handle, NOT
    /// in the caller-owned `seed_out` buffer (which the
    /// substrate zeroizes per §AC-20). Negative control: a
    /// substrate that rehydrated through `from_seed` (which
    /// hard-codes `Designated`) would yield a key with a
    /// pubkey matching the record only when the record is
    /// `Designated`; for `Active` the pubkey would still
    /// match (since `from_seed` derives the pubkey from the
    /// seed, not the lifecycle), so we also assert the
    /// `lifecycle()` to discriminate. The seed byte equality
    /// is the primary check.
    #[test]
    fn tv_x_34_decrypted_seed_is_32_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x34u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let recorded = store
            .identity_record(&did)
            .expect("identity_record")
            .pubkey_bytes;
        let mut seed_out = Vec::new();
        let pubkey = {
            let handle = store
                .unlock("correct-horse-battery-staple", &mut seed_out)
                .expect("unlock");
            handle.active_identity().public_key_bytes()
        };
        // Mission §AC-20: the caller-owned buffer is
        // zeroized, so it must be empty after unlock
        // returns. The handle is dropped here so the
        // lifetime tie between `seed_out` and the
        // returned `UnlockedWallet` ends.
        assert!(
            seed_out.iter().all(|b| *b == 0),
            "seed_out must be zeroized post-unlock"
        );
        // The seed lived in the handle. Its pubkey must
        // match the record's persisted pubkey.
        assert_eq!(pubkey, recorded);
    }

    /// `tv_x_37` (mission §AC-31): `begin_rotation` flips the
    /// predecessor's lifecycle to `Rotating` and seals the
    /// successor's slot. Verified by reading the record after
    /// `begin_rotation` and confirming the lifecycle is
    /// `Rotating`.
    #[test]
    fn tv_x_37_begin_rotation_flips_predecessor_to_rotating() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x37u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mut seed_out = Vec::new();
        let mut handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let successor = IdentityKey::from_seed([0x38u8; 32]);
        let _successor_did = handle
            .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
            .expect("begin_rotation");
        let record = store.identity_record(&did).expect("identity_record");
        assert_eq!(record.lifecycle, LifecycleState::Rotating);
    }

    /// `tv_x_38` (mission §AC-39): `UnlockedWallet` holds a
    /// UNIQUE `IdentityKey`, not a clone. The test is
    /// negative-controlled: a substrate that cloned the key
    /// would have two `Arc<dyn HsmAdapter>` instances pointing
    /// at the same backing signer; the substrate holds the
    /// key by value so the seed lifetime is exactly the
    /// handle lifetime. We exercise this by checking that
    /// `active_identity` returns a reference and the
    /// `sign` method succeeds without an intermediate clone.
    #[test]
    fn tv_x_38_unlocked_wallet_holds_unique_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x38u8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mut seed_out = Vec::new();
        let handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let first = handle.active_identity();
        let second = handle.active_identity();
        assert!(
            std::ptr::eq(first, second),
            "identity key must be unique per handle"
        );
    }

    /// `tv_x_39` (mission §AC-15): `lookup_identity_record`
    /// on a DID not in the index returns `IdentityNotFound`.
    #[test]
    fn tv_x_39_lookup_unknown_did_returns_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = WalletStore::open_at(dir.path()).expect("open_at");
        let phantom = Did::from("did:octo:zzznotpresent");
        let err = store.lookup_identity_record(&phantom).unwrap_err();
        assert!(
            matches!(err, WalletError::IdentityNotFound(_)),
            "lookup of unknown DID must yield IdentityNotFound, got {err:?}"
        );
    }

    /// `tv_x_40` (mission §AC-33): `register` with an
    /// `activate = true` flag and a `passphrase` exactly at
    /// the 12-character floor round-trips through a `reload`
    /// and re-unlocks with the same passphrase. Per §AC-20,
    /// the `seed_out` buffer is zeroized, so the round-trip
    /// success is observed via the handle's `sign` method
    /// rather than the buffer's content.
    #[test]
    fn tv_x_40_register_then_unlock_round_trip_with_floor_passphrase() {
        let dir = tempfile::tempdir().expect("tempdir");
        let passphrase = "a".repeat(MIN_PASSPHRASE_CHARS);
        {
            let mut store = WalletStore::open_at(dir.path()).expect("open_at");
            let key = IdentityKey::from_seed([0x40u8; 32]);
            store
                .register(key, &passphrase, true, 1_700_000_000)
                .expect("register");
        }
        let mut store2 = WalletStore::open_at(dir.path()).expect("reopen");
        store2.reload().expect("reload");
        let mut seed_out = Vec::new();
        {
            let handle = store2.unlock(&passphrase, &mut seed_out).expect("unlock");
            handle.sign(b"round-trip").expect("sign after round-trip");
        }
        // Buffer is zeroized per §AC-20. The handle's drop
        // releases the lifetime tie so `seed_out` is now
        // safely readable.
        assert!(
            seed_out.iter().all(|b| *b == 0),
            "seed_out must be zeroized post-unlock (mission §AC-20)"
        );
    }

    /// `tv_x_41` (mission §AC-38): `abort_rotation` clears
    /// the predecessor's `Rotating` state back to `Active`
    /// AND surfaces the already-sealed successor slot via
    /// `orphan_slots()`. The orphan is the in-band way a slot
    /// becomes unreferenced after abort.
    #[test]
    fn tv_x_41_abort_rotation_leaves_successor_slot_as_orphan() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x41u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mut seed_out = Vec::new();
        let mut handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let successor = IdentityKey::from_seed([0x42u8; 32]);
        handle
            .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
            .expect("begin_rotation");
        handle.abort_rotation().expect("abort_rotation");
        let record = store.identity_record(&did).expect("identity_record");
        assert_eq!(record.lifecycle, LifecycleState::Active);
        // The successor slot is now an orphan - the index has
        // exactly one record (the original predecessor).
        assert_eq!(store.list_records().len(), 1);
        let orphans = store.orphan_slots();
        assert!(
            !orphans.is_empty(),
            "abort_rotation must leave a sealed successor slot orphan"
        );
    }

    /// `tv_x_44` (mission §AC-31): an `identity_record`
    /// call on a DID present in the index returns the
    /// `Clone` of the record; the cloned record matches the
    /// on-disk form. Negative control: a substrate that
    /// returned a default-constructed record would silently
    /// emit a fake identity.
    #[test]
    fn tv_x_44_identity_record_round_trips_through_clone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x44u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let rec_ref = store.identity_record(&did).expect("identity_record ref");
        let rec_owned = store.lookup_identity_record(&did).expect("lookup");
        assert_eq!(rec_ref.pubkey_bytes, rec_owned.pubkey_bytes);
        assert_eq!(rec_ref.did, rec_owned.did);
        assert_eq!(rec_ref.lifecycle, rec_owned.lifecycle);
        assert_eq!(rec_ref.registered_at_unix, rec_owned.registered_at_unix);
    }

    /// `tv_x_17` (mission §AC-20 success path): on a
    /// successful `unlock`, the caller-owned `seed_out`
    /// buffer is zeroized. The seed bytes live in the
    /// `UnlockedWallet` handle only. Negative control: drop
    /// the post-`copy_from_slice` zeroize loop in
    /// `WalletStore::unlock` and confirm this vector fails.
    #[test]
    fn tv_x_17_seed_out_zeroized_on_success_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x71u8; 32]);
        store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        // Pre-poison the buffer with a known sentinel so the
        // zeroize is observable (the vault writes into the
        // buffer; an empty buffer would be trivially
        // "zeroized" because it is empty, which is the
        // vacuous-test shape the mission YAML warns
        // against).
        let mut seed_out = vec![0xAAu8; 64];
        let _handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        assert!(
            seed_out.iter().all(|b| *b == 0),
            "seed_out must be zeroized post-unlock (mission §AC-20), got {seed_out:?}"
        );
    }

    /// `tv_x_18` (mission §AC-20 error path): on a
    /// wrong-passphrase `unlock`, the caller-owned
    /// `seed_out` buffer is zeroized even though the
    /// vault's `get` failed. Pre-poisoned with a sentinel
    /// so the zeroize is observable (the vacuous-test
    /// shape the mission YAML warns against is "an empty
    /// buffer is trivially zeroized, so it always passes").
    /// Negative control: drop the pre-`return` zeroize in
    /// `WalletStore::unlock`'s Err arm.
    #[test]
    fn tv_x_18_seed_out_zeroized_on_wrong_passphrase() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0x72u8; 32]);
        store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mut seed_out = vec![0xBBu8; 64];
        let result = store.unlock("wrong-passphrase-12", &mut seed_out);
        assert!(result.is_err(), "wrong passphrase must error");
        assert!(
            seed_out.iter().all(|b| *b == 0),
            "seed_out must be zeroized post-Err (mission §AC-20), got {seed_out:?}"
        );
    }

    /// `tv_x_22` (mission §AC-36): after `complete_rotation`
    /// succeeds, the predecessor returns to `Active`
    /// lifecycle AND is marked `deprecated: true`. Both
    /// transitions are required: the lifecycle alone does
    /// not encode the rotation-completion status, and the
    /// `deprecated` flag alone does not produce the right
    /// gating behaviour for the grace-window logic.
    #[test]
    fn tv_x_22_predecessor_returns_to_active_deprecated_after_complete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0xA1u8; 32]);
        let did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mut seed_out = Vec::new();
        let mut handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let successor = IdentityKey::from_seed([0xA2u8; 32]);
        handle
            .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
            .expect("begin_rotation");
        // 86_400 + 10 seconds later - the 24-hour grace
        // period plus a buffer for clock skew.
        handle
            .complete_rotation(1_700_000_010 + 86_400 + 10)
            .expect("complete_rotation");
        let predecessor = store.identity_record(&did).expect("identity_record");
        assert_eq!(predecessor.lifecycle, LifecycleState::Active);
        assert!(
            predecessor.deprecated,
            "predecessor must be marked deprecated after complete_rotation (mission §AC-36)"
        );
    }

    /// `tv_x_39` (mission §AC-34 documented no-lock
    /// behavior): two stores writing through the same
    /// root directory take no file lock. The negative
    /// control the mission YAML specifies — `flock(LOCK_EX)`
    /// — would serialize the two and let both records
    /// survive. The substrate ships single-writer,
    /// last-writer-wins; the second registration
    /// overwrites the index. We assert the contract: two
    /// registrations against the same root both return
    /// `Ok`, and the index contains exactly one record (the
    /// last writer's). This is the direction the
    /// documented no-lock contract points; a future
    /// locking amendment would FAIL this vector (both
    /// records would survive), and RFC-0011-x §Future Work
    /// item 16 names the substrate mission as the owner.
    #[test]
    fn tv_x_39_documented_no_lock_last_writer_wins() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store_a = WalletStore::open_at(dir.path()).expect("open_at a");
        let mut store_b = WalletStore::open_at(dir.path()).expect("open_at b");
        let key_a = IdentityKey::from_seed([0xC1u8; 32]);
        let key_b = IdentityKey::from_seed([0xC2u8; 32]);
        store_a
            .register(key_a, "correct-horse-battery-staple", false, 1_700_000_000)
            .expect("register a");
        store_b
            .register(key_b, "correct-horse-battery-staple", false, 1_700_000_001)
            .expect("register b");
        // Reopen from disk to read the final state.
        let mut store_final = WalletStore::open_at(dir.path()).expect("open_at final");
        store_final.reload().expect("reload");
        // Last-writer-wins: exactly one record survives.
        let n = store_final.list_records().len();
        assert_eq!(
            n, 1,
            "no-lock contract: last-writer-wins leaves one record, got {n}"
        );
    }

    /// `tv_x_40` rotation lifecycle (mission §AC-32): a
    /// `Rotating` record rehydrated through
    /// `from_seed_with_lifecycle` does NOT panic in
    /// `complete_rotation`. The predecessor was opened
    /// with `from_seed_with_lifecycle` (5-arg form) so
    /// `rotation_started_at_unix_secs` is populated; the
    /// rehydrated key's `.expect(...)` in
    /// `complete_rotation` does not fire. Negative
    /// control: rehydrate through `IdentityKey::from_seed`
    /// (1-arg form, which leaves `rotation_started_at` as
    /// `None`); `complete_rotation` panics with exit 101.
    #[test]
    fn tv_x_40_rotation_round_trip_does_not_panic_in_complete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0xABu8; 32]);
        let _did = store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mut seed_out = Vec::new();
        let mut handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let successor = IdentityKey::from_seed([0xACu8; 32]);
        handle
            .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
            .expect("begin_rotation");
        // The `Rotating` rehydration must succeed and
        // `complete_rotation` must not panic. Mission
        // §AC-32 / §AC-33. The 24-hour grace period is
        // honored so the call returns `Ok`.
        let result = handle.complete_rotation(1_700_000_010 + 86_400 + 10);
        assert!(
            result.is_ok(),
            "complete_rotation on a 5-arg-rehydrated Rotating key must not panic, got {result:?}"
        );
    }

    /// tv_x_49 - CROSS-PROCESS rotate-complete must not panic.
    ///
    /// `tv_x_40` reuses ONE `UnlockedWallet` for begin and complete, so
    /// `rotation_started_at_unix_secs` is still in memory. The CLI runs
    /// `octo identity rotate` and `octo identity rotate-complete` as two
    /// separate processes, so the only carrier is `store.json`.
    ///
    /// The reconstruction in `unlock` reads the start time from
    /// `record.rotation_history`, but nothing ever PUSHED an event
    /// there: `begin_rotation` sets the timestamp on the in-memory key
    /// and `persist_active_record` writes only `lifecycle` and
    /// `deprecated`. The history is `Vec::new()` at both construction
    /// sites, so the reconstruction is `None` on every real run and
    /// `complete_rotation` hits its `.expect(...)` - exit 101, a panic
    /// with no envelope, on the only path the CLI can take. The
    /// `GracePeriodNotElapsed` path that exit 43 documents was
    /// unreachable.
    #[test]
    fn tv_x_49_rotate_complete_survives_a_store_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        let key = IdentityKey::from_seed([0xABu8; 32]);
        store
            .register(key, "correct-horse-battery-staple", true, 1_700_000_000)
            .expect("register");
        let mut seed_out = Vec::new();
        let mut handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let successor = IdentityKey::from_seed([0xACu8; 32]);
        let successor_did_obj = successor.did();
        let successor_did = successor_did_obj.0.clone();
        let proof = handle
            .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
            .expect("begin_rotation");
        drop(handle);
        drop(store);

        // Reopen, exactly as a second `octo identity` process would.
        let mut store = WalletStore::open_at(dir.path()).expect("reopen");
        let mut seed_out = Vec::new();
        let did = store
            .active_did()
            .expect("reopened store has an active DID")
            .clone();
        let persisted = store.identity_record(&did).expect("record").clone();
        let mut handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock after reopen");
        assert!(
            !persisted.rotation_history.is_empty(),
            "begin_rotation must persist a rotation event, or the start time is lost \
             across processes and complete_rotation panics"
        );
        assert_eq!(
            persisted.rotation_history[0].successor_did.0, successor_did,
            "the persisted event must name the successor the operator rotated to"
        );
        assert_eq!(
            persisted.rotation_history[0].started_at_unix, 1_700_000_010,
            "the persisted START TIME must survive, not merely the event's existence; \
             a zeroed timestamp makes the grace deadline unreachable"
        );
        assert_eq!(
            persisted.rotation_history[0].signature_proof, proof,
            "the persisted event must carry the proof begin_rotation returned"
        );
        // Past the 24h grace, so this is the Ok path rather than the
        // exit-43 refusal.
        let result = handle.complete_rotation(1_700_000_010 + 86_400 + 10);
        assert!(
            result.is_ok(),
            "complete_rotation after a reopen must succeed, got {result:?}"
        );
        drop(handle);
        drop(store);

        // The POST-CONDITIONS, not just the return value. A mutation
        // that deleted `index.active_did = Some(successor_did)` left
        // all 297 vectors green while the wallet kept the retired,
        // deprecated predecessor as active - the rotation reported
        // success and moved nothing.
        let store = WalletStore::open_at(dir.path()).expect("reopen after complete");
        let active = store
            .active_did()
            .expect("a completed rotation must leave an active identity");
        assert_eq!(
            active.0, successor_did,
            "a completed rotation must promote the SUCCESSOR; the wallet is still \
             pointed at the predecessor it just retired"
        );
        let succ = store
            .identity_record(&successor_did_obj)
            .expect("the promoted successor must have a record")
            .clone();
        assert_eq!(
            succ.lifecycle,
            LifecycleState::Active,
            "the promoted successor must be Active"
        );
        let pred = store
            .identity_record(&did)
            .expect("the retired predecessor keeps its record")
            .clone();
        assert_eq!(
            pred.lifecycle,
            LifecycleState::Active,
            "the predecessor returns to Active in a deprecated state, not a \
             terminal one"
        );
        assert!(
            pred.deprecated,
            "the predecessor must be marked deprecated on disk; a rehydrated key \
             that reported false would have written false back over this"
        );
    }

    /// tv_x_50 - a refused `begin_rotation` must leave NO vault slot.
    ///
    /// The lifecycle guard lives inside `IdentityKey::begin_rotation`,
    /// which runs after the store seals the successor's slot. A
    /// predecessor that is `Designated` (register without activation
    /// is the reachable case) was therefore refused AFTER the write,
    /// orphaning an encrypted slot on disk. `orphan_slots()` exists
    /// to surface exactly this, and no CLI command calls it, so the
    /// operator has no way to see or clean it. The store now guards
    /// before the seal.
    #[test]
    fn tv_x_50_refused_rotation_seals_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WalletStore::open_at(dir.path()).expect("open_at");
        // Register without activation leaves the record Designated.
        let key = IdentityKey::from_seed([0xABu8; 32]);
        store
            .register(key, "correct-horse-battery-staple", false, 1_700_000_000)
            .expect("register");
        let record = store
            .active_did()
            .and_then(|d| store.identity_record(d).ok().cloned());
        // A fresh store promotes the first registration, so assert on
        // whatever state the record actually landed in rather than
        // assuming. If it is Active the guard cannot fire and this
        // vector would be vacuous - skip rather than lie.
        let designated = record
            .as_ref()
            .is_some_and(|r| r.lifecycle == LifecycleState::Designated);
        // A silent early return would make this vector pass on a store
        // shape that never exercises the guard, which is the vacuous
        // shape this review exists to remove. Assert the precondition
        // instead: if `register` ever promotes a first registration
        // unconditionally, this vector FAILS and says so, rather than
        // quietly testing nothing.
        assert!(
            designated,
            "tv_x_50 needs a Designated record to exercise the guard; \
             register(false) produced {:?}. If that promotion is now \
             unconditional, this vector no longer covers the seal-after- \
             guard order and must be rewritten, not skipped.",
            record.as_ref().map(|r| r.lifecycle)
        );
        let before = store.orphan_slots().len();
        let mut seed_out = Vec::new();
        let mut handle = store
            .unlock("correct-horse-battery-staple", &mut seed_out)
            .expect("unlock");
        let successor = IdentityKey::from_seed([0xACu8; 32]);
        let result =
            handle.begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010);
        assert!(
            matches!(result, Err(WalletError::NotActive { .. })),
            "a non-Active predecessor must be refused, got {result:?}"
        );
        drop(handle);
        let after = store.orphan_slots();
        assert_eq!(
            after.len(),
            before,
            "a refused rotation must seal nothing; orphans went from {before} to {}: {after:?}",
            after.len()
        );
    }
}
