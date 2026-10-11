---
name: 0009-ca-protocol-authorization-algorithm-dispatch
description: RFC-0009 AC#11, octo-protocol Authorization::Signature. Dispatch the RFC-0871 envelope authorization verification and remove the allowlist entry that was skipping it.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "2.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c-verification-dispatch
    - 0009-c9-ocrypt-and-dom-wire-field-dispatch
---

# Mission `0009-ca-protocol-authorization-algorithm-dispatch` — COMPLETE 2026-10-10

## Status

Complete 2026-10-10. Split out of `0009-c3`.

### What the counterfactual showed

The `AC11_ALLOWED` entry is removed, and that is the substance of the change
rather than a side effect. Measured directly, with one plant and one variable:

- plant a real inline site in `authorization.rs`, entry **present** — all 47
  vectors green.
- identical plant, entry **removed** — `ac11_no_verification_site_rebuilds_a_key_or_signature_inline`
  goes red and names the file and both constructors.

So the entry was not a documented exception. It was the only thing standing
between RFC-0871's authorization check and an unscanned file.

## Why the allowlist entry is wrong

`AC11_ALLOWED` carries this for `crates/octo-protocol/src/authorization.rs`:

> `Authorization::Signature` carries no algorithm and the `did:octo:z` payload is
> a bare 32 bytes, so the dispatcher has nothing to dispatch on. Blocked on the
> same DID codec as mission 0009-c3.

Three claims. The third is false by inheritance — the 0009-c3 DID-codec blocker
was disproved in `13ced5e7` and this entry still cites it.

The first two are true about the _data_ and false as a _conclusion_. The DID
payload is a bare 32 bytes, yes. But so is `intent.sender_id`, which `0009-c9`
dispatched this morning by adding a caller parameter. `verify_dispatch` takes
`public_key: &[u8]`, so nothing forces the algorithm to travel with the key. The
caller supplies it, as it does at every other site in this arc.

Measured: `octo-protocol` contains **no occurrence of the word "algorithm"**.
`verify_ed25519_signature` is the only authorization verification path in the
crate, and `dispatch.rs` routes every `Authorization::Signature` through it.

## Why this is the highest-value site in the arc

This is not one envelope type among many. RFC-0871 §Adversary Analysis A6
requires that ALL authorizations in `Vec<Authorization>` verify, logical AND
not OR — so this function is the gate every signed envelope passes through
before anything else in the system happens.

It has been _unscanned_ this whole time. `AC11_ALLOWED` does not exempt one
site; it exempts the entire file from `ac11_no_verification_site_rebuilds_a_key_or_signature_inline`.
A new inline site added beside it would not be seen. That is the exact rot
`ac11_every_allowlist_entry_still_exists_and_still_has_a_reason` was written to
catch, and the reason it missed here is that the reason was long, specific, and
wrong.

## Scope

`verify_ed25519_signature` takes a `&AlgorithmId` and dispatches. Three
production call sites:

- `octo-protocol/src/dispatch.rs` — the envelope authorization path
- `octo-protocol/src/hop_signature.rs` — per-hop signatures
- `quota-router-core/src/node/envelope_v2.rs` — the router's own envelope

Plus two in `octo-identity-resolver-node`'s cross-node chain tests.

## The name has to change

The function is `verify_ed25519_signature` and the wire newtype is
`Ed25519SignatureBytes`. Once an algorithm parameter exists, the function name
asserts something it no longer guarantees.

The function is renamed to `verify_authorization_signature`. Renamed rather
than deprecated alongside a shim, per the standing directive on bare-sign entry
points — two names for one behaviour is the thing this directive exists to
prevent.

The wire newtype keeps its name. That is a different claim: the borsh wire form
is a flat 64-byte buffer, which is a statement about the format, not about how
it gets verified. Renaming it would be a wire-format change under RFC-0871 and
is out of scope here.

## The allowlist entry is removed, not rewritten

Not "moved to a better reason" — **removed**. After the dispatch there is no
inline constructor left in the file, so the file goes back under the scan. That
turns an exemption into coverage, which is the point: an allowlist entry that
outlives its site is how this file was dark in the first place.

`to_signature()` keeps its `Ed25519Signature::from_bytes` call and is not
flagged: `contains_path_call` requires a path boundary on the left, and
`Ed25519Signature::from_bytes` does not start at one.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `authorization_signature_dispatches_on_the_declared_algorithm` — happy path.
- `authorization_signature_refuses_an_unregistered_algorithm` — the fail-closed
  control. Without it a verifier that ignores the algorithm argument passes the
  entire rest of the suite.
- `authorization_signature_still_refuses_a_bad_signature`.
- existing `octo-protocol` and cross-node-chain vectors must pass unchanged.

### What the mutation actually showed

Hardcoding the algorithm inside `verify_authorization_signature` turns **exactly
one** vector red — `authorization_signature_refuses_an_unregistered_algorithm`.
The other 70 stay green, including the happy path and the tampered-message
check. Same result as every other site in this arc.

### One more drift found on the way

`ac11_every_allowlist_entry_still_exists_and_still_has_a_reason` checked two
constructors by hand while the scan checks three. An allowlisted file whose only
site is `Signature::from_slice` would have been told to keep an exception it no
longer needed. It now uses `ac11_site_count` like everything else.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-protocol -p octo-identity-resolver-node --all-targets --all-features -- -D warnings
cargo clippy -p quota-router-core --all-targets -- -D warnings
cargo test -p octo-protocol --lib
cargo test -p octo-identity-resolver-node --test cross_node_chain
cargo test -p octo-cli --test security_regressions
```

`quota-router-core` is not clippy-clean under `--all-features`: its
`litellm-mode` and `any-llm-mode` features are mutually exclusive by
`compile_error!`. Pre-existing and unrelated; run it without the flag.

71 lib vectors, 20 cross-node-chain vectors, 47 gate vectors. All pass.

## Backward compat

None required. Nothing is deployed. `verify_ed25519_signature` is public and has
three in-workspace production callers plus test callers, all updated here.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `missions/open/0009-c3-network-verification-dispatch` — the categorization
  this inherits
- `missions/claimed/0009-c9-ocrypt-and-dom-wire-field-dispatch` — the same
  argument applied to `intent.sender_id`, a bare 32-byte payload
- `crates/octo-cli/tests/security_regressions.rs` — `AC11_ALLOWED`

## Claimant

@unassigned

## Pull Request

#
