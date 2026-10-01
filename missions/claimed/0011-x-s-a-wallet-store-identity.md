# 0011-x-s-a-wallet-store-identity — Substrate additions for the `WalletStore` identity store

<!-- Machine-readable ordering per BLUEPRINT §Mission Lifecycle. The prose
     under §Dependencies states the same three gates; this block is the field an
     agent reads to decide what to claim first. Empty on the substrate side
     deliberately: nothing mission-side precedes the substrate, and the two gates
     that do apply (RFC-0011-x Accepted, supersession of 0102-a) are RFC and
     document gates rather than other missions.
     An earlier revision cited "§Mission Dependency Model", which names no
     section: that string is a bold run-in label inside the Mission template, not
     a heading, so it resolves to nothing.
-->

depends_on: []

## Status

Claimed (2026-10-01) — Substrate companion to RFC-0011-x. Layer B (`octo-wallet`). RFC-0011-x was promoted Draft → Accepted at `next 16a3368f`; the RFC gate is therefore met and the substrate-first ordering invariant now allows the substrate to land. The paired CLI mission `0011-x-wallet-store-cli` waits for this mission's substrate to land before it claims, per the substrate-first ordering invariant. Phase 1 lands the mission claim plus three new `WalletError` variants (`Locked`, `IdentityNotFound(Did)`, `WeakPassphrase`) per the mission's AC-6; Phases 2-3 land the on-disk store, the unlock split, the lifecycle persistence, the `IdentityKey::from_seed_with_lifecycle` rehydration path, and the forty-two substrate test vectors. Claimant: @cipherocto.

## RFC

RFC-0011-x §Detailed Design, §Lifecycle Requirements, §Determinism Requirements, §Test Vectors.

## Summary

Turns `WalletStore` from a zero-sized struct into a real on-disk identity store, and adds the registration write-path that nothing in the workspace currently provides.

The encrypted primitives this needs already exist and are already tested in `octo-wallet`: `Vault` (Argon2id + AES-256-GCM, 0700 slots dir, `put` / `get` / `list`), `IdentityKey` (generate, `from_seed`, `activate`, `begin_rotation`, `complete_rotation`, `abort_rotation`, `revoke`), and `IdentityRecord` / `IdentityRotationEvent` (serde-complete, no writer). **No new cryptography is introduced and no new dependency is required at all** — not even a non-cryptographic one. `Vault::default_dir()` already resolves the home directory through `directories`, and the store reuses that call rather than adding a second home crate (RFC-0011-x §Home resolution).

What does not exist is the layer that ties them together: a DID-indexed record set, an active-DID pointer, a home resolver, and any code that writes either. The parent RFC's `[ADD]` contract is satisfied in signature and violated in behaviour, and the operator guide documents the consequence in **eleven claims across seven locations**, enumerated by anchor sentence in the companion CLI mission's AC-26.

Scope note: this mission covers the store, the unlock, and the write-path. The CLI surface, the 13 call-site migrations, the three `OctoCliError` variants at slots 92, 93, and 94, and the guide update belong to `0011-x-wallet-store-cli`.

### Substrate additions target

```rust
// crates/octo-wallet/src/identity_store.rs (NEW module)
use crate::error::WalletError;
use crate::identity::IdentityKey;
use crate::identity_record::{Did, IdentityRecord};
use crate::vault::{DecryptedHandle, Vault};

// NO `PartialEq`. `IdentityRecord` derives only `Debug, Clone, Serialize, Deserialize`,
// and `Vec<IdentityRecord>: PartialEq` therefore does not hold, so a derived
// `PartialEq` here is E0369 — a hard compile error, reproduced with `rustc` rather
// than reasoned about. An earlier revision of this block derived `PartialEq` and this
// mission's §Out of Scope forbade adding it to `IdentityRecord`, so the spec as
// written could not build. The vectors that need index comparison compare **bytes**,
// which is what byte-stability actually means, and `serde_json::to_vec` needs no
// `PartialEq`. `Did` does derive `PartialEq`, so `active_did` would have been fine;
// the failure is the `records` vector alone.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub fn reload(&mut self) -> Result<(), WalletError>;

    /// Crash-state reporters. Neither repairs anything; both name a state the
    /// store should report rather than create. See RFC-0011-x
    /// §Write ordering and the two half-written states.
    /// False when the index names a record whose slot file is gone.
    pub fn active_seed_slot_present(&self) -> bool;
    /// Slot files present in the vault that no record names. A crash between
    /// seal and index write leaves one. The store REPORTS these and does not
    /// adopt them into the index — reconciling forward would invent a record
    /// for ciphertext whose passphrase nobody may still hold.
    pub fn orphan_slots(&self) -> Result<Vec<String>, WalletError>;

    /// Move the active pointer. A pure index write: it reads and writes
    /// `store.json` and touches no key material, so it needs no unlock.
    /// `&mut self` because it writes the index; the CLI is one-shot, so a
    /// mutable store across a single command is not a burden.
    /// Returns `NotActive { current_state: Revoked }` on a terminal record.
    pub fn select(&mut self, did: &Did) -> Result<(), WalletError>;

    /// BOOTSTRAP WRITE PATH. Creates a record and seals its seed slot.
    /// Takes `&mut self` because it appends to the index and writes the slot.
    /// Does **not** require an existing active identity — this is the only
    /// way a store can ever acquire its first record, so requiring one here
    /// would make the store unpopulatable. Returns `AlreadyRevoked` when
    /// `key.did()` names a record already in the terminal state.
    pub fn register(
        &mut self,
        key: IdentityKey,
        passphrase: &str,
        activate: bool,
        now_unix: i64,
    ) -> Result<Did, WalletError>;

    /// Decrypt the active identity seed slot. `seed_out` is caller-owned so the
    /// zeroization obligation has exactly one enforcement site.
    ///
    /// Seven ordered steps; the load-bearing ones are that the DID derived from
    /// the seed is reconciled against the index, and that the key's lifecycle
    /// AND rotation start time are REHYDRATED FROM THE RECORD rather than
    /// inherited from `IdentityKey::from_seed`, which hard-codes `Designated`
    /// and leaves `rotation_started_at_unix_secs` at `None`. A naive rehydrate
    /// resurrects a revoked identity as a signing key, and a naive one for a
    /// `Rotating` record panics inside `complete_rotation`.
    pub fn unlock<'a>(
        &'a mut self,
        passphrase: &str,
        seed_out: &'a mut Vec<u8>,
    ) -> Result<UnlockedWallet<'a>, WalletError>;
}

pub struct UnlockedWallet<'a> {
    // `&'a mut`, not `&'a`. Every method below mutates the index and the key,
    // so a shared reference cannot express any of them without interior
    // mutability. A type carrying a plain `&'a WalletStore` and offering
    // `revoke(&self)` describes a program that does not compile.
    store: &'a mut WalletStore,
    key: IdentityKey,
}

