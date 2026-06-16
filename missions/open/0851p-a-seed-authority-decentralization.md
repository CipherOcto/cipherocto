# Mission: 0851p-a — SeedListAuthority decentralization

## Status

Open (2026-06-16) — post-launch follow-up (after MissionSlashing v1.0 ships)

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work"

## Summary

Replace the 3-of-5 foundation multi-sig `SeedListAuthority` with a DAO multi-sig backed by the RFC-0855 §11 governance key. The foundation multi-sig is appropriate at launch (no MissionSlashing yet) but is a single point of failure: foundation members could collude to add malicious seeds, do an eclipse attack, or censor legitimate peers. Once slashing ships, the authority should rest with the DAO.

## Design

1. **Phase 1 (launch):** `SeedListAuthority` is a 3-of-5 foundation multi-sig (alice, bob, carol, dave, erin).
2. **Phase 2 (this mission, post-launch):** `SeedListAuthority` is a DAO multi-sig with M-of-N signatures from the elected governance council per RFC-0855 §11. Foundation multi-sig is deprecated but remains valid until Phase 3.
3. **Phase 3 (long-term):** Foundation multi-sig is removed entirely; only DAO multi-sig is accepted.

The seed list envelope format is unchanged: `SeedListEnvelope { authority_pubkey, signed_at_epoch, peers[] }`. The `authority_pubkey` is a multi-sig public key, which is a single Ed25519 key (the multi-sig threshold key per SLIP-10 or equivalent).

The transition is a hard fork: from a specific epoch `EPOCH_GOVERNANCE_TAKEOVER` onwards, only DAO multi-sig seed lists are accepted; foundation multi-sig is rejected with `SEED_LIST_AUTHORITY_DEPRECATED`.

## Acceptance Criteria

- [ ] `crates/octo-bootstrap/src/authority/dao_multisig.rs` — DAO multi-sig verifier
- [ ] Config: `SEED_LIST_AUTHORITY_PUBKEY` (the DAO multi-sig threshold public key)
- [ ] Hard-fork gate: `EPOCH_GOVERNANCE_TAKEOVER` constant
- [ ] Reject foundation multi-sig after `EPOCH_GOVERNANCE_TAKEOVER` with `SEED_LIST_AUTHORITY_DEPRECATED`
- [ ] Unit test: foundation multi-sig accepted before fork, rejected after
- [ ] Unit test: DAO multi-sig accepted after fork
- [ ] Integration test: full seed list load with DAO multi-sig
- [ ] Documentation: governance guide for updating `SEED_LIST_AUTHORITY_PUBKEY`

## Mitigates

D-NB-2 (single point of failure on seed list authority)

## Deadline

Post-launch (after MissionSlashing ships)
