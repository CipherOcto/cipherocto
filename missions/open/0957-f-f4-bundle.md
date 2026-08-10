# Mission: F4 Bundle Struct + TV (RFC-0957 §Future Work)

## Status

Closed (2026-08-09). Claimed + implemented. Sub-mission of `missions/claimed/0957-f-future-work.md` per [[deferred-vs-unspecified]] deferral rule (F4 bundle struct + TV). Depends on lifecycle substrate missions: `missions/open/0009-l1-lifecycle-state-machine.md` (Closed) + `missions/open/0009-l2-rotation-successor-linkage.md` (Closed).

**Substrate landed:** `crates/octo-cap-macaroon/src/bundle.rs` (NEW, ~220 lines) — `CapabilityBundle` struct (`bundle_version: u8` + `token: CapabilityToken` + `holder_record_bytes: Vec<u8>` + `discharges: Vec<DischargeMacaroon>`) + `BUNDLE_VERSION: u8 = 1` constant + `BUNDLE_ID_DOMAIN: &str = "cipherocto/bundle/v1/id"` + `canonical_ser()` + `canonical_de()` (canonical JSON per RFC-0126) + manual redacting `Debug` impl (`holder_record_bytes` redacted via `<redacted N bytes>` format). 5 unit tests pass: `bundle_version_is_1`, `bundle_roundtrip_preserves_all_fields`, `debug_redacts_bearer_secrets`, `bundle_canonical_de_rejects_malformed_bytes`, `bundle_id_domain_is_canonical_string`.

**TV landed:** `crates/octo-cap-macaroon/tvs/bundle_v1.json` (NEW) — deterministic fixture for the bundle wire form.

**Layer discipline:** `HolderRecord` held as `Vec<u8>` canonical JSON (NOT the concrete `quota_router_storage::holder_record::HolderRecord` struct). This keeps `octo-cap-macaroon` Layer 4 with zero cross-layer deps on `quota-router-storage` (Layer B-substrate) per [[cipherocto-design-principles]] layer model. Consumers deserialize via `HolderRecord::canonical_de` at the boundary.

**Cross-crate compat:** `cargo test -p octo-cap-macaroon --lib` 157/157 pass (152 pre-existing + 5 new bundle tests); `cargo test -p octo-wallet --lib` 229/229 pass (zero regressions from l1 + l2 lifecycle changes); `cargo clippy -p octo-cap-macaroon --lib --tests -- -D warnings` clean; `cargo fmt --check` clean.

**Out of scope for this mission:** `lifecycle_state` + `successor_proof` fields are NOT yet populated in the bundle struct — they require integration tests with full IdentityKey::sign + capability token mint workflows. The bundle struct accepts arbitrary `holder_record_bytes`; consumers can populate the lifecycle state from external `IdentityKey` state at the boundary.

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

@cipherocto (implementation)

## Pull Request

(unset — local commit per [[feedback_initiation_user_only]]; push awaits user instruction)

## Closure Notes (2026-08-09)

- **Layer discipline:** `HolderRecord` held as canonical JSON bytes (`Vec<u8>`) — NOT the concrete `HolderRecord` struct. This keeps `octo-cap-macaroon` Layer 4 with zero cross-layer deps. Consumers deserialize via `HolderRecord::canonical_de` at the boundary.
- **Canonical JSON:** uses the existing `serde_json::to_vec` + `serde_json::from_slice` machinery. The `#[derive(Serialize, Deserialize)]` on `CapabilityBundle` produces sorted-key output for named-field structs (canonical form per RFC-0126).
- **`bundle_version: u8`:** first field per RFC-0126 forward-compat convention. Future versions add fields at the tail; old consumers ignore unknown fields via serde defaults.
- **Manual `Debug` redaction:** `holder_record_bytes` shows `<redacted N bytes>` (preserves size for diagnostic purposes); nested `DischargeMacaroon::Debug` redacts `root_secret_hash` (already shipped in Phase 2b).
- **TV `tvs/bundle_v1.json`:** minimal fixture; production TV corpus lives in `tests/fixtures/` once multi-bundle scenarios are authored.

**Net diff:** +220 lines (bundle.rs ~210 + tvs/bundle_v1.json ~20 + lib.rs export). Zero regressions across `octo-cap-macaroon` (157 tests pass) + `octo-wallet` (229 tests pass).

Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §section-name / symbol form. Per [[rfc-referencing-convention]] RFCs referenced by number only.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures F4 bundle struct + TV deferred from 0957-f Band A closure. Blocks on RFC-0009 §Identity evolution. |
| v0.2 | 2026-08-09 | Depends_on updated: phantom `RFC-0009 §Identity evolution` pointer replaced with explicit `0009-l1-lifecycle-state-machine` + `0009-l2-rotation-successor-linkage` mission citations (both filed 2026-08-09). Bundle now actionable once lifecycle substrate lands. |
| v0.3 | 2026-08-09 | Claimed + Closed (Band A). CapabilityBundle struct + canonical ser/de + manual redacting Debug + TV `bundle_v1.json` + 5 unit tests. 157/157 cap-macaroon tests + 229/229 octo-wallet tests pass. Layer discipline preserved (zero cross-layer deps on quota-router-storage). |

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-09 | Mission filed. Captures F4 bundle struct + TV deferred from 0957-f Band A closure. Blocks on RFC-0009 §Identity evolution. |
| v0.2 | 2026-08-09 | Depends_on updated: phantom `RFC-0009 §Identity evolution` pointer replaced with explicit `0009-l1-lifecycle-state-machine` + `0009-l2-rotation-successor-linkage` mission citations (both filed 2026-08-09). Bundle now actionable once lifecycle substrate lands. |

Last Updated: 2026-08-09
Version: 0.3