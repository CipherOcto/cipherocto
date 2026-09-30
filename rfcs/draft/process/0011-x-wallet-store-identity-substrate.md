# RFC-0011-x: `WalletStore` — Identity Store Substrate, Registration Write-Path, and the Unlock Split

## Status

Draft (2026-09-30) — RFC-0011-x closes the last genuine open substrate gap in the `octo` identity surface. The parent RFC's `WalletStore` contract is implemented as a zero-sized struct that returns an empty store and an unconditional `NotActive`, so `octo whoami` exits 2 on every host regardless of operator state, and 13 CLI call sites consume that result. This amendment specifies the store's on-disk layout, the registration write-path, lifecycle persistence, and an **unlock split** that separates metadata reads (passphrase-free) from key access (passphrase-gated).

Amends RFC-0011 §Substrate-Additions item 1 and supersedes its `active_identity` placement clause. Additive at every other site. ONE new `WalletError` variant (`Locked`) + ONE new `WalletError` variant (`IdentityNotFound`) + ONE new `OctoCliError` variant (slot 92 `WalletLocked`, exit 92). Three new `octo identity` subcommands (`register`, `select`, `list`). Cross-RFC reference updates in RFC-0011, RFC-0011-f, RFC-0102, and RFC-0009. Paired with two new companion mission YAMLs per §Companion mission YAML pairing.

> **Not a layer change.** `octo-wallet` is Layer B (identity substrate, RFC-driven, additive-only). `octo-cli` is Layer C (per-RFC). Layer A is untouched — see §Appendices B.

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011 specifies a `WalletStore` in `octo-wallet` (Layer B) that opens an on-disk store, returns the active `IdentityKey`, resolves a record by DID, and enforces 0700 on creation. The shipped struct is `pub struct WalletStore;` — zero-sized — with `open()` returning an empty store and `try_active_identity()` returning `NotActive { current_state: Designated }` unconditionally. The `[ADD]` contract in the parent RFC is therefore satisfied in signature and violated in behaviour, and the guide documents the consequence at five separate locations.

This amendment is deliberately narrow about what it does **not** build. The encrypted on-disk primitives already exist and are already tested:

| Existing primitive                                                                                                           | Status                                | What this RFC does with it                                  |
| ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | ----------------------------------------------------------- |
| `Vault` — Argon2id + AES-256-GCM, 0700 slots dir, `put` / `get` / `list`                                                     | LANDED, tested                        | Reused unchanged. The identity seed becomes one vault slot. |
| `StarkliCompat` — Argon2id + chacha20-poly1305, `import` / `export`                                                          | LANDED, tested                        | Untouched. Interop format, not the native store.            |
| `IdentityKey` — `generate` / `from_seed` / `activate` / `begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` | LANDED, tested                        | Reused unchanged.                                           |
| `IdentityRecord`, `IdentityRotationEvent`                                                                                    | LANDED, serde-complete, **no writer** | Become the store's index payload.                           |
| `Did`, `LifecycleState`                                                                                                      | LANDED                                | Reused unchanged.                                           |

What does not exist is a **native identity store**: a DID-indexed record set plus an active-DID pointer, and any code path that writes either. Nothing in the workspace can put an identity into a store, so a read-only store would still resolve nothing and `octo whoami` would still exit 2. The registration write-path is therefore in scope, and it is the part that makes the rest observable.

### The unlock split

The parent RFC's signature is `WalletStore::open() -> Result<Self, WalletError>` with no passphrase parameter, while `active_identity(&self) -> Result<IdentityKey, WalletError>` must return a usable `IdentityKey`. Under an Accepted contract those two clauses can only be jointly satisfied if the filesystem permission boundary is the trust model — the store reads a plaintext seed and `0700` on the directory is the sole defence. This amendment rejects that reading and specifies a **locked / unlocked split** instead:

- `WalletStore::open()` keeps its signature and its metadata-only meaning. It reads the record index and the active-DID pointer. Both are public metadata — DIDs, public keys, lifecycle states, timestamps — and neither is secret, so neither needs a passphrase.
- `WalletStore::unlock(passphrase)` is new. It derives the Argon2id key from the passphrase, decrypts the seed slot, constructs an `IdentityKey`, zeroizes the plaintext buffer, and returns an `UnlockedWallet` handle that carries the key.

This means `octo identity show` and `octo identity list` need no passphrase at all, which is the property that keeps the migration tractable: of the 13 existing call sites, only the ones that actually sign must thread an unlock.

## Dependencies

**Requires:**

- RFC-0011 (parent RFC; the `WalletStore` `[ADD]` contract this amendment supersedes in part)
- RFC-0102 (wallet cryptography; supplies the Argon2id + AES-256-GCM storage primitive the seed slot reuses, and the passphrase-handling prohibition)
- RFC-0009 (identity evolution; supplies `LifecycleState` and the rotation state machine the store persists)

**Optional:**

- RFC-0011-f (a mirror row in its §Implicit Assumptions references `octo-wallet::WalletStore`; the cross-reference is updated but no normative text depends on it)
- RFC-0010 (canonical DID form; `Did` is already implemented against it and is unchanged here)

> **Dependency Validation Rules:**
>
> 1. Dependencies MUST form a DAG (no cycles)
> 2. All "Requires" RFCs MUST be listed as mission prerequisites
> 3. Optional dependencies MUST be documented separately from required
> 4. Dependencies on "Planned" RFCs MUST note the assumption they will be Accepted

