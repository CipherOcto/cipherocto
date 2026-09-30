# 0011-x-wallet-store-cli — `octo identity` registration surface and the unlock migration

## Status

Open (2026-09-30) — CLI companion to RFC-0011-x. Layer C (`octo-cli`). **Blocked on `0011-x-s-a-wallet-store-identity`**, which must land first per the substrate-first ordering invariant: there is no `unlock` to thread until the substrate mission provides one.

## RFC

RFC-0011-x §CLI dispatch, §Error variant, §Cross-RFC reference updates, §Compatibility.

## Summary

Turns `WalletStore` from a stub into something the CLI can actually provision against. Two deliverables:

1. **A provisioning path.** Three new `octo identity` subcommands — `register`, `select`, `list` — plus unlock threading on the existing `whoami`, `show`, `rotate`, and `revoke`. Without `register` the store is always empty, so `octo whoami` keeps exiting 2 and nothing an operator can see changes.
2. **The migration.** 13 real `WalletStore::open()` call sites across five CLI modules, and separately **18** sites that reach for the identity key, currently consume the stub's unconditional `NotActive`. Each must be classified as _metadata_ (no unlock) or _signing_ (unlock required) and migrated accordingly, and the deprecated sentinel must then be deleted.

   The two populations are **not** the same set, and the split between them is not the obvious one. The 13 are the sites that open the store. The 18 key-reaching sites divide again:

   | Route                                                 | Sites | Deprecation warnings                                             |
   | ----------------------------------------------------- | ----- | ---------------------------------------------------------------- |
   | Direct `octo_wallet::active_identity(&store)`         | 15    | 15                                                               |
   | Indirect, via `common::resolve_active_identity_key()` | 3     | 0 — the helper is defined once and calls the sentinel internally |

   Per module, the direct 15 are identity 6, capability 4, governance 3, agent 1, vault 1. The 3 indirect sites are all in `agent.rs`.

   The funnel is the part that matters. `common::resolve_active_identity_key()` opens the store and then calls the sentinel, so its three callers reach the key without naming it. The deprecation warning fires once, inside the helper's definition — which means the sentinel's build-time signal reaches 15 of 18 sites, and a sweep that trusts the warning count as the migration count will leave three sites unmigrated while every warning is accounted for. AC-1 and `tv_x_c_11` are specified against the 18 precisely because the 15 is the number the compiler reports and the 18 is the number that has to be migrated.

The classification sweep is the first acceptance criterion and it is not a formality. The split is what keeps this tractable: metadata readers need no passphrase, so most sites should not gain one. Guessing the distribution wrong in either direction is expensive — over-classifying puts a prompt on read-only commands, and under-classifying leaves a signing path reading metadata and failing at the point of use.

### New subcommands

| Subcommand                                                                    | Substrate                         | Passphrase | Exit codes                |
| ----------------------------------------------------------------------------- | --------------------------------- | ---------- | ------------------------- |
| `octo identity register --seed-file <path> [--activate] [--passphrase-stdin]` | `UnlockedWallet::register`        | yes        | 0, 2, 64                  |
| `octo identity select <did>`                                                  | `WalletStore::select`             | no         | 0, 4, 64                  |
| `octo identity list [--json]`                                                 | `WalletStore::list_records`       | no         | 0, 64                     |
| `octo whoami` (existing)                                                      | `UnlockedWallet::active_identity` | yes        | 0, 2, 92, 64              |
| `octo identity show [<did>]` (existing)                                       | `WalletStore::identity_record`    | no         | 0, 4, 64                  |
| `octo identity rotate` (existing)                                             | `UnlockedWallet::begin_rotation`  | yes        | 0, 2, 3, 4, 5, 11, 92, 64 |
| `octo identity revoke --reason <text>` (existing)                             | `UnlockedWallet::revoke`          | yes        | 0, 2, 4, 92, 64           |

Three rows carry substrate-faithfulness corrections that an earlier draft got wrong, and each was checked against `IdentityAction` in `crates/octo-cli/src/commands/identity.rs` rather than inferred:

- `select` routes to `WalletStore::select`, **not** `UnlockedWallet::select`. Moving the active pointer is a pure index write that touches no key material, so a passphrase there would be exactly the over-classification this mission's own Summary warns against — a prompt on a command that reads and writes one field.
- `show`'s `<did>` is **optional** in the substrate (`Show { did: Option<String> }`, defaulting to the active identity), so the dispatch table must show it in brackets.
- `revoke` takes a **required `--reason` flag**, not a positional. An earlier draft of this table listed it with no argument at all, which would have shipped a subcommand invocation that cannot parse.