impl<'a> UnlockedWallet<'a> {
    pub fn active_identity(&self) -> Result<IdentityKey, WalletError>;
    pub fn identity_record(&self, did: &Did) -> Result<IdentityRecord, WalletError>;

    /// `passphrase` is here because `begin_rotation` **seals the successor's
    /// slot**, exactly as `register` does. A rotation that appends a successor
    /// record without sealing it produces a store whose `active_did` names an
    /// identity that can never be unlocked. This is the seal-versus-unlock
    /// split applied a second time.
    pub fn begin_rotation(
        &mut self,
        successor: IdentityKey,
        passphrase: &str,
        now_unix: u64,
    ) -> Result<[u8; 64], WalletError>;
    /// MUST NOT `expect` the rotation start time. `IdentityKey::complete_rotation`
    /// currently reads it through `.expect("invariant: Rotating implies
    /// rotation_started_at_unix_secs is Some")`; a `Rotating` record rehydrated
    /// without that field panics with a Rust backtrace and exit 101.
    pub fn complete_rotation(&mut self, now_unix: u64) -> Result<(), WalletError>;
    /// Restores the predecessor, appends no successor record, and leaves the
    /// already-sealed successor slot on disk as an orphan — the one way an
    /// orphan slot arises without a crash.
    pub fn abort_rotation(&mut self) -> Result<(), WalletError>;
    pub fn revoke(&mut self, now_unix: u64) -> Result<(), WalletError>;
}
```

`register` and `select` sit on `WalletStore` together, and that placement is the fix for a deadlock rather than a stylistic choice. An earlier draft put `register` on `UnlockedWallet`, which makes the store unpopulatable: the only way to obtain an `UnlockedWallet` is `unlock`, and the only way to `unlock` is to have an active record with a sealed slot, so on an empty store the bootstrap call is unreachable and every later vector against a populated store is unsatisfiable. Sealing a slot needs a passphrase and never needs an unlocked key, so the two index writers that read no active key belong on the store.

`register` gains a `passphrase` parameter that `select` does not, and that difference is the seal-versus-unlock distinction stated as a signature.

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

**Three** additions to `WalletError` in `crates/octo-wallet/src/error.rs` — the count is stated here once and AC-6 is bound to it:

```rust
use crate::identity_record::Did;

/// The store is locked. `WalletStore::open` is metadata-only; the identity
/// seed requires `WalletStore::unlock(passphrase)`.
#[error("wallet store is locked; unlock with a passphrase to access the identity key")]
Locked,

/// No record for this DID in the store index.
#[error("no identity record for {0}")]
IdentityNotFound(Did),

/// A supplied passphrase is below the enforced floor. Carries no detail of
/// the passphrase itself, and none of the store's contents.
#[error("passphrase is below the {MIN_PASSPHRASE_CHARS}-character floor")]
WeakPassphrase,
```

`error.rs` gains exactly one import, shown above. `Did` is local to this crate — defined in
`identity_record.rs` and already imported by that path in `role_nonce.rs` and `agent.rs` — so
the variant needs no new dependency edge, and this is the **first** `Did`-typed payload in
`WalletError`; the existing thirty-three carry six payload types — `String` at fifteen sites,
`Uuid` at three, `LifecycleState` at two, `usize` at two, `HsmError` once, and `std::io::Error`
once — with the remaining nine unit-like, and two further types appear only as struct fields
rather than as a variant's payload: `AgentState` in the transition variant and `u64` in the
grace-period variant. An earlier revision of this sentence named five types (`String`, `Uuid`,
`AgentState`, `LifecycleState`, and `std::io::Error`) as though exhaustive, which omitted
`usize` and `HsmError` and mixed the field-only `AgentState` in with the payload types; the
substrate carries the payload types named here, not the earlier five.

`MIN_PASSPHRASE_CHARS` is **declared by this mission**, in `identity_store.rs`, at `12` — the
value the wallet foundation mission's criterion named in 2026-07 and never implemented. It is
`pub` because the `#[error]` attribute interpolates it, and `thiserror` expands that attribute
into a `write!` against the error's scope, so an undeclared or unimported identifier is a
compile error at the definition of `WalletError` — a file with no passphrase policy in it. The
constant lives beside the check that reads it, and the `Display` message interpolates the same
constant the check compares against, so the sentence an operator reads cannot drift from the
number that produced it.

`WeakPassphrase` is a hard error at **both** `register` and `unlock`. An earlier revision of
this section specified two variants and put a **non-blocking warning** at `register`, so
that a returning operator with a weak existing passphrase would not be locked out. The
split was withdrawn, and the reason is in RFC-0011-x §Future Work item 7: a floor at
`unlock` is the same lockout arriving one command later with no escape hatch, and
§Compatibility already establishes that no pre-existing identity stores exist to strand.
A generic `Config(String)` is not an acceptable substitute — the operator would see a
configuration bug rather than a policy decision, and the CLI's translation table has no
arm for a policy refusal.

Reused, not added: `VaultDecryptionFailed` (bad passphrase), `VaultSlotNotFound` (missing slot), `NotActive { current_state }` (no active identity), `AlreadyRevoked`, `RotationInProgress`, `NotRotating`, `SelfRotation`, `GracePeriodNotElapsed`, `InvalidSuccessorProof` — all already returned by the `IdentityKey` state machine. Nine in total, and AC-20 names five of them as the lifecycle refusals that spend slot 93.

`IdentityNotFound` is worth singling out, because its name is not invented here. The doc comment on the existing `WalletStore::lookup_identity_record` already says the stub's `NotActive` is a placeholder and that the real implementation "will use a dedicated `IdentityNotFound` variant in a follow-on". This mission is that follow-on, and the variant is named identically. The name was therefore chosen to match what the substrate has been asking for, which is the cheap way to keep a stub's stated intent and its eventual replacement in agreement. `Locked` and `WeakPassphrase` have no such anticipation and are genuinely new.

`Locked` and `IdentityNotFound`, however, change what callers see, and that is where the migration cost actually lives. `try_active_identity` returns `NotActive` unconditionally today, and `lookup_identity_record` returns `NotActive` for any unregistered DID, so `NotActive` is currently the answer at every call site and callers have been written to match on it. Replacing it at both paths means the arm that used to be total stops being total — which is exactly why the companion CLI mission specifies the sweep against the 26 sites that reach the key rather than against the sites that mention the two method names. The naming precedent is a small comfort, not a reduction in work.

### Dependencies

**None added.** `crates/octo-wallet/Cargo.toml` is unchanged by this mission.

