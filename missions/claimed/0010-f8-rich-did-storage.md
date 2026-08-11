# Mission: 0010-f8-rich-did-storage — StoolapDidRegistry Rich-Document Persistence

## Status

claimed (2026-08-11). Mission moved `open/` → `claimed/` after plan
approval; v0.2 corrects encoding (borsh, not serde_json) + field
names per recon.

**Substrate:** RFC-0010 v1.5 ACCEPTED 2026-08-11 (commit `a5ffd8ef`,
mission `0010-f8-rich-did-documents`) added `service_endpoints` +
`verification_methods` + `controllers` + `capability_delegations`
fields to `DidDocument`. The `StoolapDidRegistry` (mission
`0871b-storage-backend`, commit `71f8d745`) was authored against
v1.3 — schema v008 has only 4 columns; `resolve()` returns
`..Default::default()` for rich fields (all silently empty).

## Summary

Extend the Stoolap `did_registry` schema with two follow-on
migrations (v009 + v010) so the persistent registry carries the
full RFC-0010 v1.5 `DidDocument`. Rich fields are borsh-encoded
BLOBs (matching the existing `octo-ident` `WireDid`/`RawDid`
pattern + the `CapabilityBundleV2` precedent from mission
`0957-f-v2-bundle`). New columns are nullable (legacy v008 records
have NULL → empty `Vec<_>` on resolve, no backfill required).

## Schema migrations

### v009 — service endpoints + controllers

```sql
ALTER TABLE did_registry ADD COLUMN service_endpoints BLOB;
ALTER TABLE did_registry ADD COLUMN controllers BLOB;
CREATE INDEX IF NOT EXISTS did_registry_service_endpoints_idx
    ON did_registry (length(service_endpoints));
```

### v010 — verification methods + capability delegations

```sql
ALTER TABLE did_registry ADD COLUMN verification_methods BLOB;
ALTER TABLE did_registry ADD COLUMN capability_delegations BLOB;
CREATE INDEX IF NOT EXISTS did_registry_verification_methods_idx
    ON did_registry (length(verification_methods));
```

Total `did_registry` schema: 7 columns (4 legacy + 3 rich BLOBs).
All rich columns are nullable.

## Encoding

Rich fields are borsh-encoded as `Vec<u8>` BLOBs. The 4 rich types
(`ServiceEndpoint`, `ControllerReference`, `VerificationMethod`,
`CapabilityDelegation`) already have `borsh::BorshSerialize,
borsh::BorshDeserialize` derives behind the `octo-ident/borsh`
feature gate (matching the existing `WireDid`/`RawDid` pattern).
`quota-router-storage` enables the `borsh` feature on its
`octo-ident` dep.

`register`: `borsh::to_vec(&doc.service_endpoints)?` → BLOB.
`resolve`: borsh-decode via `unwrap_or_default()` (legacy NULL +
malformed bytes → empty `Vec<_>`; same fail-soft shape as V2
envelope from commit `e0c4ad62`).

## StoolapDidRegistry impl update

`register` SQL gains 4 new bind params (service_endpoints,
controllers, verification_methods, capability_delegations).

`resolve` SQL gains 4 new columns + borsh-decode each into the
`DidDocument` field. The `revoked` filter stays ahead of the
borsh decode (revoked rows return `Ok(None)` without
deserializing the rich fields).

`revoke` unchanged (canonical_hash key only).
`list` unchanged (full `DidDocument` already includes rich fields).

## Acceptance Criteria

- [ ] NEW: `v009__add_service_endpoints_and_controllers.sql`
- [ ] NEW: `v010__add_verification_methods_and_capability_delegations.sql`
- [ ] `quota-router-storage/Cargo.toml`: enable `octo-ident/borsh` feature
- [ ] `StoolapDidRegistry::register` writes 4 new BLOB columns via borsh
- [ ] `StoolapDidRegistry::resolve` reads + borsh-decodes; NULL → empty `Vec`
- [ ] 5 new unit TV in
  `crates/quota-router-storage/tests/stoolap_rich_did.rs`:
  - `register_resolve_round_trip_preserves_rich_fields`
  - `register_upsert_overwrites_rich_fields`
  - `resolve_legacy_row_returns_empty_vevs`
  - `register_with_max_service_endpoints`
  - `register_with_max_verification_methods`
- [ ] 1 integration TV:
  `migration_chain_v008_to_v010`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
      passes on `octo-ident` + `quota-router-storage`
- [ ] `cargo fmt --all -- --check` passes
- [ ] All existing `StoolapDidRegistry` TV still green (no regression)

## Implementation Guide

1. Author v009 + v010 migration SQL files.
2. Update `quota-router-storage/Cargo.toml`: `octo-ident` gains
   `features = ["borsh"]`.
3. Update `StoolapDidRegistry::register` SQL + bind params.
4. Update `StoolapDidRegistry::resolve` SQL + borsh decode.
5. Add 5 unit TV + 1 integration TV.
6. Run clippy + fmt + test sweep.

## Layer discipline

- `octo-ident` (Layer B) — UNCHANGED. Rich types already have borsh
  derives (gated behind `borsh` feature).
- `quota-router-storage` (Layer B-adjacent) — schema + impl update.
  No new deps; enables existing `octo-ident/borsh` feature.

## Cross-references

- RFC-0010 v1.5 §ServiceEndpoint + §VerificationMethod +
  §ControllerReference + §CapabilityDelegation
- Mission `0010-f8-rich-did-documents` (commit `a5ffd8ef`)
- Mission `0871b-storage-backend` (commit `71f8d745`)
- `crates/quota-router-storage/migrations/v008__create_did_registry.sql`
- `crates/octo-ident/src/registry.rs` (`DidRegistry` trait)
- `crates/octo-ident/src/rich_document.rs` (rich field substrate)
- `crates/octo-ident/Cargo.toml` (`borsh` feature gate)
- Borsh-encoding precedent: mission `0957-f-v2-bundle` (commit
  `b6bc190b`) + `0957-f-v2-bundle-consumer-migration` (commit
  `e0c4ad62`)

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-11 | open     | Filed after RFC-0010 v1.5 ACCEPTED; initial scope (wrong: serde_json encoding + wrong field names) |
| v0.2    | 2026-08-11 | claimed  | Recon corrected: borsh encoding (existing `octo-ident/borsh` feature); field names corrected to `service_endpoints`, `verification_methods`, `controllers`, `capability_delegations` |