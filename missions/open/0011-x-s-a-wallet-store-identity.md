# 0011-x-s-a-wallet-store-identity — Substrate additions for the `WalletStore` identity store

## Status

Open (2026-09-30) — Substrate companion to RFC-0011-x. Layer B (`octo-wallet`). The paired CLI mission `0011-x-wallet-store-cli` waits for this mission to land per the substrate-first ordering invariant.

## RFC

RFC-0011-x §Detailed Design, §Lifecycle Requirements, §Determinism Requirements, §Test Vectors.

## Summary

Turns `WalletStore` from a zero-sized struct into a real on-disk identity store, and adds the registration write-path that nothing in the workspace currently provides.

The encrypted primitives this needs already exist and are already tested in `octo-wallet`: `Vault` (Argon2id + AES-256-GCM, 0700 slots dir, `put` / `get` / `list`), `IdentityKey` (generate, `from_seed`, `activate`, `begin_rotation`, `complete_rotation`, `abort_rotation`, `revoke`), and `IdentityRecord` / `IdentityRotationEvent` (serde-complete, no writer). **No new cryptography is introduced and no new cryptographic dependency is required beyond the two listed below.**

What does not exist is the layer that ties them together: a DID-indexed record set, an active-DID pointer, a home resolver, and any code that writes either. The parent RFC's `[ADD]` contract is satisfied in signature and violated in behaviour, and the operator guide documents the consequence at five locations.

Scope note: this mission covers the store, the unlock, and the write-path. The CLI surface, the 13 call-site migrations, the slot 92 error variant, and the guide update belong to `0011-x-wallet-store-cli`.

### Substrate additions target

```rust
// crates/octo-wallet/src/identity_store.rs (NEW module)
use crate::error::WalletError;
use crate::identity::IdentityKey;
use crate::identity_record::{Did, IdentityRecord};
use crate::vault::{DecryptedHandle, Vault};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WalletIndex {
    /// Schema version. Currently 1; a reader MUST reject an unknown version.
    pub version: u32,
    pub active_did: Option<Did>,
    /// Ascending-DID order, so store.json is byte-stable for a given store state.
    pub records: Vec<IdentityRecord>,
}

pub struct WalletStore {
    root: PathBuf,
    index: WalletIndex,
    vault: Vault,
}

impl WalletStore {
    /// Parent-RFC signature, UNCHANGED. Metadata only; never decrypts.
    pub fn open() -> Result<Self, WalletError>;
    /// Test seam: open at an explicit root instead of the ambient home.
    pub fn open_at(root: impl Into<PathBuf>) -> Result<Self, WalletError>;

    // Metadata readers — no passphrase required.
    pub fn active_did(&self) -> Option<&Did>;
    pub fn list_records(&self) -> &[IdentityRecord];
    pub fn identity_record(&self, did: &Did) -> Result<IdentityRecord, WalletError>;
    pub fn active_seed_slot_present(&self) -> bool;
    pub fn reload(&mut self) -> Result<(), WalletError>;

    /// Decrypt the active identity seed slot. `seed_out` is caller-owned so the
    /// zeroization obligation has exactly one enforcement site.
    pub fn unlock<'a>(
        &'a self,
        passphrase: &str,
        seed_out: &'a mut Vec<u8>,
    ) -> Result<UnlockedWallet<'a>, WalletError>;
}

pub struct UnlockedWallet<'a> {
    store: &'a WalletStore,
    key: IdentityKey,
}

impl<'a> UnlockedWallet<'a> {
    pub fn active_identity(&self) -> Result<IdentityKey, WalletError>;
    pub fn identity_record(&self, did: &Did) -> Result<IdentityRecord, WalletError>;
    pub fn list(&self) -> Result<Vec<IdentityRecord>, WalletError>;

    pub fn register(
        &self,
        key: IdentityKey,
        activate: bool,
        now_unix: i64,
    ) -> Result<Did, WalletError>;
    pub fn select(&self, did: &Did) -> Result<(), WalletError>;

    pub fn begin_rotation(&self, successor: IdentityKey, now_unix: u64) -> Result<[u8; 64], WalletError>;
    pub fn complete_rotation(&self, now_unix: u64) -> Result<(), WalletError>;
    pub fn abort_rotation(&self) -> Result<(), WalletError>;
    pub fn revoke(&self, now_unix: u64) -> Result<(), WalletError>;
}
```

Storage layout, with the root created at 0700 on first **write** and each file at 0600:

