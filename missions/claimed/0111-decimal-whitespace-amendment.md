# Mission: DECIMAL Whitespace Specification Amendment

## Status

Claimed 2026-08-06; **Closed 2026-08-06** — RFC-0111 v1.21 amendment + cipherocto-side impl.

## RFC

RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Align DECIMAL parsing specification with cipherocto-side `FromStr` behavior for whitespace handling. RFC-0111 §Locale Specification was already normative for "trim leading/trailing, TRAP on internal whitespace" but lacked parsing-specific binding language. v1.21 amendment makes the policy binding on `FromStr` impls and adds an explicit step-by-step algorithm.

## Acceptance Criteria

- [x] Amend RFC-0111 spec to say whitespace is stripped before parsing (binding on `FromStr`) — RFC-0111 v1.21 §String → DECIMAL Parsing Policy step 1: "**Whitespace:** `trim()` leading + trailing whitespace (spaces, tabs, CR, LF). Internal whitespace is a **TRAP** — reject with `ParseError`." [Pending commit — see §Closure]
- [x] Clarify that internal whitespace is a TRAP (reject, not silently skip) — RFC-0111 v1.21 §Locale Specification retained; parsing policy adds binding language. Test: `fromstr_traps_internal_whitespace` covers `"1 . 5"`, `"1. 5"`, `"1.5 6"`, `"1\t2"`. [Pending commit]
- [x] Update Version History in RFC — v1.21 row added to `## Version History` table. [Pending commit]

## Dependencies

- None (spec task + cipherocto-side impl).

## Location

- `/home/mmacedoeu/_w/ai/cipherocto/rfcs/accepted/numeric/0111-deterministic-decimal.md` (amendment)
- `/home/mmacedoeu/_w/ai/cipherocto/determin/src/decimal.rs` (`impl FromStr for Decimal` + tests)

## Complexity

Low

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (C3 finding — `stoolap_parse_decimal trims whitespace`; cipherocto-side analog at `determin::Decimal::from_str` line 924)
- RFC-0111 §Locale Specification (v1.21 normative clarification)

## Closure

**Cipherocto-side commit(s):** pending — same uncommitted edits as 0111-decimal-display-error (joint closure).

**Test output:** `cargo test --lib fromstr_` in `determin/` → 12 passed; 0 failed.

**Verification commands:**

```bash
cd determin
cargo test --lib fromstr_traps_internal_whitespace
cargo test --lib fromstr_trims_leading_and_trailing_whitespace
cargo test --lib fromstr_rejects_empty
```