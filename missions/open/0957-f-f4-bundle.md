# Mission: F4 Bundle Struct + TV (RFC-0957 §Future Work)

## Status

Open (2026-08-09). Sub-mission of `missions/claimed/0957-f-future-work.md` per [[deferred-vs-unspecified]] deferral rule (F4 bundle struct + TV). Depends on lifecycle substrate missions filed 2026-08-09: `missions/open/0009-l1-lifecycle-state-machine.md` (Designated/Active/Revoked + `revoked_at_unix`) + `missions/open/0009-l2-rotation-successor-linkage.md` (Rotating + `successor_proof`).

## RFC

RFC-0957 (Economics): Capability Token Format — Accepted 2026-07-20
RFC-0009 (Identity): Identity Key Format — Accepted 2026-07-20 (§Lifecycle substrate lands via `0009-l1` + `0009-l2` missions)

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
- `missions/open/0009-l1-lifecycle-state-machine.md` — `LifecycleState` enum + `IdentityKey::revoke()` for `lifecycle_state` + `revoked_at` bundle fields
- `missions/open/0009-l2-rotation-successor-linkage.md` — `IdentityKey::begin_rotation()` + successor co-sign helper for `successor_proof` bundle field
- `missions/closed/0957-ext-macaroon-crate.md` — macaroon substrate (Phase 2 + 2b + 2c closed 2026-08-09)
- `missions/claimed/0957-c-holder-registry-impl.md` — `HolderRecord` schema (bundled via `holder_record` field)

**Mission gates:**

- Bundle format depends on lifecycle substrate landing (l1 + l2 missions must close first)
- Stoolap registry substrate (RFC-0957-A1) ships Band A — confirmed active

**Not Requires:**

- RFC-0871 acceptance (independent of NodeEnvelope work)

```yaml
depends_on:
  - 0009-l1-lifecycle-state-machine # LifecycleState + revoke() for bundle fields
  - 0009-l2-rotation-successor-linkage # successor_proof for replay across rotation
  - 0957-ext-macaroon-crate # CapabilityToken + DischargeMacaroon substrate
  - 0957-c-holder-registry-impl # HolderRecord schema
```

Real missions + RFC substrate only. No phantom pointers.

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
- Mission unblocked by `0009-l1` + `0009-l2` lifecycle substrate missions (filed 2026-08-09)
- Per `[[no-phantom-mission-pointers]]`: depends_on YAML cites real missions; `0957-f-future-work.md` §Notes phantom pointer is now resolved
- Per `[[cargo-fmt-workflow]]` + `[[feedback_clippy_zero_warnings]]`: `cargo fmt` + `cargo clippy -D warnings` green before commit

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures F4 bundle struct + TV deferred from 0957-f Band A closure. Blocks on RFC-0009 §Identity evolution. |
| v0.2 | 2026-08-09 | Depends_on updated: phantom `RFC-0009 §Identity evolution` pointer replaced with explicit `0009-l1-lifecycle-state-machine` + `0009-l2-rotation-successor-linkage` mission citations (both filed 2026-08-09). Bundle now actionable once lifecycle substrate lands. |

Last Updated: 2026-08-09
Version: 0.2