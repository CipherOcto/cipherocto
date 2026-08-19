---
name: mission-0900-d1-chain-slash-tuple-key-status
description: "0900-d1 LANDED 2026-08-18 — HashMap<([u8;32], String), ProviderStake> tuple-key + SlashOutcome.chain_id + 7 TVs landed. 2 TVs (DQA(12) byte-exact + scale=0) DEFERRED to 0900-d2 (fork Dqa driver upstreaming, filed 8dac8bf0)."
metadata:
  type: project
  modified: 2026-08-18T23:55:00.000Z
---

# Mission 0900-d1 — LANDED 2026-08-18

RFC-0900 v2.0 chain-aware slash ledger follow-on. Closes 5 of
the ACs deliberately deferred from 0900-d LANDED (`58c4c2ce`).
Commit: `0b283d29`.

## Scope as landed

- **AC-3 HashMap tuple-key**: `SlashingLedger::stakes` is now
  `HashMap<([u8; 32], String), ProviderStake>`. Public API stable
  (single-arg `provider_id`); internal `stake_key(provider_id)`
  helper centralises the tuple shape. 6 production call sites +
  2 `ProviderStake` literals + 1 `SlashOutcome` literal updated.
- **AC-4 SlashOutcome.chain_id**: first-position field, populated
  from `stake.chain_id` in `apply_penalty` for audit-table chain
  attribution.
- **AC-5 7/9 TVs**: storage-side TVs (03/06/07/08) in
  `tests/tv_0900_d1_chain_slash_remaining.rs` (4 tests, all pass).
  Cross-crate TVs (05/10/11) added to
  `marketplace/slashing.rs::tests` module (compile-check verified,
  runtime blocked by libpython3.12). TV-02 already covered by
  0900-d unit test.
- **AC-6 no regressions**: 192/192 storage lib + 23/23 migration
  chain + 4/4 new TVs green. 2 pre-existing migration tests
  updated: hardcoded `MAX(version)==12` → `15` (v012 → v015 era).
- **AC-7 clippy + fmt**: clean on storage + core (`--features full`,
  `--D warnings`).

## Deferred to 0900-d2

- **TV-0900-D-01** (DQA(12) byte-exact round-trip) — blocked by
  stoolap fork not exposing `r.get::<Dqa>(idx)`.
- **TV-0900-D-04** (scale=0 invariant via DQA(12)) — same.
- Mission 0900-d2 (`8dac8bf0`) demands the fork expose native Dqa
  driver with canonical `DqaEncoding` 16-byte BE wire form.

## Why this matters

Closes RFC-0900 v2.0 §Slash Ledger Substrate's in-memory mirror:
production `HashMap<([u8; 32], String), ProviderStake>` now mirrors
the substrate PK `(chain_id, provider_id)`. Cross-chain slashing
activates by passing non-`DEFAULT_CHAIN_ID` to future
`register_at(chain_id, provider_id, ...)` API (separate RFC owed).

**How to apply:** when working on `marketplace/slashing.rs`, the
`stakes` map is tuple-keyed. Public single-arg API still works;
`stake_key(provider_id)` is the canonical helper. Production
chain_id is `DEFAULT_CHAIN_ID` (32 zero bytes) until multi-chain
slashing lands.

Related: [[mission-0900-d-chain-aware-slash-ledger-status]] (parent
PK + migration substrate), [[mission-0900-d2-stoolap-fork-dqa-driver-upstreaming-status]]
(DQA(12) column promotion deferred upstream).
