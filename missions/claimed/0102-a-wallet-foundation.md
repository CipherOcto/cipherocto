# Mission: Wallet Foundation

## Status

Claimed (2026-07-20)

**Retro-supersession (2026-09-30, RFC-0011-x):** The identity-store and native-keystore portion of this mission moves to `missions/open/0011-x-s-a-wallet-store-identity.md` (substrate) and `missions/open/0011-x-wallet-store-cli.md` (CLI), per RFC-0011-x §Companion mission YAML pairing. This mission retains everything else. The two must not both claim the same substrate.

The reason is a scope error in this mission's framing, not a change of intent. This mission was written to stand up the wallet foundation including identity-key storage, and it has sat in `claimed/` for over two months with all 7 acceptance-criterion groups unchecked. The unchecked state does not mean the work is undone. Verified against the substrate on 2026-09-30:

| Criterion group                      | Actual state                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Crate structure                      | `crates/octo-wallet/` exists as a workspace member with 15 modules and **10** re-export statements in `lib.rs`. **Partly open:** `crates/octo-core/src/lib.rs` does not re-export `IdentityKey`, and `crates/octo-core/src/identity.rs` still exists, so the phase-out disposition is not done. An earlier revision of this table said 13 re-export groups; the count is 10, and neither the statement count nor the re-exported-name count is 13.                                                                        |
| Identity substrate (RFC-0009)        | `IdentityKey`, `CapabilityKey`, `AudienceId`, `ChannelId`, and `derive_capability_key` all exist and are re-exported from `identity.rs`. **Naming drift:** the ACs name `public_bytes()`, `seed_bytes()`, and `did() -> String`; the substrate provides `public_key_bytes()`, `seed_bytes_for_hkdf()`, and `did() -> Did` (a newtype, not `String`). `CapabilityKey` lives in `identity.rs`, not `capability.rs` as the AC states.                                                                                        |
| Provider-key vault (RFC-0009 §Vault) | LANDED. `Vault` is Argon2id + AES-256-GCM, 0700 slots dir, with `put` / `get` / `list` and 6 tests.                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Starkli-compat keystore              | LANDED. `StarkliCompat` in `keystore.rs` with `import` / `export` and 3 tests, including wrong-passphrase rejection.                                                                                                                                                                                                                                                                                                                                                                                                      |
| CLI binary                           | **PARTLY LANDED.** `crates/octo-wallet/src/bin/octo-wallet.rs` prompts via `rpassword` and drives `Vault::open_default`, `put`, and `get`; `derive-cap` is wired to `derive_capability_key`; `init` takes `--node-type` and `--seed-out`. Two gaps against the acceptance criteria below, both open: the `import` / `export` subcommands exist only as lines in the module's own doc comment and were never built, and an `ask` subtree (`publish` and siblings) was added and is not mentioned anywhere in this mission. |

Three of those groups are marked LANDED or PARTLY LANDED while the acceptance-criterion boxes beneath them are still unchecked — 9 boxes under the vault, 7 under Starkli, 5 under the CLI binary. The two are not in conflict because the boxes are stale, not the table: the test counts in the table are correct against the substrate, which is the more dangerous direction. Anyone closing this mission by working the boxes would re-implement Argon2id and AES-256-GCM that already exist and are already tested. The boxes are annotated per-group below rather than checked, because checking them would assert a verification nobody performed. Where a box is stale because the substrate _drifted_ from it rather than because the work was done, the box is annotated with the drift instead.
| RFC-0102 follow-up amendments | Not separately tracked here. |
| Cross-crate compat | Open; depends on the `octo-core` disposition above. |

The two genuinely open substrate items are the `octo-core` re-export and phase-out, and the identity store. The store is the larger of the two and is what RFC-0011-x addresses.

A note on what the store is **not**: it is not a new cryptographic feature. `Vault` already encrypts on disk with Argon2id and AES-256-GCM, `StarkliCompat` is a second independently tested encrypted keystore, and `IdentityKey` has a complete 41-test lifecycle. What is missing is a wiring layer plus the one thing no primitive supplies — anywhere to put an identity. Nothing in the workspace writes an `IdentityRecord`, so a reader alone would produce an always-empty store.

## RFC

- RFC-0102 (Numeric): Wallet Cryptography — Stark Curve substrate (KDF PBKDF2 → Argon2id)
- RFC-0009 (Process): Identity Management — Ed25519 identity substrate

Status, promotion history, and the list of which sections were added when are properties of the RFCs themselves and belong in their own Status header and Version History table. Carrying a copy here is how the two drift apart, so this mission cites the numbers only.