| Crate                                                          | Status                                                                                                                                                                                                                                                                                            |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `directories`                                                  | **Already used.** `Vault::default_dir()` calls `ProjectDirs::from(..)`. The store's home resolution reuses that call. No declaration change.                                                                                                                                                      |
| `zeroize`                                                      | **Already declared** with a rationale comment, and already used by four modules. The `seed_out` zeroization in `unlock` reuses the existing declaration.                                                                                                                                          |
| `argon2`, `aes-gcm`, `chacha20poly1305`, `serde`, `serde_json` | Already declared and reused unchanged.                                                                                                                                                                                                                                                            |
| `dirs`                                                         | **Not added.** An earlier revision of this section added it. `ProjectDirs` cannot express `$OCTO_HOME`, which is what the addition was for — but `$OCTO_HOME` is read from the environment before any library is consulted, so the second crate was solving a problem the resolver does not have. |
| `rpassword`                                                    | Already declared in `crates/octo-wallet/Cargo.toml`, but this mission does not use it. Prompting belongs to `0011-x-wallet-store-cli`, and `rpassword` is **not** currently a dependency of `octo-cli`.                                                                                           |

The `dirs` row is the one that earns its place here. It would have put **two
home-directory crates in one crate for one job**, in a mission whose own RFC section
warns about exactly that hazard two paragraphs above it as a parallel abstraction. The
principle is CLAUDE.md's "no parallel abstractions", and the clause is not "prefer the
first crate you find" — it is "do not invent a second one". `dirs` exists in the
workspace only in `crates/octo-mesh`, which is a different crate and a different layer.

One caveat the reuse does **not** remove, and it is a substrate fact rather than a
design choice: on Linux `dirs::home_dir()` is `var_os("HOME")` with no emptiness filter,
and `directories::BaseDirs` does not filter one either. An empty `HOME` therefore yields
the relative path `.octo` and the store writes key material into the operator's working
directory. The `!is_empty()` filter is **this mission's obligation**, not a library's, and
AC-3 carries it.

### Migration sentinel

Three symbols are in play and conflating them is what made the first draft of this section unsound. `WalletStore` has three methods: `open`, `try_active_identity`, and `lookup_identity_record`. It has **no** `active_identity` method. The measured call sites invoke the **free function** `octo_wallet::active_identity(&WalletStore)`, implemented in `cli_fns`, so a `#[deprecated]` on `WalletStore::active_identity` would emit zero warnings — because nothing calls it.

An earlier revision of this section went further and attributed the free-function form to the parent RFC's §Subcommand Taxonomy item 1. **It does not say that.** RFC-0011 states `active_identity` in two incompatible forms at two sites: as a method at §Subcommand Taxonomy item 1, and as a free function at the §`octo whoami` substrate row. The parent is internally inconsistent about its own symbol, and this amendment supersedes both. The conclusion does not depend on which site is cited, and the citation is written out here precisely so a reviewer checking item 1, finding a method, does not conclude the argument was fabricated: the deprecation belongs on the free function because the **measured** call sites call the free function, and 15 is a count of what the code does rather than of what a clause says.

The sentinel is therefore two changes, and the deprecation belongs on the free function:

1. `cli_fns::active_identity` is **retained with its parent-RFC signature and always returns `Err(WalletError::Locked)`**, marked `#[deprecated(note = "… use WalletStore::unlock …")]`. Its signature is unchanged precisely so the deprecation warning fires at every un-migrated site.
2. `WalletStore::try_active_identity` changes from returning an unconditional `NotActive` to returning `Err(WalletError::Locked)`. No deprecation attribute, per the note above.

Why keep a failing function at all. Deleting it outright is the cleaner end state, but it converts a silent stub into 15 independent compile errors across five CLI modules, and a partial migration would leave the store half-wired in a way no test enumerates. Retaining it as a deprecated always-failing function turns each un-migrated site into a **warning at build time** — a real, attributable signal, because the deprecation is on the symbol the sites actually call — and a hard, attributable error at run time.

`WalletStore::lookup_identity_record` is the substrate's third method and the parent RFC does not mention it. It is a metadata reader, needs no unlock, and is **not** part of the sentinel. It survives unchanged. It has exactly **one** call site — `cli_fns::identity_record` in `crates/octo-wallet/src/cli_fns.rs`, a Layer B file — and **zero** call sites in `octo-cli`, which reaches it only transitively. The sweep therefore belongs to **this** mission, not the CLI one, and AC-32 carries it. An earlier revision put the sweep in the CLI mission's AC-2 while simultaneously requiring the method's deletion, which contradicted this section and left the single real call site unassigned.

The CLI mission carries the sentinel's removal as an acceptance criterion so it cannot outlive the migration. **This mission does not remove it.**

## Test Vectors

Per RFC-0011-x §Test Vectors. Substrate vectors are `tv_x_1` through `tv_x_19`, `tv_x_21` through `tv_x_28`, `tv_x_31` through `tv_x_42`, and `tv_x_45` through `tv_x_47` — **42 of 48**. The CLI mission carries the remaining six, `tv_x_20`, `tv_x_29`, `tv_x_30`, `tv_x_43`, `tv_x_44`, and `tv_x_48`, plus its own per-call-site vectors. **42 + 6 = 48, and the six are exactly the CLI phase's vectors** — the ownership split and the phase split are the same split, so a reader can check either against the other. This figure was **44** in an earlier revision, against a range ending at `tv_x_47`; that range swallowed `tv_x_43` and `tv_x_44`, which this same file declares CLI-owned further down, so 44 and 6 were naming 50 vectors out of 48.

Three membership calls in that partition are non-obvious. The five lifecycle-persistence vectors `tv_x_21` through `tv_x_25` are substrate-owned even though they sit in the middle of the numbering — the persistence they assert happens entirely below the dispatch layer. Leaving them unowned would leave AC-13 and AC-14 with no vector able to catch a violation of either. `tv_x_31` is substrate-owned for the same kind of reason: it asserts that the deprecated `cli_fns::active_identity` free function always returns `Locked`, which is a property of a function **this** mission creates. The CLI mission originally claimed it, while simultaneously requiring the sentinel to be deleted — two postconditions that cannot both hold.

**Seventeen** vectors carry a security or durability property and each is negative-controlled: `tv_x_3`, `tv_x_4`, `tv_x_17`, `tv_x_18`, `tv_x_33` through `tv_x_42`, and `tv_x_45` through `tv_x_47`. **The range is split on purpose, and the split is where the count comes from.** Written as one sweep from `tv_x_33` through `tv_x_47` it is fifteen wide, and adding the four named before it gives nineteen — but that unbroken range crosses the ownership boundary this section draws above, because `tv_x_43` and `tv_x_44` belong to the companion mission. A count that silently sweeps in another mission's vectors is not a count of this mission's set. Bounded on both sides, the range is thirteen, and with the four named before it, seventeen — which is what the **fourteen** bullets below name, vector for vector.

