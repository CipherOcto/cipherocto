# Mission: Bare DECIMAL Default Scale Design Decision

## Status

LANDED 2026-08-13. Design decision + RFC-0111 amendment v1.22 + version history row added.

**Decision:** Option 1: `decimal_scale=0` for bare `DECIMAL` (rounds fractional values to integer via RoundHalfEven). Matches PostgreSQL default behavior; no sentinel magic; consistent with RFC-0202-A's "precision not enforced, only scale" simplification.

**RFC amendment:** New §"Bare DECIMAL Default Scale" section added to `rfcs/accepted/numeric/0111-deterministic-decimal.md` (after §Constants). Version history row v1.22 added.

## RFC

RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Resolve the bare DECIMAL default scale behavior. Currently `DECIMAL` maps to `decimal_scale=0` which rounds all fractional values to integers.

## Acceptance Criteria

- [x] Choose default behavior for bare DECIMAL — **DECIDED**: Option 1 (decimal_scale=0; RoundHalfEven rounding). Rationale: matches PostgreSQL default; no sentinel magic; consistent with RFC-0202-A's "precision not enforced, only scale" simplification. Users needing fractional precision MUST declare `DECIMAL(p,s)` explicitly.
- [x] Document the chosen design decision in RFC-0111 — **LANDED** (new §"Bare DECIMAL Default Scale" section at `rfcs/accepted/numeric/0111-deterministic-decimal.md` after §Constants; version history v1.22 row added)
- [x] Update implementation if needed — **N/A** (current Stoolap `Decimal` struct already supports scale=0; `SchemaColumn::decimal_scale = Some(0)` is the correct mapping per RFC-0202-A mapping table. No code change required — the implementation already matches Option 1.)

## Dependencies

- None

## Location

`rfcs/accepted/numeric/0111-deterministic-decimal.md`
`/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs`

## Complexity

Low

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (H3 finding)
- RFC-0111 §DECIMAL Type Definition
- RFC-0202-A §6.9 (DECIMAL(p,s) DDL) — for `decimal_scale` Option<u8> schema mapping

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                          |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | (filed)    | Mission filed. 3 ACs: choose + document + implement. 3 options on the table (decimal_scale=0 / sentinel 255 / max scale 36).                                                                                                                                                                                    |
| v0.2    | 2026-08-13 | **LANDED.** Option 1 chosen (decimal_scale=0; RoundHalfEven). New §Bare DECIMAL Default Scale added to RFC-0111 + v1.22 row. Rationale: PostgreSQL compatibility + RFC-0202-A consistency + no sentinel magic. No code change needed (current `SchemaColumn::decimal_scale` mapping already supports Option 1). |

Last Updated: 2026-08-13
Version: 0.2 (LANDED)