**BLUEPRINT gate note:** Both RFCs are **Accepted** as of 2026-07-20. Per BLUEPRINT.md "Missions REQUIRE an approved RFC. No RFC = Create one first." — this mission is now CLAIMABLE per BLUEPRT Mission Lifecycle (both Requires RFCs reached Accepted 2026-07-20). Claim filed 2026-07-20.

## Summary

Stand up `octo-wallet/` as a separate crate providing the user-facing wallet layer: Ed25519 identity substrate (RFC-0009), Stark Curve transaction substrate (RFC-0102), NodeType taxonomy, provider-key vault (file-per-slot on disk, Argon2id + AES-256-GCM, separate from identity keys), capability key derivation (HKDF-BLAKE3 with `cipherocto/cap/v1/` info string + audience DID as IKM), and starkli-compatible keystore import/export. `octo-core` re-exports wallet types via thin newtypes.

## Acceptance Criteria

### Crate structure

- [ ] `crates/octo-wallet/` builds standalone (own Cargo.toml + lib.rs)
- [ ] `crates/octo-wallet/` added to workspace via `crates/*` glob (no explicit workspace edit needed)
- [ ] `crates/octo-core/Cargo.toml` gains `octo-wallet = { path = "../octo-wallet" }`
- [ ] `crates/octo-core/src/lib.rs` re-exports `pub use octo_wallet::IdentityKey;` (NOT `Identity` — Identity struct is being phased out per RFC-0009 §Identity Struct amendment)
- [ ] **octo-core/src/identity.rs disposition:** file deleted entirely; `IdentityKey` from `octo-wallet::identity::IdentityKey` is the canonical identity type. Migration: existing callers of `octo_core::Identity` updated to use `octo_wallet::IdentityKey` directly.

### Identity substrate (RFC-0009)

- [ ] `NodeType { Wholesale, SelfHost, Hybrid }` enum at `crates/octo-wallet/src/node.rs` with derives `[Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize]` + Display + FromStr
- [ ] `IdentityKey(Ed25519Keypair)` newtype at `crates/octo-wallet/src/identity.rs` wrapping `ed25519_dalek::SigningKey`
- [ ] `IdentityKey::generate()` produces CSPRNG-backed keypair via `OsRng`
- [ ] `IdentityKey::public_bytes() -> [u8; 32]`
- [ ] `IdentityKey::did() -> String` returns `did:octo:<multibase(z)-32-bytes>` per RFC-0009 §Identity Key Format
- [ ] `IdentityKey::seed_bytes() -> [u8; 32]` returns raw Ed25519 seed for HKDF input
- [ ] `CapabilityKey([u8; 32])` newtype at `crates/octo-wallet/src/capability.rs`
- [ ] `derive_capability_key(identity, audience_did, channel_id)` per RFC-0009 §Capability Keys: HKDF-BLAKE3(salt=identity_seed, info=`b"cipherocto/cap/v1/" + channel_id`, ikm=audience_did_bytes)
- [ ] `holder_sign(identity, root_hash) -> Ed25519Signature` per RFC-0009 §Capability Keys
- [ ] `canonical_ser(identity) -> Vec<u8>` per RFC-0009 §Identity Struct
- [ ] Property test: 10K random (audience, channel) pairs produce 10K distinct CapabilityKeys (unlinkability)

### Provider-key vault (RFC-0009 §Vault)

- [ ] `Vault` struct at `crates/octo-wallet/src/vault.rs` with `slots_dir: PathBuf` + in-memory `cache: HashMap<String, EncryptedBlob>`
- [ ] Slot files at `<slots_dir>/<slot_id>.vault` (hex-encoded slot_id; sanitized at API boundary to prevent path traversal)
- [ ] `Vault::put(slot_id, plaintext, passphrase)` — Argon2id(m=64MiB, t=3, p=4) → AES-256-GCM encrypt; `flock(LOCK_EX)` during mutation
- [ ] `Vault::get(slot_id, passphrase)` — `flock(LOCK_SH)` during read; mlock at-rest on Linux, VirtualLock on Windows; returns `DecryptedHandle<'_>` (zeroize-on-drop)
- [ ] `Vault::list() -> Vec<String>` — slot IDs only, no plaintext
- [ ] Error variants live on `WalletError` in `crates/octo-wallet/src/error.rs`, **not** on a `VaultError` enum — `WalletError` is the single crate-wide error type and there is no `VaultError` in the tree. The five variants this box names map onto it as `VaultSlotNotFound(String)`, `VaultDecryptionFailed`, `VaultKdfTimeout`, `Io(#[from] std::io::Error)`, `InvalidSlotId(String)`. Two of the five names are right and gained a payload; three were renamed with a `Vault` prefix. LANDED, modulo the naming drift.
- [ ] Test: save → reload → same key bytes
- [ ] Test: wrong passphrase → `WalletError::VaultDecryptionFailed`
- [ ] Test: path traversal slot_id → `WalletError::InvalidSlotId(String)`