All three required RFCs are Accepted.

## Design Goals

1. **Make the parent contract true.** `open()` reads a real store; `identity_record` resolves a real record; 0700 is enforced on creation rather than documented.
2. **Never write plaintext key material.** The seed is a `Vault` slot, encrypted with the crate's existing Argon2id + AES-256-GCM path. No new cryptography is introduced.
3. **Keep metadata reads passphrase-free.** Only key access is gated. A read-only inspection command must not prompt.
4. **Fail closed on an un-migrated call site.** A call site that still reaches for a key through a locked handle must produce a hard, attributable error — not a silently empty result.
5. **One store, one resolver.** `$OCTO_HOME` resolution follows the documented order used by the mesh substrate. It is specified normatively here so the two do not drift; consolidating the resolver into a shared primitive is deferred per §Future Work.

## Motivation

1. **The wall is a stub, not a boundary.** The guide states the wall in five places. Four state it outright — the store "is not implemented", two steps "cannot succeed until the wallet store lands" — and the fifth, the exit-2 troubleshooting entry, prescribes a remedy that cannot work for the same reason. All five describe a deliberate design decision. None of them is true: the store is zero-sized and the crypto it would need is already shipped and tested. The honest description is that a wiring layer was never written.
2. **A read-only store fixes nothing an operator can see.** `WalletStore` has no writer anywhere in the workspace. Adding a reader without a writer produces a store that is always empty, so `octo whoami` keeps exiting 2 and the guide's three statements stay true. The write-path is the minimum slice that changes observable behaviour.
3. **The current shape invites a plaintext seed.** An `open()` that takes no passphrase but must return a key has exactly one mechanical answer under the current contract. Shipping that answer puts the only unencrypted key material in a crate that Argon2ids everything else. That is a review finding waiting to happen, in a platform whose stated thesis is sovereign, private intelligence.
4. **Identity state is lost on every invocation.** `IdentityKey` is constructed per process and discarded. Rotation history exists as a type with no writer, so `IdentityRecord::rotation_history` is permanently empty and `octo identity show` cannot report a rotation chain it can see in memory but not on disk.

## Roles and Authorities

| Role                      | Authority                                                                                                                                                                                                      |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| RFC-0011 authors          | May amend the `[ADD]` contract in §Substrate-Additions item 1. RFC-0011-x exercises that authority for the `active_identity` placement clause only.                                                            |
| RFC-0102 authors          | Own the storage cryptography. RFC-0011-x selects no new primitive and changes no parameter, so no RFC-0102 amendment is required; the cross-reference update in §Cross-RFC reference updates is informational. |
| RFC-0009 authors          | Own the lifecycle state machine. RFC-0011-x persists transitions the state machine already defines and invents none.                                                                                           |
| Mission `0102-a` claimant | Retains authority over the wallet foundation mission. RFC-0011-x narrows that mission's scope; see §Companion mission YAML pairing.                                                                            |

## Detailed Design

### Store layout

Root resolves per §Home resolution and is created with mode 0700 on first write.

```text
$OCTO_HOME/wallet/            0700  dir
  store.json                  0600  public metadata: version, active_did, records[]
  seed/                       0700  dir  (Vault slots_dir)
    <did-slug>.vault          0600  Argon2id + AES-256-GCM, holds the 32-byte seed
```

`store.json` holds `WalletIndex`:

```rust
// crates/octo-wallet/src/identity_store.rs (NEW module)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WalletIndex {
    /// Schema version. Currently 1. A reader MUST reject an unknown version.
    pub version: u32,
    /// The active identity, or `None` when the store holds records but none is selected.
    pub active_did: Option<Did>,
    /// Records sorted ascending by `did` so `store.json` is byte-stable for a given store state.
    pub records: Vec<IdentityRecord>,
}
```

Records are sorted by DID on write. The alternative — insertion order — makes `store.json` a function of write history rather than of store state, which breaks byte-stability for a file the operator may version-control or diff.

`store.json` is deliberately **not** encrypted. It contains no secret: a DID is a public key, a public key is public by construction, and lifecycle state and timestamps are operator metadata. Encrypting it would buy nothing and would force a passphrase onto read-only inspection commands, defeating Design Goal 3.

### Home resolution

Resolution order, identical to the mesh substrate's documented order:

1. `$OCTO_HOME` if set **and non-empty**. An empty value is treated as unset and falls through — it must never produce an empty path.
2. `$HOME/.octo` via the platform home directory.
3. Otherwise `WalletError::Config`.

The store root is `$OCTO_HOME/wallet`. The parent RFC also names `~/.config/octo/wallet` as an alternative; this amendment resolves that to the order above, because the mesh substrate has already settled it and a second rule would be a second health-check system.

`octo-wallet` currently has no `dirs` dependency and no home resolver. This is a new dependency, declared with a rationale comment per the repository convention.

### `WalletStore` — locked handle