```text
$OCTO_HOME/wallet/            0700
  store.json                  0600  WalletIndex: version, active_did, records[]
  seed/                       0700  Vault slots_dir
    <did-slug>.vault          0600  Argon2id + AES-256-GCM, holds the 32-byte seed
```

`store.json` is deliberately unencrypted. Its entire content is DIDs, public keys, lifecycle states, and timestamps — no secret. Encrypting it would buy nothing and would force a passphrase onto read-only inspection, defeating the design goal that metadata reads stay passphrase-free.

### New error variants (Layer B)

Two additions to `WalletError` in `crates/octo-wallet/src/error.rs`:

```rust
/// The store is locked. `WalletStore::open` is metadata-only; the identity
/// seed requires `WalletStore::unlock(passphrase)`.
#[error("wallet store is locked; unlock with a passphrase to access the identity key")]
Locked,

/// No record for this DID in the store index.
#[error("no identity record for {0}")]
IdentityNotFound(Did),
```

Reused, not added: `VaultDecryptionFailed` (bad passphrase), `VaultSlotNotFound` (missing slot), `NotActive { current_state }` (no active identity), `AlreadyRevoked`, `RotationInProgress`, `NotRotating`, `SelfRotation`, `GracePeriodNotElapsed` — all already returned by the `IdentityKey` state machine.

### New dependencies

Two, each with a Cargo.toml rationale comment per the repository convention:

| Crate     | Why                                                                                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `dirs`    | Home-directory resolution for the `$OCTO_HOME` → `$HOME/.octo` order. `octo-wallet` has no resolver today; the store needs one. |
| `zeroize` | Zeroize `seed_out` on every `unlock` return path. Not currently a direct dependency.                                            |

`argon2`, `aes-gcm`, `chacha20poly1305`, `rpassword`, `serde`, and `serde_json` are already declared and are reused unchanged.

### Migration sentinel

`WalletStore::active_identity` is **retained with its parent-RFC signature, marked `#[deprecated]`, and always returns `Err(WalletError::Locked)`**.

The parent RFC requires this method on `WalletStore`. Removing it outright turns a silent stub into 13 independent compile errors across five CLI modules, and a partial migration would leave the store half-wired in a way no test enumerates. Retaining it as a deprecated always-failing method turns each un-migrated site into a build warning and a hard runtime error.

The CLI mission carries its removal as an acceptance criterion so the sentinel cannot outlive the migration. **This mission does not remove it.**

## Test Vectors

Per RFC-0011-x §Test Vectors. Substrate vectors are `tv_x_1` through `tv_x_19`, `tv_x_26` through `tv_x_28`, and `tv_x_32`. The CLI mission carries `tv_x_20` and `tv_x_29` through `tv_x_31` where they are observable at the command surface, plus its own per-call-site vectors.

Two vectors carry the security properties and must be negative-controlled:

- `tv_x_17` / `tv_x_18` — `unlock` zeroizes `seed_out` on the success path and on the wrong-passphrase path. Negative control: remove the zeroize call, confirm `tv_x_18` fails. A test that only covers the success path would not notice a missing zeroize on the error path, which is the path an attacker probing passphrases actually exercises.
- `tv_x_3` / `tv_x_4` — mode enforcement. Negative control: drop the `set_permissions` call, confirm `tv_x_4` fails.

## Acceptance Criteria