**This sentence has now been wrong twice, and both errors were the same kind.** The revision before last said ten. The revision that replaced ten said **nineteen** vectors and **thirteen** bullets, and gave as its reason that ten had matched neither. So the number that displaced ten was also not the number the bullets give, and the sentence asserting that it had measured them was the sentence that was wrong — in three separate figures, none of which the enumeration supports. The two derivations now agree independently, which is the only reason to believe either: count the bullets and their named vectors, or count the bounded ranges, and both land on seventeen. It is also not the RFC's separate count of eleven for `tv_x_38` through `tv_x_48`, which is a narrower set defined by a different rule — those eleven are the ones that exist because a stated property had no vector, while the set here is every vector asserting a security or durability property. The two figures are about different things and are not reconcilable; naming both is what keeps a reader from subtracting one from the other. A vector whose wrong implementation is not named is a vector that may not be able to fail:

- `tv_x_17` / `tv_x_18` — `unlock` zeroizes `seed_out` on the success path and on the wrong-passphrase path. Negative control: remove the zeroize call, confirm `tv_x_18` fails. A test that only covers the success path would not notice a missing zeroize on the error path, which is the path an attacker probing passphrases actually exercises.

  `tv_x_18` must **pre-poison** `seed_out` with a known sentinel before calling `unlock` with a wrong passphrase, and assert the sentinel is gone afterwards. A wrong passphrase fails inside `Vault::get` before anything is written to the buffer, so on a naive implementation `seed_out` is never touched — and a test that passes an empty buffer and checks it is still empty would pass whether or not the zeroize call exists. That is the shape a vacuous security test takes, and it is why the negative control is mandatory here rather than optional.

- `tv_x_3` / `tv_x_4` — mode enforcement. Negative control: drop the `set_permissions` call, confirm `tv_x_4` fails.

- `tv_x_33` / `tv_x_34` — lifecycle rehydration. `IdentityKey::from_seed` hard-codes `Designated`, so a naive `unlock` rehydrates every key as `Designated` and a revoked identity signs again. `tv_x_33` revokes, drops the handle, reopens, and requires `NotActive { current_state: Revoked }`; `tv_x_34` asserts the rehydrated key's `lifecycle()` equals the record's persisted lifecycle for each of `Designated`, `Active`, and `Revoked`. Both fail against `from_seed` alone. Negative control: `tv_x_33` on its own is satisfied by a rehydration that refuses everything, so require a `Designated` record to unlock and yield a `Designated` key — the refusal has to be attributable to the persisted lifecycle, not to a blanket one.

- `tv_x_35` — `AlreadyRevoked` on re-registration. A11 retains the seed slot after revocation, so without the guard an operator re-registers the same seed and gets a working identity back from a record that was supposed to be terminal. Negative control: drop the `AlreadyRevoked` check and confirm both the error and the byte-identity assertions fail. The byte check covers **both** `store.json` and the slot file. `Vault::put` regenerates the salt and nonce on every call, so a seal-then-refuse implementation rewrites the revoked identity's ciphertext while reporting "nothing written" — and an assertion on `store.json` alone would pass, because the index is a different file.

- `tv_x_36` — `NotActive { current_state: Revoked }` on `select`. Without it the active pointer can be moved onto a terminal record, which is the same bypass as `tv_x_35` by a different door. Negative control: `select` a record that is neither revoked nor terminal and require it to succeed, so the refusal is discriminating rather than a blanket refusal of every `select`.

- `tv_x_37` — the cross-handle read. Every other persistence vector in this set is written as "…then `reload`" or "survives reload", which is a read against a **live handle**. A store that writes `store.json` and whose `reload()` is `Ok(())` passes all of them while never reading a byte. This one drops the store, reopens with `open_at`, and compares; then it edits the file behind the handle and requires `reload` to change what is reported. Negative control: run the same comparison against a handle that was never dropped, and require it to report the pre-edit value — otherwise a store that ignores the file entirely and answers from memory would satisfy the reopened comparison whenever the edit happened not to matter.

- `tv_x_38` — seed lifetime across a dropped handle. An `UnlockedWallet` over a caller-owned, uniquely-owned buffer is dropped and the buffer reads as all-zero. Negative control: hand out a clone instead of the unique buffer, or drop the zeroize call. This is the only vector that distinguishes a handle that owns the seed from one that merely borrows it, and `IdentityKey` being `Clone` with a live `Arc` signer is exactly the case where an implementation would.

- `tv_x_39` — the documented no-lock behaviour, pinned. Two handles, each registering a different identity, with a barrier forcing both to read the index before either writes: **exactly one record survives**, both calls return `Ok`, and neither takes a file lock. Negative control: take `flock(LOCK_EX)`, which serialises the two and leaves both records present. This is the right direction for this vector's subject — a future locking amendment should fail it, and RFC-0011-x §Future Work item 16 says so.

- `tv_x_40` — the rotation timestamp round-trips and `complete_rotation` does not `expect`. `begin_rotation`, drop, reopen, `unlock`, `complete_rotation`: the successor becomes active, the predecessor returns to `Active` deprecated, and the process does not panic. Negative control: rehydrate through `from_seed`, whose missing `rotation_started_at_unix_secs` makes `complete_rotation` hit `.expect(...)` and exit 101. `tv_x_33` and `tv_x_34` cover the same defect for **lifecycle**; the two fields fail independently, because `from_seed` gets lifecycle wrong by hard-coding and rotation time wrong by omission, and a fix for one is not a fix for the other.

- `tv_x_41` — `begin_rotation` seals the successor. `active_seed_slot_present()` is true for the successor slug before `complete_rotation`, and after `abort_rotation` the successor is not selectable while its slot is still on disk as an orphan. Negative control: append the successor record and leave the slot unsealed.

- `tv_x_42` — the passphrase floor is a hard error at **both** sites. Negative control: warn at `register` and error only at `unlock`, which is the split RFC-0011-x §Future Work item 7 withdrew. The message must carry no character of the supplied passphrase and no path.

- `tv_x_45` — `active_seed_slot_present()` returns `false` after the slot file is deleted behind the index, and `unlock` then returns `VaultSlotNotFound` rather than a decryption failure. Negative control: a store that checks only the index and reports the store as unlocked.

- `tv_x_46` — an orphan slot is **reported** by `orphan_slots()` and is **not** adopted into the index. Negative control: reconcile the index forward and invent a record for the ciphertext. This is the one direction that is genuinely wrong — a slot whose passphrase nobody may still hold must not become a record.

- `tv_x_47` — the generated slug passes `validate_slot_id`, and the validator is reachable. The slug is `identity-` plus lowercase hex of `key.public_key_bytes()`: 73 characters, inside the 128 cap, every character inside `[a-zA-Z0-9._-]`. Negative control: derive the slug from the DID and hex-encode it — 146 characters, which `validate_slot_id` rejects, and every `register` fails with `InvalidSlotId`. `validate_slot_id` is module-private in `vault.rs` today, so it must be promoted to `pub(crate)`; a store that re-implements the check instead is asserting the same rule through a second implementation, which is the same defect as the home resolver.

