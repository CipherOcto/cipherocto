---
name: 0009-c-verification-dispatch
description: RFC-0009 v1.3 AC#11. Move every holder- and capability-signature verification site off hardcoded ed25519_dalek::VerifyingKey::from_bytes and dispatch through AlgorithmRegistry.
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

A survey of `VerifyingKey::from_bytes` across the workspace found the call is
far more widely distributed than a single-crate migration would suggest. It
appears in, among others:

- `octo-network` — `reputation.rs`, `ocrypt/attestation.rs` (multiple),
  `dom/admission.rs`, `dot/dc.rs`, `dot/envelope.rs`, `drs/route.rs`,
  `porelay/forwarding.rs`, `heartbeat.rs`, `availability.rs`, `bandwidth.rs`
- `octo-settlement-core`
- `octo-cap-macaroon` — `holder_sig: ed25519_dalek::Signature` on the token
- `octo-cap-zk` — imports `ed25519_dalek::Signature`; `holder_sig: Signature`
- `octo-protocol`, `octo-sync`, `octo-runtime`, `quota-router-storage`,
  `octo-adapter-nostr`, `octo-reputation`, `octo-wallet-node`,
  `quota-router-sm-engine`

**Re-survey before starting** rather than trusting this list — it was a
point-in-time grep. The exhaustive check is
`no_verification_site_hardcodes_an_algorithm`, which greps the source and
fails if the symbol appears outside the registration. That vector is the
acceptance condition, not this list.

Note the distinction that matters: sites that _sign_, sites that _verify a
holder signature_, and sites that _carry a typed signature field_ are not the
same job. Only the latter two need dispatch. Signing through the registry is
`0009-e`'s problem, not this one.

### Per-key revocation lands here too

AC#14 requires the verifier to consult key-level revocation
(`VerificationMethod.revoked_at`) **before** the identity-scoped check. The
identity gate is RFC-0958's. The key gate has no home until a verification site
resolves the DID, which none of these currently do.

RFC-0009 A14's oracle: one STARK-curve key compromised, identity otherwise
sound, Ed25519 key untouched — the holder must survive that. Make
`compromised_key_revoked_without_identity_revocation` pass here.

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors (per RFC-0009 v1.3 §Acceptance Criteria)

- `no_verification_site_hardcodes_an_algorithm` — grep-based, workspace-wide
- `cross_algorithm_signature_replay_rejected` — a signature made under one
  registered algorithm does not verify under another, **including the
  prefix-stripped message case**, not only the wrong-key case
- `revoked_identity_replay_rejected` (RFC-0958 v1.7)
- `compromised_key_revoked_without_identity_revocation`

Gate: `cargo test -p octo-ident --lib` plus the per-crate suites below.

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
cargo clippy -p octo-ident -p octo-cap-macaroon -p octo-cap-zk -p octo-network --all-targets --all-features -- -D warnings
cargo test -p octo-ident --lib
cargo test -p octo-cap-macaroon -p octo-cap-zk --lib
```

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
