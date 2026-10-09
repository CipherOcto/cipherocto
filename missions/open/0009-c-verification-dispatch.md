---
name: 0009-c-verification-dispatch
description: RFC-0009 AC#11. Move every holder- and capability-signature verification site off hardcoded ed25519_dalek::VerifyingKey and dispatch through AlgorithmRegistry. Two site shapes, 65 sites across 32 files.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
---

# Mission `0009-c-verification-dispatch` — OPEN 2026-10-09

## Status

Open (2026-10-09). Claimable once `0009-b-algorithm-registry` lands.

## Dependencies

`depends_on: 0009-b-algorithm-registry`. Without the registry there is nothing
to dispatch through. `0009-e` (signable prefix) also gates full correctness but
is not a hard compile-time dependency.

> **Conflict to declare, not discover.** This mission touches
> `crates/octo-cap-macaroon` and `crates/octo-cap-zk`, where `holder_sig` is
> currently pinned as `ed25519_dalek::Signature`. Those crates are owned by
> `0957-ext-macaroon-crate` and `0957-ext-zk-crate`. Claim this alongside them
> or coordinate the handoff — do not land a conflicting signature change on top
> of in-flight work.

## Scope

Replace every hardcoded algorithm at every verification site with a registry
lookup. AC#11's requirement is that `VerifyingKey::from_bytes` and
`Signature::from_bytes` MUST NOT appear at any holder- or
capability-signature verification site outside the Ed25519 registration
itself.

### The sites, as of 2026-10-09

There are **two** shapes, and the mission's original acceptance grep only saw
one of them. This is the finding that matters most in this file.

**Shape 1 — a key is constructed from bytes.** `VerifyingKey::from_bytes`:
**38 occurrences across 25 files**, including `octo-settlement-core`,
`octo-adapter-nostr`, `octo-runtime`, `octo-protocol`, `octo-reputation`,
`octo-wallet-node`, `quota-router-sm-engine`, `quota-router-storage`,
`octo-cap-macaroon` and ten files under `octo-network`.

**Shape 2 — the algorithm is a parameter type.** `: &VerifyingKey` (or
`&ed25519_dalek::VerifyingKey`) in a signature: **27 occurrences across 7
files**, every one of them under `crates/octo-network/src/dot/` —
`dc_envelopes.rs` (7), `handover.rs` (6), `kick_envelopes.rs` (5),
`binding.rs` (5), `decommission.rs` (2), `sub_group.rs` (1), `witness.rs` (1).

Shape 2 is a typed `&VerifyingKey` parameter in a public signature — 25 of
them are `pub fn verify`, plus `validate_bind` in `witness.rs` and `append` in
`handover.rs`. An Ed25519 verifying key is baked into a public API.
**Shape 1's grep matches none of them.** An implementer who greps for
`pub fn verify` to size the refactor finds 25 and misses two. And because
shape 2 already has a typed `VerifyingKey`, it compiles perfectly after the
enum is removed — nothing forces it to change.

The `octo-wallet` sites were also missing entirely, and that crate holds the
single most important verifier in the RFC: `IdentityKey::verify`, which is the
only verification entry point on RFC-0009's own primary type. Also
`verify_successor_proof` and `verify_revocation_proof` in the same file.

**Re-survey before starting** rather than trusting this list — it was a
point-in-time grep. The exhaustive check needs **both** shapes, and the
`octo-network` `dot/` sites need their own review pass because they are a
different refactor: they take a `VerifyingKey` where the registry wants an
`AlgorithmId`, so it is a signature change across 27 public entry points, not
a body-level substitution.

Note the distinction that matters: sites that _sign_, sites that _verify a
holder signature_, and sites that _carry a typed signature field_ are not the
same job. Only the latter two need dispatch. Signing through the registry is
`0009-e`'s problem, not this one.

### Per-key revocation lands here too — but check who owns it first

AC#14 requires the verifier to consult key-level revocation
(`VerificationMethod.revoked_at`) **before** the identity-scoped check. The
identity gate is RFC-0958's. The key gate has no home until a verification site
resolves the DID, which none of these currently do.

**A verifier-side key gate cannot fire on the ZK path** while RFC-0958
§Algorithms resolves the DID inside the circuit — the verifier never reaches
the decision. `0958-a` says the same thing and explicitly disclaims this gate,
so neither mission currently holds it.

An implementer who builds it here will produce a gate that never executes
where it is needed, while this file reports it delivered. **Resolve the owner
before claiming either mission**, and record the answer in both. The honest
options: the circuit stops resolving the DID and the verifier owns both gates,
or the key gate stays a real gap until new public inputs exist. What must not
happen is two missions each half-claiming it.

