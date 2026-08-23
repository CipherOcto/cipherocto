# Discriminant Stability Sub-amendment (file: 0968-a2)

## Status

- **Version:** 0.1.0
- **Status:** Draft v0.1.0 (2026-08-22)
- **Promoted from:** research file `docs/research/2026-08-21-vault-monetary-representation-redesign.md` §20 §User Decision Matrix decision #1 (file per BLUEPRINT.md §RFC Process)
- **Sub-amendment of:** RFC-0968 (Economics): Reputation Registry (Accepted at `rfcs/accepted/economics/0968-reputation-registry.md`)

> This is a sub-amendment under the legacy RFC-0968 numbering system. Sub-amendments track discriminant-stability rules inherited from RFC-0968 body content as of its accepted version.

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @mmacedoeu

## Summary

This sub-amendment formalizes discriminant-stability rules for the error table defined in the parent RFC. Discriminant persistence rules apply to:

- cross-replica error propagation (byte-format stability required)
- test vector definition references (numeric discriminant must remain stable)
- wire-format envelope first byte (binary compatibility for historical replay)

The IMPL/RFC drift for `StakeBelowMinimum` at position `0x2D` already landed in the parent RFC body content via commit `013a5676` per Round 2 review of mission `0968a2-reputation-anchoring-binding`. No further discriminant reshuffle is scheduled.

## Dependencies

- **RFC-0968** (Accepted at `rfcs/accepted/economics/0968-reputation-registry.md`): parent RFC; canonical error discriminant table is the authoritative source
- **RFC-0955** (Accepted): Model Liquidity Layer; `ComputeOffer.reputation` is the binding target
- **RFC-0955 reputation anchoring amendment** (Accepted, umbrella under RFC-0960 per `rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md` §0+§5): the binding contract; older document handles referring to this amendment as a sub-amendment identifier are folded under the umbrella
- **Mission 0968a** (claimed, pending close): in-memory batch envelope + Merkle-root aggregation + ledger table scaffolding
- **Mission 0968a2** (claimed, in `missions/claimed/0968a2-reputation-anchoring-binding.md`): live chain-side binding driver

## Specification

### §1 Discriminant Stability Ruleset

All discriminants defined in the parent RFC error table are STABLE-once-PUBLISHED:

- New discriminants MAY be appended (numbered strictly greater than any existing)
- Existing discriminants MUST NOT be reshuffled, renamed, or reassigned to a different payload struct
- Discriminant removal requires a new RFC version; existing entries MUST NOT be silently removed

### §2 Cross-replica Error Propagation

Error discriminant bytes propagate as the leading byte of cross-replica error messages per the parent RFC Audit Trail section. The propagation format is:

```
[1-byte discriminant | payload_type_marker_0x01 | payload bytes...]
```

### §3 Wire-format Stability

All binary wire formats MUST be deterministic per the parent RFC Determinism Requirements. Discriminant bytes are the first byte of the wire envelope; their stability is REQUIRED for historical replay compatibility with existing ledger entries.

### §4 Test Vector Stability

Test vectors defined in the parent RFC reference discriminants numerically. New test vectors MUST continue to reference existing discriminants unless paired with the §1 removal procedure at a new RFC version.

## Type Coverage

| RFC Type                                          | Implemented By             |
| ------------------------------------------------- | -------------------------- |
| Discriminant stability ruleset                    | This RFC §1                |
| Cross-replica error propagation                   | This RFC §2                |
| Wire-format stability                             | This RFC §3                |
| Test vector stability                             | This RFC §4                |

## Lifecycle Requirements

- **Status:** Draft
- **Acceptance target:** user-initiated Accept per BLUEPRINT.md §RFC Process
- **VH row addition:** required upon acceptance

## Related RFCs

- RFC-0968 (Economics): Reputation Registry — parent
- RFC-0955 (Economics): Model Liquidity Layer — binding target
- RFC-0960 (Economics) — anchors the umbrella for the Reputation Anchoring Amendment

## Version History

| Version | Date       | Change                                                                                                                  |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------- |
| v0.1.0  | 2026-08-22 | Initial draft. Sub-amendment of the parent RFC error discriminant table stability ruleset. Promoted from research decision matrix. Mission 0968a2 IMPL already canonical. Body text minimal — full specification deferred to v0.2.0 if body content required beyond §1-§4. |