`tv_x_43` and `tv_x_44` are CLI-observable and owned by the companion mission.

## Acceptance Criteria

- [ ] **AC-1:** `crates/octo-wallet/src/identity_store.rs` exists and is declared plus re-exported from `crates/octo-wallet/src/lib.rs`
- [ ] **AC-2:** `WalletStore` holds `root`, `index`, and `vault`; it is no longer a zero-sized struct
- [ ] **AC-3:** `WalletStore::open()` keeps its exact parent-RFC signature, and its resolution is the same four steps as `octo-cli/src/home.rs`: `$OCTO_HOME` when set **and non-empty**; an **empty** `OCTO_HOME` is a hard error rather than a fall-through; `$HOME/.octo` when `OCTO_HOME` is unset entirely; otherwise `WalletError::Config`. The store root is `$OCTO_HOME/wallet`. The three-step arrow form this criterion used to carry could not distinguish "empty" from "unset", which is the whole point the empty-is-an-error rule turns on. **The resolved `$HOME` is filtered with `!is_empty()` by this mission, not delegated to a library** — neither `directories::BaseDirs` nor `dirs::home_dir` filters an empty value on Linux, and an unfiltered empty `HOME` yields the relative path `.octo`, which writes key material into the operator's working directory. Asserted by `tv_x_27` and `tv_x_28`. **Step 4 is unreachable through the `octo` binary and this mission does not make it reachable.** Steps 2 and 4 are exactly the two conditions `octo-cli/src/home.rs::resolve` already rejects, and every `octo` command calls that resolver first, so exit 27 is produced upstream and the store is never opened. This matters beyond bookkeeping: `WalletError::Config` is a catch-all with **sixteen** construction sites in `vault.rs` — one of them a directory error, fifteen of them crypto or serialisation failures — so a `Config` → 27 arm would print "set `$OCTO_HOME` or `$HOME`" for a full disk or a failed Argon2 hash. The directory-error site is `vault.rs:127` (`"no default config directory"`); the fifteen crypto/serialisation sites span the argon2 paths (params/hash/encode/decode), the aes-gcm encrypt path, the vault serialisation, the nonce decode + length check, the base64 length check, and four base64-validity checks (`vault.rs:152, 164, 169, 178, 188, 236, 246, 251, 255, 257, 347, 353, 354, 359, 365`). The substrate's `agent.rs` adds four more sites (`agent.rs:339, 481, 520, 554`, all `"agent registry mutex poisoned"`) which are not in `vault.rs` and therefore not part of the count. An earlier revision of this sentence named the total as twelve with eleven crypto/serialisation, an arithmetic that pointed at neither the one-directory-error nor the fifteen-crypto/serialisation split. The catch-all conclusion is unchanged; only the enumeration under it was wrong. RFC-0011-x §Home resolution step 4 carries the enumeration; the CLI mission's `Config` obligation is the absence of an arm, and it is an obligation to write nothing
- [ ] **AC-4:** `open()` on a non-existent root yields an empty index, not an error, and creates no directory
- [ ] **AC-5:** The 0700 root and 0600 `store.json` are created on first write, and a pre-existing permissive mode is corrected on open
- [ ] **AC-6:** **Three** `WalletError` variants are added with doc comments: `Locked`, `IdentityNotFound`, `WeakPassphrase`. The count is three, not two — an earlier revision of this criterion named two while the section above already carried a third, and the two places disagreed
- [ ] **AC-7:** `WalletError::IdentityNotFound` mints **no `OctoCliError` slot** — it maps to the existing `OctoCliError::IdentityNotFound(String)` at exit 4, so the CLI's slot table is unchanged for it. `WalletError` carries its exit codes in per-variant **doc comments** rather than in a table, so "adds a variant" and "adds an exit code" are two separate edits and each new variant's comment must name the exit it maps to: `Locked` → 92, `WeakPassphrase` → 2
- [ ] **AC-8:** `crates/octo-wallet/Cargo.toml` is **unchanged**. `Vault::default_dir()` already calls `ProjectDirs::from(..)`, and the store reuses it. `dirs` is **not** added — that would put two home-directory crates in one crate for one job, in the same mission that names the hazard. `zeroize` is **not** added: it is already a declared direct dependency with a rationale comment and is already used by four modules
- [ ] **AC-9:** `unlock` decrypts through `Vault::get`, reconciles the derived DID against the index, rehydrates the key through the new `IdentityKey::from_seed_with_lifecycle` so the persisted lifecycle is the one the key carries, refuses a record whose `can_sign()` is false, and zeroizes `seed_out` on **every** return path
- [ ] **AC-10:** `unlock` errors are `NotActive` for no active DID, `VaultSlotNotFound` for a missing slot, `VaultDecryptionFailed` for a bad passphrase, `Config` when the slot content contradicts the index DID, `WeakPassphrase` for a passphrase below the floor, and `NotActive { current_state: <recorded state> }` for a non-signing record. Each is a **named** variant that the CLI can match, so a translation arm can distinguish them — never a collapsed `Err(_)`
- [ ] **AC-11:** `register` is idempotent for a re-registered DID — the slot filename is the slug the key's own bytes produce, one record results, and the lifecycle is updated in place rather than reset. It takes **no DID parameter**: the DID is derived from the key, so no input can make it disagree with the key's own identity, and an earlier version of this criterion asserted a rejection path that the signature cannot reach. The reconciliation that _is_ reachable — a slot whose content contradicts the index — belongs to `unlock` and is `AC-10`'s `Config` case. The slug is defined in AC-29
- [ ] **AC-12:** `register` persists `Designated` and **leaves any existing `active_did` untouched** when `activate` is false; persists `Active` and sets the pointer when true. Note the word _leaves_, not _clears_: on a store that already has an active identity, `activate = false` must not move the operator off the identity they were using. Asserted by `tv_x_6`, which sets up a store that already has an active DID — a fresh store's `active_did` is `None` because nothing was ever active, so a `register` that wrote nothing at all would pass
- [ ] **AC-13:** `begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` persist the state machine's own outcome and never write a record the state machine did not produce
- [ ] **AC-14:** `revoke` retains the record rather than deleting it, so `identity_record` can still resolve a revoked DID
- [ ] **AC-15:** `store.json` records are sorted ascending by DID; two stores built in different write orders serialize byte-identically
- [ ] **AC-16:** The store never reads a clock — the timestamp that lands in `store.json` is the caller-supplied `now_unix` verbatim. Asserted by `tv_x_5`, which registers with two distinct `now_unix` values and checks that only the supplied one is persisted
- [ ] **AC-17:** `cli_fns::active_identity` (the free function) is `#[deprecated]` with its parent-RFC signature unchanged, and always returns `Err(WalletError::Locked)`. `WalletStore::try_active_identity` returns `Err(WalletError::Locked)` and carries no deprecation attribute
- [ ] **AC-18:** `cli_fns` wrappers — named individually, since no vector reaches them today — accept the `UnlockedWallet` handle and forward to the unlocked surface. They take `&mut UnlockedWallet<'_>`, not a shared borrow, because every method they forward to takes `&mut self`
- [ ] **AC-19:** `tv_x_1` through `tv_x_19`, `tv_x_21` through `tv_x_28`, `tv_x_31` through `tv_x_42`, and `tv_x_45` through `tv_x_47` all pass — the **42** substrate-owned vectors, each with the negative control named in §Test Vectors
- [ ] **AC-20:** `tv_x_17` and `tv_x_18` are negative-controlled individually — removing the zeroize call makes each fail
- [ ] **AC-21:** `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean
- [ ] **AC-22:** `cargo test -p octo-wallet --lib` green
- [ ] **AC-23:** `cargo fmt --check -p octo-wallet` clean
- [ ] **AC-24:** Layer discipline preserved — Layer B only, zero Layer A change
- [ ] **AC-25:** A full guide-executor run confirms the guide wall statements are now false (informational, and **not a criterion** — a guide-executor does not exist and this mission landing makes none of them false, since the guide update is the CLI mission's. There are **eleven claims across seven locations**, not seven statements; enumerated by anchor sentence in that mission's AC-26)
- [ ] **AC-26:** The two guards that keep a revoked record terminal are both present: `register` on a revoked DID returns `AlreadyRevoked` and leaves `store.json` byte-identical, and `select` on a revoked DID returns `NotActive { current_state: Revoked }`. A11 retains the seed slot, so without the first guard an operator re-registers the same seed and gets a working identity back from a record that was meant to be terminal; the second closes the same bypass by a different door
- [ ] **AC-27:** The test exercising the deprecated `cli_fns::active_identity` free function carries an explicit `#[allow(deprecated)]`. Without it `tv_x_31` cannot be written, and without that attribute AC-21's clippy gate fails on this mission's own sentinel test
- [ ] **AC-28:** The 12-character passphrase floor is enforced as a **hard `WalletError::WeakPassphrase` at both** `unlock` and `register`, and at no other point. The floor is **`pub const MIN_PASSPHRASE_CHARS: usize = 12` declared in `identity_store.rs`**, and the check at both sites compares against that constant rather than a literal — a second copy of the number is the same defect as a second home resolver, and the `#[error]` message interpolates the same constant, so the sentence and the threshold cannot drift apart. Asserted by `tv_x_42`. The wallet foundation mission `0102-a` wrote the criterion in 2026-07 and filed it at `init`, which never receives a passphrase, so it was unenforceable where it sat; RFC-0011-x §Future Work item 7 carries it here, at the real enforcement point. **An earlier revision of this criterion made it a non-blocking warning at `register` and a hard error at `unlock`, on the theory that a floor at `register` would strand a returning operator with a weak existing passphrase. The split is withdrawn**: a floor at `unlock` is the same lockout arriving one command later with no escape hatch, and RFC-0011-x §Compatibility establishes that no pre-existing identity stores exist to strand. See §Notes
- [ ] **AC-29:** The seed slot's filename is `identity-` + lowercase hex of `key.public_key_bytes()` — 73 characters — and it **passes `validate_slot_id`**. That validator is module-private in `crates/octo-wallet/src/vault.rs` and must be promoted to `pub(crate)`. The store must **not** re-implement the rule: a second implementation of the same check is the same defect as a second home resolver. Asserted by `tv_x_47`. The DOB-derived form is what the earlier revisions specified, and it is rejected: hex-encoding a 73-character DID gives 146 characters, and `validate_slot_id` caps slot ids at 128, so every `register` would fail `InvalidSlotId` and the bootstrap path would be unreachable
- [ ] **AC-30:** The lifecycle table is implemented for **every** edge, not only the two the parent RFC states. `register` never demotes: `Active → Active` writes no lifecycle change, and a re-registration with `activate = true` leaves an already-`Active` record `Active`. `Rotating → Rotating` is a no-op on the record and a second `begin_rotation` call. `Rotating → Revoked` is a **real** edge — `LifecycleState::can_transition_to` admits `(Active | Rotating, Revoked)` — and it leaves the successor record orphaned, so `revoke` on a `Rotating` identity must still persist both records rather than only the predecessor. The predecessor is `deprecated` and the successor `Designated`
- [ ] **AC-31:** `begin_rotation` takes a `passphrase` and **seals the successor's slot**, because it appends a successor record whose slot must be reachable. A rotation that writes the record without the slot produces a store whose `active_did` names an identity that can never be unlocked. Asserted by `tv_x_41`
- [ ] **AC-32:** `IdentityKey::complete_rotation` **does not `expect`** the rotation start time. It currently reads `rotation_started_at_unix_secs` through `.expect("invariant: Rotating implies rotation_started_at_unix_secs is Some")`; a `Rotating` record rehydrated without that field panics with a Rust backtrace and **exit 101**. The refusal is `WalletError::NotRotating { current_state }`, the existing variant that fits. A `debug_assert` is not a substitute — it is compiled out of release builds, which is where a wallet runs. Asserted by `tv_x_40`
- [ ] **AC-33:** `IdentityKey::from_seed_with_lifecycle` takes **five** parameters: `(seed, lifecycle, activated_at, revoked_at, rotation_started_at)`. Four are not enough. `unlock` reconstructs `rotation_started_at` from the newest event in the record's own `rotation_history` when the persisted lifecycle is `Rotating`, and passes `None` otherwise — **no new field is added to `IdentityRecord`**, because `IdentityRotationEvent::started_at_unix` already carries it. Asserted by `tv_x_40`
- [ ] **AC-34:** `WalletStore` takes **no lock**, and this is stated rather than implied. `Vault` takes none anywhere in the workspace — no `flock`, no `fcntl`, no `fs2`/`FileExt` — and the model is single-writer, last-writer-wins. An earlier revision of the RFC's performance table claimed "no lock contention beyond the process", which asserts a lock the implementation does not hold. Asserted by `tv_x_39`, whose negative control is `flock(LOCK_EX)`. The lock is deferred to RFC-0011-x §Future Work item 16 with the substrate mission named as owner
- [ ] **AC-35:** `register`'s write ordering is normative: **guards first before any write**, then seal the slot, then write the index via write-to-temp / `sync_all` / `rename` — the same sequence `Vault::put` uses, not a `File::create` truncate-and-write. A refused `register` writes nothing at all, including no re-encryption of the revoked identity's slot. `active_seed_slot_present()` detects the index-over-a-missing-slot state, and `orphan_slots()` reports a slot no record names **without adopting it into the index**. Asserted by `tv_x_35`, `tv_x_45`, and `tv_x_46`
- [ ] **AC-36:** `IdentityRecord` gains a `deprecated: bool` field so a completed rotation is visible in the index rather than inferable. The field is **additive** with a serde default, so a `store.json` written before this change still parses. `tv_x_22` asserts the predecessor returns to `Active` **and** is marked deprecated
- [ ] **AC-37:** The single `WalletStore::lookup_identity_record` call site — `cli_fns::identity_record` in `crates/octo-wallet/src/cli_fns.rs` — is swept. The method is **retained**; only the deprecated `cli_fns::active_identity` free function is deleted, and that deletion is the CLI mission's criterion. An earlier revision of the CLI mission's AC-2 required this sweep while also requiring the method's deletion, which contradicted itself and left the one real call site unassigned
- [ ] **AC-38:** The `begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` surface leaves an operator a **way out of `Rotating`** in both directions, and `abort_rotation` is the only path that returns a sealed successor slot to an orphan. Without both, a `rotate` leaves the identity half-alive — `can_sign` admits `Rotating`, so it still signs — which is harder to notice and worse to hand an operator than a hard failure. Asserted by `tv_x_41`
- [ ] **AC-39:** An `UnlockedWallet` hands the caller a **unique** `IdentityKey` and does not clone it. `IdentityKey` is `Clone` with a live `Arc<dyn HsmAdapter>` signer, and for `InMemorySigner` the clone holds the raw seed bytes — so a clone extends the seed's life past the handle. Asserted by `tv_x_38`