### Starkli-compat keystore

- [ ] `StarkliCompat` keystore impl at `crates/octo-wallet/src/keystore.rs`
- [ ] Format: starkli v0.3+ JSON (Argon2id + chacha20-poly1305)
- [ ] **Cipher divergence note:** Starkli uses chacha20-poly1305, NOT AES-256-GCM as RFC-0102 §Key Storage specifies. Implement BOTH: vault = AES-256-GCM (per RFC-0102 post-amendment); starkli import = chacha20-poly1305 (interop only). Document divergence in RFC-0102 §Starkli Keystore Divergence section (added this mission). **OPEN.** The divergence itself is real and the substrate honours it — `StarkliCompat` implements chacha20-poly1305 and `Vault` implements AES-256-GCM — but the section this box says was "added this mission" does not exist in RFC-0102, which has no such heading. The documentation obligation is unmet; the code obligation is met.
- [ ] `StarkliCompat::import(path) -> IdentityKey` (reads chacha20-poly1305, decrypts, returns Ed25519 seed)
- [ ] `StarkliCompat::export(key, path)` (writes chacha20-poly1305 JSON)
- [ ] Round-trip test with fixture under `crates/octo-wallet/tests/fixtures/starkli-v0.3/`
- [ ] Cross-impl test: export → read with `starkli` CLI if available

### CLI binary

- [ ] `crates/octo-wallet/src/bin/octo-wallet.rs` (binary `octo-wallet`)
- [ ] Subcommands. **As written this box is 30% phantom**: `import --from starkli` and `export --to starkli` were never built — they appear only in the module's own doc comment, which is where a reader would look and conclude the surface exists. `vault get` takes `--slot` and no `--out`; it writes the decrypted bytes to a caller buffer, not a file. The real surface as of 2026-09-30 is `init --node-type <wholesale|self-host|hybrid> --seed-out <path>`, `derive-cap --audience <DID> --channel <id> --seed <path> [--hex|--no-hex]`, `vault put --slot <id> [--stdin]`, `vault get --slot <id>`, `vault list`, and `ask publish` — the last of which is absent from this box entirely, so this mission does not currently own the `ask` subtree it shipped.
- [ ] Tests via `assert_cmd` + `predicates`
- [ ] Vault passphrase prompt uses `rpassword` crate (NEVER argv — visible in `ps` output)
- [ ] Minimum passphrase length enforced at `init`: 12+ chars; dictionary rejection via simple wordlist check. **OPEN, and misfiled as well as unimplemented.** Two separate problems. First, `init` never receives a passphrase — it writes a seed file and returns — so any policy enforced there would be unreachable even once implemented; the passphrase enters at `vault put`, which is where a check would have to live. Second, no length or dictionary check exists anywhere in the tree, so a one-character passphrase produces a vault whose only protection is Argon2id's work factor. This is the same exposure RFC-0011-x §Adversary Analysis carries as **A7** (weak passphrase), and it is a genuine open item rather than a stale box, which is why it is recorded here instead of being annotated away with the landed groups.

### RFC-0102 follow-up amendments

- [ ] Add §Starkli Keystore Divergence section to RFC-0102 (chacha20-poly1305 vs AES-256-GCM, rationale: interop with starkli ecosystem). Still outstanding — this and the box above name the same undone obligation, and the paragraph above is the second record of it.
- [ ] Add §Implementation Companion Guide cross-link to `docs/07-developers/wallet-implementation-guide.md` (author per BLUEPRINT.md "Tools" section if not yet present)

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green (existing octo-core/octo-cli tests still pass)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] `cargo doc --workspace --no-deps` builds without broken-doc-links warnings

## Dependencies

None — first session.

## Type Coverage

Per BLUEPRINT.md Mission template, the RFC-0009 specification defines the following types; this mission implements them as listed:

