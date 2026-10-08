# 0973-v36-attestation-core-substrate — Attestation type, strategy traits, fail-closed registry, envelope wire form, mint and attenuation rules

**Status:** Claimed — @mmacedoeu (agent), 2026-10-07
**RFC:** RFC-0973 (Economics): Pluggable Proof Attestation for Capabilities — §Implementation Phases §Phase 1: Core
**Substrate:** `octo-cap-macaroon` (envelope, `AttestationTypeId`) + `octo-wallet::capability` (traits, registry)
**Depends on:** nothing. This is the base mission of the RFC-0973 set.

## Scope

Land the declaration, the traits, the registry, and the wire form. No strategy
crate is shipped here and nothing is verified at dispatch yet — this mission
produces the substrate the later missions build on.

1. **`AttestationTypeId`** — declare in `octo-cap-macaroon` alongside
   `CapabilityClass`, not in the wallet. The macaroon's canonical form must
   carry the id, and the macaroon cannot name a wallet type without a cycle.
   Same declare-once-then-re-export shape `CapabilityClass` already uses.

2. **Traits** in `octo-wallet::capability`:
   `AttestationStrategy` (`type_id`, `verify`) and the optional companion
   `AttestationAdmission` (`required_payload_kinds`,
   `verifier_needs_producing_kinds`, `admission_profile`). The split matters:
   `admission_profile` belongs to the **admission** trait, not the strategy
   trait, so a strategy that implements neither is distinguishable from one
   that implements the admission trait and answers `None`. Neither trait
   carries a default: absence is a claim the strategy must be true about, so it
   is written rather than inherited.

3. **Supporting types** — `Admission`, `AdmissionProfile`, `CostFigure`
   (`Measured` / `Unmeasured`), `SecurityFigure` (no bare-integer
   constructor), `SecurityParams`, `Expectation`, `PolicySource`,
   `AttestationError`, `AttestationVerifyCtx` (expectation required, not
   optional), `AttestationOutcome` (`Satisfied` / `Unsatisfied`, reason
   **required** on rejection). `AttestationError` is core-owned because
   `verify` returns it — a strategy in an extension crate cannot construct an
   error type it does not own.

   `Admission` is always present at the dispatch and boot sites, built from the
   strategy's own admission values when it implements `AttestationAdmission` and
   from the permissive value when it does not. There is no `Option<Admission>`
   at those sites: an absent profile and a permissive strategy must not be the
   same value, or a cost-bearing crate that never implements the trait would be
   indistinguishable from a genuine no-cost one and would skip both gates.

4. **`AttestationRegistry`** — fail-closed. Resolution is a pure lookup on a
   stable 128-bit key. Add the wallet's `octo-protocol` dependency here, in the
   same change as the admission trait: `required_payload_kinds` returns
   `PayloadKindId` and has no type to name until that dependency exists.

5. **`register_strategy`** — RFC-range ids require a membership hit in the
   allocation record _and_ a match against its named owner. The claimant is the
   crate identity the registration host observes **at link time**, never
   anything the strategy reports: a strategy asked to name itself would name
   whatever the record names. Every id is exclusively claimed, in either range,
   and a second claim is refused rather than replacing the incumbent.

6. **Allocation record** — the list of RFC-range ids core has allocated, each
   naming the crate it is allocated _to_. Until it exists the RFC range admits
   nothing. Binding the owner is what stops any crate claiming a core-sanctioned
   id and serving whatever it likes under it.

7. **Envelope wire form** — extend the capability canonical form with the
   attestation envelope, omitted entirely when absent, so an unattested
   capability serialises byte-identically. The envelope sits **inside** the
   hashed canonical form and **not** in the proof-bundle segment, which the hash
   excludes; that placement is normative, since it is what makes an attestation
   transplanted onto another capability produce a different registry key.

8. **Decode-time validation** — malformed envelope and oversized payload are
   both decode failures on the same path, before any strategy is resolved. The
   limit is set by the transport; this RFC fixes only that one exists and is
   enforced on read.

9. **Mint rule** — mint registers a capability under its **post-attestation**
   capability id, in the same operation that attaches the envelope. Registering
   first and attesting second would leave every attested capability absent from
   the registry, because the attestation changes the id the registry is keyed
   on. Mint MUST refuse to attach an attestation to a capability form carrying no
   value outside its own hashed form: the capability id is not a substitute,
   since it hashes the canonical form the envelope sits inside, so an
   expectation returning it would be a function of the attestation and the
   strategy would be comparing a token against itself.

10. **Attenuation rule** — carry the envelope forward **unchanged** and
    register the child under its recomputed id, in the same operation. Two
    directions are refusals: stripping (an unattested parent may not produce an
    attested child) and substitution (no different
    `(attestation_type_id, payload)` pair). Registration is additionally
    conditioned on the parent being active — without that, a bearer could
    attenuate a revoked capability into a fresh active record the revocation
    never named. The revocation half is a record flag and needs no clock. The
    expiry half is read wherever the bearer performs the attenuation, which is
    a different vantage point from the verifier and inherits the same
    untrusted-time exposure.

11. **Standing invariant** — a capability carrying an attestation MUST NOT be
    accepted without that attestation being checked. This binds at runtime, not
    only at a phase boundary, and it is why **this mission does not mint an
    attested capability**. The wire format can be extended before anything is
    minted into it.

12. **Back-compat corpus** — capture and freeze the pre-RFC capability fixtures.
    An unattested capability's canonical form must not change as a result of
    this mission.