`register` takes a **seed file** rather than generating in-process. The operator guide's onboarding step already writes a 0600 seed file via `octo-wallet init --seed-out`, so composing with that step costs one added command in the guide rather than a rewritten section. Passing a freshly generated seed through the same path covers in-process generation.

### New error variant (Layer C)

ONE new `OctoCliError` variant at **slot 92**, the first slot above the 91 high-water mark:

```rust
/// The wallet store is locked and the operation needs the identity key.
#[error("wallet store is locked: unlock with a passphrase to continue")]
WalletLocked,
```

- `WalletError::Locked` → `WalletLocked` → exit 92
- `WalletError::IdentityNotFound` → the **existing** `OctoCliError::IdentityNotFound(String)` at exit 4. No new slot is spent, because the parent RFC's `octo identity show` exit table already reserves exit 4 for that case. The two variants are unrelated types that happen to share a name; this row is the mapping between them, and no artifact may refer to a "no-such-identity" variant — no such variant exists.
- A wrong passphrase surfaces as `WalletError::VaultDecryptionFailed` and exits 92 rather than a distinct code. The operator's remedy is identical, and a distinct slot would add vocabulary without adding capability.

Passphrase acquisition follows the existing `octo-wallet` binary pattern: `rpassword` for the prompt, `--passphrase-stdin` for non-interactive contexts. **Never a command-line flag** — a passphrase in `argv` is readable by every local user through the process table. When neither a prompt nor the flag is available, the unlock fails with exit 92 rather than blocking on a hidden prompt forever in a cron job.

### Call-site migration

13 real call sites. `WalletStore::open` appears 16 times across `octo-cli`; the other 3 are doc comments, not calls — two in `commands/identity.rs` and one in `commands/agent.rs`.

| Module                   | Sites | Notes                                                  |
| ------------------------ | ----- | ------------------------------------------------------ |
| `commands/identity.rs`   | 4     | `whoami`, `show`, `rotate`, `revoke`                   |
| `commands/capability.rs` | 4     | capability mint and attenuate sign with the holder key |
| `commands/governance.rs` | 3     | attest and vote sign                                   |
| `commands/vault.rs`      | 1     | classification to be established by AC-1               |
| `commands/agent.rs`      | 1     | classification to be established by AC-1               |

The `vault.rs` and `agent.rs` sites are the ones most likely to be metadata-only — vault transfer authoring and agent attach may only need the DID. AC-1 settles it against the code rather than by assumption.

Two corrections worth recording, because the earlier forms of this note were wrong in ways that would have misled the sweep.

**The doc comments.** The three doc comments were described as "two in `identity.rs` describing the `map_wallet_open_error` helper". The 3-comment count is right; the parenthetical explaining what they describe is not, and a reviewer following it would have gone looking for a helper that is not where the note said it was.

**The call-site count.** `map_wallet_open_error` is defined once and called at **4** production sites, all in `commands/identity.rs`. An earlier revision of this note claimed "7 times in `commands/identity.rs` and once in `commands/capability.rs`"; both numbers conflate textual occurrences with call sites. The 7 is the count of every mention in that file — one definition, four calls, and two references inside the sanitizer's own test — and the `capability.rs` mention is a doc comment that describes the helper without calling it. A migration that sized its work at 8 sites would have found 4.

The correct shape across the 13 `WalletStore::open()` sites: 4 route through the shared helper, 4 route through `map_capability_internal` in `commands/capability.rs`, and 5 inline the sanitization directly. All 13 sanitize, so there is no path-leak gap to close here — but that is three different spellings of the same mapping, and `unlock` adds a second, larger error surface. AC-1 should record which spelling each site uses, because thirteen hand-maintained error arms is where a `WalletError` variant added for the unlock path would be handled at some sites and dropped at others.

`WalletStore::lookup_identity_record` is a third method on the store that the parent RFC does not mention. Any call site reaching for it must be included in the sweep.

## Test Vectors

Per RFC-0011-x §Test Vectors, the CLI-observable subset: `tv_x_20` (select moves the active pointer, and fails on a miss), `tv_x_29` (`WalletError::Locked` maps to slot 92 and exit 92), `tv_x_30` (`WalletError::IdentityNotFound` maps to exit 4 and mints no new slot).

