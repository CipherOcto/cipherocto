# Mission: 0010-d — Wallet Audience Validation (RFC-0010 F4 Gap Closure)

## Status

Claimed (2026-08-09). RFC-0010 amendment adds F4 (Wallet audience validation) to the Future Work section, prompted by the 2026-08-08 specialized node protocol research (`docs/research/2026-08-08-specialized-node-protocol-research.md`). Claimant: @cipherocto.

## RFC

RFC-0010 (Process): Canonical OctoID Identifier Codec

**BLUEPRINT gate note:** RFC-0010 is Accepted. Mission 0010-d implements the F4 future-work item.

This mission closes the wallet-side gap surfaced by RFC-0010 §Motivation (the `AudienceId::from_str` accepts any non-empty string) and is the foundational piece of the broader specialized node protocol work (RFC-0871, Draft) which requires `from_did: WireDid` to be validated at every envelope boundary.

## Summary

Update `crates/octo-wallet/src/identity.rs::AudienceId::from_str` to call `octo_ident::CanonicalCodec::parse(s, false)` instead of accepting any non-empty string. Mirror the parse step in every other `AudienceId::new`/constructor entry point. Add unit tests covering: canonical wire form accepted, legacy `did:octo:b<base32>` rejected post-deprecation window, malformed input rejected, empty input rejected.

The change is intentionally surgical: ONE source file modification (`identity.rs`); minimal blast radius. RFC-0010 already provides the canonical codec; this mission wires wallet into it.

## Acceptance Criteria

### Top-level: RFC-0010 F4 gap closure

- [x] `crates/octo-wallet/src/identity.rs::AudienceId::from_str` calls `octo_ident::CanonicalCodec::parse(s, allow_legacy_bare: false)`
- [x] `allow_legacy_bare: false` for production paths; no `#[cfg(test)]` legacy fixtures added
- [x] Error type surfaces `DidError` discriminant via `WalletError::InvalidAudienceId(String)` wrapper
- [x] `derive_capability_key(audience_did: &AudienceId, ...)` unchanged; uses already-validated `AudienceId` (parse-time validation is sufficient — derive_capability_key does not re-parse)
- [x] Unit tests added:
  - `canonical_did_audience_accepted` — `did:octo:z<base58btc>` accepted
  - `legacy_did_b_form_rejected_post_deprecation` — `did:octo:b<base32>` rejected
  - `bare_did_octo_suffix_rejected_post_window` — `did:octo:evil` rejected (LegacyFormExpired)
  - `wrong_prefix_did_rejected` — `did:foo:...` rejected (UnrecognizedShape)
  - `malformed_did_truncated_rejected` — `did:octo:zabc` rejected (InvalidLength)
  - `empty_audience_rejected` — empty string rejected (existing test)
- [x] No regressions: 320/320 octo-wallet unit tests pass; existing `derive_independent_audiences` test updated to use two canonical sample_did() outputs instead of legacy form
- [x] `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean
- [x] `cargo fmt --check` clean

### Out of scope (tracked by RFC-0871 Phase 2)

- Wallet signing via `HsmAdapter` (separate gap; RFC-0871 Phase 2 work)
- Wallet node `NetworkReceiver` impl (RFC-0871 Phase 2)
- Per-extension crate extraction for `CapabilityCatalog` (RFC-0871 Phase 4)
- `WalletQuery` / `WalletTransport` abstractions (REJECTED per RFC-0871 — use existing `NodeTransport`)

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` green

## Dependencies

**Requires:**

- RFC-0009 — Identity substrate, DID format definition
- RFC-0010 — Canonical DID codec, `crates/octo-ident::DidCodec` trait, `CanonicalCodec::parse()` function

**Mission gates:**

- `missions/claimed/0010-a-canonical-did-codec-crate.md` — Band A closed 2026-08-06; `crates/octo-ident` ships the codec
- `missions/claimed/0010-b-canonical-did-codemod.md` — Band A closed 2026-08-06; production code uses canonical form

**Not Requires:**

- RFC-0871 — this mission is foundational but does not depend on the full envelope RFC; closure of F4 enables RFC-0871 §Specification, doesn't block on it
- HSM wiring — separate gap, tracked independently

## Implementation Guide

- `crates/octo-ident/src/lib.rs` — read `CanonicalCodec::parse` and `WireDid::new` to confirm shape
- `crates/octo-wallet/src/identity.rs` — modify `AudienceId::from_str` and any other constructor; add unit tests
- Test fixtures: `crates/octo-ident/src/test_helpers.rs::sample_did` returns canonical wire form, already accepted under `allow_legacy_bare: false`

## Decomposition Rationale

RFC-0010 F4 is a 1-source-file modification. Below the BLUEPRINT §Multi-Mission Decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

## Claimant

@cipherocto (claimed 2026-08-09)

## Pull Request

d9070a78 (local; push + remote writes await user instruction per [[git-workflow]])

## Closure Summary

Implementation landed in commit `d9070a78` on `next` branch. 3 files changed,
184 insertions / 10 deletions. `AudienceId::from_str` now routes through
`octo_ident::CanonicalCodec::parse(s, false)` per RFC-0010 v1.2 F4. Legacy
`did:octo:b<base32>` form, bare `did:octo:<suffix>`, wrong prefixes, and
truncated payloads all rejected at parse time. `octo-ident` moved from
`[dev-dependencies]` to `[dependencies]` (was misplaced by mission 0010-b;
runtime dep required by 0010-d for parse-time validation).

320/320 unit tests pass; clippy -D warnings clean; fmt clean; workspace builds.

Next serial-path mission: **`0957-ext-macaroon-crate`** — extract macaroon
+ caveat + discharge types from `crates/octo-wallet/src/capability/macaroon.rs`
into a standalone `crates/octo-cap-macaroon/` Layer-4 extension crate;
registers `CapabilitySpec` impl per RFC-0965. Depends on `0010-d` having
landed (so the extension crate can use `CanonicalCodec::parse` for its own
audience validation).

## Notes

- This mission was surfaced by the 2026-08-08 specialized node protocol research + RFC-0871. It is **independent of RFC-0871 acceptance** — the wallet DID validation gap is real today and exists regardless of the broader envelope work. Claiming this mission before RFC-0871 reaches Accepted is encouraged.
- Mission `0010-c-canonical-did-deprecation.md` is the open follow-up to close the legacy `did:octo:b<base32>` form. Coordinate: this mission (`0010-d`) tightens the parse path; `0010-c` flips the default `allow_legacy_bare` flag. Both are small surgical changes.
- Per `[[deferred-vs-unspecified]]` named-owner rule: this mission has a concrete target (RFC-0010 F4 closure). No further deferral.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0010 amendment adds F4; mission captures gap closure scope. Cross-references RFC-0871 + `docs/research/2026-08-08-specialized-node-protocol-research.md`. |

Last Updated: 2026-08-08
Version: 0.1