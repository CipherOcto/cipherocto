# Mission: 0010-c — Canonical OctoID Deprecation Gate

## Status

Open (2026-07-27). Trigger: 6-month timer from Mission 0010-a ship.

## RFC

RFC-0010: Canonical OctoID Identifier Codec

## Dependencies

- Mission 0010-a (codec crate): REQUIRED
- Mission 0010-b (codemod): REQUIRED

## Summary

Close the 6-month dual-parse window. After the window closes:

- `octo_ident::DidCodec::parse` step 3 returns `DidError::LegacyFormExpired` instead of accepting legacy `did:octo:b<52>` or bare `did:octo:<name>` strings.
- `crates/octo-wallet/src/identity.rs::AudienceId::from_str` accepts only canonical form; legacy path returns `WalletError::InvalidAudienceId` with descriptive diagnostic.
- An operator escape hatch: `quota-router-cli --disable-legacy-did-deprecation` extends the window at the caller's risk.

## Acceptance Criteria

- [ ] 6-month timer activated at Mission 0010-a merge date + 180 days.
- [ ] `octo_ident::DidCodec::parse` step 3 returns `DidError::LegacyFormExpired` post-window.
- [ ] `crates/octo-wallet/src/identity.rs::AudienceId::from_str` deprecation_attr annotated: "Use `parse_canonical` instead; legacy form becomes invalid in 6 months post 0010-a ship".
- [ ] CLI escape hatch `--disable-legacy-did-deprecation` flag plumbed through `octo-ident` config.
- [ ] Migration guide `docs/07-developers/octoid-deprecation-guide.md` (new file): "How to upgrade from `did:octo:buyer` literals to canonical W3C form; how to re-encode the 52-byte raw DID; how to enable the legacy escape hatch in operator-facing flags."
- [ ] Test: pre-window invocation accepts legacy form; post-window invocation rejects with `LegacyFormExpired`.
- [ ] `cargo test --workspace --lib` green.

### Type Coverage

| RFC Type | Implemented By |
|----------|----------------|
| `during_deprecation_window()` function | This mission |
| `LegacyFormExpired` error variant (already in `DidError` per Mission A) | This mission (flag-gate logic) |
| Operator-facing CLI flag | This mission |

## Claimant

@unclaimed

## Pull Request

#

## Notes

- This mission is FUTURE-DATED. The 180-day timer fires 2027-01-23 (assuming 0010-a merges 2026-07-27). The mission file exists for discoverability.
- The deprecation is non-violent: legacy storage rows remain valid (the reputation layer's read path uses 52-byte raw, NOT wire form). Only NEW wire-form inputs are rejected.