```rust
pub struct WalletStore {
    root: PathBuf,
    index: WalletIndex,
    vault: Vault,
}

impl WalletStore {
    /// Parent-RFC signature, UNCHANGED. Metadata only; never decrypts.
    pub fn open() -> Result<Self, WalletError>;

    /// Test seam. `WalletStore::open()` reads the ambient home; this takes it explicitly.
    pub fn open_at(root: impl Into<PathBuf>) -> Result<Self, WalletError>;

    /// Metadata. No passphrase.
    pub fn active_did(&self) -> Option<&Did>;

    /// Metadata. No passphrase. Sorted ascending by DID.
    pub fn list_records(&self) -> &[IdentityRecord];

    /// Metadata. No passphrase. `WalletError::IdentityNotFound` on miss.
    pub fn identity_record(&self, did: &Did) -> Result<IdentityRecord, WalletError>;

    /// Whether the active identity has a seed slot on disk.
    pub fn active_seed_slot_present(&self) -> bool;

    /// Reload `store.json` from disk, discarding the cached index.
    pub fn reload(&mut self) -> Result<(), WalletError>;

    /// MIGRATION SENTINEL — see §Migration sentinel. Always fails.
    pub fn active_identity(&self) -> Result<IdentityKey, WalletError>;
}
```

`open()` on a root that does not exist yields an **empty index**, not an error. A store with no records is a valid state and maps to the no-active-identity case the guide already documents, so failing to open would be a second failure mode with the same operator meaning. The 0700 directory is created on first **write**, not on open, so a read-only command against a non-existent store does not leave a directory behind.

### `unlock` — the key path

```rust
pub struct UnlockedWallet<'a> {
    store: &'a WalletStore,
    key: IdentityKey,
}

impl WalletStore {
    /// Decrypt the active identity's seed slot and return a key-bearing handle.
    pub fn unlock<'a>(
        &'a self,
        passphrase: &str,
        seed_out: &'a mut Vec<u8>,
    ) -> Result<UnlockedWallet<'a>, WalletError>;
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

    pub fn begin_rotation(
        &self,
        successor: IdentityKey,
        now_unix: u64,
    ) -> Result<[u8; 64], WalletError>;

    pub fn complete_rotation(&self, now_unix: u64) -> Result<(), WalletError>;
    pub fn abort_rotation(&self) -> Result<(), WalletError>;
    pub fn revoke(&self, now_unix: u64) -> Result<(), WalletError>;
}
```

`unlock` follows the existing `Vault::get` shape, which takes an explicit output buffer and returns a borrowing `DecryptedHandle` over it. The caller owns `seed_out`; that is what makes the zeroization obligation enforceable at a single site rather than smeared across callers.

The sequence inside `unlock`:

1. `active_did()` — `None` yields `WalletError::NotActive { current_state: Designated }`.
2. Slot slug derived from the DID. Slot not present on disk yields `WalletError::VaultSlotNotFound`; wrong passphrase yields `WalletError::VaultDecryptionFailed`. Both variants already exist.
3. `IdentityKey::from_seed` copies the 32 bytes into the key.
4. `seed_out` is zeroized before `unlock` returns, on **every** path including the error paths. A plaintext seed must not outlive the call regardless of outcome.

`seed_out` being a caller-owned buffer is the reason `unlock` cannot return the handle directly: an owned `Vec<u8>` behind a self-referential `DecryptedHandle<'a>` is not expressible without unsafe, and this crate has no business introducing that for a convenience.

`activate` is a parameter of `register` rather than a separate call so that a record can never be written in `Active` state without an explicit `IdentityKey::activate` transition having occurred. Registering without the flag stores `Designated` and clears the active pointer; registering with it stores `Active` and sets the active pointer.

### Migration sentinel

`WalletStore::active_identity` is **retained with its parent-RFC signature and always returns `Err(WalletError::Locked)`**, and is marked `#[deprecated(note = "...")]`.

The parent RFC requires this method on `WalletStore`. Removing it outright is the cleaner end state, but it converts a silent stub into 13 independent compile errors scattered across five CLI modules, and a partial migration would leave the store half-wired in a way no test enumerates. Retaining it as a deprecated always-failing method turns each un-migrated site into a warning at build time and a hard, attributable error at run time.

The end state still deletes it. It exists only for the migration window, and the companion mission carries a removal acceptance criterion so it cannot survive the migration.

### Error variants (Layer B)

Two new `WalletError` variants in `crates/octo-wallet/src/error.rs`:

```rust
/// The store is locked. `WalletStore::open` is metadata-only; the identity
/// seed requires `WalletStore::unlock(passphrase)`.
#[error("wallet store is locked; unlock with a passphrase to access the identity key")]
Locked,

/// No record for this DID in the store index.
#[error("no identity record for {0}")]
IdentityNotFound(Did),
```

`Locked` is not reachable from `WalletStore::unlock`, which is the operation that produces an unlocked handle. It is reachable from `WalletStore::active_identity` (the sentinel) and from any `cli_fns` wrapper still forwarding a locked handle.

Reused rather than added: `VaultDecryptionFailed` for a bad passphrase, `VaultSlotNotFound` for a missing slot, `NotActive { current_state }` for a store with no active identity, `AlreadyRevoked`, `RotationInProgress`, `NotRotating`, `SelfRotation`, `GracePeriodNotElapsed` — all of which the existing `IdentityKey` state machine already returns and all of which now propagate to disk instead of dying with the process.

### Error variant (Layer C)

ONE new `OctoCliError` variant, **slot 92**, the first slot above the 91 high-water mark:

```rust
/// The wallet store is locked and the operation needs the identity key.
#[error("wallet store is locked: unlock with a passphrase to continue")]
WalletLocked,
```

`WalletError::Locked` maps to it (exit 92). `WalletError::IdentityNotFound` maps to the **existing** `OctoCliError::NoSuchIdentity` at exit 4, which the parent RFC's `octo identity show` exit table already specifies — no new slot is spent on it.

Slots 92 through 99 were reserved for amendments beyond `-h`. This is that first amendment, and it takes the lowest free slot.

