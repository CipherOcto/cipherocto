# Mission: 0957-ext-macaroon — Extract Macaroon to Per-Extension Crate (RFC-0957)

## Status

Phase 2 closed (2026-08-09). RFC-0957 + RFC-0965 amendments mandate per-extension crate layout.

## RFC

RFC-0957 (Economics): Capability Token Format
RFC-0965 (Economics): Capability Extension Format

**BLUEPRINT gate note:** Both RFCs are Accepted. Mission 0957-ext-macaroon implements the per-extension crate extraction mandate.

This mission extracts `crates/octo-wallet/src/capability/macaroon.rs` + `caveat.rs` + `discharge.rs` + related macaroon substrate into a dedicated `crates/octo-cap-macaroon/` crate. Wallet core becomes thin substrate (identity, HSM, capability registry).

## Summary

Extract the macaroon v1 capability type into a dedicated `crates/octo-cap-macaroon/` crate. Migrate all callers (proxy.rs, capability/macaroon.rs consumers, capability/zk_mint.rs for `mint_with_zk`, etc.) to the new crate. Register `CapabilitySpec` impl via plugin in `octo-wallet::capability::CapabilityRegistry`. Wallet core becomes thin substrate; macaroon substrate becomes a plugin.

## Acceptance Criteria

### Top-level: Extraction

- [x] NEW: `crates/octo-cap-macaroon/` crate created with `Cargo.toml` + `src/lib.rs` (Phase 1 commit `f123fe1b`)
- [x] Phase 1 extraction: `hmac_blake3`, `macaroon_id`, `MacaroonId`, `CAPABILITY_ID_DOMAIN`, `MACAR_ID_DOMAIN` constants moved from `crates/octo-wallet/src/capability/macaroon.rs` (lines 40-76) to `crates/octo-cap-macaroon/src/lib.rs`
- [x] Phase 2 extraction: `caveat.rs` (1382 lines, 24 caveat variants) + `Macaroon` struct + `mint`/`attenuate`/`verify_signature`/`verify_full`/`extend_chain`/`compute_capability_id` + `CapabilityCatalog` trait + `CapabilityGossip` trait + `InMemoryCatalog` + `TransportDeliveryCatalog` + `MacaroonError` + `CatalogGossipError` + `check_wrapped_chain`/`check_wrapped_depth` + proptest property tests moved from `crates/octo-wallet/src/capability/{caveat,macaroon}.rs` to `crates/octo-cap-macaroon/src/{caveat,macaroon}.rs` (Phase 2 commit `9d28ddc8`)
- [x] `octo-wallet::capability::{caveat,macaroon}` modules converted to re-export shims: `pub use octo_cap_macaroon::{caveat,macaroon}::*;` (backward compat for all call sites)
- [x] `octo-wallet` `Cargo.toml` adds `octo-cap-macaroon = { path = "../octo-cap-macaroon" }` dep
- [x] Workspace auto-registers new crate via `crates/*` glob (no Cargo.toml edit needed)
- [x] All existing macaroon tests pass: `cargo test -p octo-cap-macaroon --lib` = 97/97; `cargo test -p octo-wallet --lib` = 231/231 (89 migrated out, zero regressions)
- [x] Zero regressions across downstream: `cargo test -p quota-router-core --lib` = 1529/1529; `cargo test -p octo-wallet-node --lib` = 17/17; `cargo test -p octo-paid-query --lib` = 15/15
- [x] `cargo clippy -p octo-cap-macaroon -p octo-wallet --all-targets -- -D warnings` clean
- [x] `cargo fmt -p octo-cap-macaroon -p octo-wallet --check` clean
- [x] `cargo build --workspace` green

### Phase 2b follow-on (deferred to subsequent sessions)

- [ ] Migrate `CapabilityToken` struct + `CapabilitySpec` trait into `octo-cap-macaroon`
- [ ] Migrate `crates/octo-wallet/src/capability/wire.rs` (439 lines, canonical serialization)
- [ ] Migrate `crates/octo-wallet/src/capability/discharge.rs` (962 lines, 4 discharge providers)
- [ ] Eliminate `quota-router-storage` cross-layer dep (move `HolderRegistry` accessor or define local trait)
- [ ] Eliminate `octo-transport` cross-layer dep (move `TransportDeliveryCatalog` to transport glue crate)

