# Mission: Wallet Foundation

## Status

Claimed (2026-07-20)

## RFC

- RFC-0102 (Numeric): Wallet Cryptography — Stark Curve substrate (ACCEPTED 2026-07-20; KDF PBKDF2 → Argon2id)
- RFC-0009 (Process): Identity Management — Ed25519 identity substrate (ACCEPTED 2026-07-20; promoted 2026-07-19 from Planned; §Node, §Vault, §Capability Keys, §Identity Key Format added; §Roles/§Adversary/§Lifecycle/§Determinism/§Security/§Implicit Assumptions/§Dependency Validation sections added)

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
- [ ] `VaultError` enum: `SlotNotFound`, `DecryptionFailed`, `KdfTimeout`, `Io(std::io::Error)`, `InvalidSlotId`
- [ ] Test: save → reload → same key bytes
- [ ] Test: wrong passphrase → `VaultError::DecryptionFailed`
- [ ] Test: path traversal slot_id → `VaultError::InvalidSlotId`

### Starkli-compat keystore

- [ ] `StarkliCompat` keystore impl at `crates/octo-wallet/src/keystore.rs`
- [ ] Format: starkli v0.3+ JSON (Argon2id + chacha20-poly1305)
- [ ] **Cipher divergence note:** Starkli uses chacha20-poly1305, NOT AES-256-GCM as RFC-0102 §Key Storage specifies. Implement BOTH: vault = AES-256-GCM (per RFC-0102 post-amendment); starkli import = chacha20-poly1305 (interop only). Document divergence in RFC-0102 §Starkli Keystore Divergence section (added this mission).
- [ ] `StarkliCompat::import(path) -> IdentityKey` (reads chacha20-poly1305, decrypts, returns Ed25519 seed)
- [ ] `StarkliCompat::export(key, path)` (writes chacha20-poly1305 JSON)
- [ ] Round-trip test with fixture under `crates/octo-wallet/tests/fixtures/starkli-v0.3/`
- [ ] Cross-impl test: export → read with `starkli` CLI if available

### CLI binary

- [ ] `crates/octo-wallet/src/bin/octo-wallet.rs` (binary `octo-wallet`)
- [ ] Subcommands: `init --node-type <wholesale|self-host|hybrid>`, `import --from starkli --path <keystore.json>`, `export --to starkli --out <keystore.json>`, `derive-cap --audience <DID> --channel <id>`, `vault put --slot <id>`, `vault get --slot <id> --out <path>`, `vault list`
- [ ] Tests via `assert_cmd` + `predicates`
- [ ] Vault passphrase prompt uses `rpassword` crate (NEVER argv — visible in `ps` output)
- [ ] Minimum passphrase length enforced at `init`: 12+ chars; dictionary rejection via simple wordlist check

### RFC-0102 follow-up amendments

- [ ] Add §Starkli Keystore Divergence section to RFC-0102 (chacha20-poly1305 vs AES-256-GCM, rationale: interop with starkli ecosystem)
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

| RFC-0009 Type | Implemented By |
|---------------|----------------|
| `Identity` struct (with `canonical_ser`) | This mission (in `crates/octo-wallet/src/identity.rs`) |
| `IdentityKey` newtype | This mission (in `crates/octo-wallet/src/identity.rs`) |
| `NodeType` enum | This mission (in `crates/octo-wallet/src/node.rs`) |
| `Vault` struct | This mission (in `crates/octo-wallet/src/vault.rs`) |
| `EncryptedBlob` struct | This mission (in `crates/octo-wallet/src/vault.rs`) |
| `DecryptedHandle<'a>` struct | This mission (in `crates/octo-wallet/src/vault.rs`) |
| `VaultError` enum | This mission (in `crates/octo-wallet/src/vault.rs`) |
| `CapabilityKey` newtype | This mission (in `crates/octo-wallet/src/capability.rs`) |
| `derive_capability_key` fn | This mission (in `crates/octo-wallet/src/capability.rs`) |
| `holder_sign` fn | This mission (in `crates/octo-wallet/src/capability.rs`) |
| `StarkliCompat` keystore | This mission (in `crates/octo-wallet/src/keystore.rs`) |
| Full macaroon v1 capability token (Caveat, Discharge, etc.) | **NOT this mission** — RFC-0957 (S02) |
| Identity ↔ Stark Curve keypair wallet metadata | **NOT this mission** — out of scope; tracked separately |

