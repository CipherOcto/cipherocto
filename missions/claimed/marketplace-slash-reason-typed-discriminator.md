# Mission: marketplace-slash-reason-typed-discriminator

## Status

Closed. LANDED 2026-08-13.

## RFC

RFC-0900 (Economics): Marketplace §Slashing Model

## Dependencies

- Round 1 marketplace review (commit `264e2665`) — substrate stable

## Acceptance Criteria

- [x] `enum SlashReason` replaced with `struct SlashReason { type_id: [u8; 16], payload: Vec<u8> }`
- [x] 5 RFC-namespace UUIDs allocated (`0x0900:0001:0001..0005`)
- [x] 5 back-compat constants: `SlashReason::TIMEOUT`, `SlashReason::PROVIDER_ERROR`, `SlashReason::LATENCY_HIGH`, `SlashReason::GARBAGE_RESPONSE`, `SlashReason::FAILED_RESPONSE`
- [x] `verifiability()` is now a trait method on `SlashReasonSpec` registered via `HashMap<TypeId, Arc<dyn SlashReasonSpec>>`
- [x] Each existing reason gets a `SlashReasonSpec` impl in `marketplace/slashing.rs` (`TimeoutSpec`, `ProviderErrorSpec`, `LatencyHighSpec`, `GarbageResponseSpec`, `FailedResponseSpec`)
- [x] `pub trait SlashReasonSpec: Send + Sync { type_id, verifiability, name }`
- [x] `pub fn register_reason_spec(spec: Arc<dyn SlashReasonSpec>)` for extension crates (extension namespace `0xFFFF:0001:...`)
- [x] 3 extension-registration tests: `slash_reason_core_constants_have_distinct_type_ids`, `register_extension_spec_dispatches_weight_correctly`, `unknown_type_id_fails_closed_at_zero_weight`
- [x] `SlashError` matching preserved (only `SlashReason` shape changed; `SlashError` has no SlashReason field)
- [x] Clippy passes with zero warnings
- [x] All existing tests pass: 21 slashing (18 prior + 3 new) + 95 marketplace lib + 15 e2e + 32 task_market = 163 tests

## Claimant

mmacedoeu (2026-08-13)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/marketplace/slashing.rs` — enum→struct + 5 consts + SlashReasonSpec trait + 5 RFC specs + SlashReasonSpecRegistry + register_reason_spec + 3 new tests
- `crates/quota-router-core/tests/marketplace_e2e.rs` + `tests/task_market.rs` — sed `SlashReason::Timeout` → `SlashReason::TIMEOUT` etc. (PascalCase variant → SCREAMING_SNAKE constant)

Design notes:
- `serde_bytes_payload` inline module (not the external `serde_bytes` crate) so `payload: Vec<u8>` serializes via the default Vec<u8> impl (byte sequence) without the `&[u8]` slice semantics that would conflict with owned-vec deserialization.
- 5 constants constructed inline (not via `rfc_type_id_alloc(1..5)`) because Rust const-eval cannot call a non-const helper that builds `[u8; 16]` from a runtime u16 — explicit array literals stay in `const` context.
- `OnceLock<SlashReasonSpecRegistry>` for the default registry: the marketplace crate has no `init()` lifecycle, so the registry self-populates the 5 RFC specs on first access. `register_reason_spec` mutates the global via RwLock-protected insert.
- Fail-closed weight (0.0) for unknown type_ids: extension crates must register a spec before their reasons are dispatched; this prevents accidental "treat unknown as 1.0" silent acceptance in dispute-evidence scoring.
- Wire format: `type_id (16 bytes BE) || payload (varint length-prefixed)` per RFC-0900 §Slashing Model (the length-prefix is up to the wire codec; the in-memory struct stores payload as `Vec<u8>`).

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. Round 1 review follow-on. 12 ACs. |
| v0.2    | 2026-08-13 | Mission CLOSED. Enum→typed-discriminator struct + 5 consts + SlashReasonSpec trait/registry + 3 extension tests land. 163 marketplace tests pass. |

Last Updated: 2026-08-13
Version: 0.2
