# Mission: 0862-c5 — Domain-separator hygiene for untagged hash prefixes

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 1 security review
finding #6: sweep of `crates/` surfaced untagged legacy hashers —
`update(b"vault/v1")` (`quota-router-core/tests/eleven_step.rs`),
`b"cap/v1"`, `b"escrow/v1"`, `b"reservation/v1"`
(`quota-router-sm-engine/src/lib.rs`) — a second, unnamespaced
"vault/v1" hash space coexisting with the canonical one.

## RFC

- Primary: RFC-0862 v2.0 §StoolapSpendLedger substrate (clarify
  cross-cutting audit result)
- Co-RFC: RFC-0105 v1.9 (DqaEncoding prefix is the canonical
  reference pattern)
- Audit source: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.2 (domain separator hygiene)

## Dependency edges

| From                                              | To                                      | Why               | Layer direction |
| ------------------------------------------------- | --------------------------------------- | ----------------- | --------------- |
| `crates/quota-router-sm-engine` (rename prefixes) | `cipherocto/<name>/v1/` (canonical)     | Domain separation | lib → self      |
| `crates/quota-router-core` (test fixtures)        | `#[cfg(test)]` annotation + doc comment | Test hygiene      | test → test     |

No new cyclic edges. No new external crate deps.

## Problem

Canonical prefix: `"cipherocto/vault/v1/"` (per
`octo-vault::vault_id_unchecked`).

Untagged legacy:

- `b"vault/v1"` — `crates/quota-router-core/tests/eleven_step.rs`
- `b"cap/v1"` — `crates/quota-router-sm-engine/src/lib.rs`
- `b"escrow/v1"` — same
- `b"reservation/v1"` — same

The canonical `cipherocto/vault/v1/` derivation is now
domain-separated from all of these (verified by TV-0862
`tv_0862_vault_id_cross_ref_domain_separation`), but the
short-prefix ones in `quota-router-sm-engine` are a separate hash
space that could collide with a future canonical derivation if
they were ever promoted to use the same prefix.

## Acceptance Criteria

- AC-1: Audit complete: every `blake3::hash` + `update(b"...")` call
  in `crates/` has either (a) `cipherocto/`-prefixed domain tag, or
  (b) explicit `#[cfg(test)]` + doc comment "test-only placeholder,
  no canonical derivation"
- AC-2: Untagged production code paths: rename to
  `cipherocto/<name>/v1/` with a follow-on TV pinning the new
  derivation
- AC-3: Untagged test fixtures: annotate with doc comment
- AC-4: New TV-0862-13 (optional, cross-cutting): sweep test that
  grep-asserts no untagged `blake3::hash` calls outside test fixtures
- AC-5: RFC-0862 §StoolapSpendLedger `Vault row cross-ref` updated:
  reference the audit result (link to follow-on TV)

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Audit context:** `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.2 (domain separator hygiene)
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Critical files

- `crates/quota-router-sm-engine/src/lib.rs` (modify — rename
  `b"cap/v1"` / `b"escrow/v1"` / `b"reservation/v1"` prefixes if
  production paths; otherwise annotate test-only)
- `crates/quota-router-core/tests/eleven_step.rs` (modify —
  annotate `b"vault/v1"` test fixture with doc comment)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — §StoolapSpendLedger `Vault row cross-ref` reference audit)

## Out of scope

- Cross-crate sweep of ALL hashing primitives (sha2 / sha3 / blake2
  — defer to wider hashing-hygiene audit mission)
- Renaming of `b"cipherocto/..."` prefixes (already canonical)

## Risks

- **Test placeholder annotation risk** (LOW): if a test fixture is
  later promoted to a production path, the missing `cipherocto/`
  prefix becomes a domain-sep bug. Mitigation: lint rule via
  `cargo-hakari`-style sweep OR CI grep.
- **Renaming production paths** (HIGH): if any of the short-prefix
  hashes in `quota-router-sm-engine` are production (not test),
  renaming them is a wire-format break. Verify each before rename.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                                                     |
| ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 security review finding #6 (untagged hash prefixes sweep).                                                                                                                                                                                                  |
| 2026-08-17 | @mmacedoeu | Round 2 cleanup: drop `crates/octo-vault/src/lib.rs:334` line ref (use `vault_id_unchecked` symbol name instead), pin `§14.x` to concrete `§14.2`, add `## RFC` + `## Dependency edges` + `## Critical files` + `## Out of scope` sections consistent with parent 0862-c1, add AC anchors. |
