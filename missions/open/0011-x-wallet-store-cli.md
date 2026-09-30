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

| Subcommand                                                                    | Substrate                         | Passphrase                       | Exit codes                |
| ----------------------------------------------------------------------------- | --------------------------------- | -------------------------------- | ------------------------- |
| `octo identity register --seed-file <path> [--activate] [--passphrase-stdin]` | `WalletStore::register`           | yes — to **seal**, not to unlock | 0, 2, 64                  |
| `octo identity select <did>`                                                  | `WalletStore::select`             | no                               | 0, 4, 64                  |
| `octo identity list [--json]`                                                 | `WalletStore::list_records`       | no                               | 0, 64                     |
| `octo whoami` (existing)                                                      | `UnlockedWallet::active_identity` | yes                              | 0, 2, 92, 64              |
| `octo identity show [<did>]` (existing)                                       | `WalletStore::identity_record`    | no                               | 0, 4, 64                  |
| `octo identity rotate` (existing)                                             | `UnlockedWallet::begin_rotation`  | yes                              | 0, 2, 3, 4, 5, 11, 92, 64 |
| `octo identity revoke --reason <text>` (existing)                             | `UnlockedWallet::revoke`          | yes                              | 0, 2, 4, 92, 64           |

Four rows carry substrate-faithfulness corrections that an earlier draft got wrong, and each was checked against `IdentityAction` in `crates/octo-cli/src/commands/identity.rs` rather than inferred:

- `register` routes to **`WalletStore::register`**, not `UnlockedWallet::register`. This is the correction that matters, because the earlier routing was not merely a naming slip — it was unsatisfiable. `register` was placed on `UnlockedWallet`, the only way to obtain one is `unlock`, and the only way to `unlock` is to already have a sealed slot, so on an empty store the bootstrap command could not run at all. The `Passphrase` column is still **yes** and the reason is now stated in the cell: `register` needs a passphrase to **seal** the new slot. Sealing and unlocking are different operations that happen to share a parameter, and conflating them is what produced the deadlock.
- `select` routes to `WalletStore::select`, **not** `UnlockedWallet::select`. Moving the active pointer is a pure index write that touches no key material, so a passphrase there would be exactly the over-classification this mission's own Summary warns against — a prompt on a command that reads and writes one field. `register` and `select` are the two store methods that take `&mut self`, read no active key, and need no unlock; they sit together for that reason.
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

| Vector      | Asserts                                                                                                                                                                                             | Negative control                                                                                                  |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `tv_x_c_1`  | `octo identity register` writes a record, `octo identity list` returns it, and the `list` envelope's top-level keys are exactly the canonical set in the canonical order                            | emit the payload keys before the envelope's own, or sort them                                                     |
| `tv_x_c_2`  | `octo identity list`, `octo identity show`, and `octo identity select` all complete with **stdin closed** and no prompt — the three store-level commands that read no key                           | route any of the three through `unlock`                                                                           |
| `tv_x_c_3`  | `octo whoami` with an active identity exits 0 and prints the DID                                                                                                                                    | —                                                                                                                 |
| `tv_x_c_4`  | `octo whoami` on a store with records but no active identity exits 2                                                                                                                                | —                                                                                                                 |
| `tv_x_c_5`  | `octo whoami` on an empty store exits 2                                                                                                                                                             | —                                                                                                                 |
| `tv_x_c_6`  | A wrong passphrase exits 92, not 2 — locked is not no-active-identity                                                                                                                               | map `Locked` to exit 2                                                                                            |
| `tv_x_c_7`  | `--passphrase-stdin` reads from standard input and does not prompt                                                                                                                                  | —                                                                                                                 |
| `tv_x_c_8`  | No passphrase flag exists on any subcommand, so it cannot appear in `argv`. Asserted against the parsed `clap` command tree, not against a grep of the source                                       | add a `--passphrase` flag and watch the assertion fail                                                            |
| `tv_x_c_9`  | `register` then `rotate` then `show` reports the rotation chain from disk                                                                                                                           | —                                                                                                                 |
| `tv_x_c_10` | `revoke` then `show` still resolves the revoked DID, and `revoke` then `register` with the same seed is refused with `AlreadyRevoked`                                                               | let the re-registration through (A14)                                                                             |
| `tv_x_c_11` | Every one of the **18** key-reaching call sites is named in a source-level inventory, each mapped to the vector that exercises it, and no site is unnamed. The vector **fails** on an unlisted site | delete one entry from the inventory                                                                               |
| `tv_x_c_12` | The sentinel is gone from the source tree after the migration. Asserted by a **source query returning zero hits**, not by a count of zero                                                           | —                                                                                                                 |
| `tv_x_c_13` | With neither a TTY on stdin nor `--passphrase-stdin`, a signing command exits 92 within a bounded time. The bound is the assertion                                                                  | remove the no-TTY pre-flight and let the test hang — a passing run must not be able to take longer than the bound |
| `tv_x_c_14` | No `lookup_identity_record` call site survives the sweep. Asserted by a source query returning zero hits                                                                                            | —                                                                                                                 |