RFC-0009 A14's oracle: one STARK-curve key compromised, identity otherwise
sound, Ed25519 key untouched — the holder must survive that. Ownership of
`compromised_key_revoked_without_identity_revocation` is **undecided**; see the
table below. Do not claim the vector until the answer is recorded in both
`0009-c` and `0958-a`.

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors (per RFC-0009 §Acceptance Criteria)

- `no_verification_site_hardcodes_an_algorithm` — grep-based, workspace-wide,
  and it must match **both** shapes or it proves nothing:
  - `VerifyingKey::from_bytes` and `Signature::from_bytes` outside the Ed25519
    registration
  - `: &VerifyingKey` and `&ed25519_dalek::VerifyingKey` as a **parameter
    type** anywhere under `crates/`

  The second pattern is the one that catches the 27 `pub fn verify` sites in
  `octo-network/src/dot/`, and it is the pattern the original mission omitted.
  A vector that greps only the first reports green with 27 hardcoded Ed25519
  entry points still live.

- `cross_algorithm_signature_replay_rejected` — **owned by `0009-e`, not this
  mission's to deliver.** Listed because the dispatch sites here are what the
  prefix protects; the vector itself runs in `0009-e`, and it needs `0009-d` to
  have registered a second algorithm before it can run over a real pair.
- `revoked_identity_replay_rejected` — RFC-0958's, i.e. `0958-a`'s. Listed
  here for completeness; it is **not** this mission's to deliver.
- `compromised_key_revoked_without_identity_revocation` — blocked on the
  ownership question above, and on `0009-d` for a second key to compromise.

### Vector ownership, stated once

Three vectors have more than one plausible owner. Per BLUEPRINT
§Multi-Mission Decomposition, ownership is decided **before** implementation,
not discovered when two branches collide:

| Vector                                                | Owner         | Note                                                                                                                                                    |
| ----------------------------------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `no_verification_site_hardcodes_an_algorithm`         | `0009-c`      | dispatch, uniquely this mission's                                                                                                                       |
| `cross_algorithm_signature_replay_rejected`           | `0009-e`      | it is a property of the prefix, and `0009-e` already co-owns it. Dispatch-level mismatch rejection is this mission's, but that is a different assertion |
| `revoked_identity_replay_rejected`                    | `0958-a`      | the identity gate is RFC-0958's                                                                                                                         |
| `compromised_key_revoked_without_identity_revocation` | **undecided** | see §Per-key revocation                                                                                                                                 |

Gate: the per-crate suites below. **`octo-network` is in the clippy list but
was missing from the test list** — and `octo-network` holds all 27 shape-2
sites, so a green run that skips it validates nothing about the hard part.

## Layer direction (per `cipherocto-design-principles`)

- `octo-ident` (Layer B) — owns the registry; every consumer dispatches through
  it and MUST NOT reach into `ed25519_dalek` for a verification decision
- `octo-cap-macaroon`, `octo-cap-zk` (Layer E) — consume the registry, define no
  parallel abstraction

The point of the registry is that this crate's signature stops being a
cross-crate coupling. If `octo-cap-macaroon` still names `ed25519_dalek::Signature`
in its public API after this mission, the abstraction did not do its job.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-ident -p octo-cap-macaroon -p octo-cap-zk -p octo-network -p octo-wallet --all-targets --all-features -- -D warnings
cargo test -p octo-ident --lib
cargo test -p octo-cap-macaroon -p octo-cap-zk --lib
cargo test -p octo-network --lib
cargo test -p octo-wallet --lib
```

`octo-network` and `octo-wallet` are in the test commands, not just the clippy
one: `octo-network` holds every shape-2 site and `octo-wallet` holds
`IdentityKey::verify`. Both were missing from the test gate.

Do not run the full workspace suite for this mission; `octo-network` alone is
long and the touched crates are the ones under change.

## Backward compat

None required. Nothing is deployed. The failure mode to avoid is leaving a
site on the old path "temporarily" — that is a parallel abstraction, and per
§No parallel abstractions it never converges.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11, AC#14, A12, A14
- `missions/open/0009-b-algorithm-registry` — the registry this dispatches through
- `missions/claimed/0957-ext-macaroon-crate`, `0957-ext-zk-crate` — crate ownership conflict
- `missions/open/0009-e-signable-domain-separation`

## Claimant

@unassigned

## Pull Request

#
