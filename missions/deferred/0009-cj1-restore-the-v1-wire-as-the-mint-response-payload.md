---
name: 0009-cj1-restore-the-v1-wire-as-the-mint-response-payload
description: "Implements draft RFC-0009-A1. The mint handlers respond with a CapabilityBundleV2 descriptor that contains no macaroon, while ProductionCapabilityVerifier and the attenuate handler consume the v1 macaroon wire. Measured: 2 production consumers of the v1 wire, 0 of the V2 bundle, every V2 canonical_de call site in a test. The capability lifecycle has no join because the cutover moved the issuing end and left both consumers alone. Blocked on RFC-0009-A1 acceptance, because the fix is a semver-major edit to a frozen Layer A wire form and the decision belongs to the RFC, not to a mission."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009-A1
    - RFC-0957
    - RFC-0957-A1
---

# Mission `0009-cj1-restore-the-v1-wire-as-the-mint-response-payload`

> **DEFERRED — blocked on RFC-0009-A1 acceptance.**
>
> The work below is specified and ready. It must not be implemented while the RFC
> is Draft, because the decision it encodes is a semver-major change to a frozen
> Layer A wire form and belongs to the RFC review, not to a mission. Per
> `docs/BLUEPRINT.md` §Mission Lifecycle, "Deferred" is the state for an upstream
> blocker, and an RFC pending is one of the three named triggers.

## Status

Deferred (2026-10-12) — pending RFC-0009-A1 acceptance. Move to `missions/open/`
when the RFC is Accepted.

## The blocker, measured

| Wire form            | Production consumers                                                                                 | Test-only consumers |
| -------------------- | ---------------------------------------------------------------------------------------------------- | ------------------- |
| v1 macaroon wire     | **2** — `verify_with_resolve`, the `WALLET_ATTENUATE_CAPABILITY` handler                             | several             |
| `CapabilityBundleV2` | **0** — every `CapabilityBundleV2Envelope::canonical_de` call site in the workspace is inside a test | every call site     |

Production writes the V2 bundle from two mint handlers and reads it nowhere. The
credential the system verifies is not the credential the system issues.

Each half still passes its own tests. Mint returns a well-formed envelope.
Attenuate accepts a wire token. Verify accepts a wire token. **No test spans the
join**, which is why the disconnect survived a cutover mission that had tests for
every component it touched.

## Why this is deferred rather than filed open

RFC-0009 §Forward compatibility defines `CapabilityTokenV2` with six fields, none
of them a capability. The implementation matches that schema field for field, so
this is an RFC defect rather than an implementation slip. RFC-0009 is RFC-frozen
and moves only by semver-major amendment. A mission that quietly reverted the
payload would be making the decision the RFC process exists to make.

## Scope

### In Scope

- Both mint handlers respond with `serialize_wire(&token)`.
- Both drop V2 envelope construction from the response path.
- The response payload contract is asserted: ASCII, 3 `.`-separated segments.
- An integration test spanning mint → attenuate → `ProductionCapabilityVerifier::verify`.
- The descriptor's documentation stops implying it is a credential.

### Out of Scope

- Deleting `CapabilityBundleV2`. RFC-0009-A1 §Implementation Phases Phase 3
  leaves that as a maintainer decision, and this mission assumes retention.
- The macaroon, the chain, the holder signature, the discharge protocol,
  `HolderRegistry` and `HolderRecord`. None of these change.
- Attaching `Caveat::AskBinding`. That is RFC-0959 §Phase 3 and sits behind this.
- `expires_at_unix_secs`, which stays `u64::MAX` with expiry in caveats.

## Implementation Phases

### Phase 1: Restore the payload

- [ ] `crates/octo-wallet-node/src/handlers/mint.rs` responds with the v1 wire.
- [ ] `crates/octo-capability-issuer-node/src/handlers/issue.rs` responds with the v1 wire.
- [ ] `crates/octo-cap-macaroon/src/bundle_v2.rs` documents the descriptor as a
      descriptor, with no wording that implies it can be presented.
- [ ] Both handlers' existing envelope-shape tests are replaced, not deleted —
      they assert what the response is, and the answer changes.

### Phase 2: Close the loop

- [ ] An integration test that mints through a node handler with a real
      in-memory `StoolapHolderRegistry`, feeds the returned wire to
      `ProductionCapabilityVerifier`, and asserts the specific refusal for each
      unmet precondition (`AskUnbound`, `HolderUnknown`) rather than a bare error.
- [ ] The same test attenuates before verifying, so `chain_depth = 1` and the
      holder signature are exercised across the join.

### Phase 3: Guard the join

- [ ] A guard in `crates/octo-cli/tests/security_regressions.rs` asserting both
      mint response payloads parse as a v1 wire. **Verified to go red by planting
      a descriptor payload** — a guard that only confirms the current shape is a
      census, not a check.

## Acceptance Criteria

- [ ] A capability minted through either node handler round-trips into
      `ProductionCapabilityVerifier::verify`. **No current test covers this span**,
      which is why the defect survived.
- [ ] Both mint handlers emit a payload that is ASCII and `.`-separated, asserted
      as a shape contract rather than by fixture bytes.
- [ ] No response payload contains a capability root secret. This is the regression
      test for the leak that the descriptor's `holder_record_bytes` field carried.
- [ ] The two producers agree on what they emit, in the sense RFC-0009-A1
      §Compatibility requires.
- [ ] The join guard is red when a descriptor is planted as the payload, and green
      when a wire is.

## What must not happen

**Do not implement this before the RFC is Accepted.** The direction is not the
only question — option (a) in RFC-0009-A1 §Alternatives Considered puts the
macaroon inside the V2 bundle instead, and it is a legitimate reading of the same
defect. Choosing between them without review is the failure this deferral exists
to prevent.

**Do not add the capability to the descriptor and call it fixed.** A descriptor
carrying the macaroon it describes is the alternative the RFC rejects, and
implementing it here would foreclose the decision.

**Do not write the join test against fixture bytes.** RFC-0009-A1 §Test Vectors TV
2 is the test that matters and it must run against genuinely minted output. A
fixture-byte version passes whether or not the producer is correct, which is
exactly how the current tests let this through.