## Dependencies

Hard sequencing:

1. **RFC-0011-x must be Accepted** before this mission's substrate lands. It is **Draft** today, so this gate is live and unmet, and the mission does not claim otherwise.
2. **This mission must land BEFORE `0011-x-wallet-store-cli`** — the CLI cannot thread an unlock through call sites until `unlock` exists.
3. **This mission supersedes the identity-store portion of `0102-a-wallet-foundation` — proposed, not yet in effect.** `0102-a` has been `claimed/` since 2026-07-20 with every acceptance criterion unchecked while several of the substrate items it describes are already landed. The supersession does **not** take effect until RFC-0011-x is `Accepted`, because BLUEPRINT requires an approved RFC before a mission carries scope; until then the identity store is claimed by `0102-a` _and_ specified here, and the overlap is deliberate and temporary. The supersession covers the **identity-store portion only**: `0102-a`'s seven unchecked `StarkliCompat` boxes under its own §Starkli-compat keystore have no receiver here, because this mission reuses `StarkliCompat` unchanged and forbids changing it. See §Notes.

Required RFCs, per BLUEPRINT §Dependency Validation Rules rule 2 — every "Requires" entry on RFC-0011-x is a prerequisite here:

- **RFC-0011** — the parent CLI substrate RFC whose §Subcommand Taxonomy item 1 and §`octo whoami` substrate row this amendment supersedes. Its `active_identity` placement clause is the contract being corrected.
- **RFC-0102** — owns the storage cryptography. This mission reuses the shipped `Vault`, `StarkliCompat`, and `IdentityKey` unchanged and mints no primitive, so no RFC-0102 amendment is required; the dependency is for provenance, not for a pending change.
- **RFC-0009** — owns the lifecycle state machine. This mission persists transitions the machine already defines and invents none, so it adds no state.