### CLI dispatch

`IdentityAction` gains three variants. The existing `Show`, `Rotate`, and `Revoke` keep their shapes and gain an unlock.

| Subcommand                                                                    | Substrate                         | Passphrase                            | Exit codes                |
| ----------------------------------------------------------------------------- | --------------------------------- | ------------------------------------- | ------------------------- |
| `octo identity register --seed-file <path> [--activate] [--passphrase-stdin]` | `UnlockedWallet::register`        | **yes** — the seed is being encrypted | 0, 2, 64                  |
| `octo identity select <did>`                                                  | `UnlockedWallet::select`          | no                                    | 0, 4, 64                  |
| `octo identity list [--json]`                                                 | `WalletStore::list_records`       | no                                    | 0, 64                     |
| `octo whoami` (existing)                                                      | `UnlockedWallet::active_identity` | **yes**                               | 0, 2, 92, 64              |
| `octo identity show <did>` (existing)                                         | `WalletStore::identity_record`    | no                                    | 0, 4, 64                  |
| `octo identity rotate` (existing)                                             | `UnlockedWallet::begin_rotation`  | **yes**                               | 0, 2, 3, 4, 5, 11, 92, 64 |
| `octo identity revoke` (existing)                                             | `UnlockedWallet::revoke`          | **yes**                               | 0, 2, 4, 92, 64           |

`register` takes a seed **file** rather than generating in-process. The guide's existing onboarding step already writes a 0600 seed file via `octo-wallet init --seed-out`, and composing with that step means the guide gains one command rather than a rewritten section. Generating in-process is available by passing the freshly generated seed through the same path.

`--passphrase-stdin` reads the passphrase from standard input for non-interactive contexts. When neither the flag nor an interactive terminal is available, the unlock fails with `WalletLocked` (exit 92) rather than hanging. A CLI that blocks forever on a hidden prompt in a cron job is a denial of service against the operator's own automation.

Passphrase acquisition follows the existing `octo-wallet` binary pattern: `rpassword` for the prompt, and never a command-line flag. A passphrase on `argv` is visible in the process table to every local user.

### Cross-RFC reference updates

| RFC                                    | Location                                                                                                                                                                              | Change                                                                                                                                                                                     |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| RFC-0011                               | §Substrate-Additions item 1                                                                                                                                                           | `active_identity` moves from `WalletStore` to `UnlockedWallet`; add `unlock`, `UnlockedWallet`, `WalletIndex`, `open_at`, `active_did`, `list_records`, `reload`. Record the supersession. |
| RFC-0011                               | §Substrate-Additions item 1, 0700 clause                                                                                                                                              | Mark satisfied — enforcement moves from documented to actual.                                                                                                                              |
| RFC-0011                               | §Error Handling, §Exit Codes                                                                                                                                                          | Slot 92 `WalletLocked`, exit 92.                                                                                                                                                           |
| RFC-0011                               | §Implicit Assumptions Audit, "Local file permissions on config dir are 0700"                                                                                                          | Row closes.                                                                                                                                                                                |
| RFC-0011-f                             | §Implicit Assumptions mirror row                                                                                                                                                      | Update the referenced API name from `WalletStore` to `UnlockedWallet`.                                                                                                                     |
| RFC-0102                               | §Key Storage                                                                                                                                                                          | Informational cross-reference: the identity seed is a `Vault` slot under the primitive this section already specifies. No normative change.                                                |
| RFC-0009                               | §Specification lifecycle subsections                                                                                                                                                  | Informational cross-reference: transitions are persisted by this store. No normative change.                                                                                               |
| `docs/06-operations/operator-guide.md` | Five wall statements: the §4 step 2a comment, the §`OctoCliError::NoActiveIdentity` troubleshooting entry, the `role select` step comment, and the §10 step 8 and §20 step 2 comments | Five locations updated to describe the working store. Enumerated in the companion CLI mission's AC-19.                                                                                     |

RFC-0102 and RFC-0009 receive cross-references only. The store introduces no new cryptographic primitive, no new parameter, and no new lifecycle transition, so neither RFC's normative text needs to move.

## Lifecycle Requirements

| Transition              | Trigger                                      | Persisted effect                                                   | On-disk state after                     |
| ----------------------- | -------------------------------------------- | ------------------------------------------------------------------ | --------------------------------------- |
| none → `Designated`     | `register(activate = false)`                 | Record appended; `active_did` untouched                            | Record present, no active               |
| none → `Active`         | `register(activate = true)`                  | Record appended; `active_did` set                                  | Record present and active               |
| `Designated` → `Active` | `IdentityKey::activate` via a future command | Record lifecycle updated                                           | Record present and active               |
| `Active` → `Rotating`   | `begin_rotation`                             | Record gains an `IdentityRotationEvent`; successor record appended | Both records present; old is `Rotating` |
| `Rotating` → `Active`   | `complete_rotation`                          | Successor activated; old record retains the rotation event         | Successor active, old still `Rotating`  |
| `Rotating` → `Active`   | `abort_rotation`                             | Rotation event dropped from the old record                         | Successor active, old restored          |
| `Active` → `Revoked`    | `revoke`                                     | Record lifecycle set terminal                                      | Record present, `Revoked`               |

A `Rotating` identity is persisted as `Rotating`, not as `Active` with a pending event. The state machine is already in the `IdentityKey`; the store's obligation is to record the state it reports rather than to re-derive it.

