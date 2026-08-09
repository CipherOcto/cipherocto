# Mission: 0957-ext-macaroon — Extract Macaroon to Per-Extension Crate (RFC-0957)

## Status

Claimed (2026-08-09). RFC-0957 + RFC-0965 amendments mandate per-extension crate layout. Claimant: @cipherocto.

## RFC

RFC-0957 (Economics): Capability Token Format
RFC-0965 (Economics): Capability Extension Format

**BLUEPRINT gate note:** Both RFCs are Accepted. Mission 0957-ext-macaroon implements the per-extension crate extraction mandate.

This mission extracts `crates/octo-wallet/src/capability/macaroon.rs` (1905 lines) + `caveat.rs` (1382 lines) + `discharge.rs` (962 lines) + related macaroon substrate into a dedicated `crates/octo-cap-macaroon/` crate. Wallet core becomes thin substrate (identity, HSM, capability registry).

## Summary

Extract the macaroon v1 capability type into a dedicated `crates/octo-cap-macaroon/` crate. Migrate all callers (proxy.rs, capability/macaroon.rs consumers, capability/zk_mint.rs for `mint_with_zk`, etc.) to the new crate. Register `CapabilitySpec` impl via plugin in `octo-wallet::capability::CapabilityRegistry`. Wallet core becomes thin substrate; macaroon substrate becomes a plugin.

## Acceptance Criteria

### Top-level: Extraction

- [x] NEW: `crates/octo-cap-macaroon/` crate created with `Cargo.toml` + `src/lib.rs`
- [x] Phase 1 extraction: `hmac_blake3`, `macaroon_id`, `MacaroonId`, `CAPABILITY_ID_DOMAIN`, `MACAR_ID_DOMAIN` constants moved from `crates/octo-wallet/src/capability/macaroon.rs` (lines 40-76) to `crates/octo-cap-macaroon/src/lib.rs`
- [x] `octo-wallet::capability::macaroon` re-exports the moved items via `pub use octo_cap_macaroon::{...}` for backward compatibility
- [x] `octo-wallet` `Cargo.toml` adds `octo-cap-macaroon = { path = "../octo-cap-macaroon" }` dep
- [x] Workspace auto-registers new crate via `crates/*` glob (no Cargo.toml edit needed)
- [x] All existing macaroon tests pass: `cargo test -p octo-cap-macaroon --lib` = 8/8; `cargo test -p octo-wallet --lib` = 320/320 (zero regressions)
- [x] `cargo clippy -p octo-cap-macaroon -p octo-wallet --all-targets -- -D warnings` clean
- [x] `cargo fmt --check` clean

### Phase 2 follow-on (not in scope of this session)

- Migrate `Macaroon` struct + `verify` / `attenuate` methods (~lines 97-428 of macaroon.rs) — depends on `caveat::Caveat` type which lives in a separate file
- Migrate `caveat.rs` (1382 lines) — 22 caveat types + `CaveatName` + `CaveatSet` + `RawCaveat`
- Migrate `discharge.rs` (962 lines) — `EscrowDischargeProvider` + `RevocationDischargeProvider` + `RateLimitDischargeProvider` + `ChannelProvider` trait
- Migrate `wire.rs` (439 lines) — wire format v1 + canonical serialization
- Implement `CapabilitySpec` trait for plugin registration

### Cross-crate compat

- [ ] `cargo build --workspace --features full` green
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` green

### Adversary coverage

- [ ] Attenuation invariant (RFC-0957 §3.5) preserved across crate boundary: tests in `crates/octo-cap-macaroon/tests/attenuation.rs` pass
- [ ] Wire format byte-exact across crate boundary: `cargo test -p octo-cap-macaroon --test wire_v1_roundtrip` + `cargo test -p octo-wallet --tests capability/wire` both pass
- [ ] Debug redaction preserved: `CapabilityToken`, `Caveat`, `HolderRecord` all retain manual redacting `Debug` impls (per RFC-0957-A1 §Security)

## Dependencies

**Requires:**

- RFC-0957 — per-extension crate layout mandate
- RFC-0965 — per-extension crate layout mandate
- `crates/octo-wallet/src/capability/{macaroon,caveat,discharge,wire,holder}.rs` — source code to extract

**Mission gates:**

- RFC-0957 + RFC-0965 amendments (committed 2026-08-08)
- Workspace `Cargo.toml` member registration
- Mission `0957-ext-zk-crate.md` (parallel; both extract different capability types)

**Not Requires:**

- RFC-0871 acceptance (per-extension crate layout is independent of NodeEnvelope work)

## Implementation Guide

- NEW crate: `crates/octo-cap-macaroon/` with standard Rust crate layout
- Migrate source files via `git mv` preserving history:
  - `crates/octo-wallet/src/capability/macaroon.rs` → `crates/octo-cap-macaroon/src/mint.rs`
  - `crates/octo-wallet/src/capability/caveat.rs` → `crates/octo-cap-macaroon/src/caveat.rs`
  - `crates/octo-wallet/src/capability/discharge.rs` → `crates/octo-cap-macaroon/src/discharge.rs`
  - `crates/octo-wallet/src/capability/wire.rs` → `crates/octo-cap-macaroon/src/wire.rs`
  - `crates/octo-wallet/src/capability/holder.rs` → `crates/octo-cap-macaroon/src/holder.rs`
- Update `use` statements in all consumers (proxy.rs, capability/zk_mint.rs, etc.)
- Add `pub use octo_cap_macaroon::{CapabilityToken, Caveat, ...}` re-exports in `octo-wallet::capability` module for backward compat
- CapabilitySpec impl: `crates/octo-cap-macaroon/src/spec.rs`

## Decomposition Rationale

Per-extension crate extraction is multi-file (1 NEW crate + 5 source migrations + re-exports + tests). Below the BLUEPRINT §Multi-Mission Decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

## Claimant

@cipherocto (claimed 2026-08-09)

## Pull Request

(unset)

## Notes

- This mission is the FIRST per-extension crate extraction (the macaroon v1 reference impl). Future extractions (`0957-ext-zk`, `0957-ext-federation`, etc.) follow the same pattern.
- Coordinate with parallel mission `0957-ext-zk-crate.md` (both touch `crates/octo-wallet/src/capability/zk_mint.rs`).
- Per `[[stoolap-general-purpose-db]]` red line: this is workspace-side extraction (NOT a fork PR); per-extension crates live in cipherocto workspace.
- Per `[[cargo-fmt-workflow]]` + `[[feedback_clippy_zero_warnings]]`: `cargo fmt` + `cargo clippy -D warnings` green before commit.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0957 + RFC-0965 amendments mandate per-extension crate layout. Mission captures extraction scope. |

Last Updated: 2026-08-08
Version: 0.1