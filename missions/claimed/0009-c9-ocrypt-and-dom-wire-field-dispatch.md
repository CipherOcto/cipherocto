---
name: 0009-c9-ocrypt-and-dom-wire-field-dispatch
description: RFC-0009 AC#11, the three remaining octo-network wire-field verifications. The recorded RFC-0853 blocker is wrong for all three.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "2.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c-verification-dispatch
    - 0009-c7-network-caller-supplied-key-dispatch
    - 0009-c8-dot-transport-envelope-algorithm-dispatch
---

# Mission `0009-c9-ocrypt-and-dom-wire-field-dispatch` — COMPLETE 2026-10-10

## Status

Complete 2026-10-10. Split out of `0009-c3`.

All three functions dispatch. Both files are recorded in `AC11_PENDING` at a
count of **zero**, and the reason text on each now records why the blocker was
false rather than leaving the old claim in place for the next reader to
inherit.

This mission exists because the blocker recorded for these three sites was
inherited, not derived — the fifth time in this arc, and the same shape as
`0009-c5`, `0009-c6`, `0009-c7` and the blanket `octo-network` text it
replaced.

## The recorded blocker, and why it is wrong

`AC11_PENDING` says, for `ocrypt/attestation.rs`:

> The remaining 6 are KeyRotation::verify and RevocationNotice, whose keys are
> FIELDS ON THE WIRE STRUCT — carrying an algorithm there is a wire format
> change under RFC-0853, which is a different blocker under a different RFC.

Three claims, checked one at a time.

**1. "RevocationNotice's key is a field on the wire struct" is false.**
`RevocationNotice` carries `compromised_key_id` and `successor_key_id` — those
are _identifiers_, not keys. The verifying key arrives as a parameter:
`verify_signature(&self, successor_public_key: &[u8; 32])`. This is the
`GatewayAttestation::verify_signature` shape that `0009-c7` dispatched three
commits ago. The reason text names the type and never opened it.

**2. "Carrying an algorithm there" presumes the only carrier is the wire.** It is
not. `verify_dispatch` takes `public_key: &[u8]`, so nothing forces the
algorithm to travel with the key. Every site in this arc resolved the same way:
the caller supplies the algorithm, because the caller is the only party that
knows which algorithm a key belongs to. That is a public-signature change,
not a format change.

**3. A wire field would be the worse answer here, not the blocked one.** For
`KeyRotation` and for `OverlayIntent`, the key is attacker-supplied wire data —
`intent.sender_id` is whatever the sender put in the message. An algorithm read
off that same message is under the same sender's control, so a wire field would
mean letting the party being verified choose the verifier. Resolving the
algorithm from the caller's registry instead is strictly safer. The blocker
argued for the option that weakens the check.

## Also: the "8 sites" were three functions

The pending count is a count of **constructor calls**, which is what
`ac11_site_count` measures. It has been read as a count of verification sites.
For these files the two differ:

| Function                             | Constructor calls |
| ------------------------------------ | ----------------- |
| `KeyRotation::verify`                | 4                 |
| `RevocationNotice::verify_signature` | 2                 |
| `dom/admission.rs` `admit_intent`    | 2                 |

Six counts, three functions. The count is not wrong — it is a different unit
than the one the surrounding prose assumed, and that mismatch is what made six
look like six pieces of work.

## Scope

Three verification functions, each gaining caller-supplied `AlgorithmId`
parameters.

- `RevocationNotice::verify_signature` takes one, matching `0009-c7`.
- `admit_intent` takes one. `sender_id` doubles as the sequence-tracking key,
  so the algorithm is resolved once by the caller and applied to the
  verification, not to the dedup map.
- `KeyRotation::verify` takes **two** — `old_algorithm` and `new_algorithm`.
  A rotation may cross algorithms, and a single parameter would assert they are
  the same. That is the one place where the wire-field concern has real force,
  and the two-parameter form is what resolves it.

## Blast radius — measured, not estimated

Neither `KeyRotation` nor `RevocationNotice` has a production caller. Both
appear only in `attestation.rs`'s own tests. `admit_intent` is called from the
DOM admission path. Nothing outside the workspace calls any of them.

Recorded because it is the fact that would have blocked this work had it been
assumed rather than checked, and it is also the reason the change is cheap —
not a reason to skip it.

## What this is NOT

Not a wire format change, and not an RFC-0853 amendment. The structs stay
byte-identical. The wire `signature` fields stay fixed 64-byte arrays.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `key_rotation_dispatches_both_algorithms` — happy path.
- `key_rotation_refuses_an_unregistered_algorithm_on_either_side` — the
  fail-closed half. Two halves matter here: a hardcoded verifier passes the
  happy path and the bad-signature check on both keys.
- `key_rotation_still_refuses_a_bad_signature`.
- `revocation_notice_dispatches_on_the_declared_algorithm`.
- `revocation_notice_refuses_an_unregistered_algorithm`.
- `admit_intent_dispatches_on_the_declared_algorithm`.
- `admit_intent_refuses_an_unregistered_algorithm`.
- existing `attestation` and `dom` vectors must pass unchanged.

**On the control:** the unregistered-algorithm vector is the only one that can
distinguish dispatch from a hardcoded Ed25519. Verified on every site in this
arc — vault, coordinator-types, the octo-network byte-parameter sites, the dot
envelopes. A hardcoded verifier satisfies the happy path _and_ the bad-signature
check everywhere.

### What the mutation actually showed

Hardcoding the algorithm at all four dispatch sites turned **exactly three**
vectors red: `key_rotation_refuses_an_unregistered_algorithm_on_either_side`,
`revocation_notice_refuses_an_unregistered_algorithm` and
`admit_intent_refuses_an_unregistered_algorithm`. The other 1532 stayed green,
including every happy path and every bad-signature check.

That is the point of the control, stated as a number: a verifier that ignores
the algorithm argument passes everything else in the suite. Only the
fail-closed vectors notice.

`KeyRotation` has two halves because it has two keys, and the mutation covers
both — passing one algorithm on both sides is refused, not accepted as a
convenience.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-network --all-targets --all-features -- -D warnings
cargo test -p octo-network --lib
cargo test -p octo-network --test dom_mempool --test e2e_live_scenarios
cargo test -p octo-cli --test security_regressions
```

1535 lib vectors, 16 and 14 integration vectors, 47 gate vectors. All pass.

## Backward compat

None required. Nothing is deployed, and the two `ocrypt` types have no
production caller at all.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `missions/open/0009-c3-network-verification-dispatch` — the categorization
  this corrects
- `missions/claimed/0009-c7-network-caller-supplied-key-dispatch` — the same
  edit for byte-parameter sites
- `crates/octo-cli/tests/security_regressions.rs` — `AC11_PENDING`

## Claimant

@unassigned

## Pull Request

#