Revocation is terminal per the `LifecycleState` definition. The record is retained, not deleted, so `octo identity show` can still explain why a DID stopped working. Deleting it would make revocation indistinguishable from never having existed.

## Determinism Requirements

1. `store.json` records are sorted ascending by DID. Two stores holding the same record set serialize byte-identically regardless of write order.
2. `list_records` and `list` return records in that same order. A `BTreeMap` keyed by DID is the natural backing structure.
3. `version` is a plain integer with exactly one reader, so a version gate is a comparison and nothing more.
4. The slot slug is a pure function of the DID. The same DID always maps to the same filename, so `register` is idempotent for a re-registered DID.
5. Lifecycle timestamps are supplied by the caller as `now_unix`. The store never reads a clock, which keeps tests deterministic and keeps a write-path that cannot silently record a different instant than the one the caller signed over.

## Security Considerations

1. **No plaintext seed reaches disk.** The seed is encrypted through the existing `Vault` slot path — Argon2id with the crate's existing cost parameters, then AES-256-GCM. A store directory left on a lost laptop yields ciphertext.
2. **Plaintext seed lifetime is bounded and explicit.** `seed_out` is caller-owned so the zeroization obligation has one enforcement site. Zeroize on every return path.
3. **The store directory is 0700 and each file is 0600**, created on first write. A permissive mode on an existing tree is corrected on open, matching what the parent RFC specifies and what the mesh peer table already does.
4. **Passphrase never appears in `argv`.** Acquisition is `rpassword` or `--passphrase-stdin`. A passphrase flag would be readable by every local user through the process table.
5. **Metadata is public by design and contains no secret.** Encrypting `store.json` would create a false impression of confidentiality over a file whose entire content is derivable from the public key.
6. **A locked store is a type-level and runtime-level boundary.** There is no method on `WalletStore` that returns key material except the deprecated sentinel, which returns an error.
7. **Rotation events are verified before persistence.** `begin_rotation` and `complete_rotation` already validate the successor proof and the grace window; the store records the outcome rather than re-checking it, and never writes a record whose `signature_proof` it has not obtained from the state machine.

## Adversary Analysis

### A1 — Local user reads the seed from the store directory.

The seed is an AES-256-GCM slot under a 0700 directory. A local user outside the operator's account cannot traverse the directory. A process running **as** the operator can — but such a process can equally read the process's own memory, intercept the `rpassword` prompt, or read the operator's shell history. The passphrase defends against offline disk access and backup leakage, which is the realistic threat. It does not defend against a same-user attacker, and no filesystem design does.

### A2 — Attacker substitutes a `store.json`.

`store.json` is unencrypted and unauthenticated, so an attacker with write access to the store can rewrite the active-DID pointer or forge a record. This is a deliberate trade: signing uses the key from the seed slot, and the DID in a forged record would have to match the key's actual DID for a forged record to be used. `IdentityKey::did()` is derived from the public key, so a record whose `did` and `pubkey_bytes` disagree is detectable. The store rejects a record whose `did` does not match `IdentityKey::did()` at `register` time. Detecting post-hoc tampering with a legitimately-written record would require an authenticated envelope, which is deferred per §Future Work.

### A3 — Attacker replaces the seed slot with another identity's.

Replacing the slot changes the key that `unlock` yields. The resulting `IdentityKey` has a different DID, and `register` and `select` reconcile the index against the derived DID. A slot whose content does not match the index's `pubkey_bytes` for the active DID is rejected at unlock with `WalletError::Config`. The full mitigation — binding the ciphertext to the record — is the authenticated envelope in §Future Work.

### A4 — Rollback to an older `store.json`.

An attacker restoring a previous index resurrects a `Revoked` record as `Active`. The lifecycle state is not authenticated, so this is not detected. The same authenticated envelope closes it.

### A5 — Passphrase brute force.

Argon2id cost parameters are the crate's existing ones, chosen for the vault's threat model. The identity seed inherits that posture rather than raising it, so the store is exactly as strong as the vault beside it.

### A6 — Unlock prompt hangs in automation.

Covered in §CLI dispatch: `--passphrase-stdin` for non-interactive contexts, and `WalletLocked` when neither is available. The CLI never blocks indefinitely on a hidden prompt.

## Companion mission YAML pairing

Two new mission YAMLs, per the substrate-first ordering invariant:

| Mission                            | Layer             | Scope                                                                                                      |
| ---------------------------------- | ----------------- | ---------------------------------------------------------------------------------------------------------- |
| `0011-x-s-a-wallet-store-identity` | B (`octo-wallet`) | Store module, `WalletIndex`, unlock, registration, lifecycle persistence, error variants, 0700 enforcement |
| `0011-x-wallet-store-cli`          | C (`octo-cli`)    | Three new subcommands, unlock threading through the 13 existing call sites, slot 92 variant, guide update  |

Mission `0102-a-wallet-foundation` has been `claimed/` since 2026-07-20 with every acceptance criterion unchecked, while the substrate several of its criteria describe — the `IdentityKey` lifecycle, the capability-key derivation, the Starkli-compat keystore — is landed and tested. That mission's status is a bookkeeping drift, and RFC-0011-x narrows it: the identity-store and native-keystore portion moves to `0011-x-s-a-wallet-store-identity`; the remainder stays with `0102-a`. The two must not both claim the same substrate.

## Compatibility

