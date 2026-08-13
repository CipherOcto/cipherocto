# Mission: marketplace-slash-reason-typed-discriminator

## Status

Open. Follow-on to Round 1 marketplace review (commit `264e2665`).
`SlashReason` is a closed enum on an extension-bearing RFC surface.
CLAUDE.md §"Extension over enumeration" requires typed-discriminator
+ RFC-allocated namespace for extension-bearing types.

## RFC

RFC-0900 (Economics): Marketplace §Slashing Model

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [ ] Replace `enum SlashReason { Timeout, ProviderError, LatencyHigh, GarbageResponse, FailedResponse }` with `struct SlashReason { type_id: [u8; 16], payload: serde_bytes::Bytes }`
- [ ] Allocate RFC-namespace UUIDs for the 5 existing reasons (e.g., `0x0900:0001:...:0001` through `:0005` per the marketplace RFC prefix)
- [ ] Provide 5 constants: `SlashReason::TIMEOUT`, `SlashReason::PROVIDER_ERROR`, etc. for back-compat
- [ ] `verifiability()` becomes a trait method on `SlashReasonSpec` registered via `HashMap<TypeId, Arc<dyn SlashReasonSpec>>`
- [ ] Each existing reason gets a `SlashReasonSpec` impl in the marketplace crate
- [ ] Add `pub trait SlashReasonSpec: Send + Sync { fn type_id(&self) -> [u8; 16]; fn verifiability(&self) -> f64; }`
- [ ] Add `pub fn register_reason_spec(spec: Arc<dyn SlashReasonSpec>)` for extension crates to register PrivacyBreach / KeyLeak / etc.
- [ ] Add ≥3 extension-registration tests: register new SlashReasonSpec, fire slash via the new reason, old code fails-closed on unknown type_id
- [ ] Update `SlashError` matching across consumers to handle the typed-discriminator payload (versioned)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass + new extension tests (≥3)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/marketplace/slashing.rs:22-33` — enum → struct
- `crates/quota-router-core/src/marketplace/slashing.rs:42-46` — verifiability() → trait
- All consumers (slashing ledger, task_market/slashing.rs, marketplace facade)

Round 1 review context (Pass 2 HIGH #H4): CLAUDE.md §Extension over
enumeration prohibits central enums for extension-bearing types.
SlashReason is explicitly extension-bearing (RFC-0900 §Slashing Model
leaves room for PrivacyBreach, KeyLeak, StorageLeak, plus slashing-
extension crates). New reason = central enum edit + cross-crate review.

Alternative considered: keep enum, add #[non_exhaustive]. Rejected —
`#[non_exhaustive]` doesn't help serde wire format evolution and still
forces cross-crate review for new variants.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 12 ACs. |

Last Updated: 2026-08-13
Version: 0.1