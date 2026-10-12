---
name: 0957-a2-capability-chain-and-discharge-verification
description: The live capability verification path in quota-router-core verifies the holder signature and the attestation, but neither the macaroon caveat chain nor third-party discharges. verify_with_resolve documents chain verification as delegated to the caller's catalog; the only catalog on that path is annotated dead-code pending a future acceptance criterion, so the delegation terminates in nothing.
metadata:
  node_type: mission
  type: specification
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0957
---

# Mission `0957-a2-capability-chain-and-discharge-verification`

> **A documented contract that governs nothing.** The wallet's verification
> entry point says the macaroon-chain check is "the caller's responsibility via
> the issuer's catalog". The only caller on the live path holds its catalog
> under `#[allow(dead_code)]`, annotated as future work. The chain check has no
> implementation on that path.
>
> **What this mission does not claim.** It does not claim the capability system
> is exploitable today. The parts that _are_ implemented are implemented well
> and mutation-tested. What is missing is the half of the verification that the
> prose promises.

## Status

Open (2026-10-12) — filed by audit. The fix is a protocol decision (which
caveat forms bind, how a third-party channel root is resolved at verification
time), so it is maintainer work.

## The path, traced

`GatewayAuthenticator` holds a `ProductionCapabilityVerifier`. Its `verify`
calls `verify_with_resolve`, then resolves the ask binding. Between them these
run:

1. wire parse and cap-root-hash computation
2. holder record lookup, with the registry authoritative on holder identity
3. active check, covering revoked and expired
4. `verify_holder_sig` — the Ed25519 signature over root id and caveats wire
5. the RFC-0973 attestation dispatch, deliberately placed inside the production
   entry point
6. `ask_binding`, refusing the ambiguous multi-binding case

Not on that path: the macaroon caveat chain, and third-party discharges.

## Where the two halves go

**The chain.** `verify_with_resolve` documents the deviation in place:

> Deviation: this implementation uses the existing `verify_holder_sig` (which
> covers the Ed25519 signature over the macaroon root_id + caveats wire) — the
> macaroon-chain verify is delegated to the catalog (RFC-0957-A1
> §Algorithms:adapter_mode).

The catalog is constructed and stored on `GatewayAuthenticator` as
`pub catalog: Arc<dyn CapabilityCatalog>` carrying
`#[allow(dead_code)] // Future AC: catalog lookup for gossip/settlement`. So the
delegation has a holder and no use. `CompositeCapabilityCatalog` and
`TransportDeliveryCatalog` exist and are implemented; nothing on the ingress
path reaches them.

**Discharges.** `verify_discharges` is correct and thoroughly tested. It is
re-exported from production source in both `octo-cap-macaroon` and
`octo-wallet`, and it is called from neither. The only production read of
`token.discharges` anywhere in the tree is inside `verify_discharges` itself.
Every other reference constructs, serialises, or tests.

The consequence is concrete. A `CapabilityToken` carrying third-party caveats —
escrow, revocation, rate-limit per RFC-0957 §3.6 — passes this path with those
caveats unexamined, because the verifier that would examine them is never
invoked. `verify_holder_sig` covers the signature over the caveat bytes; it
does not evaluate them.

## Scope

### In Scope

- Decide and implement macaroon-chain verification on the live ingress path, or
  record with the threat named that it is deliberately deferred at this layer.
- Decide and implement third-party discharge verification, or record the
  accepted gap the same way.
- If both stay deferred, correct the prose on `verify_with_resolve` so it stops
  describing a delegation that no caller performs. **This half is not optional.**
  A comment naming a check that nothing runs is the same defect class as a
  function named after a check it does not perform.

### Out of Scope

- `verify_holder_sig`, `lookup_active`, the attestation dispatch, and the ask
  binding — all present, all tested, none in question.
- The escrow, revocation and rate-limit channel operators themselves.
- Capability _minting_.

## Acceptance Criteria

- [ ] Either the live path verifies the caveat chain, or the deviation is
      recorded against the threat it leaves open. Mutation-verified either way:
      a token carrying an unsatisfiable caveat must be refused once the check
      lands, and restoring the current state must turn that test red.
- [ ] Either the live path verifies discharges, or the gap is recorded. The
      acceptance test is a token whose third-party caveat is unsatisfied and
      which must be refused.
- [ ] The `verify_with_resolve` doc describes what the code does. Today it
      names a delegation that terminates in a dead-code field.
- [ ] If the catalog stays unused on ingress, `#[allow(dead_code)]` says why in
      terms a reader cannot mistake for "already wired".

## What must not happen

**Do not treat the existence of `verify_discharges` as enforcement.** It is a
public function with no production caller. Its presence is the reason this gap
is easy to miss on a read-through, and it is exactly the shape this repository
keeps finding: a check that exists, is tested, and authorises nothing.

**Do not close this by adding a call without deciding what the check means.**
`verify_discharges` needs channel roots resolved out of band. Calling it with a
resolver that returns something trivially true would convert an honest gap into
a silent one, which is worse than the current state.

**Do not fix only the doc.** The prose correction is required, but a comment
that admits the gap while the gap stays open is a documentation task, not a
resolution of the threat.