1. **`WalletStore::open()` keeps its signature.** No caller outside `octo-cli` needs to change. RFC-0011's `open()` clause is satisfied, not superseded.
2. **`WalletStore::active_identity` keeps its signature and is deprecated, not removed.** It changes from "returns a stub error" to "returns a real error" — a behaviour change at a call site that must be migrated, which is the point.
3. **No Layer A change.** Nothing in this amendment touches canonical encoding, capability derivation, or any frozen wire format.
4. **No `OctoCliError` variant is removed or renumbered.** Slot 92 is additive above the 91 high-water mark.
5. **Existing stores.** There are none — the store has never been written to. A store directory that exists but lacks `store.json` is treated as an empty index rather than an error, so a hand-made directory does not brick the CLI.
6. **The guide's five wall statements become false** once the store lands and are updated in the same change. A guide that keeps describing a wall the code no longer has is worse than no guide. The five locations are enumerated in the companion CLI mission's AC-19; two of them are textually near-identical and sit in different sections, so a single find-and-replace does not reach both.

## Implicit Assumptions Audit

| #   | Assumption                                                               | Status                                                            | If false                                                |
| --- | ------------------------------------------------------------------------ | ----------------------------------------------------------------- | ------------------------------------------------------- |
| 1   | The store root is writable by the operator                               | Unverified                                                        | `WalletError::Io`. The CLI surfaces it as exit 64.      |
| 2   | `$HOME` or `$OCTO_HOME` is set                                           | Verified — mesh substrate already fails closed on this            | `WalletError::Config`.                                  |
| 3   | Local file permissions on the config dir are 0700                        | **Closes** — this amendment makes it enforced rather than assumed | n/a                                                     |
| 4   | The vault's Argon2id cost parameters are adequate for the identity seed  | Unverified — inherited from the vault's threat model              | Slots for a separate RFC-0011-y; not bundled here       |
| 5   | `store.json` tampering by a same-user process is out of scope            | Accepted for this slice                                           | Adversaries A2 through A4 become live; see §Future Work |
| 6   | `IdentityKey::from_seed` does not retain a reference to the input buffer | **Verified required** — the zeroization in `unlock` depends on it | `unlock` cannot zeroize; the design changes             |

## Test Vectors

Names follow the amendment-chain convention `tv_x_{N}`.

| Vector    | Asserts                                                                           |
| --------- | --------------------------------------------------------------------------------- |
| `tv_x_1`  | `open()` on a non-existent root yields an empty index, not an error               |
| `tv_x_2`  | `open()` creates no directory; the 0700 directory appears only on first write     |
| `tv_x_3`  | First write creates the root at 0700 and `store.json` at 0600                     |
| `tv_x_4`  | A pre-existing permissive mode on the root is corrected to 0700 on open           |
| `tv_x_5`  | `register` then `reload` round-trips the record through `store.json`              |
| `tv_x_6`  | `register` with `activate = false` leaves `active_did` as `None`                  |
| `tv_x_7`  | `register` with `activate = true` sets `active_did`                               |
| `tv_x_8`  | `register` rejects a record whose DID does not match `IdentityKey::did()`         |
| `tv_x_9`  | `register` is idempotent for a re-registered DID — same slot slug, one record     |
| `tv_x_10` | `store.json` is byte-identical for two stores built by different write orders     |
| `tv_x_11` | `list_records` returns ascending-DID order                                        |
| `tv_x_12` | `identity_record` on a miss returns `IdentityNotFound`                            |
| `tv_x_13` | `active_identity` and `list_records` need no passphrase                           |
| `tv_x_14` | `unlock` with a wrong passphrase returns `VaultDecryptionFailed`                  |
| `tv_x_15` | `unlock` with a missing slot returns `VaultSlotNotFound`                          |
| `tv_x_16` | `unlock` with no active DID returns `NotActive { current_state: Designated }`     |
| `tv_x_17` | `unlock` zeroizes `seed_out` on the success path                                  |
| `tv_x_18` | `unlock` zeroizes `seed_out` on the wrong-passphrase path                         |
| `tv_x_19` | `unlock` on a store whose slot content contradicts the index DID returns `Config` |
| `tv_x_20` | `select` moves the active pointer; `select` on a miss returns `IdentityNotFound`  |
| `tv_x_21` | `begin_rotation` persists a `Rotating` record and a successor record              |
| `tv_x_22` | `complete_rotation` persists the successor as active                              |
| `tv_x_23` | `abort_rotation` drops the rotation event and restores the predecessor            |
| `tv_x_24` | `revoke` persists a terminal record that survives reload                          |
| `tv_x_25` | A revoked record is retained, not deleted                                         |
| `tv_x_26` | `$OCTO_HOME` set and non-empty wins over `$HOME`                                  |
| `tv_x_27` | `$OCTO_HOME` set but empty falls through to `$HOME`, never yielding an empty path |
| `tv_x_28` | Neither set yields `WalletError::Config`                                          |
| `tv_x_29` | `WalletError::Locked` maps to `OctoCliError::WalletLocked` and exit 92            |
| `tv_x_30` | `WalletError::IdentityNotFound` maps to exit 4, minting no new slot               |
| `tv_x_31` | The deprecated `WalletStore::active_identity` always returns `Locked`             |
| `tv_x_32` | A `store.json` with an unknown `version` is rejected rather than parsed           |

`tv_x_1` through `tv_x_32` are the substrate vector set. The CLI mission carries its own vectors for the three new subcommands and the unlock threading, negative-controlled per call site — a single un-migrated call site is invisible to a suite that only exercises the migrated ones.

