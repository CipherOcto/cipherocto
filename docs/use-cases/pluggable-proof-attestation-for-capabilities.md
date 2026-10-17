# Use Case: Pluggable Proof Attestation for Capabilities

## Problem

CipherOcto is about to define **two ZK-based capability types**, and third parties
are expected to add more. There is no substrate for a capability to declare _what
it proves_.

The current code has no such axis. `CapabilityClass { V1, ZKBearing }` looks like
one, but it is a closed two-variant enum: adding a third kind of proof is a
central edit plus a cross-crate review. It was redeclared in three crates until
converged, and it answers "does this token carry a proof" rather than "what does
this proof establish".

It also conflates two things a proving substrate must keep separate. **What is
proven** (knowledge of a root secret; that an inference ran inside the protocol
boundary; something else entirely) is extensible and unbounded. **Whether the
verifier can check it** is a local capability. Binding both into one enum caps
the first at the second's vocabulary.

## Stakeholders

- **Primary:** node operators. A `quota-router-node` operator decides whether
  their node accepts attested capabilities, and which attestations it runs
  verifiers for.
- **Primary:** third-party extension authors, who implement a new attestation
  strategy in their own crate with no core edit.
- **Secondary:** capability issuers, who must know which attestations a
  consuming node will accept before minting.
- **Affected:** core maintainers, whose review surface must not grow per
  attestation type.

## Motivation

Three independent facts force this.

**ZK here is a general proving substrate, not one authorization mechanism.** It
is easy to read `Authorization::Proof(ProofBundle)` in RFC-0871's envelope and
conclude that a proof is an authorization variant alongside signatures and
capabilities. That framing is too narrow to hold what is coming. A proof is a
general way for a holder to establish a fact to a verifier without revealing
the witness. Knowledge of a root secret is one instance. That an inference ran
inside the protocol boundary is another. A third party will want a third, and
the mechanism must not have to be renegotiated for each.

**Node specialization already has the right extension model, and this must
reconnect to it.** RFC-0871 §Node Role — Payload-Kind Declared, amended
2026-10-07, makes a node's role the set of `PayloadKindId` UUIDs it serves. An
attestation strategy can name the payload kinds a node must serve in order to
_produce_ a valid attestation, which is what links "what is proven" to "who may
prove it" without a role enum.

**Operators need to choose, and the choice must be safe.** A node that accepts
an attestation it cannot verify is worse than a node that rejects it, because
the operator's configuration would be an unauditable claim rather than a
decision.

## Success Metrics

| Metric                                          | Target                               | Measurement                                          |
| ----------------------------------------------- | ------------------------------------ | ---------------------------------------------------- |
| Core edits to add the Nth attestation strategy  | 0, for all N                         | strategy crate PR touches no `octo-wallet` file      |
| Strategies shippable by a third party           | ≥1 registered from an external crate | an out-of-tree crate registering without core change |
| Unknown attestation discriminator               | 100% rejected, never downgraded      | negative test with an unregistered id                |
| Policy naming an unverifiable strategy          | 100% fail-closed at boot             | config test refuses to start                         |
| Operator can restrict to non-attested operation | supported                            | config-driven, unit-tested                           |
| Pre-existing capabilities remain readable       | 100%                                 | historical token corpus replay                       |

## Constraints

- **Must not** require a central edit to add an attestation strategy.
- **Must not** let an operator configure acceptance of a strategy the node
  cannot verify.
- **Must not** treat an unknown discriminator as a downgrade.
- **Must not** change the meaning of any already-minted capability.
- **Limited to** what a capability attests and how a node checks it. Caveat
  evaluation, trace folding, and inference execution semantics stay with the
  RFCs that own them.
- **Limited to** the attestation axis. Node role stays with RFC-0871 and is not
  redefined here.

## Non-Goals

- Choosing which strategies are mandatory. Every strategy is optional; operators
  decide.
- Fixing RFC-0958's proof-size gap (G3, measured 2,993,325 bytes against a
  50–500 KB target). RFC-0958 §Future Work F7, recursive composition, is the
  named answer and is tracked there.
- Re-implementing `cairo/src/blake3.cairo`. Its cost is measured and known; it
  is not this RFC's decision.
- Building `quota-router-node` or `wallet-node`. They are named here as the first
  consumers whose requirements should shape the interface.
- Splitting `HolderKind`'s conflation of verification and delivery classes.
  That is an RFC-0957-A1 matter and is independent.

## Impact

- `octo-wallet::capability` gains an `AttestationStrategy` trait and a
  fail-closed registry. This is the Layer E registration point CLAUDE.md
  describes, made concrete for the capability substrate.
- Strategies ship as their own `octo-cap-*` crates per RFC-0965's layout.
- A node operator configures which attestations their node accepts, with
  `authoritative` as the shipped default. The operator may restrict the set,
  including to non-attested operation, and the node refuses to start if it is
  configured to accept something it cannot verify.
- `CapabilityClass` survives as a coarse wire discriminator for whether a proof
  is present, and stops being the extensibility axis.
- The two ZK-based capability types being defined get first instances rather
  than a bespoke mechanism each.

## Related RFCs

- RFC-0871 (Networking): Specialized Node Protocol Envelope — §Node Role — Payload-Kind Declared
- RFC-0957 (Economics): Capability Token Format
- RFC-0958 (Proof Systems): ZK Capability Subclass
- RFC-0965 (Economics): Capability Extension Format — §Per-Extension Crate Layout
- RFC-0630 (Consensus): Proof of Inference