All three are Accepted. RFC-0011-f and RFC-0010 are **optional** to this mission: RFC-0011-f carries a mirror row in its §Implicit Assumptions and RFC-0010 owns the canonical DID form that `Did` is already implemented against. Neither is a prerequisite, and nothing in this mission's acceptance criteria depends on either.

### Type Coverage

Every type RFC-0011-x specifies, and which mission implements it. Nothing is unaccounted for, and nothing is deferred without a named owner.

| RFC type                                                                | Layer | Implemented by                                                                                                                                                                                                                                                                                                                         |
| ----------------------------------------------------------------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WalletIndex`                                                           | B     | This mission — NEW                                                                                                                                                                                                                                                                                                                     |
| `WalletStore` (extended from a zero-sized struct)                       | B     | This mission — gains `root`, `index`, `vault`; gains `unlock`, `register`, `select`, `active_seed_slot_present`, `orphan_slots`; retains the parent-RFC `open()` signature. `register` and `select` take `&mut self` and need no unlocked key, so they live on the store rather than on `UnlockedWallet`                               |
| `UnlockedWallet<'a>`                                                    | B     | This mission — NEW                                                                                                                                                                                                                                                                                                                     |
| `WalletError::Locked`                                                   | B     | This mission — NEW variant                                                                                                                                                                                                                                                                                                             |
| `WalletError::IdentityNotFound(Did)`                                    | B     | This mission — NEW variant                                                                                                                                                                                                                                                                                                             |
| `WalletError::WeakPassphrase`                                           | B     | This mission — NEW variant, hard error at both `register` and `unlock`                                                                                                                                                                                                                                                                 |
| `IdentityKey::from_seed_with_lifecycle`                                 | B     | This mission — NEW, `pub(crate)`, **five** parameters. `from_seed` hard-codes `Designated` and leaves `rotation_started_at_unix_secs` at `None`, so rehydrating through it resurrects a revoked one and panics on the next `complete_rotation`. The only legitimate caller is the rehydration step inside `unlock`                     |
| `IdentityKey::complete_rotation` (panic removal)                        | B     | This mission — the `.expect(...)` on the rotation start time is replaced by `WalletError::NotRotating { current_state }`                                                                                                                                                                                                               |
| `IdentityRecord`                                                        | B     | **Reused, plus one additive field** — `deprecated: bool`, so a completed rotation is visible in the index. Serde-defaulted so a pre-change `store.json` still parses. The struct otherwise is unchanged: it derives `Debug, Clone, Serialize, Deserialize` and **not** `PartialEq`, which is why `WalletIndex` cannot derive it either |
| `Did`, `LifecycleState`                                                 | B     | **Reused unchanged** — RFC-0009 and RFC-0010 own them; this mission persists them and redefines none                                                                                                                                                                                                                                   |
| `Vault` (`validate_slot_id`)                                            | B     | This mission — `validate_slot_id` promoted from module-private to `pub(crate)`. The store composes it rather than re-implementing the 128-character cap and the `[a-zA-Z0-9._-]` character class                                                                                                                                       |
| `StarkliCompat`                                                         | B     | **Reused unchanged** — RFC-0102 owns it                                                                                                                                                                                                                                                                                                |
| `WalletStore::try_active_identity` (return change)                      | B     | This mission — `NotActive` → `Err(Locked)`, no deprecation                                                                                                                                                                                                                                                                             |
| `WalletStore::lookup_identity_record`                                   | B     | **Reused unchanged**, and its single call site is swept here. `IdentityRecord` remains reader, not a sentinel                                                                                                                                                                                                                          |
| `cli_fns::active_identity` (deprecation sentinel)                       | B     | This mission creates it; `0011-x-wallet-store-cli` deletes it                                                                                                                                                                                                                                                                          |
| `HsmAdapter` and its impls                                              | B     | **Reused unchanged, handoff not wired** — RFC-0011-x §Future Work item 2. The trait has three shipped impls, `InMemorySigner`, `LedgerSigner`, and `NullSigner`; what does not exist is the `IdentityRecord::hsm_slot` branch inside `unlock`, and this mission names that branch point without implementing it                        |
| `IdentityAction::{Register, Select, List, RotateComplete, RotateAbort}` | C     | `0011-x-wallet-store-cli`                                                                                                                                                                                                                                                                                                              |
| `OctoCliError::WalletLocked` (slot 92)                                  | C     | `0011-x-wallet-store-cli`                                                                                                                                                                                                                                                                                                              |
| `OctoCliError::IdentityTransitionRefused` (slot 93)                     | C     | `0011-x-wallet-store-cli` — five `WalletError` rotation refusals that today fall into the generic `Internal` arm                                                                                                                                                                                                                       |
| `OctoCliError::WeakPassphrase` (slot 94)                                | C     | `0011-x-wallet-store-cli` — the character floor reaching a named variant at **exit 2**, and no existing exit-2 variant reused, because all **seven** render a sentence about roles, agents, anchors, proposals, confirmation, or a parse error                                                                                         |
| An authenticated store envelope                                         | A/B   | **Not implemented by either mission** — RFC-0011-x §Future Work item 1. Listed so it is visibly unaccounted-for rather than silently absent                                                                                                                                                                                            |
| `flock(LOCK_EX)` on the index                                           | B     | **Not implemented by either mission** — RFC-0011-x §Future Work item 16, substrate mission as owner, follow-on amendment. `tv_x_39` pins the no-lock behaviour this mission ships                                                                                                                                                      |

