# Mission: 0862-c6 — Production keyspace fixture risk (test DID reservation)

## Status

**OPEN 2026-08-17 (@mmacedoeu).** Follow-on to `0862-c1-dqa-vault-bump-amendment`
(S6c LANDED 2026-08-17). Filed per S6c Round 1 security review
finding #7: TV fixture DIDs (`did:octo:zTV086201`..`zTV086209b`) +
macaroon_ids (sequential `0x01..0xA0`) sit in the production
keyspace; RFC-0010 defines NO reserved test prefix.

## Problem

Per RFC-0010, the `did:octo:` keyspace is the production identifier
space. `StoolapSpendLedger` stores `holder_did` as raw bytes with
zero `CanonicalCodec` validation (no DID format check). TV fixture
DIDs land in the production keyspace without a reservation
prefix.

Practical collision risk is low (z-multibase strings contain `0`,
invalid base58btc; macaroon_id collision is 2^-128), but the
fixture DIDs are still in production keyspace.

## Acceptance Criteria

1. **Either** propose reserved test prefix in RFC-0010
   (e.g. `did:octo:test:`); new section §Reserved test prefixes
   with TV pinning the reservation prefix itself
2. **Or** (minimal) document in RFC-0862 §StoolapSpendLedger that
   `StoolapSpendLedger` performs no DID validation + relies on
   the wallet-node boundary (the existing convention)
3. New TV-0862-14: pinning the convention that the substrate
   accepts ANY byte slice as `holder_did` (regression: over-strict
   validation in the substrate)
4. Existing TV-0862-01..09 stay byte-stable

## Cross-reference

- **Parent:** `missions/open/0862-c1-dqa-vault-bump-amendment.md` (LANDED)
- **Co-RFC:** RFC-0010 (DID spec) — owns the reserved test prefix
  decision if option (1) is chosen
- **Wallet boundary:** `crates/octo-paid-query/handlers/`
  (canonical validation site per `macaroon_id` uniqueness invariant)
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6c follow-on)

## Risks

- **RFC-0010 amendment scope** (LOW): adding a reserved test prefix
  is a wire-format-adjacent spec change; should be RFC-0010 v1.6
  amendment, not RFC-0862 v2.1. Coordinate with S6g if applicable.
- **Wallet-node boundary dependence** (MED): relying on
  wallet-node canonical validation means a downstream caller
  bypassing the boundary (e.g. direct DB write for migration
  tooling) could insert non-canonical DIDs. The substrate's
  "accepts any bytes" contract is intentional; document.

## Version history

| Date       | Author     | Change                                                                                    |
| ---------- | ---------- | ----------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial filing per S6c Round 1 security review finding #7 (test DID production keyspace). |
