# 0011-x-wallet-store-cli — `octo identity` registration surface and the unlock migration

## Status

Open (2026-09-30) — CLI companion to RFC-0011-x. Layer C (`octo-cli`). **Blocked on `0011-x-s-a-wallet-store-identity`**, which must land first per the substrate-first ordering invariant: there is no `unlock` to thread until the substrate mission provides one.

## RFC

RFC-0011-x §CLI dispatch, §Error variant, §Cross-RFC reference updates, §Compatibility.

## Summary

Turns `WalletStore` from a stub into something the CLI can actually provision against. Two deliverables:

1. **A provisioning path.** Three new `octo identity` subcommands — `register`, `select`, `list` — plus unlock threading on the existing `whoami`, `show`, `rotate`, and `revoke`. Without `register` the store is always empty, so `octo whoami` keeps exiting 2 and nothing an operator can see changes.
2. **The migration.** 13 real `WalletStore::open()` call sites across five CLI modules currently consume the stub's unconditional `NotActive`. Each must be classified as _metadata_ (no unlock) or _signing_ (unlock required) and migrated accordingly, and the deprecated sentinel must then be deleted.

The classification sweep is the first acceptance criterion and it is not a formality. The split is what keeps this tractable: metadata readers need no passphrase, so most sites should not gain one. Guessing the distribution wrong in either direction is expensive — over-classifying puts a prompt on read-only commands, and under-classifying leaves a signing path reading metadata and failing at the point of use.

### New subcommands

| Subcommand                                                                    | Substrate                         | Passphrase | Exit codes                |
| ----------------------------------------------------------------------------- | --------------------------------- | ---------- | ------------------------- |
| `octo identity register --seed-file <path> [--activate] [--passphrase-stdin]` | `UnlockedWallet::register`        | yes        | 0, 2, 64                  |
| `octo identity select <did>`                                                  | `UnlockedWallet::select`          | no         | 0, 4, 64                  |
| `octo identity list [--json]`                                                 | `WalletStore::list_records`       | no         | 0, 64                     |
| `octo whoami` (existing)                                                      | `UnlockedWallet::active_identity` | yes        | 0, 2, 92, 64              |
| `octo identity show <did>` (existing)                                         | `WalletStore::identity_record`    | no         | 0, 4, 64                  |
| `octo identity rotate` (existing)                                             | `UnlockedWallet::begin_rotation`  | yes        | 0, 2, 3, 4, 5, 11, 92, 64 |
| `octo identity revoke` (existing)                                             | `UnlockedWallet::revoke`          | yes        | 0, 2, 4, 92, 64           |

`register` takes a **seed file** rather than generating in-process. The operator guide's onboarding step already writes a 0600 seed file via `octo-wallet init --seed-out`, so composing with that step costs one added command in the guide rather than a rewritten section. Passing a freshly generated seed through the same path covers in-process generation.

### New error variant (Layer C)

ONE new `OctoCliError` variant at **slot 92**, the first slot above the 91 high-water mark:

```rust
/// The wallet store is locked and the operation needs the identity key.
#[error("wallet store is locked: unlock with a passphrase to continue")]
WalletLocked,
```

- `WalletError::Locked` → `WalletLocked` → exit 92
- `WalletError::IdentityNotFound` → the **existing** no-such-identity variant at exit 4. No new slot is spent, because the parent RFC's `octo identity show` exit table already reserves exit 4 for that case.
- A wrong passphrase surfaces as `WalletError::VaultDecryptionFailed` and exits 92 rather than a distinct code. The operator's remedy is identical, and a distinct slot would add vocabulary without adding capability.

Passphrase acquisition follows the existing `octo-wallet` binary pattern: `rpassword` for the prompt, `--passphrase-stdin` for non-interactive contexts. **Never a command-line flag** — a passphrase in `argv` is readable by every local user through the process table. When neither a prompt nor the flag is available, the unlock fails with exit 92 rather than blocking on a hidden prompt forever in a cron job.

### Call-site migration

