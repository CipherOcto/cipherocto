# 0011-h-s-a-local-gateway-identity-state — Substrate additions for LocalGatewayIdentity state

## Status

Claimed (2026-09-20) — Substrate-additions prerequisite per RFC-0011-i §Substrate-Additions Companion Missions (NEW row added by R1.5 fix per F2 finding). Companion mission to RFC-0011-i `octo network identity show`. Substrate implementation landing in `crates/octo-network/src/mon/local_gateway_identity.rs` per mission YAML substrate additions target.

## RFC

RFC-0011-i §Substrate-Additions Companion Missions

## Summary

Adds local gateway identity state storage. Required by `octo network identity show`. Substrate `GatewayIdentity::new(public_key, network_id, gateway_class, creation_epoch)` requires 4 arguments but the CLI source (octo-wallet `IdentityKey`) provides only `public_key`. The 3 missing fields (`network_id`, `gateway_class`, `creation_epoch`) must persist locally for the CLI to construct a deterministic `GatewayIdentity` that matches the network's view of the local gateway.

### Substrate additions target

```rust
// crates/octo-network/src/mon/local_gateway_identity.rs (NEW)
pub struct LocalGatewayIdentity {
    pub network_id: u32,
    pub gateway_class: GatewayClass,
    pub creation_epoch: u64,
}

impl LocalGatewayIdentity {
    pub fn load(octo_home: &Path) -> Result<Self, LocalGatewayIdentityError>;
    pub fn save(&self, octo_home: &Path) -> Result<(), LocalGatewayIdentityError>;
    pub fn exists(octo_home: &Path) -> bool;
}

pub enum LocalGatewayIdentityError {
    NotInitialized,
    IoError(std::io::Error),
    TomlParseError(toml::de::Error),
}
```

Storage path: `$OCTO_HOME/network/local-gateway-identity.toml` per octo-home substrate convention.

(Stub: full type signatures + ACs land in Phase X of this mission's own RFC/DRY cycle per [[no-phantom-mission-pointers]].)

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/local_gateway_identity.rs (NEW)` per RFC-0011-i §Substrate-Additions row (NEW)
- [ ] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green
- [ ] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests (load/save/exists round-trip + not-initialized error path) + ≥1 integration test (CLI consumes loaded state to construct GatewayIdentity)
- [ ] CLI predicate `octo network identity show` reads `LocalGatewayIdentity::load(octo_home)`; if `NotInitialized` → exit 83 `NetworkLocalKeyUnavailable` (per RFC-0011-i §Error Handling row 83)

## Dependencies

Hard sequencing:

1. **RFC-0011-i must be Accepted** before this mission's substrate additions land (mission YAML cites real RFC per [[no-phantom-mission-pointers]])
2. **This mission must land BEFORE Phase 1 implementation** — `octo network identity show` cannot be implemented faithfully against existing substrate without these additions (verified by RFC-0011-i R1 finding F2)
3. Mission YAML `0011-h-network-peers-identity` (CLI dispatch) waits for this mission to land per substrate-first ordering invariant

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-peers-identity` covers that surface)
- Wallet integration for storing public_key alongside identity state (octo-wallet already exposes `IdentityKey`; this mission only adds the 3-field local identity state)
- Network discovery of remote gateway identity (separate substrate; deferred to RFC-0011-m Phase 5)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)

## Notes

Stub filed 2026-09-20 per [[no-phantom-mission-pointers]] + RFC-0011-i R1 review finding F2. The substrate gap was identified at RFC-0011-i draft time: substrate `GatewayIdentity::new` requires 4 args but CLI source provides 1 (public_key from wallet). Without this mission, `octo network identity show` would either panic on missing args OR derive a deterministic-but-wrong gateway_id using zero-defaults (network_id=0, creation_epoch=0) that mismatches the network's view of the local gateway. This mission establishes the substrate contract for the missing 3 fields per [[cipherocto-design-principles]] §No premature coupling.