### Implementation Guide

None. This mission has no companion implementation guide in `docs/07-developers/`. The RFC's §Detailed Design and §Store layout carry the specification, and the substrate it builds on is already documented in the guide's wallet sections. If one is written later, it belongs to the substrate mission that implements it, not to this YAML.

## Claimant

(none — Open mission)

## Pull Request

(none — not yet opened)

## Out of Scope

- CLI dispatch, the 13 call-site migrations, the three `OctoCliError` variants at slots 92, 93, and 94, and the guide update — all belong to `0011-x-wallet-store-cli`
- An authenticated store envelope — the same-user-tamper and rollback adversaries are real and open, and closing them is a new format decision with its own review (RFC-0011-x §Future Work item 1)
- `flock(LOCK_EX)` on the index — the store ships documented single-writer last-writer-wins, and the lock is a behaviour change in a follow-on amendment (RFC-0011-x §Future Work item 16). AC-34 pins the shipped behaviour
- A journal or recovery path for the two half-written states — the store **detects and reports** them (`active_seed_slot_present`, `orphan_slots`) and does not repair them. Detection is not recovery, and the repair is a format decision
- HSM handoff — the branch point in `unlock` is identified but the `HsmAdapter` implementation does not exist
- Home-resolver consolidation across crates — RFC-0011-x specifies the order normatively to prevent drift. Consolidating the primitive is deferred, and so is teaching `octo-mesh` the empty-`OCTO_HOME` fail-closed rule it does not currently honour
- Argon2id cost-parameter review for identity seeds — inherited from the vault's posture, and an accepted risk with a deadline in RFC-0011-x §Implicit Assumptions Audit row 4
- The dictionary check and the documented passphrase-rotation procedure — the length floor lands here (AC-28), the other two pieces stay in RFC-0011-x §Future Work item 7
- Removal of the migration sentinel — the CLI mission's criterion
- Any change to `StarkliCompat`, `Did`, or `LifecycleState` — all reused unchanged
- Any Layer A change

**Three types this mission earlier listed as out of scope are now in scope, and the
list above used to contradict them.** An earlier revision said "any change to `Vault`,
`StarkliCompat`, `IdentityKey`, `IdentityRecord`, `Did`, or `LifecycleState` — all
reused unchanged" while the same file specified `IdentityKey::from_seed_with_lifecycle`,
`complete_rotation` losing its `expect`, and `IdentityRecord` gaining `deprecated`. A
criterion that forbids what another criterion requires is not a scope boundary, it is a
contradiction, and the implementer resolves it whichever way they happen to read first.
The three are now named individually in §Type Coverage with the exact change, and the
out-of-scope list names only the types that genuinely do not change.

## Notes

Filed 2026-09-30 per RFC-0011-x §Companion mission YAML pairing.

The framing this mission was written against was that closing `WalletStore` "means building an on-disk keystore, which is a substantial standalone Layer B crypto feature." That is false against the substrate. The on-disk keystore exists — `Vault` encrypts with Argon2id and AES-256-GCM, creates its directory at 0700, and is covered by tests including a wrong-passphrase rejection. `StarkliCompat` is a second, independently tested encrypted keystore. `IdentityKey` has a complete lifecycle with 41 tests. **This mission adds no dependency at all** — not even a non-cryptographic one.

The genuine gap is a wiring layer that was never written, plus the one thing no primitive can supply: somewhere to put an identity. A store with a reader and no writer is always empty, so `octo whoami` keeps exiting 2 and all ten of the guide's wall claims keep being true. The write-path is in scope for that reason and not because the crypto was missing.

The unlock split is the one genuinely new design decision in RFC-0011-x. The parent RFC's `open()` takes no passphrase while `active_identity` must return a usable key, and under a strict reading those two clauses are only jointly satisfiable by a store that reads a plaintext seed. RFC-0011-x chooses the locked/unlocked split instead, which is why metadata reads stay passphrase-free and only the signing paths need an unlock.

**On the passphrase floor, because the criterion was wrong twice.** The first version put
a non-blocking warning at `register` and a hard error at `unlock`, reasoning that a
returning operator with a weak existing passphrase must not be locked out by an upgrade.
The reasoning reads well and does not survive contact with the code: a floor at `unlock`
is the same lockout, arriving one command later, with no escape hatch and the operator
already standing in front of a signing command. The population the split was protecting
provably does not exist, because RFC-0011-x §Compatibility establishes that there are no
pre-existing identity stores — this mission is what creates the first one. The floor is a
hard error at both sites, the refusal is `WalletError::WeakPassphrase`, and the exit code
is 2. Two artifacts asserted the split and one of them was the mission that would have
implemented it.