13 real call sites. `WalletStore::open` appears 16 times across `octo-cli`; the other 3 are doc comments, not calls — two in `commands/identity.rs` describing the `map_wallet_open_error` helper, and one in `commands/agent.rs`.

| Module                   | Sites | Notes                                                  |
| ------------------------ | ----- | ------------------------------------------------------ |
| `commands/identity.rs`   | 4     | `whoami`, `show`, `rotate`, `revoke`                   |
| `commands/capability.rs` | 4     | capability mint and attenuate sign with the holder key |
| `commands/governance.rs` | 3     | attest and vote sign                                   |
| `commands/vault.rs`      | 1     | classification to be established by AC-1               |
| `commands/agent.rs`      | 1     | classification to be established by AC-1               |

The `vault.rs` and `agent.rs` sites are the ones most likely to be metadata-only — vault transfer authoring and agent attach may only need the DID. AC-1 settles it against the code rather than by assumption.

`WalletStore::lookup_identity_record` is a third method on the store that the parent RFC does not mention. Any call site reaching for it must be included in the sweep.

## Test Vectors

Per RFC-0011-x §Test Vectors, the CLI-observable subset: `tv_x_20` (select moves the active pointer, and fails on a miss), `tv_x_29` (`WalletError::Locked` maps to slot 92 and exit 92), `tv_x_30` (`WalletError::IdentityNotFound` maps to exit 4 and mints no new slot), `tv_x_31` (the sentinel always returns `Locked`).

Plus this mission's own vectors:

| Vector      | Asserts                                                                      |
| ----------- | ---------------------------------------------------------------------------- |
| `tv_x_c_1`  | `octo identity register` writes a record and `octo identity list` returns it |
| `tv_x_c_2`  | `octo identity list` and `octo identity show` prompt for no passphrase       |
| `tv_x_c_3`  | `octo whoami` with an active identity exits 0 and prints the DID             |
| `tv_x_c_4`  | `octo whoami` on a store with records but no active identity exits 2         |
| `tv_x_c_5`  | `octo whoami` on an empty store exits 2                                      |
| `tv_x_c_6`  | A wrong passphrase exits 92, not 2 — locked is not no-active-identity        |
| `tv_x_c_7`  | `--passphrase-stdin` reads from standard input and does not prompt           |
| `tv_x_c_8`  | No passphrase flag exists on any subcommand, so it cannot appear in `argv`   |
| `tv_x_c_9`  | `register` then `rotate` then `show` reports the rotation chain from disk    |
| `tv_x_c_10` | `revoke` then `show` still resolves the revoked DID                          |
| `tv_x_c_11` | Every one of the 13 migrated call sites is exercised by at least one vector  |
| `tv_x_c_12` | The sentinel is gone from the source tree after the migration                |

`tv_x_c_6` and `tv_x_c_11` are the two that matter. A wrong passphrase exiting 2 would be a lie — the identity exists, the operator just failed to unlock — and the guide's troubleshooting entry for exit 2 sends the operator to re-provision, which would not help. `tv_x_c_11` exists because a suite that only exercises migrated call sites cannot see a missed one; the count is asserted, not the behaviour.

## Acceptance Criteria

