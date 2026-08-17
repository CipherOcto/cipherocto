# Mission: 0862-c6 — Production keyspace fixture risk (test DID reservation)

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 1 security review
finding #7: TV fixture DIDs (`did:octo:zTV086201`..`zTV086209b`) +
macaroon_ids (sequential `0x01..0xA0`) sit in the production
keyspace; RFC-0010 defines NO reserved test prefix.

## RFC

- Primary: RFC-0862 v2.0 §StoolapSpendLedger substrate (document
  no-DID-validation boundary convention)
- Co-RFC: RFC-0010 (DID spec) — owns the reserved test prefix
  decision if option AC-1 chosen

## Dependency edges

| From                                                   | To                           | Why                       | Layer direction     |
| ------------------------------------------------------ | ---------------------------- | ------------------------- | ------------------- |
| RFC-0010 amendment (reserved test prefix, option AC-1) | RFC-0862 §StoolapSpendLedger | Cross-reference           | n/a (RFC text only) |
| Wallet-node boundary (`crates/octo-paid-query/src/`)   | `StoolapSpendLedger`         | Canonical validation site | lib → lib           |

No new cyclic edges. No new external crate deps.

## Problem

Per RFC-0010, the `did:octo:` keyspace is the production identifier
space. `StoolapSpendLedger` stores `holder_did` as raw bytes with
zero `CanonicalCodec` validation (no DID format check). TV fixture
DIDs land in the production keyspace without a reservation prefix.

Practical collision risk is low (z-multibase strings contain `0`,
invalid base58btc; macaroon_id collision is 2^-128), but the
fixture DIDs are still in production keyspace.

## Acceptance Criteria

- AC-1: **Either** propose reserved test prefix in RFC-0010
  (e.g. `did:octo:test:`); new section §Reserved test prefixes
  with TV pinning the reservation prefix itself
- AC-2: **Or** (minimal) document in RFC-0862 §StoolapSpendLedger
  that `StoolapSpendLedger` performs no DID validation + relies on
  the wallet-node boundary (the existing convention)
- AC-3: New TV-0862-14: pinning the convention that the substrate
  accepts ANY byte slice as `holder_did` (regression: over-strict
  validation in the substrate)
- AC-4: Existing TV-0862-01..09b stay byte-stable

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Co-RFC:** RFC-0010 (DID spec) — owns the reserved test prefix
  decision if AC-1 is chosen
- **Wallet boundary:** `crates/octo-paid-query/src/` (canonical
  validation site per `macaroon_id` uniqueness invariant)
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Critical files

- `rfcs/accepted/identity/0010-did-method.md` (modify — option AC-1
  adds §Reserved test prefixes)
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md`
  (modify — option AC-2 documents no-DID-validation convention)
- `crates/quota-router-storage/tests/tv_0862_spend_ledger.rs` (modify
  — add TV-0862-14 substrate-accepts-any-bytes regression)
- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` (modify
  — doc comment on no-DID-validation if option AC-2 chosen)

## Out of scope

- Migration of existing test fixtures to new prefix (defer until
  option AC-1 lands and convention is settled)
- `macaroon_id` collision detection (256-bit random IDs already
  render collision negligible)

## Risks

- **RFC-0010 amendment scope** (LOW): adding a reserved test prefix
  is a wire-format-adjacent spec change; should be a future RFC-0010
  amendment (not RFC-0862 v2.x follow-on). Coordinate with S6g if
  applicable.
- **Wallet-node boundary dependence** (MED): relying on
  wallet-node canonical validation means a downstream caller
  bypassing the boundary (e.g. direct DB write for migration
  tooling) could insert non-canonical DIDs. The substrate's
  "accepts any bytes" contract is intentional; document.

## Version history

| Date       | Author     | Change                                                                                                                                                                                                                                                                                             |
| ---------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 security review finding #7 (test DID production keyspace).                                                                                                                                                                                                          |
| 2026-08-17 | @mmacedoeu | Round 2 cleanup: drop phantom `RFC-0010 v1.6` + `RFC-0862 v2.1` forward-looking version pins, drop phantom `crates/octo-paid-query/handlers/` subdir path, add `## RFC` + `## Dependency edges` + `## Critical files` + `## Out of scope` sections consistent with parent 0862-c1, add AC anchors. |
