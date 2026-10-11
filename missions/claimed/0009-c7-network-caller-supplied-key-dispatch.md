---
name: 0009-c7-network-caller-supplied-key-dispatch
description: RFC-0009 AC#11, octo-network slice. Dispatch the seven verification sites whose key arrives as a caller-supplied parameter, not a DID.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c-verification-dispatch
    - 0009-c5-vault-cache-bus-algorithm-dispatch
---

# Mission `0009-c7-network-caller-supplied-key-dispatch` — OPEN 2026-10-10

## Status

Open 2026-10-10. Split out of `0009-c3` after measurement showed its blanket
blocker was wrong for most of the set.

## Why this mission exists

`0009-c3` recorded every `octo-network` site as blocked on the RFC-0010 DID
codec, on the grounds that a key parsed out of a DID into bare 32 bytes carries
no algorithm. That reason was true where it was written and then **pasted onto
sites where it is not** — the same failure that made `0009-c5` and `0009-c6`
false blockers.

Read each site. The ten pending files split three ways, and only none of them
matches the recorded reason:

| Category                               | Files                                                                                                                                    | Real blocker                      |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| **Key is a caller-supplied parameter** | `porelay/heartbeat`, `porelay/forwarding`, `porelay/bandwidth`, `porelay/availability`, `drs/route`, `dot/envelope`, `gossip/reputation` | none — this mission               |
| **Key is a field on the wire struct**  | `dot/dc`, `dom/admission`                                                                                                                | wire format change under RFC-0853 |
| **Actually DID-derived**               | none of the ten                                                                                                                          | —                                 |

`GatewayAttestation::verify_signature` is the first category;
`KeyRotation::verify` and `RevocationNotice` are the second. They share a
file, which is why `ocrypt/attestation.rs` is one pending entry covering two
blockers.

**There is no genuinely DID-derived site in the pending set.** The only
`pubkey_from_did` callers in the crate are `dc/admin_attest.rs` and
`octo-role/src/select.rs`, neither of which is on this table.

## Scope

Seven `verify_signature` methods that take `public_key: &[u8; 32]` from the
caller, plus `GatewayAttestation::verify_signature` which does the same.

## The actual work, and why the codec never mattered

The key arrives as bytes the caller already holds. The caller therefore knows
which algorithm those bytes belong to — the same situation as the
`octo-vault` trust list, where the algorithm was available and being thrown
away. No DID is parsed between the caller and the verification, so no codec
change is involved.

Adding an `AlgorithmId` parameter is the same edit `0009-c5` made. **The real
prerequisite is a dependency edge, not a codec**: `octo-network` does not
depend on `octo-ident` today. The manifest mentions `octo-ident` only inside a
comment describing a future codec swap, which is exactly the kind of thing a
reader skims as "already there".

That comment is the trap this mission walks into if nobody checks. The
dependency line has to be added with its layer justification, the same
wording `octo-wallet`, `octo-reputation`, `octo-vault` and
`octo-coordinator-types` now carry.

## Blast radius

Seven signature changes and their callers. `porelay` and `drs` methods are
called from their own modules and from `octo-network`'s tests. Blast radius is
**not yet measured** — the earlier "106 sites" figure in this area was wrong
and this file does not repeat it.

## Layer direction

`octo-network` is Layer C. C → B is permitted. `octo-ident` is Layer B and
`octo-algo-ed25519` is a Layer E algorithm crate — the same pair `octo-wallet`
(Layer B) already takes, so the direction is established rather than novel.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `network_envelope_verification_dispatches_on_the_declared_algorithm` — the
  happy path. **Control:** refusing everything must not pass this.
- `network_envelope_verification_refuses_an_unregistered_algorithm` — the
  fail-closed half. **This is the vector that can distinguish dispatch from a
  hardcoded Ed25519**, and the only one: on `octo-vault` and
  `octo-coordinator-types` a hardcoded verifier passed the happy path _and_ the
  bad-signature check, and only this control went red.
- `network_envelope_verification_still_refuses_a_bad_signature` — dispatch did
  not degrade the check into a lookup.
- existing `porelay` / `drs` / `dot` vectors must pass unchanged.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-network --all-targets --all-features -- -D warnings
cargo test -p octo-network --lib
```

`octo-network` is long to build. Do not add it to another mission's gate.

## Backward compat

None required. Nothing is deployed, and the affected methods have no caller
outside the crate that has already been migrated.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `missions/open/0009-c3-network-verification-dispatch` — where the sweep and
  the corrected categorization live
- `missions/claimed/0009-c5-vault-cache-bus-algorithm-dispatch` — the pattern
  to follow
- `missions/claimed/0009-c6-coordinator-slashing-algorithm-dispatch` — the
  second false blocker
- `crates/octo-cli/tests/security_regressions.rs` — `AC11_PENDING`

## Claimant

@unassigned

## Pull Request

#