- [ ] **AC-1:** Every one of the 13 call sites is classified as metadata or signing **against the code**, and the classification is recorded in a comment at each site. No site is classified by assumption
- [ ] **AC-2:** `WalletStore::lookup_identity_record` call sites are included in the sweep
- [ ] **AC-3:** `OctoCliError::WalletLocked` is added at slot 92 with a doc comment, a translation arm, and an exit-code arm returning 92
- [ ] **AC-4:** `WalletError::IdentityNotFound` maps to exit 4 via the existing variant, minting no new slot
- [ ] **AC-5:** `IdentityAction` gains `Register`, `Select`, and `List` variants
- [ ] **AC-6:** All three subcommands have output envelopes in the established `CliOutput` form, with deterministic field order
- [ ] **AC-7:** Every signing call site acquires its `IdentityKey` through `UnlockedWallet`, not through a locked `WalletStore`
- [ ] **AC-8:** No metadata call site prompts for a passphrase
- [ ] **AC-9:** Passphrase acquisition is `rpassword` or `--passphrase-stdin`; no passphrase flag exists on any subcommand
- [ ] **AC-10:** With neither a prompt nor `--passphrase-stdin`, the unlock fails with exit 92 rather than blocking
- [ ] **AC-11:** `WalletStore::active_identity` and `WalletStore::lookup_identity_record` are **deleted** from the substrate, together with the `Locked` return from the sentinel path, and `cli_fns` has no wrapper that forwards a locked handle to a signing operation
- [ ] **AC-12:** `tv_x_20`, `tv_x_29`, `tv_x_30`, `tv_x_31`, and `tv_x_c_1` through `tv_x_c_12` all pass
- [ ] **AC-13:** `tv_x_c_6` is negative-controlled — mapping a wrong passphrase to exit 2 makes it fail
- [ ] **AC-14:** `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [ ] **AC-15:** `cargo test -p octo-cli --lib` green
- [ ] **AC-16:** `cargo fmt --check -p octo-cli` clean
- [ ] **AC-17:** `cargo build --workspace --all-targets` shows no downstream breakage
- [ ] **AC-18:** Layer discipline preserved — Layer C only, zero Layer A change

### Guide update

- [ ] **AC-19:** All **five** wall statements in `docs/06-operations/operator-guide.md` are updated. Enumerated, because a partial sweep leaves a guide that describes a wall the code no longer has — which is worse than no guide:

  | #   | Location                                                | Current text                                                                                            |
  | --- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
  | 1   | §4 step 2a comment                                      | "Every identity-gated command downstream of this step is blocked until the wallet store is implemented" |
  | 2   | §`OctoCliError::NoActiveIdentity` troubleshooting entry | Prescribes `octo-wallet init --seed-out`, which writes a seed file nothing in the workspace consumes    |
  | 3   | `role select` step comment                              | "`octo whoami` still exits 2 afterwards, so the `role select` on the next line has no identity"         |
  | 4   | §10 step 8 comment                                      | "This step cannot succeed until the wallet store lands."                                                |
  | 5   | §20 step 2 comment                                      | "so this step cannot succeed until the wallet store lands."                                             |

  Four state the wall outright; the fifth prescribes a remedy that cannot work for the same reason. Locations 4 and 5 are byte-identical in substance and must both be swept — a find-and-replace on one does not reach the other.

- [ ] **AC-20:** The guide's onboarding flow gains the `register` and `select` steps in the order a real operator runs them
- [ ] **AC-21:** The guide's exit-2 troubleshooting entry no longer tells the operator that re-running `octo-wallet init` will not help, since after this change it will
- [ ] **AC-22:** `npx prettier --check` clean on the guide

## Dependencies

Hard sequencing:

1. **`0011-x-s-a-wallet-store-identity` must land first** — the unlock does not exist until it does
2. **RFC-0011-x must be Accepted** before the slot 92 variant mints
3. **AC-11 depends on the substrate mission having added the sentinel** — a mission landing in the other order would have nothing to delete

## Out of Scope

- The store itself, the unlock, the write-path, and the `WalletError` variants — all in `0011-x-s-a-wallet-store-identity`
- An authenticated store envelope, HSM handoff, home-resolver consolidation, and Argon2id cost review — all in RFC-0011-x §Future Work
- A full guide-executor — this mission is a precondition for one, not the enabler
- Any change to the vault's own command surface or its 0600 seed-file output
- Any Layer A change

## Notes

Filed 2026-09-30 per RFC-0011-x §Companion mission YAML pairing.

The `octo` binary is a one-shot dispatcher with no daemon and no listening socket, so a passphrase prompt per command is the whole of the operator experience. That is a real cost, and it is the reason metadata readers must stay prompt-free: an operator listing identities should not have to unlock a key they are not using.

The guide currently documents the wall in five places (enumerated in AC-19). Four of them state it outright and one prescribes a remedy that cannot work. All five become actively misleading after this change: a reader who follows the exit-2 troubleshooting entry will re-provision an identity that is already registered, and a reader who follows the §4 caveat will treat a working login as a seed-generation step.
