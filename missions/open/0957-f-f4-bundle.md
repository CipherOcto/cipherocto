# Mission: F4 Bundle Struct + TV (RFC-0957 §Future Work)

## Status

Open (2026-08-09). Sub-mission of `missions/claimed/0957-f-future-work.md` per [[deferred-vs-unspecified]] deferral rule (F4 bundle struct + TV, blocks on RFC-0009 §Identity evolution).

## RFC

RFC-0957 (Economics): Capability Token Format — Accepted 2026-07-20
RFC-0009 (Identity): Identity Key Format — Accepted 2026-08-02 (§Identity evolution not yet active upstream)

**Sub-mission of:** `missions/claimed/0957-f-future-work.md` (F-series future work sub-mission)

## Summary

Implement the F4 deferred AC from `0957-f-future-work.md` Band A closure (2026-08-06): the bundle struct + test vector. The bundle encodes a portable representation of a `CapabilityToken` + `HolderRecord` + `DischargeMacaroon` triplet for offline archival + replay.

## Acceptance Criteria

### Bundle struct

- [ ] NEW: `CapabilityBundle` struct aggregating:
  - `CapabilityToken` (holder-bound envelope)
  - `HolderRecord` (storage-side PK)
  - `Vec<DischargeMacaroon>` (channel-specific discharges)
- [ ] Manual redacting `Debug` impl (all bearer-secret fields redacted)
- [ ] `serialize_bundle` / `deserialize_bundle` canonical JSON encoding (RFC-0126 §canonical JSON)
- [ ] Versioned: `bundle_version: u8 = 1` discriminator field
- [ ] Tests: `bundle_roundtrip_preserves_all_fields`, `debug_redacts_bearer_secrets`, `bundle_version_is_1`

### Test vector (TV)

- [ ] `tvs/bundle_v1.json` test vector: deterministic fixture with known holder pub + caveats + ask_id
- [ ] `bundle_from_tv` / `bundle_to_tv` round-trip test asserts byte-identical canonical JSON

### Cross-crate compat

- [ ] `cargo test -p octo-cap-macaroon --lib` (bundle lives in the macaroon crate per RFC-0957 §Future Work)
- [ ] `cargo test -p octo-wallet --lib capability` zero regressions
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires:**

- `missions/claimed/0957-f-future-work.md` — F-series future work scope
- `missions/closed/0957-ext-macaroon-crate.md` — macaroon substrate (Phase 2 + 2b + 2c closed 2026-08-09)
- RFC-0009 §Identity evolution — currently NOT active upstream; this mission blocks on its activation

**Mission gates:**

- RFC-0009 §Identity evolution must be active before F4 bundle format can be finalized (bundle fields depend on the active identity primitive)
- Stoolap registry substrate (RFC-0957-A1) ships Band A — confirmed active

**Not Requires:**

- RFC-0871 acceptance (independent of NodeEnvelope work)

## Implementation Guide

- Bundle struct layout:
  ```rust
  pub struct CapabilityBundle {
      pub bundle_version: u8,    // = 1
      pub token: CapabilityToken,
      pub holder_record: HolderRecord,
      pub discharges: Vec<DischargeMacaroon>,
  }
  ```
- Wire format: canonical JSON (RFC-0126); `bundle_version` first field for forward-compat
- Bundle lives in `crates/octo-cap-macaroon/src/bundle.rs` (or new `crates/octo-cap-macaroon/src/future.rs`)

## Decomposition Rationale

Single-file mission: 1 NEW struct + canonical ser + tests. Below BLUEPRINT §Multi-Mission Decomposition threshold.

## Claimant

@unassigned (per `[[feedback_initiation_user_only]]` — user initiates the claim)

## Pull Request

(unset)

## Notes

- Mission captured in `0957-f-future-work.md` §F4 deferral note 2026-08-06
- Mission blocks on RFC-0009 §Identity evolution upstream activation
- Per `[[no-phantom-mission-pointers]]`: mission file now exists; the phantom pointer is now resolved
- Per `[[cargo-fmt-workflow]]` + `[[feedback_clippy_zero_warnings]]`: `cargo fmt` + `cargo clippy -D warnings` green before commit

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures F4 bundle struct + TV deferred from 0957-f Band A closure. Blocks on RFC-0009 §Identity evolution. |

Last Updated: 2026-08-09
Version: 0.1