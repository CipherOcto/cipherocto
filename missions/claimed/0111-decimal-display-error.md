# Mission: DECIMAL Display and Error Handling

## Status

Claimed 2026-08-06; **Closed 2026-08-06** — cipherocto-side impl + RFC-0111 v1.21 amendment.

## RFC

RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Fix DECIMAL display formatting and `decimal_to_string` error handling. Cipherocto-side `determin::Decimal` gained `Display` + `FromStr` impls (RFC-0111 v1.21 normative). External `/home/mmacedoeu/_w/databases/stoolap/` paths from original mission were stale — actual cipherocto surface is `determin/src/decimal.rs` per [[stoolap-general-purpose-db]] red line (cipherocto fork pinned via git, no local source drop).

## Acceptance Criteria

- [x] Add DECIMAL Display impl (RFC-0111 v1.21) — `impl fmt::Display for Decimal` in `determin/src/decimal.rs` delegating to `decimal_to_string`; length TRAP surfaced as `<decimal:length-trap>` sentinel rather than panic. Tests: `display_canonical_zero`, `display_canonical_integer`, `display_fractional`, `display_negative`, `display_leading_zeros_in_fraction`. [Commit pending — see §Closure]
- [x] Add DECIMAL FromStr impl (RFC-0111 v1.21) — `impl FromStr for Decimal` with trim policy, internal-whitespace TRAP, optional `+` sign, period-only decimal separator, no thousands separators, no exponent, bare-dot reject, trailing-dot accept, scale boundary 36, mantissa range 10^36-1. Tests: 9 cases covering trim, internal-whitespace TRAP, empty, bare dot, comma reject, exponent reject, non-digit reject, integer-only, trailing-zeros canonicalization, scale overflow, scale-at-boundary. [Commit pending — see §Closure]
- [x] All DECIMAL Display tests pass — 519/519 determin lib tests green; `cargo clippy --lib --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean.
- [x] RFC-0111 v1.21 amendment — added §String → DECIMAL Parsing Policy (Normative, v1.21) + §DECIMAL ↔ String Round-Trip Invariant (Normative, v1.21) to `rfcs/accepted/numeric/0111-deterministic-decimal.md`; bumped Status header to v1.21 (2026-08-06); added v1.21 row to Version History.

## Dependencies

- Mission: 0111-decimal-core-type (completed)
- Mission: 0111-decimal-serialization (completed)

## Location

- `/home/mmacedoeu/_w/ai/cipherocto/determin/src/decimal.rs` (impl + tests)
- `/home/mmacedoeu/_w/ai/cipherocto/rfcs/accepted/numeric/0111-deterministic-decimal.md` (spec amendment)

## Complexity

Low

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (M1, M2 findings — referenced but pointed at external stoolap fork; cipherocto-side implementation is `determin/src/decimal.rs`)
- RFC-0111 §Display and String Representation (v1.21 normative clarification)

## Closure

**Cipherocto-side commit(s):** pending — code currently uncommitted on `next` branch (4 edits: imports `use std::fmt; use std::str::FromStr;`, `impl Display`, `impl FromStr`, 14 unit tests, clippy nit fix).

**Missions closed:**

- 0111-decimal-display-error — this mission.
- 0111-decimal-whitespace-amendment — closed jointly (RFC-0111 v1.21 §Locale Specification whitespace policy is now normative binding).

**Test output:** `cargo test --lib` in `determin/` → 519 passed; 0 failed; 0 ignored; 14 new tests added (5 Display + 9 FromStr).

**Verification commands:**

```bash
cd determin
cargo fmt --all -- --check
cargo clippy --lib --all-targets -- -D warnings
cargo test --lib
```