## Alternatives Considered

### Alt 1 — 0700-trust: no passphrase, plaintext seed

Keep the parent RFC's signature and read a 0600 seed file directly. Zero new error variants, zero call-site changes, fastest to land.

Rejected. It is the only mechanical answer to the parent contract, and it puts the only unencrypted key material in a crate that Argon2ids everything else. It also makes the guide's security posture untrue: a platform whose thesis is sovereign, private intelligence would document that any process running as the operator can read its identity key. The cost of the alternative is one deprecated method, two error variants, and an unlock at the sites that sign.

### Alt 2 — HSM-first, no software seed on disk

Route key access through the existing `HsmAdapter` trait that `IdentityKey::signer()` already returns, and never write a seed at all.

Deferred, not rejected — it is the correct long-term answer. It is blocked on a real local `HsmAdapter` implementation, which does not exist, and building one is larger than this amendment. The unlock split is compatible with it: `unlock` is where the HSM handoff would branch, and a record carrying `hsm_slot: Some(n)` would route there instead of decrypting a slot. `IdentityRecord` already has the field.

### Alt 3 — Store only, no write-path

Land the reader and leave `WalletStore` a read-only view of a store nothing can write.

Rejected. The store has no writer anywhere in the workspace, so this lands a component that is always empty. `octo whoami` keeps exiting 2 and the guide's five wall statements stay true. It moves the wall rather than closing it.

### Alt 4 — Reuse `StarkliCompat` as the store format

The keystore already does Argon2id, chacha20-poly1305, `import`, and `export`, and it round-trips `IdentityKey`.

Rejected. It is an interop format for an external ecosystem, deliberately diverging from the native vault's AES-256-GCM. A record index, an active-DID pointer, and multi-identity rotation history are not that format's model. Using it would couple the native store to an external ecosystem's schema, which is the definition of a parallel abstraction.

## Implementation Phases

### Phase 1: Store module + unlock

`WalletIndex`, `WalletStore` state, home resolution, 0700 enforcement, `open` / `open_at`, metadata readers, `unlock`, `UnlockedWallet`, the two `WalletError` variants, `tv_x_1` through `tv_x_19` and `tv_x_26` through `tv_x_28` and `tv_x_32`.

### Phase 2: Write-path

`register` / `select`, DID reconciliation, `tv_x_5` through `tv_x_12`, `tv_x_20`.

### Phase 3: Lifecycle persistence

`begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` persisting through the state machine, `tv_x_21` through `tv_x_25`.

### Phase 4: CLI dispatch

The three new subcommands, unlock threading through the 13 call sites, the slot 92 variant, the sentinel removal, the guide update, `tv_x_29` through `tv_x_31` plus the CLI mission's own vectors.

Phases 1 through 3 are the substrate mission; phase 4 is the CLI mission. Substrate lands first per the substrate-first ordering invariant.

## Key Files to Modify

| File                                                                  | Layer | Change                                                        |
| --------------------------------------------------------------------- | ----- | ------------------------------------------------------------- |
| `crates/octo-wallet/src/identity_store.rs`                            | B     | NEW — the store, the index, the unlock                        |
| `crates/octo-wallet/src/identity_record.rs`                           | B     | `WalletStore` gains state; the sentinel is added then removed |
| `crates/octo-wallet/src/error.rs`                                     | B     | `Locked`, `IdentityNotFound`                                  |
| `crates/octo-wallet/src/lib.rs`                                       | B     | Module declaration and re-exports                             |
| `crates/octo-wallet/Cargo.toml`                                       | B     | `dirs` and `zeroize`, each with a rationale comment           |
| `crates/octo-wallet/src/cli_fns.rs`                                   | B     | Wrappers take the unlocked handle                             |
| `crates/octo-cli/src/error.rs`                                        | C     | Slot 92 `WalletLocked`, translation arm, exit arm             |
| `crates/octo-cli/src/lib.rs`                                          | C     | Three `IdentityAction` variants                               |
| `crates/octo-cli/src/commands/identity.rs`                            | C     | Handlers, output envelopes, unlock threading                  |
| `crates/octo-cli/src/commands/{governance,vault,agent,capability}.rs` | C     | Unlock threading at the remaining call sites                  |
| `docs/06-operations/operator-guide.md`                                | docs  | Five wall statements replaced                                 |

## Future Work

1. **Authenticated store envelope.** A2, A3, and A4 are real and open. A MAC over the index binding it to the seed ciphertext closes all three. Deferred because it is a new format decision with its own review, and bundling it would make the wire-format change look incidental to a wiring change.
2. **HSM handoff.** Branch `unlock` on `IdentityRecord::hsm_slot`. The field already exists; the `HsmAdapter` implementation does not.
3. **Home resolver consolidation.** The resolution order is now specified normatively in two places. The right end state is one shared primitive, which would remove the possibility of drift this amendment has to prevent by fiat.
4. **Argon2id cost review for identity seeds.** Inherited from the vault; may deserve its own parameters.
5. **Remove the sentinel.** Once no call site references it, the deprecated `WalletStore::active_identity` is deleted. Carried as an acceptance criterion in the CLI mission so it cannot outlive the migration.
6. **Guide executor.** A full guide-executor becomes buildable once a command can provision a node. Out of scope here; this amendment is a precondition, not the enabler.

## Rationale

The store is a wiring layer over shipped primitives, and treating it as a cryptographic build is what kept it open. `Vault` already encrypts. `IdentityKey` already generates and rotates. `IdentityRecord` already serializes. The gap is a struct that holds a path, a Vault handle, and an index, plus the write-path that populates it.

