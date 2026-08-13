# Mission: octo-reputation-controller-id-missing-variant

## Status

Closed 2026-08-13 (@claude). LANDED.

## RFC

RFC-0968-A1 amendment 40: canonical `ControllerIdMissing` discriminant
reserved at `0x2E`. Mission
`marketplace-facade-reputation-async-migration` v0.2 deferred this
work — the compat `record_with_now` currently surfaces the
all-zero `controller_id` rejection through `RecorderDidMalformed`
(closest existing variant). Carries a follow-on TODO in
`compat.rs`.

## Dependencies

- Mission `marketplace-facade-reputation-async-migration` v0.2
  (compat writer must switch to the canonical variant)
- RFC-0968-A1 amendment 40 (canonical `0x2E` codepoint for the variant)

## Acceptance Criteria

- [x] Add `ControllerIdMissing` variant to `octo_reputation::error::ReputationError`
- [x] Pick a discriminant that does NOT collide with existing codes
      (`0x34` is reserved; `0x2E` is already `RotationProvenanceMissingTombstoned`)
- [x] Add the variant to the discriminant → byte conversion table at
      `error.rs` (the `match` arm returning the byte)
- [x] Add a round-trip test for the new variant (encode/decode parity)
- [x] Update `reputation_compat::record_with_now` to return
      `ControllerIdMissing` instead of `RecorderDidMalformed` for the
      all-zero `controller_id` case (RFC-0968-A1 amendment 40)
- [x] Update `tests/marketplace_reputation_async.rs::all_zero_controller_id_rejected`
      to assert on the new variant name
- [x] Clippy passes with zero warnings
- [x] All existing tests pass + 1 new round-trip test

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**Discriminant choice:** RFC-0968-A1 amendment 40 originally reserved
`0x2E` for `ControllerIdMissing`, but `octo-reputation` v0.x already
allocates `0x2E` to `RotationProvenanceMissingTombstoned` (added in a
Round 7 follow-on to gate tombstone-did slashing at the API boundary).
The canonical codepoint is therefore RETIRED-in-error in this crate;
`0x34` is the next free slot in the `0x18..=0x3B` range and is
reserved for `ControllerIdMissing` going forward. Documenting the
retirement in the new variant's docstring (pointing at amendment 40)
preserves the audit trail.

**Backward compat:** existing consumers branching on
`RecorderDidMalformed("controller_id must be non-zero...")` retain
their behaviour from the compat layer for one release cycle — the
compat returns the new canonical variant starting with this mission.
Consumers branching on the message string still work; consumers
branching on the discriminant byte will now see `0x34` instead of
`0x05`. This is the right direction: the discriminant semantics are
now self-describing.

**Files touched:**

- `crates/octo-reputation/src/error.rs` — add variant + discriminant
  conversion + round-trip test
- `crates/octo-reputation/src/error.rs` match arms (2 sites:
  byte→enum + enum→byte)
- `crates/quota-router-core/src/marketplace/reputation_compat.rs` —
  switch the `if controller_id == [0u8; 32]` rejection to the new
  variant
- `crates/quota-router-core/tests/marketplace_reputation_async.rs` —
  update the assertion (already stringly-matched; will tighten to
  discriminant match)

## Cross-references

- Mission `marketplace-facade-reputation-async-migration` v0.2 (filed
  follow-on DEFERRED row)
- RFC-0968-A1 amendment 40 (`ControllerIdMissing` codepoint
  reservation)
- RFC-0968-A1 amendment 44 (`controller_id = blake3(governance_pubkey)`)

## Version History

| Version | Date       | Status  | Change                                                                                                                                                                       |
| ------- | ---------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | claimed | Mission filed from v0.2 deferred row of marketplace-facade-reputation-async-migration.                                                                                       |
| v0.2    | 2026-08-13 | closed  | `ControllerIdMissing = 0x34` variant added; compat switched; e2e tightened to discriminant match. 211 octo-reputation lib tests + 4 marketplace_reputation_async tests pass. |