Three of these could not fail as previously written, and each named the wrong implementation that would pass:

- `tv_x_c_11` read "every one of the 18 sites is exercised by at least one vector". That is a count of the mission's own prose, so a mission that listed 18 sites and exercised 3 would satisfy it. The falsifiable form is a **source-level inventory**: 18 named call sites, each mapped to a vector, and the assertion fails if any real site is absent from the list. The 18 comes from grepping, not from counting this document — the 15 the compiler reports and the 18 that has to be migrated differ by exactly the three that funnel through `common::resolve_active_identity_key()`.
- `tv_x_c_13` read "exits 92 rather than blocking". A blocking test either hangs the suite or is wrapped in a timeout that reports success on timeout, which is the opposite of what it claims. The bound is now the assertion: the run must complete **and** exit 92 within it. A hang is a failure, not a pass, and removing the no-TTY pre-flight must produce that failure rather than a slow green.
- `tv_x_c_2` covered `list` and `show` but not `select`. `select` is the one store-level command that **writes**, and it is the one a reviewer is most likely to over-classify into a prompt. Running all three with stdin closed is what makes the vector test the classification rather than two of the three read-only cases.

`tv_x_c_6` and `tv_x_c_11` are still the two that matter most. A wrong passphrase exiting 2 would be a lie — the identity exists, the operator just failed to unlock — and the guide's troubleshooting entry for exit 2 sends the operator to re-provision, which would not help.

`tv_x_c_13` covers the case that is easy to leave uncovered and expensive in production. `tv_x_c_7` tests the flag when it is present and `tv_x_c_6` tests a wrong passphrase, so between them the non-interactive-with-no-flag path — an unattended `cron` job, a CI step — has no other vector. That path either blocks forever on an invisible prompt or hangs waiting for stdin, and it is the exact case RFC-0011-x §Adversary Analysis A6 describes.

## Acceptance Criteria

