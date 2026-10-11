---
name: 0009-c5-vault-cache-bus-algorithm-dispatch
description: RFC-0009 AC#11, octo-vault slice. Dispatch the cache-bus envelope verification through AlgorithmRegistry instead of hardcoding Ed25519.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c-verification-dispatch
    - 0009-c3-network-verification-dispatch
---

# Mission `0009-c5-vault-cache-bus-algorithm-dispatch` — OPEN 2026-10-10

## Status

**Claimed and implemented 2026-10-10.** Found while sweeping for crates that
carried production AC#11 sites while appearing in no AC#11 list; recorded in
`missions/archived/completed/0009-c3-network-verification-dispatch` as one of three.

`ProducerTrustList` now stores a `TrustedProducerKey { algorithm, key }` and
verification dispatches through `verify_dispatch` on the recorded algorithm.
`with_algorithms` is the new entry point; `new` delegates to it with Ed25519
for callers that genuinely only have Ed25519 keys.

**The dalek `Signature` is gone from the verification path**, and that is the
part that matters: building a `Signature` before dispatch would assert Ed25519
before the registry got a say, and an unregistered algorithm would surface as a
malformed signature rather than as an unknown one.

Its `AC11_PENDING` entry stays at a count of **zero** rather than being
removed, so a future site here is a gate failure instead of a new silent gap.

## What landed

| Vector                                                           | What it pins                                                          |
| ---------------------------------------------------------------- | --------------------------------------------------------------------- |
| `trust_list_verification_dispatches_on_the_registered_algorithm` | the happy path via the explicit constructor                           |
| `trust_list_refuses_an_unregistered_algorithm`                   | the fail-closed half — **the control that makes the pair meaningful** |
| `trust_list_still_refuses_a_bad_signature`                       | dispatch did not degrade the check into a lookup                      |
| `trust_list_still_enforces_replay_and_sequence_monotonicity`     | the existing TV-CB behaviour survived the change                      |

All 71 `octo-vault` lib vectors pass, including the 67 pre-existing ones.

**Mutation-verified.** Replacing the dispatch's algorithm argument with a
hardcoded `ed25519_algorithm_id()` turns exactly one test red — the
unregistered-algorithm control — and leaves the other three green. That is the
result worth having: the happy path, the bad-signature check, and the replay
check all pass under a hardcoded algorithm, so none of them can be mistaken
for evidence that dispatch works. Only the fail-closed vector can tell the
difference, which is why it exists.

## A judgement call: `new` is kept, not removed

`new` now delegates to `with_algorithms` with Ed25519 rather than being
deleted. It is **not** a silent fallback: an entry built through `with_algorithms`
with an unregistered algorithm is still refused at verification time. Keeping it
is what lets the 67 existing vectors — and any Layer C binary that has not
migrated — keep working while the new entry point is the one that can express a
non-Ed25519 producer.

Had `new` been removed instead, the migration would have had to be all-or-nothing
across every caller, and there is no caller outside this file yet to migrate.

## Scope

`crates/octo-vault/src/cache_subscriber.rs` §`ProducerTrustList::verify_and_update_sequence`.

One production site. It resolves a producer's key by DID, then builds an
`ed25519_dalek::Signature` from the wire bytes and verifies against it.

## Why this one is NOT blocked, unlike the rest of the pending set

The pending sites in `AC11_PENDING` all carry the same stated blocker: the
RFC-0010 canonical DID codec, because the key is recovered by parsing a DID
into a bare 32 bytes that carries no algorithm. **That blocker does not
apply here, and assuming it did would have parked this site forever.**

`ProducerTrustList::new` takes `Vec<(String, VerifyingKey)>` — the keys are
**supplied by the caller**, not parsed out of a DID string. The DID is a map
key used for lookup only. So:

- There is no `pubkey_from_did` in the path.
- No codec change is needed for the verification to dispatch.
- The algorithm identity already exists at the point of construction; it is
  simply dropped when the tuple is stored.

That last point is the defect. `new` receives an already-typed
`VerifyingKey` and discards every hint of which algorithm produced it. The
information is available and thrown away.

## Preferred shape

`ProducerTrustList` stores `(String, AlgorithmId, VerifyingKey)` and
verification dispatches through `octo_algo_ed25519::verify_dispatch` with
the stored id.

**Not** a fallback to Ed25519 when the id is absent — AC#11 forbids a silent
default, and a trust list that silently verifies under Ed25519 when it meant
something else is exactly the failure the registry exists to prevent.

## Blast radius

One file, one struct, two constructors (`new`, `get_verifying_key`), one
verification method. `ProducerTrustList` has **no external caller**: the only
other reference in the workspace is a re-export in `octo-vault/src/lib.rs`.
`spawn_cache_subscriber_with_trust_list` and `init_producer_trust_list` are
the public entry points and both take the trust list as a parameter, so the
type's shape is not yet pinned by a Layer C binary.

## Layer direction — settled by measurement, not assumed

`octo-vault` is Layer B. The registry itself lives in `octo-ident`, also
Layer B, but `verify_dispatch` lives in `octo-algo-ed25519`, which is a
Layer E per-algorithm extension. So reaching the dispatch helper means a
B → E edge.

**This is not a forbidden direction, and the workspace already models it.**
`octo-wallet` is Layer B and depends on `octo-algo-ed25519` with this exact
manifest comment:

> constructing a `VerifyingKey` itself (RFC-0009 AC#11). Layer E
> per-algorithm extension depending on Layer B — the arrow never points
> back, which is why `octo-ident` itself has no ed25519-dalek dependency.

The arrow that must never point back is **E → B's internals**, i.e. a
per-algorithm crate reaching into the identity substrate. A Layer B
consumer depending on a Layer E algorithm crate is the ordinary direction:
the registry is the abstraction, the algorithm supplies an implementation
of it, and B names the trait rather than the implementation.

`octo-reputation` and `quota-router-sm-engine` do the same. The pattern is
established; this crate just has not needed it yet.

So `octo-vault` adds `octo-algo-ed25519` with a manifest comment naming
this reasoning, matching `octo-wallet`'s wording.

## Acceptance Criteria

Verified by the §Test Vectors list.

## Test Vectors

- `trust_list_verification_dispatches_on_the_registered_algorithm` — a
  genuine signature verifies under the algorithm the entry names. **Control:**
  refusing everything must not pass this.
- `trust_list_refuses_an_unregistered_algorithm` — the fail-closed half. An
  identifier this deployment does not implement must be refused, not treated
  as Ed25519. This is the control that proves the dispatch is real.
- `trust_list_still_refuses_a_bad_signature` — dispatch did not degrade the
  check into a lookup.
- `trust_list_still_enforces_replay_and_sequence_monotonicity` — the
  existing TV-CB vectors must survive the change unchanged.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-vault --all-targets --all-features -- -D warnings
cargo test -p octo-vault
```

## Backward compat

None required. Nothing is deployed, and the trust list has no external
constructor to break.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `missions/archived/completed/0009-c3-network-verification-dispatch` — the sweep that
  found this, and the pending table that is tracking it
- `missions/claimed/0009-b-algorithm-registry` — the registry
- `crates/octo-cli/tests/security_regressions.rs` — `AC11_PENDING`

## Claimant

@unassigned

## Pull Request

#