## Acceptance Criteria

- [ ] `AttestationTypeId` declared in `octo-cap-macaroon`, re-exported to the
      wallet, no dependency cycle
- [ ] Both traits land with no defaults; absence means "declared no
      requirement", not "unspecified"
- [ ] `SecurityFigure` has no constructor taking a bare integer: the field is
      private, `from_prover_parameters` is the only way in, and `bits()` is the
      only way out, so a caller cannot read the value as a struct field and
      supply it another way
- [ ] `AttestationVerifyCtx.expectation` is a required field, not an `Option`
- [ ] `AttestationOutcome` carries a reason on rejection, and none on
      satisfaction
- [ ] Registry resolution is fail-closed and unregistered ids reject
- [ ] Registration refuses an RFC-range id absent from the allocation record,
      and one present but allocated to a different crate
- [ ] The claimant is supplied by the registration host, not the strategy
- [ ] An unattested capability's canonical bytes are unchanged
- [ ] Envelope is inside the hashed canonical form and outside the
      proof-bundle segment
- [ ] Malformed and oversized envelopes both reject at decode, before strategy
      resolution
- [ ] Mint registers under the post-attestation id
- [ ] Mint refuses an attestation on a form carrying no bindable value
- [ ] Attenuation refuses stripping and substitution
- [ ] Attenuation refuses when the parent record is absent, revoked, or expired
- [ ] No attested capability is minted by this mission

### Type Coverage

| RFC type                           | Implemented by |
| ---------------------------------- | -------------- |
| `AttestationTypeId`                | This mission   |
| `AttestationStrategy` (trait)      | This mission   |
| `AttestationAdmission` (trait)     | This mission   |
| `AttestationRegistry`              | This mission   |
| `Admission`                        | This mission   |
| `AdmissionProfile`                 | This mission   |
| `CostFigure`                       | This mission   |
| `SecurityFigure`                   | This mission   |
| `SecurityParams`                   | This mission   |
| `Expectation` (trait)              | This mission   |
| `PolicySource` (trait)             | This mission   |
| `AttestationVerifyCtx`             | This mission   |
| `AttestationError`                 | This mission   |
| `AttestationOutcome`               | This mission   |
| Id allocation record               | This mission   |
| Envelope wire form + decode bounds | This mission   |
| Mint rule (register, refuse)       | This mission   |
| Attenuation rule (carry, refuse)   | This mission   |

Deferred, and accounted for rather than unlisted:

| RFC item                            | Implemented by                                     |
| ----------------------------------- | -------------------------------------------------- |
| `register_strategy` execution       | Declared here, exercised by mission A              |
| Back-compat corpus                  | This mission, re-verified by mission A             |
| `AttestationVerifyCtx` verification | Mission A (`0973-v36-a`), which builds the context |
| `octo-protocol` dependency          | This mission                                       |

### Test Vectors

This mission closes: **TV5**, **TV5b**, **TV6b** (mint side), **TV9**,
**TV15**, **TV7e**, **TV7f**.

**TV14** is split here. The half this mission can run is the mint-side gate:
refusal happens before a hash relation is ever evaluated. The verification half
is closed by mission A, which is where `AttestationVerifyCtx` is built and the
acceptance set read.

The registration-refusal vectors **TV7g** and **TV7h** are declared here because
the gate is declared here, but they are only _runnable_ once mission A lands the
registration host that supplies the claimant. Mission A closes them.

Not claimed here: **TV16**, which needs two independent implementations of one
relation and belongs to mission A.

## Layer direction

- `octo-cap-macaroon` — Layer B years-stable substrate. `AttestationTypeId`
  and the envelope are additive to the canonical form; the unattested form
  stays byte-identical.
- `octo-wallet` — Layer B years-stable substrate. New module in
  `octo-wallet::capability`. No existing capability type changes shape. The
  mint and attenuation call sites in `octo-cap-macaroon` **are** modified, by
  design: they gain the type-id and envelope parameters, and refusing them is
  the whole point of the mint rule.
- No strategy crate exists yet, so nothing here depends on Layer E. That is the
  point of landing the substrate first.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-cap-macaroon -p octo-wallet --all-targets --all-features -- -D warnings
cargo test -p octo-cap-macaroon --lib
cargo test -p octo-wallet --lib capability
```

The back-compat corpus is the check that matters most here: if any pre-RFC
fixture's bytes changed, this mission is wrong regardless of what else passes.

## Backward compatibility

An unattested capability must serialise byte-identically, and every capability
minted before this mission must still verify. The frozen corpus is the
mechanism, not an assertion.

## Cross-references

- RFC-0973 §Data Structures — type declarations and the name-collision table
- RFC-0973 §Root-Secret Binding — why the envelope placement is normative
- RFC-0973 §Lifecycle Requirements — requirement 1 (registration), requirement
  4 (mint and attenuation), requirement 5 (back-compat)
- RFC-0973 §Roles and Authorities — who may attach an attestation
- RFC-0973 §Adversarial Analysis — A2 (transplant), A3 (id collision), A3b
  (unallocated id), A6 (stripping by attenuation)
- RFC-0957 — Capability Token Format, amended by this RFC's envelope
- RFC-0871 §Node Role — `PayloadKindId` source
- RFC-0008 — execution-class taxonomy this RFC carries a mapping for

## Claimant

@mmacedoeu (agent, 2026-10-07) — autonomous implementation, PR(s) for human review per BLUEPRINT.md Agent Workflow.

## Pull Request

#