- [ ] **AC-1:** `crates/octo-wallet/src/identity_store.rs` exists and is declared plus re-exported from `crates/octo-wallet/src/lib.rs`
- [ ] **AC-2:** `WalletStore` holds `root`, `index`, and `vault`; it is no longer a zero-sized struct
- [ ] **AC-3:** `WalletStore::open()` keeps its exact parent-RFC signature and resolves `$OCTO_HOME` → `$HOME/.octo` → `WalletError::Config`, with an empty `OCTO_HOME` treated as unset
- [ ] **AC-4:** `open()` on a non-existent root yields an empty index, not an error, and creates no directory
- [ ] **AC-5:** The 0700 root and 0600 `store.json` are created on first write, and a pre-existing permissive mode is corrected on open
- [ ] **AC-6:** `WalletError::Locked` and `WalletError::IdentityNotFound` are added with doc comments
- [ ] **AC-7:** `WalletError::IdentityNotFound` does **not** consume a `WalletError` slot — the slot table is unchanged
- [ ] **AC-8:** `dirs` and `zeroize` are added to `crates/octo-wallet/Cargo.toml`, each with a rationale comment naming its layer and the RFC clause it serves
- [ ] **AC-9:** `unlock` decrypts through `Vault::get`, derives via `IdentityKey::from_seed`, and zeroizes `seed_out` on **every** return path
- [ ] **AC-10:** `unlock` errors are `NotActive` for no active DID, `VaultSlotNotFound` for a missing slot, `VaultDecryptionFailed` for a bad passphrase, `Config` when the slot content contradicts the index DID
- [ ] **AC-11:** `register` rejects a record whose DID does not match `IdentityKey::did()`, and is idempotent for a re-registered DID
- [ ] **AC-12:** `register` persists `Designated` and clears the active pointer when `activate` is false; persists `Active` and sets it when true
- [ ] **AC-13:** `begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` persist the state machine's own outcome and never write a record the state machine did not produce
- [ ] **AC-14:** `revoke` retains the record rather than deleting it, so `identity_record` can still resolve a revoked DID
- [ ] **AC-15:** `store.json` records are sorted ascending by DID; two stores built in different write orders serialize byte-identically
- [ ] **AC-16:** The store never reads a clock — every timestamp is a caller-supplied parameter
- [ ] **AC-17:** `WalletStore::active_identity` is `#[deprecated]` and always returns `WalletError::Locked`
- [ ] **AC-18:** `cli_fns` wrappers accept the unlocked handle and forward to the unlocked surface
- [ ] **AC-19:** `tv_x_1` through `tv_x_19`, `tv_x_26` through `tv_x_28`, `tv_x_32` all pass
- [ ] **AC-20:** `tv_x_17` and `tv_x_18` are negative-controlled individually — removing the zeroize call makes each fail
- [ ] **AC-21:** `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean
- [ ] **AC-22:** `cargo test -p octo-wallet --lib` green
- [ ] **AC-23:** `cargo fmt --check -p octo-wallet` clean
- [ ] **AC-24:** Layer discipline preserved — Layer B only, zero Layer A change
- [ ] **AC-25:** A full guide-executor run confirms the five guide wall statements are now false (informational; the guide update itself is the CLI mission's, enumerated in its AC-19)

## Dependencies

Hard sequencing:

1. **RFC-0011-x must be Accepted** before this mission's substrate lands.
2. **This mission must land BEFORE `0011-x-wallet-store-cli`** — the CLI cannot thread an unlock through call sites until `unlock` exists.
3. **This mission supersedes the identity-store portion of `0102-a-wallet-foundation`**, which has been `claimed/` since 2026-07-20 with every acceptance criterion unchecked while several of the substrate items it describes is already landed. See §Notes.

## Out of Scope

- CLI dispatch, the 13 call-site migrations, the slot 92 `OctoCliError` variant, and the guide update — all belong to `0011-x-wallet-store-cli`
- An authenticated store envelope — the same-user-tamper and rollback adversaries are real and open, and closing them is a new format decision with its own review (RFC-0011-x §Future Work)
- HSM handoff — the branch point in `unlock` is identified but the `HsmAdapter` implementation does not exist
- Home-resolver consolidation across crates — RFC-0011-x specifies the order normatively to prevent drift; consolidating the primitive is deferred
- Argon2id cost-parameter review for identity seeds — inherited from the vault's posture
- Removal of the migration sentinel — the CLI mission's criterion
- Any change to `Vault`, `StarkliCompat`, `IdentityKey`, `IdentityRecord`, `Did`, or `LifecycleState` — all reused unchanged
- Any Layer A change

## Notes

Filed 2026-09-30 per RFC-0011-x §Companion mission YAML pairing.

The framing this mission was written against was that closing `WalletStore` "means building an on-disk keystore, which is a substantial standalone Layer B crypto feature." That is false against the substrate. The on-disk keystore exists — `Vault` encrypts with Argon2id and AES-256-GCM, creates its directory at 0700, and is covered by tests including a wrong-passphrase rejection. `StarkliCompat` is a second, independently tested encrypted keystore. `IdentityKey` has a complete lifecycle with 41 tests. The only cryptographic dependency this mission adds is `zeroize`, and the only new one for the store's own resolution is `dirs`.

The genuine gap is a wiring layer that was never written, plus the one thing no primitive can supply: somewhere to put an identity. A store with a reader and no writer is always empty, so `octo whoami` keeps exiting 2 and the guide's five wall statements keep being true. The write-path is in scope for that reason and not because the crypto was missing.

The unlock split is the one genuinely new design decision in RFC-0011-x. The parent RFC's `open()` takes no passphrase while `active_identity` must return a usable key, and under a strict reading those two clauses are only jointly satisfiable by a store that reads a plaintext seed. RFC-0011-x chooses the locked/unlocked split instead, which is why metadata reads stay passphrase-free and only the signing paths need an unlock.