The unlock split is the one genuinely new decision, and it is a security-posture choice rather than a mechanical one. Both the parent's no-passphrase signature and a 0700-trust store are internally consistent; which one is correct depends on whether the threat model includes offline disk access, and for a platform whose thesis is private, sovereign intelligence the answer is yes it does.

Phasing write-path into the same slice as the reader is deliberate. The alternative — a reader now, a writer later — produces a component that is always empty, which is the failure mode this amendment exists to end.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                  |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-09-30 | Initial draft. Store layout, home resolution, `WalletIndex`, the locked/unlocked split, registration write-path, lifecycle persistence, two `WalletError` variants, slot 92 `WalletLocked`, three new subcommands, 32 substrate test vectors, cross-RFC reference updates in four RFCs plus the guide, and two companion mission YAMLs. |

## Related RFCs

- RFC-0011 — parent CLI substrate RFC; this amendment supersedes the `active_identity` placement clause of its §Substrate-Additions item 1
- RFC-0011-f — carries a mirror row in its §Implicit Assumptions referencing `octo-wallet::WalletStore`
- RFC-0102 — wallet cryptography; supplies the storage primitive the seed slot reuses
- RFC-0009 — identity evolution; supplies the lifecycle state machine the store persists

## Related Use Cases

- An operator registers an identity, selects it, and then runs identity-gated CLI commands without a prompt storm
- An operator rotates an identity, restarts the process, and reads the rotation chain from `octo identity show`
- An operator revokes an identity and later investigates why a DID stopped working
- A full operator-guide executor, which becomes buildable once a command can provision a node

## Appendices

### A. Substrate-faithfulness verification

Every API this amendment specifies against already exists in `octo-wallet`, verified against the substrate rather than the parent RFC's prose:

| Specified                                                                                      | Verified in substrate                                             | Note                                                                                                                                                                |
| ---------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Vault::put` / `get` / `list`                                                                  | LANDED                                                            | Argon2id + AES-256-GCM; 0700 on the slots dir; `get` takes an explicit `out: &mut Vec<u8>` and returns a borrowing `DecryptedHandle<'a>`                            |
| `IdentityKey::from_seed`                                                                       | LANDED                                                            | Takes `[u8; 32]` by value, so the seed is copied and the buffer is free to zeroize                                                                                  |
| `IdentityKey::activate` / `begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` | LANDED                                                            | The full state machine                                                                                                                                              |
| `IdentityKey::did`                                                                             | LANDED                                                            | Derived from the public key — the basis of the `tv_x_8` and `tv_x_19` reconciliation checks                                                                         |
| `IdentityRecord` field set                                                                     | LANDED                                                            | `{ did, pubkey_bytes, lifecycle, hsm_slot, registered_at_unix, rotation_history }`, serde-complete with `serde_bytes_32` and `serde_lifecycle_state` adapters       |
| `IdentityRotationEvent` field set                                                              | LANDED                                                            | Distinct from the `RotationEvent` in `vault_rotation`                                                                                                               |
| `Did`, `LifecycleState`                                                                        | LANDED                                                            | `LifecycleState` is `Designated` / `Active` / `Rotating` / `Revoked` with `Revoked` terminal                                                                        |
| `WalletError` reuse set                                                                        | LANDED                                                            | `VaultDecryptionFailed`, `VaultSlotNotFound`, `NotActive`, `AlreadyRevoked`, `RotationInProgress`, `NotRotating`, `SelfRotation`, `GracePeriodNotElapsed` all exist |
| `OctoCliError::NoSuchIdentity` at exit 4                                                       | Verified against the parent RFC's `octo identity show` exit table | No new slot spent                                                                                                                                                   |
| `rpassword` prompting                                                                          | LANDED                                                            | Already the `octo-wallet` binary's pattern                                                                                                                          |

Two corrections to the framing this amendment was written against, both established by reading the substrate rather than the parent RFC's prose:

1. The store has **13** real call sites, not 18. `WalletStore::open` appears 16 times in `octo-cli`, of which 3 are doc comments — two in `identity` describing the `map_wallet_open_error` helper, one in `agent`. The 13 real calls are spread across `governance` (3), `identity` (4), `vault` (1), `agent` (1), and `capability` (4).
2. `WalletStore` has a **third** method the parent RFC does not mention, `lookup_identity_record`. The sentinel migration must account for it.

### B. Layer A frozen check

Layer A is untouched. The store selects an existing Layer B primitive, adds no canonical encoding, derives no capability identity, and changes no wire format. The `dqa`-style amount types are unrelated. The only Layer C change is an additive error variant above the current high-water mark.

### C. Slot arithmetic summary

| Slot   | Variant                       | State                                |
| ------ | ----------------------------- | ------------------------------------ |
| 2      | `NoActiveIdentity`            | LANDED, reused                       |
| 4      | no-such-identity              | LANDED, reused by `IdentityNotFound` |
| 91     | `NetworkKeyRotationUnknownId` | LANDED (highest existing)            |
| **92** | **`WalletLocked`**            | **NEW in this RFC**                  |
| 93–99  | —                             | Remain free                          |

`WalletError::IdentityNotFound` deliberately spends **no** slot: the parent RFC's `octo identity show` exit table already reserves exit 4 for "no such identity", and minting a parallel slot for a case the parent already names would be a duplicate vocabulary.
