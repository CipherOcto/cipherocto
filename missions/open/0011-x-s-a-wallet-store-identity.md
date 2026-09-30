# 0011-x-s-a-wallet-store-identity — Substrate additions for the `WalletStore` identity store

## Status

Open (2026-09-30) — Substrate companion to RFC-0011-x. Layer B (`octo-wallet`). The paired CLI mission `0011-x-wallet-store-cli` waits for this mission to land per the substrate-first ordering invariant.

## RFC

RFC-0011-x §Detailed Design, §Lifecycle Requirements, §Determinism Requirements, §Test Vectors.

## Summary

Turns `WalletStore` from a zero-sized struct into a real on-disk identity store, and adds the registration write-path that nothing in the workspace currently provides.

The encrypted primitives this needs already exist and are already tested in `octo-wallet`: `Vault` (Argon2id + AES-256-GCM, 0700 slots dir, `put` / `get` / `list`), `IdentityKey` (generate, `from_seed`, `activate`, `begin_rotation`, `complete_rotation`, `abort_rotation`, `revoke`), and `IdentityRecord` / `IdentityRotationEvent` (serde-complete, no writer). **No new cryptography is introduced and no new cryptographic dependency is required at all** — the one new dependency this mission adds is `dirs`, which is not cryptographic.

What does not exist is the layer that ties them together: a DID-indexed record set, an active-DID pointer, a home resolver, and any code that writes either. The parent RFC's `[ADD]` contract is satisfied in signature and violated in behaviour, and the operator guide documents the consequence at seven locations, enumerated by anchor sentence in the companion CLI mission's AC-19.

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

    /// Move the active pointer. A pure index write: it reads and writes
    /// `store.json` and touches no key material, so it needs no unlock.
    /// `&mut self` because it writes the index; the CLI is one-shot, so a
    /// mutable store across a single command is not a burden.
    pub fn select(&mut self, did: &Did) -> Result<(), WalletError>;

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

    pub fn register(
        &self,
        key: IdentityKey,
        activate: bool,
        now_unix: i64,
    ) -> Result<Did, WalletError>;

    pub fn begin_rotation(&self, successor: IdentityKey, now_unix: u64) -> Result<[u8; 64], WalletError>;
    pub fn complete_rotation(&self, now_unix: u64) -> Result<(), WalletError>;
    pub fn abort_rotation(&self) -> Result<(), WalletError>;
    pub fn revoke(&self, now_unix: u64) -> Result<(), WalletError>;
}
```

Three methods here are not additions, and their absence from this block would be a real specification gap rather than a presentational one:

- `WalletStore::try_active_identity` — exists today, returns an unconditional `NotActive`, and its return type changes to `Err(WalletError::Locked)` under this mission. It carries **no** `#[deprecated]` attribute: it is not what the CLI calls, and marking it would be theatre.
- `WalletStore::lookup_identity_record` — exists today, is a metadata reader that needs no unlock, and is not mentioned in the parent RFC. It survives unchanged and the CLI mission's sweep must reach its call sites.
- `cli_fns::active_identity` — the free function, not a method. See §Migration sentinel.

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

**One**, with a Cargo.toml rationale comment per the repository convention:

| Crate  | Why                                                                                                                                                    |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `dirs` | Home-directory resolution for the `$OCTO_HOME` → `$HOME/.octo` order. `ProjectDirs` cannot express `$OCTO_HOME`, so the store needs a second resolver. |

`zeroize` is **already** a declared direct dependency of `octo-wallet` and already carries a rationale comment, and `Vault` and `IdentityKey` already use it — the `seed_out` zeroization in `unlock` reuses the existing declaration rather than adding one. An earlier draft of this mission listed it as new; that was wrong against the substrate, and the "no new cryptographic dependency" claim in §Summary is only sound because there is exactly one new dependency and it is not cryptographic.

`argon2`, `aes-gcm`, `chacha20poly1305`, `rpassword`, `serde`, and `serde_json` are already declared and are reused unchanged.

### Migration sentinel

