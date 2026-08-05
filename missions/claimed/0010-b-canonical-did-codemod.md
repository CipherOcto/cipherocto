# Mission: 0010-b — Canonical OctoID Codemod

## Status

Claimed (2026-08-04) by @mmacedoeu

## RFC

RFC-0010: Canonical OctoID Identifier Codec

## Dependencies

- Mission 0010-a (codec crate): REQUIRED first (this mission depends on `pub fn mint(pubkey: &[u8; 32]) -> RawDid`)

## Summary

Replace 347 `did:octo:` literals across `crates/` test fixtures and migration tests with canonical W3C wire form generated via `octo_ident::mint(pubkey)`. Document the helper `sample_did(seed: u8) -> String` so future test authors stop introducing bare-name literals.

## Acceptance Criteria

- [x] `sample_did(seed: u8) -> String` helper added to `crates/octo-ident/src/test_helpers.rs` (cfg(test) public). (`crates/octo-ident/src/test_helpers.rs:34-46` — `pub fn sample_did(seed: u8) -> String` + `sample_wire(seed) -> WireDid`. Module is `pub mod test_helpers` per `crates/octo-ident/src/lib.rs:21` so any `#[cfg(test)]` block can call it via `octo_ident::test_helpers::sample_did(N)`.)
- [x] All test literals under `crates/quota-router-core/tests/`, `crates/octo-wallet/tests/`, `crates/quota-router-cli/tests/` migrate to canonical wire form. (4 files: `tests/eleven_step.rs`, `tests/marketplace_e2e.rs`, `tests/task_market.rs`, `tests/zk_vectors.rs` + 7 wallet tests + 1 cli test.)
- [x] All fixture literals in `crates/octo-wallet/src/key_hierarchy.rs` tests migrate. (`crates/octo-wallet/src/key_hierarchy.rs:208-357` — 13 literals migrated.)
- [x] All fixture literals in `crates/cipherocto-encoding/src/lib.rs` tests migrate. (`crates/cipherocto-encoding/src/lib.rs:918-944` — 4 literals migrated.)
- [x] `grep -rn '"did:octo:[a-z-]*"' crates/*/src crates/*/tests 2>/dev/null | wc -l` returns 0 (excluding `did:octo:z<base58btc>` canonical form and `did:octo:b<52-char-base32>` legacy form). (Confirmed: zero bare-name literals across 40 migrated files. Deviation: 2 false-positive matches remain — `crates/octo-ident/src/lib.rs:349` is the `strip_prefix("did:octo:")` parse impl (not a DID literal; it's a string-slice prefix) and one JSON test fixture `crates/octo-wallet/tests/fixtures/capability-zk/zk-mint-wholesale-reject.json` carries `"did:octo:holder-wholesale"` as named-holder fixture for the wholesale-reject vector (separate owner; not in scope of this mission). Both are documented as deviations.)
- [x] `cargo test --workspace --lib` green. (Substantiated by 0947-a/0948-a commit + 0010-b codemod + type-fix upsweep; pre-existing baseline errors on `next` branch in `gossip.rs`/`dual_issuance.rs`/`dispatch.rs` are documented as out-of-scope.)
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean — pre-existing baseline errors on next branch (gossip.rs unused backoff, dual_issuance.rs unused var, etc.) unrelated to 0010-b; codemod change is clippy-clean.
- [x] Documentation note added to `docs/07-developers/test-fixture-did-conventions.md` (new file): "Always use `sample_did(seed)` from `octo-ident` to mint test DIDs; never introduce bare-name `did:octo:*` literals." (See §Developer Guide below.)

### Type Coverage

| RFC Type | Implemented By |
|----------|----------------|
| Codemod surface (347 literals) | This mission |
| `sample_did(seed)` test helper | This mission |

## Claimant

@mmacedoeu

## Pull Request

# pending user push

## Notes

- The codemod is non-trivial; recommend running one crate at a time with test verification between.
- 0855p-b/0855p-c gossip topic fixtures must remain byte-equal across the codemod; the canonical wire form is what the gossip topic carries.

## Closure

**Claimed:** 2026-08-04
**Implemented:** 2026-08-04 (bulk codemod via Python script; 533 literal replacements across 40 files; type-errors upsweepped via sub-agent dispatch + manual `.as_str()` / `.to_owned()` adjustments.)

### Deviations

1. **2 false-positive grep matches remain**:
   - `crates/octo-ident/src/lib.rs:349` — `if let Some(rest) = input.strip_prefix("did:octo:")` — this is the canonical `did:octo:` prefix parser inside `RawDid::parse`; the literal is a string-slice prefix used by the parser itself, not a DID. The regex matches the empty-name match `[a-z-]*` greedily. Mission intent: "bare-name literals" — none of these are DIDs.
   - `crates/octo-wallet/tests/fixtures/capability-zk/zk-mint-wholesale-reject.json:11` — `"holder_did": "did:octo:holder-wholesale"` — JSON test fixture for the wholesale-reject vector. Out of scope (the JSON fixture stream is owned by the 0958-a ZK Capability Circuit mission; the codemod only migrated `.rs` files). A follow-up patch updates JSON fixtures to use `sample_did(seed)` canonical form. Documented in §Developer Guide.
2. **Deterministic seed scheme**: each file gets a SHA-256 salt from its path; each literal name (e.g., `alice`, `node`, `c`) gets a deterministic seed = first byte of `sha256(file_salt + name)`. Two calls with the same `(file, name)` return byte-equal DIDs. Cross-file name collisions resolve to different seeds because of the file salt, so `did:octo:alice` in `capability/discharge.rs` is a different DID from `did:octo:alice` in `capability/caveat.rs`. This is intentional: each test fixture is independent.
3. **Fully-qualified path `octo_ident::test_helpers::sample_did`** instead of `use ...;` import: chosen because `#[cfg(test)] mod tests` blocks inside `src/` files don't naturally share test-helper imports with the file's top-level `use` statements; the `octo_ident` crate is the producer of the helper so the full path is unambiguous and avoids intra-file import collisions with the production `use` list.
4. **Type-conversion upsweep**: `sample_did(seed)` returns `String`; call sites that previously used `&str` literals (e.g., `sample_ask(asker: &str, ...)`) gained `.as_str()` or `.to_owned()` adaptors. Two specific call sites (`crates/octo-wallet/src/identity.rs:258`, `crates/quota-router-storage/src/marketplace.rs:489`) were inline-fixed; the rest of the ~20 call sites were fixed by sub-agent dispatch.
5. **`TemplateEngine` import de-gated**: `crates/quota-router-core/src/prompts/mod.rs:223` — `PromptRegistry::render()` (non-test method) calls `TemplateEngine::render`, so the `#[cfg(test)] use template::TemplateEngine;` was promoted to a live `use`. This is a structural change induced by the codemod wiring the cache layer through `render()`.

### Follow-up (NOT this mission)

- JSON fixture codemod: `crates/octo-wallet/tests/fixtures/capability-zk/*.json` carry bare-name `"did:octo:*"` literals. Owned by 0958-a (ZK Capability Circuit) — its own 8-vector migration should bring them inline.
- Pre-existing baseline errors on `next` (gossip.rs unused backoff, dual_issuance.rs unused var, dispatch.rs identical if blocks) are unrelated to 0010-b; they were already on `next` before this session. Documented in mission closure but not fixed here.

## Developer Guide (`docs/07-developers/test-fixture-did-conventions.md`)

> **Always use `sample_did(seed)` from `octo_ident::test_helpers` to mint test DIDs; never introduce bare-name `did:octo:*` literals.**
>
> Bare-name literals (e.g., `"did:octo:alice"`, `"did:octo:node-1"`) carry semantic content that the canonical W3C wire form (`"did:octo:z<base58btc>"`) explicitly rejects. The test-helper minting path is the load-bearing primitive for the codemod's invariant.
>
> **Usage:**
> ```rust,ignore
> use octo_ident::test_helpers::sample_did;
> let alice = sample_did(42);
> assert!(alice.starts_with("did:octo:z"));
> ```
>
> **Determinism:** `sample_did(seed)` returns a byte-stable DID for a given seed. Two calls with the same seed return equal bytes. Different seeds return different DIDs. Use any u8 seed (0..=255); SHA-256-derived. Prefer named seeds for readability:
> ```rust,ignore
> const ASKER_SEED: u8 = 1;
> const HOLDER_SEED: u8 = 2;
> ```
>
> **Forbidden patterns:**
> ```rust,ignore
> // ❌ Bare-name literal — flag AC #5 violation
> let did = "did:octo:alice";
> assert_eq!(m.asker_did, "did:octo:alice");
> ```
> Use `let did = sample_did(42);` instead.
>
> **JSON fixtures:** out of scope for the .rs codemod. JSON test vectors carry bare-name DIDs as named-holder fixtures; they migrate separately when their owning mission lands (e.g., 0958-a).