### Cross-crate compat

- [x] `cargo build --workspace` green
- [x] `cargo test -p octo-cap-macaroon --lib` 97/97 green
- [x] `cargo test -p octo-wallet --lib` 231/231 green
- [x] `cargo test -p quota-router-core --lib` 1529/1529 green (zero regressions)
- [x] `cargo test -p octo-wallet-node --lib` 17/17 green (zero regressions)
- [x] `cargo test -p octo-paid-query --lib` 15/15 green (zero regressions)
- [x] `cargo clippy -p octo-cap-macaroon -p octo-wallet --all-targets -- -D warnings` clean
- [x] `cargo fmt -p octo-cap-macaroon -p octo-wallet --check` clean

### Adversary coverage

- [x] Attenuation invariant (RFC-0957 §3.5) preserved across crate boundary: `caveat.rs::tests` cover `set_subsumes` monotonicity + `macaroon.rs::tests::verify_full_enforces_attenuation_subsumption` + 10K prop test for random monotonic caveat sequences
- [ ] Wire format byte-exact across crate boundary: deferred to Phase 2b (wire.rs migration)
- [x] Debug redaction preserved: `CapabilityToken`, `Caveat`, `HolderRecord` retain manual redacting `Debug` impls (carried verbatim from octo-wallet source)

## Implementation Guide (Phase 1 + Phase 2 history)

**Phase 1 (commit `f123fe1b`):** crypto foundation
- `crates/octo-cap-macaroon/` with `src/lib.rs` owning `hmac_blake3` + `macaroon_id` + `MacaroonId` + domain constants
- Re-export shim in `octo-wallet/src/capability/macaroon.rs`

**Phase 2 (commit `9d28ddc8`):** substrate migration
- `cp caveat.rs → octo-cap-macaroon/src/caveat.rs` (verbatim copy; no internal cross-refs)
- `cp macaroon.rs → octo-cap-macaroon/src/macaroon.rs` (verbatim copy + surgical import edits)
- `octo-cap-macaroon/Cargo.toml` deps added: `serde`, `serde_json`, `rand`, `async-trait`, `thiserror`, `cipherocto-encoding`, `quota-router-storage` (cross-layer cost, documented), `octo-transport` (cross-layer cost, documented)
- `pub use crate::{hmac_blake3, macaroon_id, MacaroonId, CAPABILITY_ID_DOMAIN, MACAR_ID_DOMAIN};` added to `octo-cap-macaroon/src/macaroon.rs` so `pub use octo_cap_macaroon::macaroon::*;` glob catches everything
- `pub(crate) fn extend_chain` → `pub fn extend_chain` (cross-crate use from `octo-wallet::capability::mod.rs::CapabilityToken::mint`)
- `InMemoryCatalog` + `impl CapabilityCatalog for InMemoryCatalog` + `impl CapabilityGossip for TransportDeliveryCatalog` removed from `#[cfg(test)]` gate (now always pub for downstream test fixtures)
- `octo-wallet/src/capability/{caveat,macaroon}.rs` rewritten as re-export shims (`pub use octo_cap_macaroon::{caveat,macaroon}::*;`)
- Cargo.toml lints: `#![allow(missing_docs)]` (4681 lines of copied substrate; original `octo-wallet` source lacked full docs; polish pass deferred); clippy allows `empty_line_after_doc_comments`, `doc_lazy_continuation`

## Decomposition Rationale

Per-extension crate extraction is multi-file (1 NEW crate + multiple source migrations + re-exports + tests). Below the BLUEPRINT §Multi-Mission Decomposition threshold for Phase 2 itself; Phase 2b follow-on is the next decomposition (wire + discharge + CapabilityToken).

## Claimant

@cipherocto (Phase 1 + Phase 2 claimed 2026-08-09)

## Pull Request

