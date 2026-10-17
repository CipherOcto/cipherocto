# Mission: 0957-f-v2-bundle — V2 Bundle Struct (chain_depth + chain_parent)

## Status

claimed (2026-08-11; moved from `missions/open/` to `missions/claimed/`;
RFC-0009 v1.2 Accepted 2026-08-11 unblocks V2 wire form authoring).

**Substrate:** RFC-0009 v1.2 (Accepted 2026-08-11) + Mission
`0957-phase1-fixture-author` (R8 H1 fixture owner).

## Summary

Implement the V2 wire form of `CapabilityBundle` (per RFC-0009 v1.2
§Phase 2). The V2 bundle embeds `chain_depth` + `chain_parent`
fields in the V2 `CapabilityToken` struct. This is the new V2 work — the V1
`CapabilityBundle` (shipped 2026-08-09) does NOT carry chain state.

**Correction (2026-10-12).** This summary originally read "for hierarchical
attenuation chain verification". The fields carry the shape of that work; they do
not perform any of it. No code computes a `chain_parent` binding and no code
verifies one, so the values are reserved space rather than a defence. A reader
who took "for ... verification" as a statement of behaviour would be wrong about
this type. The two type specs below also drifted from what landed: the mission
specified `chain_parent: Option<[u8; 32]>`, and the shipped field is
`chain_parent: [u8; 32]` — a plain array with no absence case, which is why every
production mint writes the zero value.

## V1 → V2 differences

- **V1** (`CapabilityBundle { bundle_version: u8 = 1, token:
CapabilityToken, holder_record_bytes, discharges }`): shipped
  2026-08-09 in `crates/octo-cap-macaroon/src/bundle.rs`. No
  chain state.
- **V2** (`CapabilityBundleV2 { bundle_version: u8 = 2, token_v2:
CapabilityTokenV2, holder_record_bytes, discharges }`): NEW
  struct in `crates/octo-cap-macaroon/src/bundle_v2.rs`. Adds
  `chain_depth: u8` + `chain_parent: Option<[u8; 32]>` to V2
  token.

## Acceptance Criteria

Checked against the tree. The work landed in commit `b6bc190b` and onward; the
boxes below had never been ticked, which is how the type drift in the summary went
unnoticed for two months.

- [x] NEW: `CapabilityBundleV2` struct in
      `crates/octo-cap-macaroon/src/bundle_v2.rs`:
  - `bundle_version: u8 = 2`
  - `token_v2: CapabilityTokenV2`
  - `holder_record_bytes: Vec<u8>`
  - `discharges: Vec<DischargeMacaroon>`
- [~] NEW: `CapabilityTokenV2` struct:
  - All V1 `CapabilityToken` fields (audit per RFC-0870 §NodeEnvelope
    Adoption)
  - NEW: `chain_depth: u8` (0-8)
  - NEW: `chain_parent` — **shipped as `[u8; 32]`, specified as
    `Option<[u8; 32]>`**. The `Option` was dropped, so "no parent" is
    expressed as the zero array rather than as absence. That is the reason
    every root mint writes the same value.
- [x] V2 parser REJECTS V1 — implemented as an explicit
      `bundle_version != BUNDLE_VERSION_V2` check returning
      `BundleV2Error::UnsupportedVersion`, rather than the
      `== 1` comparison written here. Rejection behaviour is the same.
- [~] V1 parser rejects V2 via unknown enum variant (separate
  struct, not nested) — **moot**. V1 was deleted, so there is no V1 parser.
  The structural half holds: V1 and V2 were separate structs, never nested.
- [x] V2 wire format TV against
      `tests/bundle_v2_tv.rs` (filed via Mission
      `0957-f-v2-bundle-tv-fixture`; inline `[u8;N]` arrays, not JSON
      fixture — V1's `tvs/bundle_v1.json` is a dead file)
- [~] Consumers migrated in SAME commit as V2 wire (Wallet +
  Capability issuer + `octo-cap-zk`) — Wallet and Capability issuer
  migrated. `octo-cap-zk` gained the `capability_v2` field but nothing
  consumes it; that residue is `0009-ci`, and the atomicity this criterion
  demanded did not hold.
- [x] `cargo test -p octo-cap-macaroon --lib -- v2_bundle` passes.
      (Full lib suite: 415 passing.)

## Implementation Guide

1. Add `CapabilityTokenV2` struct (separate from V1) with chain
   fields.
2. Add `CapabilityBundleV2` struct in `bundle_v2.rs`.
3. Wire V2 parser with `bundle_version == 1` rejection.
4. Author TV in `tests/fixtures/v2_bundle_tv.json`.
5. Migrate consumers atomically.

## Cross-references

- RFC-0009 v1.2 §Phase 2 — V2 wire form spec
- Mission `0957-phase1-fixture-author` — fixture author
- `crates/octo-cap-macaroon/src/bundle.rs` — V1 substrate (do NOT
  modify V1)

## Version History

| Version | Date       | Status            | Changes                                                                                                                                                                                                                                                                                                                                         |
| ------- | ---------- | ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-09 | closed (F4 V1)    | Original F4 bundle struct + TV landed                                                                                                                                                                                                                                                                                                           |
| v0.2    | 2026-08-10 | open (V2 spec)    | Renamed from F4 → V2; content rewritten for V2 work per R8 H1                                                                                                                                                                                                                                                                                   |
| v0.4    | 2026-10-12 | claimed (landed)  | Reconciled against the tree. Removed "for hierarchical attenuation chain verification" from the summary — the fields carry the shape of that work and none of its behaviour. Recorded the `chain_parent` type drift (`Option<[u8;32]>` specified, `[u8;32]` shipped) and ticked the acceptance criteria that had stayed unticked since landing. |
| v0.3    | 2026-08-11 | claimed (V2 wire) | Mission moved `open/` → `claimed/`; V2 substrate authoring in progress                                                                                                                                                                                                                                                                          |