`tv_x_31` — the deprecated `cli_fns::active_identity` free function always returns `Locked` — is **substrate-owned**, not CLI-owned. It asserts a property of a function the substrate mission creates, so this mission did not exist when it could run. It is listed in the substrate mission's vector set for that reason, and it must be recorded here as passing **before** AC-11 deletes the sentinel, not after. An earlier draft of this mission claimed `tv_x_31` as its own while also requiring the sentinel's deletion in the same acceptance block, which made the two mutually unsatisfiable.

Plus this mission's own vectors:

| Vector      | Asserts                                                                                                                                      |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `tv_x_c_1`  | `octo identity register` writes a record and `octo identity list` returns it, and the `list` envelope's key order is the canonical one       |
| `tv_x_c_2`  | `octo identity list` and `octo identity show` prompt for no passphrase                                                                       |
| `tv_x_c_3`  | `octo whoami` with an active identity exits 0 and prints the DID                                                                             |
| `tv_x_c_4`  | `octo whoami` on a store with records but no active identity exits 2                                                                         |
| `tv_x_c_5`  | `octo whoami` on an empty store exits 2                                                                                                      |
| `tv_x_c_6`  | A wrong passphrase exits 92, not 2 — locked is not no-active-identity                                                                        |
| `tv_x_c_7`  | `--passphrase-stdin` reads from standard input and does not prompt                                                                           |
| `tv_x_c_8`  | No passphrase flag exists on any subcommand, so it cannot appear in `argv`                                                                   |
| `tv_x_c_9`  | `register` then `rotate` then `show` reports the rotation chain from disk                                                                    |
| `tv_x_c_10` | `revoke` then `show` still resolves the revoked DID                                                                                          |
| `tv_x_c_11` | Every one of the **18** key-reaching call sites is exercised by at least one vector, and all **13** `WalletStore::open` sites are classified |
| `tv_x_c_12` | The sentinel is gone from the source tree after the migration                                                                                |
| `tv_x_c_13` | With neither a prompt nor `--passphrase-stdin`, a signing command exits 92 rather than blocking on a hidden prompt                           |
| `tv_x_c_14` | No `lookup_identity_record` call site survives the sweep                                                                                     |

`tv_x_c_6` and `tv_x_c_11` are the two that matter most. A wrong passphrase exiting 2 would be a lie — the identity exists, the operator just failed to unlock — and the guide's troubleshooting entry for exit 2 sends the operator to re-provision, which would not help. `tv_x_c_11` exists because a suite that only exercises migrated call sites cannot see a missed one, and because the compiler's 15 deprecation warnings undercount the 18 key-reaching sites by exactly the three that funnel through `common::resolve_active_identity_key()`: the count is asserted, not the behaviour.

`tv_x_c_13` covers the case that is easy to leave uncovered and expensive in production. `tv_x_c_7` tests the flag when it is present and `tv_x_c_6` tests a wrong passphrase, so between them the non-interactive-with-no-flag path — an unattended `cron` job, a CI step — has no vector. That path either blocks forever on an invisible prompt or hangs waiting for stdin, and it is the exact case the §Adversary A6 entry describes.

## Acceptance Criteria

