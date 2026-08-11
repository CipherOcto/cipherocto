# Mission: 0957-f-v2-bundle-tv-fixture — Authoritative V2 Bundle Byte-Exact TV

## Status

claimed 2026-08-11 (@claude).

**Substrate:** Mission `0957-f-v2-bundle` LANDED (commit `b6bc190b`)
+ cutover LANDED (commit `7a967095`). `CapabilityBundleV2` +
`CapabilityBundleV2Envelope` are the production wire form.
RFC-0009 §Phase 2 TV slot empty prior to this mission.

## Summary

Author 10 byte-exact test vectors (TV-1..TV-10) covering
`CapabilityBundleV2` + `CapabilityBundleV2Envelope` + `bundle_id`.
Vectors assert precomputed canonical bytes against live derivation,
closing the gap left by 17 inline structural tests in
`crates/octo-cap-macaroon/src/bundle_v2.rs::tests`.

## Scope (and NOT in scope)

**In scope:**
- `crates/octo-cap-macaroon/tests/bundle_v2_tv.rs` (NEW) — 10 TV functions.
- Precomputed borsh bytes for root + child bundles, prefix, bundle_id BLAKE3 digests.
- Boundary chain_depth accept/reject assertions.

**NOT in scope:**
- V1 wire form — V1 substrate removed in `0957-f-v2-bundle-consumer-migration`.
- `tvs/bundle_v2.json` file — V1's `tvs/bundle_v1.json` is dead (no runtime consumer); match precedent, skip.
- Cross-impl conformance suite (other Rust crates) — single-impl tests in this crate only.
- `CapabilityTokenV2` standalone serialization TV (the wire contract is the bundle envelope, not the token in isolation).

## Acceptance Criteria

- [x] NEW: `crates/octo-cap-macaroon/tests/bundle_v2_tv.rs` (10 TV).
- [x] `cargo test -p octo-cap-macaroon --test bundle_v2_tv` green (10/10).
- [x] `cargo test -p octo-cap-macaroon` green (185 inline + 10 new TV).
- [x] `cargo clippy -p octo-cap-macaroon --all-targets -- -D warnings` zero warnings.
- [x] `cargo fmt --all -- --check` clean.
- [x] No `include_bytes!`, no fixture files, no JSON.
- [x] Memory card `memory/mission-0957-f-v2-bundle-tv-fixture-status.md`.
- [x] `missions/claimed/0957-f-v2-bundle.md` AC §5 path reference patched.

## Implementation Guide

1. Create `crates/octo-cap-macaroon/tests/` directory.
2. Add `tests/bundle_v2_tv.rs` — mirror `bundle_v2.rs::tests::v2_root_fixture`/`v2_child_fixture` field shapes locally (those are private `#[cfg(test)]`).
3. Hardcode precomputed bytes as inline `[u8;N]` arrays (NOT loaded from a fixture file).
4. Use the included `#[ignore]` `print_precomputed_bytes` helper to regenerate byte arrays after borsh schema drift.
5. Update `0957-f-v2-bundle.md` AC §5 — replace `tests/fixtures/v2_bundle_tv.json` reference with `tests/bundle_v2_tv.rs`.
6. File memory card.

## Byte-sourcing strategy

1. Author builder fns in the test file.
2. On first commit, run `cargo test -p octo-cap-macaroon --test bundle_v2_tv -- --ignored --nocapture` to print canonical bytes.
3. Hardcode the bytes as inline `[u8;N]` arrays.
4. Each TV is a single `assert_eq!` (mirrors `chain_namespace_tv::tv10` template).
5. On future borsh schema drift, regenerate + commit new bytes via the helper.

RFC-0008 Class A determinism + pinned `borsh = "=1.5.0"` (per `Cargo.toml:50`) guarantees identical bytes across platforms.

## Cross-references

- Mission `0957-f-v2-bundle` (commit `b6bc190b`) — V2 substrate
- Mission `0957-f-v2-bundle-cutover` (commit `7a967095`) — envelope cutover
- `crates/octo-ident/tests/chain_namespace_tv.rs` — TV pattern template (`tv10`)
- RFC-0009 §Phase 2 — V2 wire form spec
- RFC-0008 Class A — determinism taxonomy

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-11 | claimed  | Initial mission file (NOT FILED prior — card `next-session-candidates-2026-08-11.md` flagged as unresolved) |
| v0.2    | 2026-08-11 | closed   | LANDED 2026-08-11. 10/10 TV pass; 185 lib tests unchanged; clippy + fmt clean |