| RFC-0009 Type                                               | Implemented By                                                              |
| ----------------------------------------------------------- | --------------------------------------------------------------------------- |
| `Identity` struct (with `canonical_ser`)                    | This mission (in `crates/octo-wallet/src/identity.rs`)                      |
| `IdentityKey` newtype                                       | This mission (in `crates/octo-wallet/src/identity.rs`)                      |
| `NodeType` enum                                             | This mission (in `crates/octo-wallet/src/node.rs`)                          |
| `Vault` struct                                              | This mission (in `crates/octo-wallet/src/vault.rs`)                         |
| `EncryptedBlob` struct                                      | This mission (in `crates/octo-wallet/src/vault.rs`)                         |
| `DecryptedHandle<'a>` struct                                | This mission (in `crates/octo-wallet/src/vault.rs`)                         |
| `WalletError` enum (the vault's five error cases)           | This mission (in `crates/octo-wallet/src/error.rs`; no `VaultError` exists) |
| `CapabilityKey` newtype                                     | This mission (in `crates/octo-wallet/src/capability.rs`)                    |
| `derive_capability_key` fn                                  | This mission (in `crates/octo-wallet/src/capability.rs`)                    |
| `holder_sign` fn                                            | This mission (in `crates/octo-wallet/src/capability.rs`)                    |
| `StarkliCompat` keystore                                    | This mission (in `crates/octo-wallet/src/keystore.rs`)                      |
| Full macaroon v1 capability token (Caveat, Discharge, etc.) | **NOT this mission** — RFC-0957 (S02)                                       |
| Identity ↔ Stark Curve keypair wallet metadata              | **NOT this mission** — out of scope; tracked separately                     |

## Location

- New crate: `crates/octo-wallet/`
- RFC edits this mission:
  - `rfcs/accepted/numeric/0102-wallet-cryptography.md` (add §Starkli Keystore Divergence)
  - `rfcs/accepted/process/0009-identity-management.md`
- Plan: `docs/plans/2026-07-19-session-01-wallet-foundation.md`

## Complexity

Medium-High (new crate, dual substrate, vault crypto, CLI, RFC additions)

## Reference

- `docs/plans/2026-07-19-identity-master-plan.md` § 0 BLUEPRINT Workflow Gate
- `docs/plans/2026-07-19-session-01-wallet-foundation.md` § 0 BLUEPRINT Workflow Gate + § 3 Steps 1-8
- RFC-0009 (Process: Identity Management) — mission's primary spec authority
- RFC-0102 (Numeric: Wallet Cryptography) — sibling spec authority
- Existing scaffolding: `crates/octo-wallet/Cargo.toml` + `crates/octo-wallet/src/lib.rs` (preview per user direction 2026-07-19; finalized with stub modules in this mission)

## Security Review Status

- Round 1 adversarial review (2026-07-19): completed; all CRITICAL + HIGH findings resolved in RFC-0009 amendments + scaffolding fixes. See `docs/reviews/round-1-session-01-adversarial.md` (created this session, per BLUEPRINT.md ephemeral review artifact policy).
- 5-Question Adversary Test (RFC-0009 §Adversary Analysis): 5 findings (A1-A5), all resolved or mitigated.
- Threat model: see RFC-0009 §Security Considerations.

## Claimant

CLAIMED 2026-07-20 (mission promoted from Open to Claimed per BLUEPRINT Mission Lifecycle; both RFC-0102 + RFC-0009 reached Accepted 2026-07-20)

## Pull Request

(none yet — implementation pending per S01 plan §3 Steps 1-8 sequencing)

## Notes

- **Substrate split (architectural decision 2026-07-19):** RFC-0102 = Stark Curve; RFC-0009 = Ed25519; RFC-0957 (planned S02) = capability token. Each RFC owns its substrate; wallet crate hosts both.
- **Vault vs Keystore:** Vault = provider-key storage (slot-based, file-per-slot on disk at `~/.config/cipherocto/vault/<slot>.vault`); Keystore = identity-key storage (starkli-compatible JSON, chacha20-poly1305 + Argon2id for interop). Distinct concerns; vault uses AES-256-GCM (per RFC-0102 amendment), starkli uses chacha20-poly1305.
- **Scaffolding policy:** preview files at `crates/octo-wallet/Cargo.toml` + `src/lib.rs` + `src/bin/octo-wallet.rs` exist uncommitted; stubbed with empty module bodies that compile but `unimplemented!()` at runtime. Finalized during claim/implementation phase.
- **RFC-0957 dependency:** capability token format RFC planned for S02. S01 only needs the `holder_sign` primitive; full macaroon implementation in S02.
- **Mission decomposition (rule trigger acknowledged and overridden):** per BLUEPRINT.md §Multi-Mission Decomposition, "RFC has >10 specification types → decompose". The type table above lists 13, of which **11** belong to this mission and 2 are explicitly assigned elsewhere. 11 is above the threshold, so the rule fires and this mission did not follow it. The note originally claimed the RFC defines 12 types and that this mission handles all 12; neither number matches the table. The override is recorded here rather than silently absorbed: the types are cohesive in that they all resolve to the one `octo-wallet` crate, and the two out-of-scope rows are already separated, so the decomposition the rule asks for is partly done by assignment rather than by split. Anyone re-deriving the count should count the table, not this sentence.
- **Identity struct phase-out:** older `Identity { id: String, public_key: [u8; 32] }` struct is replaced by `IdentityKey` newtype wrapping ed25519-dalek. Migration path: octo-core/src/identity.rs deleted; callers updated to use `octo_wallet::IdentityKey`.