Three symbols are in play and conflating them is what made the first draft of this section unsound. `WalletStore` has three methods: `open`, `try_active_identity`, and `lookup_identity_record`. It has **no** `active_identity` method. The parent RFC's `[ADD]` clause specifies `active_identity` as a **free function**, `octo_wallet::active_identity(&WalletStore)`, implemented in `cli_fns` — and that free function, not the method, is what the CLI's key-reaching call sites invoke. A `#[deprecated]` on `WalletStore::active_identity` would emit zero warnings, because nothing calls it.

The sentinel is therefore two changes, and the deprecation belongs on the free function:

1. `cli_fns::active_identity` is **retained with its parent-RFC signature and always returns `Err(WalletError::Locked)`**, marked `#[deprecated(note = "… use WalletStore::unlock …")]`. Its signature is unchanged precisely so the deprecation warning fires at every un-migrated site.
2. `WalletStore::try_active_identity` changes from returning an unconditional `NotActive` to returning `Err(WalletError::Locked)`. No deprecation attribute, per the note above.

Why keep a failing function at all. Deleting it outright is the cleaner end state, but it converts a silent stub into 15 independent compile errors across five CLI modules, and a partial migration would leave the store half-wired in a way no test enumerates. Retaining it as a deprecated always-failing function turns each un-migrated site into a **warning at build time** — a real, attributable signal, because the deprecation is on the symbol the sites actually call — and a hard, attributable error at run time.

`WalletStore::lookup_identity_record` is the substrate's third method and the parent RFC does not mention it. It is a metadata reader, needs no unlock, and is **not** part of the sentinel. It survives unchanged.

The CLI mission carries the sentinel's removal as an acceptance criterion so it cannot outlive the migration. **This mission does not remove it.**

## Test Vectors

Per RFC-0011-x §Test Vectors. Substrate vectors are `tv_x_1` through `tv_x_19`, `tv_x_21` through `tv_x_28`, `tv_x_31`, and `tv_x_32` — 29 of the 32. The CLI mission carries the three CLI-observable ones, `tv_x_20`, `tv_x_29`, and `tv_x_30`, plus its own per-call-site vectors.

Three membership calls in that partition are non-obvious. The five lifecycle-persistence vectors `tv_x_21` through `tv_x_25` are substrate-owned even though they sit in the middle of the numbering — the persistence they assert happens entirely below the dispatch layer. Leaving them unowned would leave AC-13 and AC-14 with no vector able to catch a violation of either. `tv_x_31` is substrate-owned for the same kind of reason: it asserts that the deprecated `cli_fns::active_identity` free function always returns `Locked`, which is a property of a function **this** mission creates. The CLI mission originally claimed it, while simultaneously requiring the sentinel to be deleted — two postconditions that cannot both hold.

Two vectors carry the security properties and must be negative-controlled:

- `tv_x_17` / `tv_x_18` — `unlock` zeroizes `seed_out` on the success path and on the wrong-passphrase path. Negative control: remove the zeroize call, confirm `tv_x_18` fails. A test that only covers the success path would not notice a missing zeroize on the error path, which is the path an attacker probing passphrases actually exercises.

  `tv_x_18` must **pre-poison** `seed_out` with a known sentinel before calling `unlock` with a wrong passphrase, and assert the sentinel is gone afterwards. A wrong passphrase fails inside `Vault::get` before anything is written to the buffer, so on a naive implementation `seed_out` is never touched — and a test that passes an empty buffer and checks it is still empty would pass whether or not the zeroize call exists. That is the shape a vacuous security test takes, and it is why the negative control is mandatory here rather than optional.

- `tv_x_3` / `tv_x_4` — mode enforcement. Negative control: drop the `set_permissions` call, confirm `tv_x_4` fails.

## Acceptance Criteria

