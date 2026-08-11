# Mission: 0957-f-v2-bundle — V2 Bundle Struct (chain_depth + chain_parent)

## Status

open (2026-08-10; renamed from `0957-f-f4-bundle.md` per R7 H1 fix;
V2 work spec rewritten per R8 H1).

**Substrate:** RFC-0009 v1.2 (Draft 2026-08-10) + Mission
`0957-phase1-fixture-author` (R8 H1 fixture owner).

## Summary

Implement the V2 wire form of `CapabilityBundle` (per RFC-0009 v1.2
§Phase 2). The V2 bundle embeds `chain_depth` + `chain_parent`
fields in the V2 `CapabilityToken` struct for hierarchical
attenuation chain verification. This is the new V2 work — the V1
`CapabilityBundle` (shipped 2026-08-09) does NOT carry chain state.

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

- [ ] NEW: `CapabilityBundleV2` struct in
  `crates/octo-cap-macaroon/src/bundle_v2.rs`:
  - `bundle_version: u8 = 2`
  - `token_v2: CapabilityTokenV2`
  - `holder_record_bytes: Vec<u8>`
  - `discharges: Vec<DischargeMacaroon>`
- [ ] NEW: `CapabilityTokenV2` struct:
  - All V1 `CapabilityToken` fields (audit per RFC-0870 §NodeEnvelope
    Adoption)
  - NEW: `chain_depth: u8` (0-8)
  - NEW: `chain_parent: Option<[u8; 32]>`
- [ ] V2 parser REJECTS V1 via explicit `bundle_version == 1` check
- [ ] V1 parser rejects V2 via unknown enum variant (separate
  struct, not nested)
- [ ] V2 wire format TV against
  `tests/fixtures/v2_bundle_tv.json` (filed via Mission
  `0957-phase1-fixture-author`)
- [ ] Consumers migrated in SAME commit as V2 wire (Wallet +
  Capability issuer + `octo-cap-zk`)
- [ ] `cargo test -p octo-cap-macaroon --lib -- v2_bundle` passes

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

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-09 | closed (F4 V1) | Original F4 bundle struct + TV landed |
| v0.2    | 2026-08-10 | open (V2 spec) | Renamed from F4 → V2; content rewritten for V2 work per R8 H1 |