- [ ] **AC-1:** Every one of the 13 `WalletStore::open()` sites and every one of the 18 key-reaching sites is classified as metadata or signing **against the code**, and the classification is recorded in a comment at each site. No site is classified by assumption, and the 18 are enumerated by route — 15 direct `octo_wallet::active_identity` calls and 3 through `common::resolve_active_identity_key()` — because the compiler reports only the 15
- [ ] **AC-2:** `WalletStore::lookup_identity_record` call sites are included in the sweep, and `tv_x_c_14` confirms none survives
- [ ] **AC-3:** `OctoCliError::WalletLocked` is added at slot 92 with a doc comment, a translation arm, and an exit-code arm returning 92
- [ ] **AC-4:** `WalletError::IdentityNotFound` maps to exit 4 via the existing `OctoCliError::IdentityNotFound(String)`, minting no new slot
- [ ] **AC-5:** `IdentityAction` gains `Register`, `Select`, and `List` variants. `Select`'s `<did>` is positional; `Revoke`'s existing `--reason` stays a required flag
- [ ] **AC-6:** All three subcommands have output envelopes in the established `OutputEnvelope<T>` form, with deterministic field order asserted by `tv_x_c_1`. Asserting the values alone would not catch a key-order regression, and canonical field order is load-bearing in this workspace. The envelope type is `OutputEnvelope<T>`, declared in `octo-cli`'s `output` module and already pinned by the existing `whoami` and `identity show` handlers; an earlier draft of this criterion named `CliOutput`, which is not a type in the workspace
- [ ] **AC-7:** Every signing call site acquires its `IdentityKey` through `UnlockedWallet`, not through a locked `WalletStore`
- [ ] **AC-8:** No metadata call site prompts for a passphrase. `octo identity select` is a metadata site: it moves the active pointer and touches no key material, so a passphrase there would be the over-classification §Summary warns against
- [ ] **AC-9:** Passphrase acquisition is `rpassword` or `--passphrase-stdin`; no passphrase flag exists on any subcommand. `rpassword` is **not** currently a dependency of `octo-cli` — it is declared in `crates/octo-wallet/Cargo.toml` and used only by the `octo-wallet` binary. This criterion adds it to `octo-cli/Cargo.toml` with a rationale comment, per the repository convention
- [ ] **AC-10:** With neither a TTY on stdin nor `--passphrase-stdin`, the unlock **fails closed with exit 92 before prompting**, covered by `tv_x_c_13` with a bounded runtime as part of the assertion. A no-TTY pre-flight is preferred over a read timeout: refusing before the prompt is honest, whereas timing out mid-prompt leaves a half-entered passphrase on the terminal. RFC-0011-x §Future Work item 10 carries the timeout question
- [ ] **AC-11:** The deprecated `cli_fns::active_identity` free function is **deleted** from the substrate, together with its `#[deprecated]` attribute and the `Locked` return from the sentinel path, and `cli_fns` has no wrapper that forwards a locked handle to a signing operation. **`WalletStore::lookup_identity_record` is RETAINED, not deleted.** It is a Layer B metadata reader with a real caller in `octo-wallet`'s own `cli_fns`, it needs no unlock, and deleting a substrate symbol is not this mission's decision to make. What this mission does is migrate its call sites, which is what `tv_x_c_14` asserts. An earlier draft of this criterion deleted both symbols together, which would have removed a working reader to satisfy a deprecation
- [ ] **AC-12:** `tv_x_20`, `tv_x_29`, `tv_x_30`, and `tv_x_c_1` through `tv_x_c_14` all pass. `tv_x_31` is **not** in this list: it is the substrate mission's, and it must have passed there before this mission's AC-11 deleted the sentinel it covers
- [ ] **AC-13:** `tv_x_c_6` is negative-controlled — mapping a wrong passphrase to exit 2 makes it fail
- [ ] **AC-14:** `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [ ] **AC-15:** `cargo test -p octo-cli --lib` green
- [ ] **AC-16:** `cargo fmt --check -p octo-cli` clean
- [ ] **AC-17:** `cargo build --workspace --all-targets` shows no downstream breakage
- [ ] **AC-18:** Layer discipline preserved — Layer C only, zero Layer A change

### Guide update

- [ ] **AC-19:** All **nine wall claims across seven locations** in `docs/06-operations/operator-guide.md` are updated. Enumerated, because a partial sweep leaves a guide that describes a wall the code no longer has — which is worse than no guide, and because a count asserted in prose has been wrong five times in this mission's history:

  | #   | Anchor sentence (the criterion)                                                                                                        | Section                              | Names the store? | Line as of 2026-09-30 |
  | --- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ---------------- | --------------------- |
  | 1   | "`WalletStore::open()` returns an empty store, so `octo whoami` still exits 2"                                                         | §4 step 2a, first claim              | yes              | 460                   |
  | 2   | "Every identity-gated command downstream of this step is blocked until the wallet store is implemented"                                | §4 step 2a, second claim             | yes              | 463                   |
  | 3   | "`WalletStore::open()` returns an empty store on the current substrate"                                                                | §17 `OctoCliError::NoActiveIdentity` | yes              | 2067                  |
  | 4   | "`octo-wallet init` writes seed material only and registers nothing with the CLI's identity resolution"                                | §18 step 5, first claim              | **no**           | 2201                  |
  | 5   | "`octo whoami` still exits 2 afterwards, so the `role select` on the next line has no identity to select"                              | §18 step 5, second claim             | **no**           | 2203                  |
  | 6   | "This step cannot succeed until the wallet store lands."                                                                               | §18 step 8                           | yes              | 2242                  |
  | 7   | "on the current substrate `octo whoami` exits 2 ... so this step cannot succeed until the wallet store lands"                          | §22 step 3                           | yes              | 2747–2749             |
  | 8   | "this exits 2 with 'no active identity' on the current substrate, because `WalletStore::open()` returns an empty store"                | §22 step 14                          | yes              | 2891–2893             |
  | 9   | "both `init` calls below write seed material only and register nothing ... so the `role select` commands that follow have no identity" | §31 step 1                           | **no**           | 3888–3889             |

  Six state the wall outright; the remaining three state it in terms of `init` registering nothing and `role select` having no identity to select, which is the same obstruction described from the symptom rather than the cause. Rows 1 and 2, and rows 4 and 5, are **two claims in one location** — §4 step 2a and §18 step 5 each carry a caveat block in which a second sentence restates the same wall in different words.

  **The unit is the claim, not the location and not the line.** That distinction is the whole content of this criterion, and the number was wrong five times because the artifacts never said which of the three they were counting. The honest answer is the pair — **9 claims, 7 locations** — and no single number reproduces it.

  **The seven that kept being recorded, and why the line sweep produces it.** A case-insensitive sweep for `wallet store` or `WalletStore` returns exactly **7 lines**: 460, 464, 2067, 2242, 2748, 2892, 2893. That is the number this criterion carried for five revisions, and it is wrong in a way that looks right — it is 7 lines, 7 locations, and 6 claims, all at once. Line 2893 is the tail of claim 8, whose first line is 2892; lines 2748 and 2749 are one claim whose phrase is split across the wrap. So a reviewer who greps, counts 7, and writes "seven wall statements" is not being careless. They are reporting a line count under a claim label, and the two differ.

  A second sweep over the symptom phrasing — `registers nothing`, `register nothing`, `no identity to select` — returns lines 2202, 2204, 3888, 3889, which are claims 4, 5, and 9. **No single keyword sweep finds all nine**, and the two sweeps do not overlap: every claim in the first group names the store and none in the second does. The `Names the store?` column exists to make that visible at a glance, because a sweep keyed on the store necessarily misses three rows and a sweep keyed on the symptom necessarily misses six.

  **The anchor sentence is the criterion; the line number is a convenience that expires.** An earlier revision of this table made the line number primary, and four of the seven had already drifted — the guide grows, and an anchor that moves silently turns a precise criterion into a wrong one. Grep the sentence. Rows 7, 8, and 9 have anchor sentences that wrap across two or three lines, so a single-line `grep -n` reports a line that is not the claim's start; the `Line` column gives the start of the claim, and the anchor must be matched across the wrap.

  Rows 6 and 7 carry the same substance in different sections, and row 7's phrase is split by the line wrap, so a single-line find-and-replace reaches row 6 and misses row 7.

  Two locations are **deliberately not** walls: the §0 `if ! octo whoami` guard is correct operational advice, and the §18 step 4 expectation is correct because that step runs before step 5 creates the identity. A sweep that "fixes" either one breaks the guide.

- [ ] **AC-20:** The guide's onboarding flow gains the `register` and `select` steps in the order a real operator runs them
- [ ] **AC-21:** The guide's exit-2 troubleshooting entry no longer tells the operator that re-running `octo-wallet init` will not help, since after this change it will
- [ ] **AC-22:** `npx prettier --check` clean on the guide

## Dependencies

Hard sequencing:

1. **`0011-x-s-a-wallet-store-identity` must land first** — the unlock does not exist until it does
2. **RFC-0011-x must be Accepted** before the slot 92 variant mints
3. **AC-11 depends on the substrate mission having added the sentinel** — a mission landing in the other order would have nothing to delete

Required RFCs, per BLUEPRINT §Dependency Validation Rules rule 2 — every "Requires" entry on RFC-0011-x is a prerequisite here:

- **RFC-0011** — the parent CLI substrate RFC. This mission edits its subject's dispatch surface and supersedes the `active_identity` placement clause it states in §Subcommand Taxonomy item 1 and in its §`octo whoami` substrate row.
- **RFC-0102** — owns the storage cryptography and the `Wallet` surface whose unlock this mission reaches. No amendment to it is required; the dependency is for provenance.
- **RFC-0009** — owns the lifecycle state machine behind the `identity` subcommands this mission adds. No amendment to it is required; the dependency is for provenance.

All three are Accepted. RFC-0011-f and RFC-0010 are **optional**: neither is a prerequisite, and nothing in this mission's acceptance criteria depends on either.

### Type Coverage

| RFC type                                                                                                 | Layer | Implemented by                                                                                                                                                                                                                                                                                       |
| -------------------------------------------------------------------------------------------------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `IdentityAction::Register`                                                                               | C     | This mission — NEW                                                                                                                                                                                                                                                                                   |
| `IdentityAction::Select`                                                                                 | C     | This mission — NEW                                                                                                                                                                                                                                                                                   |
| `IdentityAction::List`                                                                                   | C     | This mission — NEW                                                                                                                                                                                                                                                                                   |
| `OctoCliError::WalletLocked` (slot 92)                                                                   | C     | This mission — the one new `OctoCliError` variant                                                                                                                                                                                                                                                    |
| `OctoCliError::IdentityNotFound(String)`                                                                 | C     | **Reused unchanged** at exit 4 — no new slot                                                                                                                                                                                                                                                         |
| `OctoCliError::NoActiveIdentity` (exit 2)                                                                | C     | **Reused unchanged** — an empty store still exits 2                                                                                                                                                                                                                                                  |
| `WalletStore::register` / `select`                                                                       | C     | This mission dispatches to them; `0011-x-s-a-wallet-store-identity` defines them. `register` is on the store, not on `UnlockedWallet` — sealing a slot needs a passphrase and never needs an unlocked key, so the two writers that read no active key sit together                                   |
| `UnlockedWallet::active_identity` / `begin_rotation` / `complete_rotation` / `abort_rotation` / `revoke` | C     | This mission dispatches to them; `0011-x-s-a-wallet-store-identity` defines them                                                                                                                                                                                                                     |
| `WalletStore::list_records` / `identity_record`                                                          | C     | This mission dispatches to them; the substrate mission defines them                                                                                                                                                                                                                                  |
| `OutputEnvelope<T>` envelope form                                                                        | C     | **Reused unchanged** — the real type in the workspace, pinned in the existing `identity` handlers. An earlier draft of this row named `CliOutput`, which is not a type anywhere in the tree                                                                                                          |
| Passphrase acquisition (`rpassword`, `--passphrase-stdin`)                                               | C     | This mission — no new type; the acquisition helper is new. **`rpassword` is not currently a dependency of `octo-cli`**; it is declared in `crates/octo-wallet/Cargo.toml` and used only by the `octo-wallet` binary. This mission adds the dependency to `octo-cli`, with the same rationale comment |
| An authenticated store envelope                                                                          | A/B   | **Not implemented** — RFC-0011-x §Future Work item 1                                                                                                                                                                                                                                                 |

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

The guide currently documents the wall in **nine claims across seven locations** (enumerated by anchor sentence in AC-19). Six of the nine state it outright; the other three describe it from the symptom — `init` registers nothing, `role select` has no identity to select — which is the same obstruction stated in different words. All nine become actively misleading after this change: a reader who follows the exit-2 troubleshooting entry will re-provision an identity that is already registered, and a reader who follows the §4 caveat will treat a working login as a seed-generation step.

**The count was wrong six times before it was measured**, and the sequence is worth recording because each wrong number is a different mistake rather than a restatement of the one before it. Two early drafts said three. One said five. An external review of the artifacts said six. Then two reviewers of this same revision reported different numbers — one said seven and called the table a partial enumeration, the other said the count of seven verified. Every one of those was a number reached by reasoning about what ought to count rather than by enumerating what does, and the disagreement between the two reviewers is the sharpest evidence available: they were counting different units and neither could tell from the prose which unit the artifacts meant.

The resolution is that the artifacts never said which unit they were counting, and **no single number can express the answer**, because the three defensible units give three different results: comment blocks give 5, locations give 7, claims give 9. A fourth unit, lines containing the phrase `wallet store`, gives 7 — the same figure as the location count, which is the coincidence that made "seven" survive five revisions looking correct. Two guide locations are near-misses that a broader definition sweeps in — the §0 `if ! octo whoami` guard and the §18 step 4 expectation — and a narrower one drops the three symptom-phrased claims entirely.

Two further errors repeated while writing the table itself. The first attempt made **line numbers** primary, and four of the seven had already drifted by the time it was re-measured; the guide grows, and a line anchor silently becomes a wrong criterion. The second attempt used a keyword sweep to re-derive the list and found **five**, because rows 5 and 9 state the wall in terms of `role select` having no identity and never mention the store — and the third attempt found a clean-looking **seven** that was really a line count. All three are the same error: a filter narrower than the category being counted, or a count of the wrong unit. The table survives only because it was built by reading the guide, and it stays correct only if a future sweep reads the guide rather than searching it.

The rule this mission now carries, which applies to every number in these four artifacts: **a count in prose is a claim, not a fact. Either enumerate it or key the criterion on something stable.**