- [ ] **AC-1:** `crates/octo-wallet/src/identity_store.rs` exists and is declared plus re-exported from `crates/octo-wallet/src/lib.rs`
- [ ] **AC-2:** `WalletStore` holds `root`, `index`, and `vault`; it is no longer a zero-sized struct
- [ ] **AC-3:** `WalletStore::open()` keeps its exact parent-RFC signature and resolves `$OCTO_HOME` → `$HOME/.octo` → `WalletError::Config`, matching `octo-cli/src/home.rs`. An **empty** `OCTO_HOME` is a hard error, not a fall-through
- [ ] **AC-4:** `open()` on a non-existent root yields an empty index, not an error, and creates no directory
- [ ] **AC-5:** The 0700 root and 0600 `store.json` are created on first write, and a pre-existing permissive mode is corrected on open
- [ ] **AC-6:** `WalletError::Locked` and `WalletError::IdentityNotFound` are added with doc comments
- [ ] **AC-7:** `WalletError::IdentityNotFound` mints **no `OctoCliError` slot** — it maps to the existing `OctoCliError::IdentityNotFound(String)` at exit 4, so the CLI's slot table is unchanged. `WalletError` is an unslotted enum and has no slot table of its own
- [ ] **AC-8:** `dirs` is added to `crates/octo-wallet/Cargo.toml` with a rationale comment naming its layer and the RFC clause it serves. `zeroize` is **not** added: it is already a declared direct dependency of `octo-wallet` and already carries a rationale comment
- [ ] **AC-9:** `unlock` decrypts through `Vault::get`, derives via `IdentityKey::from_seed`, and zeroizes `seed_out` on **every** return path
- [ ] **AC-10:** `unlock` errors are `NotActive` for no active DID, `VaultSlotNotFound` for a missing slot, `VaultDecryptionFailed` for a bad passphrase, `Config` when the slot content contradicts the index DID. Each is a **named** variant that the CLI can match, so a translation arm can distinguish them — never a collapsed `Err(_)`
- [ ] **AC-11:** `register` rejects a record whose DID does not match `IdentityKey::did()`, and is idempotent for a re-registered DID
- [ ] **AC-12:** `register` persists `Designated` and clears the active pointer when `activate` is false; persists `Active` and sets it when true
- [ ] **AC-13:** `begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` persist the state machine's own outcome and never write a record the state machine did not produce
- [ ] **AC-14:** `revoke` retains the record rather than deleting it, so `identity_record` can still resolve a revoked DID
- [ ] **AC-15:** `store.json` records are sorted ascending by DID; two stores built in different write orders serialize byte-identically
- [ ] **AC-16:** The store never reads a clock — the timestamp that lands in `store.json` is the caller-supplied `now_unix` verbatim. Asserted by `tv_x_5`, which registers with two distinct `now_unix` values and checks that only the supplied one is persisted
- [ ] **AC-17:** `cli_fns::active_identity` (the free function) is `#[deprecated]` with its parent-RFC signature unchanged, and always returns `Err(WalletError::Locked)`. `WalletStore::try_active_identity` returns `Err(WalletError::Locked)` and carries no deprecation attribute
- [ ] **AC-18:** `cli_fns` wrappers — named individually, since no vector reaches them today — accept the unlocked handle and forward to the unlocked surface
- [ ] **AC-19:** `tv_x_1` through `tv_x_19`, `tv_x_21` through `tv_x_28`, `tv_x_31`, and `tv_x_32` all pass
- [ ] **AC-20:** `tv_x_17` and `tv_x_18` are negative-controlled individually — removing the zeroize call makes each fail
- [ ] **AC-21:** `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean
- [ ] **AC-22:** `cargo test -p octo-wallet --lib` green
- [ ] **AC-23:** `cargo fmt --check -p octo-wallet` clean
- [ ] **AC-24:** Layer discipline preserved — Layer B only, zero Layer A change
- [ ] **AC-25:** A full guide-executor run confirms the seven guide wall statements are now false (informational, and **not a criterion** — a guide-executor does not exist and this mission landing makes none of the seven false, since the guide update is the CLI mission's. Enumerated by anchor sentence in that mission's AC-19)

## Dependencies

Hard sequencing:

1. **RFC-0011-x must be Accepted** before this mission's substrate lands.
2. **This mission must land BEFORE `0011-x-wallet-store-cli`** — the CLI cannot thread an unlock through call sites until `unlock` exists.
3. **This mission supersedes the identity-store portion of `0102-a-wallet-foundation`**, which has been `claimed/` since 2026-07-20 with every acceptance criterion unchecked while several of the substrate items it describes is already landed. See §Notes.

### Type Coverage

Every type RFC-0011-x specifies, and which mission implements it. Nothing is unaccounted for, and nothing is deferred without a named owner.

| RFC type                                           | Layer | Implemented by                                                                                                                   |
| -------------------------------------------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------- |
| `WalletIndex`                                      | B     | This mission — NEW                                                                                                               |
| `WalletStore` (extended from a zero-sized struct)  | B     | This mission — gains `root`, `index`, `vault`; gains `unlock`, `select`; retains the parent-RFC `open()` signature               |
| `UnlockedWallet<'a>`                               | B     | This mission — NEW                                                                                                               |
| `WalletError::Locked`                              | B     | This mission — NEW variant                                                                                                       |
| `WalletError::IdentityNotFound(Did)`               | B     | This mission — NEW variant                                                                                                       |
| `WalletStore::try_active_identity` (return change) | B     | This mission — `NotActive` → `Err(Locked)`, no deprecation                                                                       |
| `cli_fns::active_identity` (deprecation sentinel)  | B     | This mission creates it; `0011-x-wallet-store-cli` deletes it                                                                    |
| `IdentityRecord`, `Did`, `LifecycleState`          | B     | **Reused unchanged** — RFC-0009 and RFC-0010 own them; this mission persists them and redefines none                             |
| `Vault`, `StarkliCompat`, `IdentityKey`            | B     | **Reused unchanged** — RFC-0102 owns them                                                                                        |
| `HsmAdapter`, `InMemorySigner`                     | B     | **Reused, handoff not wired** — §Future Work item 2; the branch point is named, the `HsmAdapter` impl does not exist             |
| `IdentityAction::{Register, Select, List}`         | C     | `0011-x-wallet-store-cli`                                                                                                        |
| `OctoCliError::WalletLocked` (slot 92)             | C     | `0011-x-wallet-store-cli`                                                                                                        |
| An authenticated store envelope                    | A/B   | **Not implemented by either mission** — §Future Work item 1. Listed so it is visibly unaccounted-for rather than silently absent |