## Location

- New crate: `crates/octo-wallet/`
- RFC edits this mission:
  - `rfcs/accepted/numeric/0102-wallet-cryptography.md` (add §Starkli Keystore Divergence)
  - `rfcs/accepted/process/0009-identity-management.md` (DONE 2026-07-19; §Roles, §Adversary, §Lifecycle, §Determinism, §Security, §Implicit Assumptions, §Dependency Validation, §Version History added)
- Plan: `docs/plans/2026-07-19-session-01-wallet-foundation.md`

## Complexity

Medium-High (new crate, dual substrate, vault crypto, CLI, RFC additions)

## Reference

- `docs/plans/2026-07-19-identity-master-plan.md` § 0 BLUEPRINT Workflow Gate
- `docs/plans/2026-07-19-session-01-wallet-foundation.md` § 0 BLUEPRINT Workflow Gate + § 3 Steps 1-8
- RFC-0009 (Process: Identity Management) — ACCEPTED (2026-07-20); mission's primary spec authority
- RFC-0102 (Numeric: Wallet Cryptography) — ACCEPTED (2026-07-20); sibling spec authority
- Existing scaffolding: `crates/octo-wallet/Cargo.toml` + `crates/octo-wallet/src/lib.rs` (preview per user direction 2026-07-19; finalized with stub modules in this mission)

## Security Review Status

- Round 1 adversarial review (2026-07-19): completed; all CRITICAL + HIGH findings resolved in RFC-0009 amendments + scaffolding fixes. See `docs/reviews/round-1-session-01-adversarial.md` (created this session, per BLUEPRINT.md ephemeral review artifact policy).
- 5-Question Adversary Test (RFC-0009 §Adversary Analysis): 5 findings (A1-A5), all resolved or mitigated.
- Threat model: see RFC-0009 §Security Considerations.

## Claimant

CLAIMED 2026-07-20 (mission moved from missions/open/0102-a-wallet-foundation.md to missions/claimed/0102-a-wallet-foundation.md per BLUEPRT Mission Lifecycle; both RFC-0102 + RFC-0009 reached Accepted 2026-07-20)

## Pull Request

(none yet — implementation pending per S01 plan §3 Steps 1-8 sequencing)

## Notes

- **Substrate split (architectural decision 2026-07-19):** RFC-0102 = Stark Curve; RFC-0009 = Ed25519; RFC-0957 (planned S02) = capability token. Each RFC owns its substrate; wallet crate hosts both.
- **Vault vs Keystore:** Vault = provider-key storage (slot-based, file-per-slot on disk at `~/.config/cipherocto/vault/<slot>.vault`); Keystore = identity-key storage (starkli-compatible JSON, chacha20-poly1305 + Argon2id for interop). Distinct concerns; vault uses AES-256-GCM (per RFC-0102 amendment), starkli uses chacha20-poly1305.
- **Scaffolding policy:** preview files at `crates/octo-wallet/Cargo.toml` + `src/lib.rs` + `src/bin/octo-wallet.rs` exist uncommitted; stubbed with empty module bodies that compile but `unimplemented!()` at runtime. Finalized during claim/implementation phase.
- **RFC-0957 dependency:** capability token format RFC planned for S02. S01 only needs the `holder_sign` primitive; full macaroon implementation in S02.
- **Mission decomposition:** RFC-0009 has 12 types defined; per BLUEPRINT.md "Multi-Mission Decomposition" rule, "RFC has >10 specification types → decompose". This mission handles all 12 types in one PR because they form a cohesive unit (wallet crate); future decomposition possible if PR size becomes unwieldy.
- **Identity struct phase-out:** older `Identity { id: String, public_key: [u8; 32] }` struct is replaced by `IdentityKey` newtype wrapping ed25519-dalek. Migration path: octo-core/src/identity.rs deleted; callers updated to use `octo_wallet::IdentityKey`.