- [ ] **AC-1:** Every one of the 13 `WalletStore::open()` sites and every one of the 18 key-reaching sites is classified as metadata or signing **against the code**, and the classification is recorded in a comment at each site. No site is classified by assumption, and the 18 are enumerated by route — 15 direct `octo_wallet::active_identity` calls and 3 through `common::resolve_active_identity_key()` — because the compiler reports only the 15
- [ ] **AC-2:** `WalletStore::lookup_identity_record` call sites are included in the sweep, and `tv_x_c_14` confirms none survives
- [ ] **AC-3:** `OctoCliError::WalletLocked` is added at slot 92 with a doc comment, a translation arm, and an exit-code arm returning 92
- [ ] **AC-4:** `WalletError::IdentityNotFound` maps to exit 4 via the existing `OctoCliError::IdentityNotFound(String)`, minting no new slot
- [ ] **AC-5:** `IdentityAction` gains `Register`, `Select`, and `List` variants. `Select`'s `<did>` is positional; `Revoke`'s existing `--reason` stays a required flag
- [ ] **AC-6:** All three subcommands have output envelopes in the established `CliOutput` form, with deterministic field order asserted by `tv_x_c_1`. Asserting the values alone would not catch a key-order regression, and canonical field order is load-bearing in this workspace
- [ ] **AC-7:** Every signing call site acquires its `IdentityKey` through `UnlockedWallet`, not through a locked `WalletStore`
- [ ] **AC-8:** No metadata call site prompts for a passphrase. `octo identity select` is a metadata site: it moves the active pointer and touches no key material, so a passphrase there would be the over-classification §Summary warns against
- [ ] **AC-9:** Passphrase acquisition is `rpassword` or `--passphrase-stdin`; no passphrase flag exists on any subcommand
- [ ] **AC-10:** With neither a prompt nor `--passphrase-stdin`, the unlock fails with exit 92 rather than blocking, covered by `tv_x_c_13`
- [ ] **AC-11:** `cli_fns::active_identity` and `WalletStore::lookup_identity_record` are **deleted** from the substrate, together with the `Locked` return from the sentinel path, and `cli_fns` has no wrapper that forwards a locked handle to a signing operation
- [ ] **AC-12:** `tv_x_20`, `tv_x_29`, `tv_x_30`, and `tv_x_c_1` through `tv_x_c_14` all pass. `tv_x_31` is **not** in this list: it is the substrate mission's, and it must have passed there before this mission's AC-11 deleted the sentinel it covers
- [ ] **AC-13:** `tv_x_c_6` is negative-controlled — mapping a wrong passphrase to exit 2 makes it fail
- [ ] **AC-14:** `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [ ] **AC-15:** `cargo test -p octo-cli --lib` green
- [ ] **AC-16:** `cargo fmt --check -p octo-cli` clean
- [ ] **AC-17:** `cargo build --workspace --all-targets` shows no downstream breakage
- [ ] **AC-18:** Layer discipline preserved — Layer C only, zero Layer A change
- [ ] **AC-19:** All **seven** wall statements in `docs/06-operations/operator-guide.md` are updated. The criterion is the table of anchor sentences below, not the number.

### Guide update

- [ ] **AC-19:** All **seven** wall statements in `docs/06-operations/operator-guide.md` are updated. Enumerated, because a partial sweep leaves a guide that describes a wall the code no longer has — which is worse than no guide, and because a count asserted in prose has been wrong three times in this mission's history:

  | #   | Anchor sentence (the criterion)                                                                         | Section                              | Line as of 2026-09-30 |
  | --- | ------------------------------------------------------------------------------------------------------- | ------------------------------------ | --------------------- |
  | 1   | "Every identity-gated command downstream of this step is blocked until the wallet store is implemented" | §4 step 2a                           | 460                   |
  | 2   | "`WalletStore::open()` returns an empty store on the current substrate"                                 | §17 `OctoCliError::NoActiveIdentity` | 2067                  |
  | 3   | "`octo whoami` still exits 2 afterwards, so the `role select` on the next line has no identity"         | §18 step 5                           | 2201                  |
  | 4   | "This step cannot succeed until the wallet store lands."                                                | §18 step 8                           | 2242                  |
  | 5   | "this step cannot succeed until the wallet store lands"                                                 | §22 step 3                           | 2748                  |
  | 6   | "`WalletStore::open()` returns an empty store. The wallet store landing is what unblocks it"            | §22 step 14                          | 2892                  |
  | 7   | "the `role select` commands that follow have no identity to select"                                     | §31 step 1                           | 3887                  |

  Six state the wall outright; the seventh prescribes a remedy that cannot work for the same reason.

  **The anchor sentence is the criterion; the line number is a convenience that expires.** An earlier revision of this table made the line number primary, and four of the seven had already drifted — the guide grows, and an anchor that moves silently turns a precise criterion into a wrong one. Grep the sentence. Re-deriving the list by keyword search does not work either: a search for `WalletStore` and the obvious wall phrasings finds only five of the seven, because rows 3 and 7 describe the wall in terms of `role select` having no identity and never name the store. That is the same mistake this criterion exists to prevent — enumerating against a category narrower than the thing being counted — and it is why the table is the specification and any fresh sweep must reproduce it rather than restate a number.

  Locations 4 and 5 carry the same substance in different sections, and in location 5 the phrase is split across two lines, so a single-line find-and-replace reaches one and not the other.

  Two locations are **deliberately not** walls: the §0 `if ! octo whoami` guard is correct operational advice, and the §18 step 4 expectation is correct because that step runs before step 5 creates the identity. A sweep that "fixes" either one breaks the guide.

- [ ] **AC-20:** The guide's onboarding flow gains the `register` and `select` steps in the order a real operator runs them
- [ ] **AC-21:** The guide's exit-2 troubleshooting entry no longer tells the operator that re-running `octo-wallet init` will not help, since after this change it will
- [ ] **AC-22:** `npx prettier --check` clean on the guide

## Dependencies

Hard sequencing:

1. **`0011-x-s-a-wallet-store-identity` must land first** — the unlock does not exist until it does
2. **RFC-0011-x must be Accepted** before the slot 92 variant mints
3. **AC-11 depends on the substrate mission having added the sentinel** — a mission landing in the other order would have nothing to delete

### Type Coverage

| RFC type                                                                     | Layer | Implemented by                                                                   |
| ---------------------------------------------------------------------------- | ----- | -------------------------------------------------------------------------------- |
| `IdentityAction::Register`                                                   | C     | This mission — NEW                                                               |
| `IdentityAction::Select`                                                     | C     | This mission — NEW                                                               |
| `IdentityAction::List`                                                       | C     | This mission — NEW                                                               |
| `OctoCliError::WalletLocked` (slot 92)                                       | C     | This mission — the one new `OctoCliError` variant                                |
| `OctoCliError::IdentityNotFound(String)`                                     | C     | **Reused unchanged** at exit 4 — no new slot                                     |
| `OctoCliError::NoActiveIdentity` (exit 2)                                    | C     | **Reused unchanged** — an empty store still exits 2                              |
| `UnlockedWallet::register` / `active_identity` / `begin_rotation` / `revoke` | C     | This mission dispatches to them; `0011-x-s-a-wallet-store-identity` defines them |
| `WalletStore::select` / `list_records` / `identity_record`                   | C     | This mission dispatches to them; the substrate mission defines them              |
| `CliOutput` envelope form                                                    | C     | **Reused unchanged** — established workspace form, no new envelope type          |
| Passphrase acquisition (`rpassword`, `--passphrase-stdin`)                   | C     | This mission — no new type; the acquisition helper is new                        |
| An authenticated store envelope                                              | A/B   | **Not implemented** — RFC-0011-x §Future Work item 1                             |

### Implementation Guide

None. The three new subcommands follow the guide's existing `octo identity` conventions, and the onboarding-flow update is that mission's AC-20 rather than a separate document. A guide section is warranted if and only if the unlock model turns out to need explaining beyond what the RFC and §Notes here already say.

## Claimant

(none — Open mission, blocked on the substrate mission)

## Pull Request

(none — not yet opened)

## Out of Scope

- The store itself, the unlock, the write-path, and the `WalletError` variants — all in `0011-x-s-a-wallet-store-identity`
- An authenticated store envelope, HSM handoff, home-resolver consolidation, and Argon2id cost review — all in RFC-0011-x §Future Work
- A full guide-executor — this mission is a precondition for one, not the enabler
- Any change to the vault's own command surface or its 0600 seed-file output
- Any Layer A change

## Notes

Filed 2026-09-30 per RFC-0011-x §Companion mission YAML pairing.

The `octo` binary is a one-shot dispatcher with no daemon and no listening socket, so a passphrase prompt per command is the whole of the operator experience. That is a real cost, and it is the reason metadata readers must stay prompt-free: an operator listing identities should not have to unlock a key they are not using.

The guide currently documents the wall in seven places (enumerated by anchor sentence in AC-19). Six of them state it outright and one prescribes a remedy that cannot work. All seven become actively misleading after this change: a reader who follows the exit-2 troubleshooting entry will re-provision an identity that is already registered, and a reader who follows the §4 caveat will treat a working login as a seed-generation step.

The count is seven and the anchors are in AC-19. This is worth stating plainly because the count was wrong four times before it was measured: two early drafts said three, one said five, and an external review of the artifacts said six. Every one of those was a number reached by reasoning about what ought to count rather than by enumerating what does. Two guide locations are near-misses — the §0 `if ! octo whoami` guard and the §18 step 4 expectation — and a narrower or broader definition moves the answer without moving the guide. The enumeration is what settles it, which is why AC-19 is keyed on anchor sentences and not on the count.

That correction is not finished by the table, because the same failure repeated while writing it. The first attempt at this criterion made **line numbers** primary, and four of the seven had already drifted by the time it was re-measured — the guide grows and a line anchor silently becomes a wrong criterion. The second attempt used a keyword sweep to re-derive the list and found **five**, because two of the seven state the wall in terms of `role select` having no identity and never mention the store. Both errors are the same error: a filter narrower than the category being counted. The table survives only because it was built by reading the guide, and it stays correct only if a future sweep reads the guide rather than searching it.