Phase 1: `f123fe1b` (local; push + remote writes await user instruction per [[git-workflow]])
Phase 2: `9d28ddc8` (local; push + remote writes await user instruction per [[git-workflow]])

## Closure Summary

**Phase 1 (commit `f123fe1b`):** NEW `crates/octo-cap-macaroon/` Layer-4 extension crate owns the pure-crypto foundation (HMAC-BLAKE3 + macaroon_id + domain constants). 8 new tests; `octo-wallet` 320/320 zero regressions.

**Phase 2 (commit `9d28ddc8`):** Migrated 3287 lines of macaroon substrate (caveat DSL + Macaroon struct + Catalog traits + InMemoryCatalog + TransportDeliveryCatalog) from `octo-wallet/src/capability/{caveat,macaroon}.rs` into `octo-cap-macaroon/src/{caveat,macaroon}.rs`. Backward compat via re-export shims — zero call-site changes across `octo-wallet`, `quota-router-core`, `octo-wallet-node`, `octo-paid-query`, etc.

**Phase 2 deliverables:**
- 24 caveat variants (RFC-0957 §3.1 + RFC-0965 §3) + CaveatName + CaveatSet + RawCaveat + set_subsumes
- Macaroon struct + mint + extend_chain + attenuate + verify_signature + verify_full + compute_capability_id (RFC-0957 §3.2 + RFC-0965 §3.7)
- CapabilityCatalog trait + CapabilityGossip trait (split for dyn-compatibility per mission 0959-c3)
- MacaroonError + CatalogGossipError
- InMemoryCatalog + TransportDeliveryCatalog
- check_wrapped_chain + check_wrapped_depth (RFC-0965 §3.7 R7-F1, MAX_WRAPPED_DEPTH=16)
- proptest property tests (10K random chains + HMAC avalanche)

**Tests:** `octo-cap-macaroon` 97/97 (8 Phase 1 + 89 migrated), `octo-wallet` 231/231 (89 migrated out), `quota-router-core` 1529/1529 zero regressions, `octo-wallet-node` 17/17 zero regressions, `octo-paid-query` 15/15 zero regressions.

**Honest scope disclosure:** Phase 2 leaves 4 follow-on items for Phase 2b:
1. `CapabilityToken` + `CapabilitySpec` trait (still in `octo-wallet/src/capability/mod.rs`)
2. `wire.rs` (439 lines, canonical serialization — still in `octo-wallet/src/capability/wire.rs`)
3. `discharge.rs` (962 lines, 4 discharge providers — still in `octo-wallet/src/capability/discharge.rs`)
4. Cross-layer dep cleanup: `octo-cap-macaroon` currently depends on `quota-router-storage` (for the optional `HolderRegistry` accessor on `CapabilityCatalog::holder_registry()`) + `octo-transport` (for `TransportDeliveryCatalog`'s `NodeTransport` field). Phase 2b may either relocate the accessor / catalog or define local traits to eliminate these cross-layer edges.

## Notes

- This mission is the FIRST per-extension crate extraction (the macaroon v1 reference impl). Future extractions (`0957-ext-zk`, `0957-ext-federation`, etc.) follow the same pattern.
- Coordinate with parallel mission `0957-ext-zk-crate.md` (both touch `crates/octo-wallet/src/capability/zk_mint.rs`).
- Per `[[stoolap-general-purpose-db]]` red line: this is workspace-side extraction (NOT a fork PR); per-extension crates live in cipherocto workspace.
- Per `[[cargo-fmt-workflow]]` + `[[feedback_clippy_zero_warnings]]`: `cargo fmt` + `cargo clippy -D warnings` green before commit.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0957 + RFC-0965 amendments mandate per-extension crate layout. Mission captures extraction scope. |
| v0.2 | 2026-08-09 | Phase 2 closure: caveat DSL + Macaroon substrate + Catalog traits + InMemoryCatalog migrated. Phase 2b follow-on documented for CapabilityToken / wire / discharge / cross-layer cleanup. |

Last Updated: 2026-08-09
Version: 0.2