### Implementation Guide

None. This mission has no companion implementation guide in `docs/07-developers/`. The RFC's §Detailed Design and §Store layout carry the specification, and the substrate it builds on is already documented in the guide's wallet sections. If one is written later, it belongs to the substrate mission that implements it, not to this YAML.

## Claimant

(none — Open mission)

## Pull Request

(none — not yet opened)

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

The framing this mission was written against was that closing `WalletStore` "means building an on-disk keystore, which is a substantial standalone Layer B crypto feature." That is false against the substrate. The on-disk keystore exists — `Vault` encrypts with Argon2id and AES-256-GCM, creates its directory at 0700, and is covered by tests including a wrong-passphrase rejection. `StarkliCompat` is a second, independently tested encrypted keystore. `IdentityKey` has a complete lifecycle with 41 tests. The only new dependency this mission adds is `dirs`, and it is not cryptographic.

The genuine gap is a wiring layer that was never written, plus the one thing no primitive can supply: somewhere to put an identity. A store with a reader and no writer is always empty, so `octo whoami` keeps exiting 2 and the guide's seven wall statements keep being true. The write-path is in scope for that reason and not because the crypto was missing.

The unlock split is the one genuinely new design decision in RFC-0011-x. The parent RFC's `open()` takes no passphrase while `active_identity` must return a usable key, and under a strict reading those two clauses are only jointly satisfiable by a store that reads a plaintext seed. RFC-0011-x chooses the locked/unlocked split instead, which is why metadata reads stay passphrase-free and only the signing paths need an unlock.
